//! Kani proofs: the engine against [`crate::spec`] for one opcode byte and
//! byte order, over every field value and machine state.
//!
//! Each harness fixes the opcode byte and leaves every other bit of the
//! instruction (and of an LDDW's second slot), all eleven registers, and the
//! data memory symbolic. It proves:
//!
//! 1. `decode_insn` accepts exactly the encodings that the RFC registry and
//!    the documented platform contract admit;
//! 2. one step of the direct CPU, and of SMIR at O0, O1, and O2, produces the
//!    specified registers, PC, memory, trap, exit, or call.

use rax::isa::ebpf::{
    EbpfCallFrame, EbpfConfig, EbpfCpu, EbpfDialect, EbpfExit, EbpfInsn, EbpfMemFault, EbpfTrap,
    decode_insn,
};
use rax::smir::ir::memory::{FlatMemory, SmirMemory};
use rax::smir::ir::types::Endian;

use crate::spec::{self, Fields, Order, Outcome, State};

mod fetch;
mod memory;
mod opcodes;
mod operators;
pub(crate) mod smir;

use memory::ProofMemory;

/// Bytes of code (one wide instruction) at address 0.
pub(crate) const CODE: usize = 16;
/// Platform memory: the code followed by a data window.
pub(crate) const MEMORY: usize = 32;
/// The RFC 9669 call stack frame of the default configuration.
const FRAME_SIZE: u64 = 4096;

/// Fixed-key `RandomState`: hash-map iteration order is irrelevant to every
/// proof, and Kani does not model the host RNG that seeds it.
pub(crate) fn fixed_random_state() -> std::hash::RandomState {
    // SAFETY: `RandomState` holds two `u64` keys; any key pair is valid.
    unsafe { std::mem::transmute::<[u64; 2], std::hash::RandomState>([0, 0]) }
}

pub(crate) fn endian(order: Order) -> Endian {
    match order {
        Order::Little => Endian::Little,
        Order::Big => Endian::Big,
    }
}

/// Encode a basic instruction with a raw `regs` byte (section 3.1).
pub(crate) fn encode(opcode: u8, regs: u8, offset: i16, imm: i32, order: Order) -> [u8; 8] {
    let (offset, imm) = match order {
        Order::Little => (offset.to_le_bytes(), imm.to_le_bytes()),
        Order::Big => (offset.to_be_bytes(), imm.to_be_bytes()),
    };
    [
        opcode, regs, offset[0], offset[1], imm[0], imm[1], imm[2], imm[3],
    ]
}

/// A symbolic machine: the instruction, its second slot, registers, and
/// memory, with the first byte fixed to `opcode`.
pub(crate) struct Machine {
    pub first: Fields,
    pub next: Fields,
    /// The decoded instruction; `None` exactly when admission fails.
    pub insn: Option<EbpfInsn>,
    pub state: State<MEMORY>,
    pub order: Order,
}

pub(crate) fn machine(opcode: u8, order: Order) -> Machine {
    machine_with(opcode, kani::any(), order)
}

/// [`machine`] with a fixed register-field byte.
pub(crate) fn machine_with(opcode: u8, regs: u8, order: Order) -> Machine {
    let first_bytes = encode(opcode, regs, kani::any(), kani::any(), order);
    let next_bytes: [u8; 8] = kani::any();
    let data: [u8; MEMORY - CODE] = kani::any();
    let mut memory = [0u8; MEMORY];
    memory[..8].copy_from_slice(&first_bytes);
    memory[8..16].copy_from_slice(&next_bytes);
    memory[CODE..].copy_from_slice(&data);
    let first = spec::fields(first_bytes, order);
    let next = spec::fields(next_bytes, order);
    let wide = opcode == 0x18;
    let admitted =
        spec::admitted(first).is_some() && (!wide || spec::continuation_admitted(first, next));

    // Decoding accepts exactly the admitted encodings.
    let length = if wide { 16 } else { 8 };
    let decoded = decode_insn(&memory[..length], EbpfDialect::Rfc9669, endian(order));
    assert!(
        decoded.is_ok() == admitted,
        "decoder admission differs from the registry"
    );

    Machine {
        first,
        next,
        insn: decoded.ok(),
        state: State {
            regs: kani::any(),
            pc: 0,
            memory,
        },
        order,
    }
}

pub(crate) fn flat_memory(bytes: &[u8; MEMORY]) -> FlatMemory {
    let mut memory = FlatMemory::new(MEMORY);
    memory.load(0, bytes);
    memory
}

pub(crate) fn memory_bytes(memory: &mut FlatMemory) -> [u8; MEMORY] {
    let mut bytes = [0u8; MEMORY];
    SmirMemory::read(memory, 0, &mut bytes).unwrap();
    bytes
}

/// One direct-CPU step against the specification.
///
/// The step executes the decoded instruction (`EbpfCpu::execute_decoded`);
/// [`fetch`] proves that the CPU's fetch yields exactly these bytes, and
/// [`memory`] that `FlatMemory` meets the [`ProofMemory`] contract used here.
pub(crate) fn check_direct(opcode: u8, order: Order) {
    check_direct_machine(machine(opcode, order));
}

fn check_direct_machine(m: Machine) {
    let Some(insn) = m.insn else {
        return;
    };
    let mut memory = ProofMemory {
        bytes: m.state.memory,
    };
    let mut cpu = EbpfCpu::new(EbpfConfig {
        endian: endian(m.order),
        ..EbpfConfig::default()
    });
    cpu.regs.r = m.state.regs;
    cpu.regs.pc = 0;
    let result = cpu.execute_decoded(&insn, &mut memory);
    let regs = m.state.regs;
    let unchanged = |cpu: &EbpfCpu, memory: &mut ProofMemory| {
        cpu.regs.r == regs && memory.bytes == m.state.memory
    };

    match spec::execute(m.first, m.next, m.state, m.order) {
        Outcome::Next(expected) => {
            assert!(result == Ok(None));
            assert!(cpu.regs.r == expected.regs);
            assert!(cpu.regs.pc == expected.pc);
            assert!(memory.bytes == expected.memory);
            assert!(cpu.call_frames.is_empty());
        }
        Outcome::Fault { addr, size, write } => {
            let fault = EbpfMemFault {
                addr,
                size: size as usize,
                write,
            };
            assert!(result == Err(EbpfTrap::MemoryFault { pc: 0, fault }));
            assert!(cpu.regs.pc == 0);
            assert!(unchanged(&cpu, &mut memory));
        }
        Outcome::Exit { r0 } => {
            assert!(result == Ok(Some(EbpfExit::Exit { r0 })));
            assert!(unchanged(&cpu, &mut memory));
        }
        Outcome::LocalCall { target, return_pc } => {
            // Platform ABI: the frame saves r6..r9, r10, and the return PC,
            // and the callee's frame pointer is one frame lower.
            assert!(result == Ok(None));
            assert!(cpu.regs.pc == target);
            let frame = EbpfCallFrame {
                caller_saved_regs: [regs[6], regs[7], regs[8], regs[9]],
                frame_pointer: regs[10],
                return_pc,
            };
            assert!(cpu.call_frames == [frame]);
            assert!(cpu.regs.r[..10] == regs[..10]);
            assert!(cpu.regs.r[10] == regs[10].wrapping_sub(FRAME_SIZE));
        }
        Outcome::Helper {
            id,
            by_btf,
            next_pc,
        } => {
            let exit = if by_btf {
                EbpfExit::BtfHelper { id, next_pc }
            } else {
                EbpfExit::Syscall { key: id, next_pc }
            };
            assert!(result == Ok(Some(exit)));
            assert!(cpu.regs.pc == next_pc);
            assert!(unchanged(&cpu, &mut memory));
        }
        Outcome::PseudoLoad { src_type } => {
            // The default platform binds nothing (see the platform proofs).
            assert!(result == Err(EbpfTrap::UnsupportedPseudoLoad { pc: 0, src_type }));
            assert!(cpu.regs.pc == 0);
            assert!(unchanged(&cpu, &mut memory));
        }
    }
}
