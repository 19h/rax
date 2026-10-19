//! SMIR planes: lift the symbolic instruction, optimize at the requested
//! level, and interpret exactly one guest instruction.
//!
//! The program is the instruction at address 0 (8 bytes, or 16 for an LDDW)
//! and `EXIT` at every other address, so the fall-through and every other
//! successor the lifter discovers is a concrete `EXIT`. A one-instruction run
//! budget stops at the next guest boundary, where the context PC is the
//! successor the specification names.

use rax::isa::ebpf::{
    EbpfCallFrame, EbpfConfig, EbpfDialect, EbpfHelperKey, EbpfMemFault, EbpfTrap,
};
use rax::smir::ir::SmirFunction;
use rax::smir::ir::memory::{MemoryError, SmirMemory};
use rax::smir::ir::ops::RvVectorState;
use rax::smir::ir::types::{ArchReg, EbpfReg, GuestAddr};
use rax::smir::lift::{LiftContext, MemoryReader, SmirLifter};
use rax::smir::optimize::{OptLevel, optimize_function};
use rax::smir::{EbpfLifter, ExitReason, SmirContext, SmirInterpreter, SmirOp};

use super::{CODE, Machine, ProofMemory, endian, machine, machine_with};
use crate::spec::{self, Order, Outcome};

struct Program {
    code: [u8; CODE],
    /// Bytes of `code` that belong to the instruction (8, or 16 for LDDW).
    length: u64,
    exit: [u8; 8],
}

impl MemoryReader for Program {
    fn read(&self, addr: GuestAddr, size: usize) -> Result<Vec<u8>, MemoryError> {
        let mut bytes = Vec::with_capacity(size);
        for index in 0..size as u64 {
            let at = addr.wrapping_add(index);
            bytes.push(if at < self.length {
                self.code[at as usize]
            } else {
                self.exit[(at % 8) as usize]
            });
        }
        Ok(bytes)
    }
}

fn read_regs(context: &SmirContext) -> [u64; 11] {
    let mut regs = [0u64; 11];
    for (index, value) in regs.iter_mut().enumerate() {
        *value = context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(index as u8)));
    }
    regs
}

/// Lift the instruction once, then check one guest step at each level.
pub(crate) fn check_smir(opcode: u8, order: Order, levels: &[OptLevel]) {
    check_smir_machine(machine(opcode, order), opcode, order, levels);
}

/// [`check_smir`] for each register-field byte in turn. With the register
/// numbers concrete, the lifted IR and the interpreter's dispatch over it are
/// concrete in shape; every other field and all state stay symbolic.
pub(crate) fn check_smir_each_register_field(opcode: u8, order: Order, levels: &[OptLevel]) {
    for regs in 0..=u8::MAX {
        check_smir_machine(machine_with(opcode, regs, order), opcode, order, levels);
    }
}

fn check_smir_machine(m: Machine, opcode: u8, order: Order, levels: &[OptLevel]) {
    if m.insn.is_none() {
        return;
    }
    let endian = endian(order);
    let mut code = [0u8; CODE];
    code.copy_from_slice(&m.state.memory[..CODE]);
    let exit = super::encode(0x95, 0, 0, 0, order);
    let length = if opcode == 0x18 { 16 } else { 8 };
    let program = Program { code, length, exit };

    let mut lifter = EbpfLifter::new(EbpfDialect::Rfc9669, endian);
    let mut lift_context = LiftContext::new(lifter.source_arch());
    lift_context.endian = endian;
    let functions = lifter.lift_program(0, &program, &mut lift_context);
    assert!(functions.is_ok(), "every admitted instruction lifts");
    let functions = functions.unwrap();
    for &level in levels {
        check_level(&m, &functions, level);
    }
}

fn check_level(m: &Machine, functions: &[SmirFunction], level: OptLevel) {
    let endian = endian(m.order);
    let mut interpreter = SmirInterpreter::new();
    interpreter.set_max_insns(1);
    for function in functions {
        let mut function = function.clone();
        if level != OptLevel::O0 {
            optimize_function(&mut function, level);
        }
        interpreter.add_function(function);
    }

    let config = EbpfConfig {
        endian,
        ..EbpfConfig::default()
    };
    let mut context = SmirContext::new_ebpf_with_config(config);
    let regs = m.state.regs;
    for (index, value) in regs.iter().enumerate() {
        context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(index as u8)), *value);
    }
    context.pc = 0;
    let mut memory = ProofMemory {
        bytes: m.state.memory,
    };
    let reason = interpreter.run(&mut context, &mut memory);
    assert!(context.insn_count == 1);
    let final_regs = read_regs(&context);
    let unchanged = final_regs == regs && memory.bytes == m.state.memory;

    match spec::execute(m.first, m.next, m.state, m.order) {
        Outcome::Next(expected) => {
            assert!(matches!(reason, ExitReason::InsnLimit));
            assert!(context.pc == expected.pc);
            assert!(final_regs == expected.regs);
            assert!(memory.bytes == expected.memory);
        }
        Outcome::Fault { addr, size, write } => {
            let fault = EbpfMemFault {
                addr,
                size: size as usize,
                write,
            };
            let trap = EbpfTrap::MemoryFault { pc: 0, fault };
            assert!(matches!(reason, ExitReason::EbpfTrap(t) if t == trap));
            assert!(context.pc == 0);
            assert!(unchanged);
        }
        Outcome::Exit { r0 } => {
            assert!(matches!(reason, ExitReason::Return { to } if to == r0));
            assert!(unchanged);
        }
        Outcome::LocalCall { target, return_pc } => {
            assert!(matches!(reason, ExitReason::InsnLimit));
            assert!(context.pc == target);
            let frame = EbpfCallFrame {
                caller_saved_regs: [regs[6], regs[7], regs[8], regs[9]],
                frame_pointer: regs[10],
                return_pc,
            };
            let rax::smir::ir::context::ArchRegState::Ebpf(state) = &context.arch_regs else {
                panic!("eBPF context");
            };
            assert!(state.call_frames == [frame]);
            assert!(final_regs[..10] == regs[..10]);
            assert!(final_regs[10] == regs[10].wrapping_sub(4096));
        }
        Outcome::Helper {
            id,
            by_btf,
            next_pc,
        } => {
            let key = if by_btf {
                EbpfHelperKey::Btf(id)
            } else {
                EbpfHelperKey::Static(id)
            };
            assert!(matches!(&reason, ExitReason::Syscall { num, .. } if *num == key.smir_key()));
            assert!(context.pc == next_pc);
            assert!(unchanged);
        }
        Outcome::PseudoLoad { src_type } => {
            let trap = EbpfTrap::UnsupportedPseudoLoad { pc: 0, src_type };
            assert!(matches!(reason, ExitReason::EbpfTrap(t) if t == trap));
            assert!(unchanged);
        }
    }
}

/// SMIR model reduction (see the README). The interpreter dispatches an
/// operation through a chain of category functions, each of which matches
/// its own `OpKind`s and passes every other kind on unchanged. The eBPF
/// lifter emits only kinds handled by the arithmetic, logic, shift, bit,
/// data-movement, memory, flags (`SetCC`), and system (`EbpfInstruction`,
/// `Syscall`) categories, so for eBPF IR the chain from
/// `execute_op_x86_stack_flags` through `execute_op_binary` is a pure
/// forward to `execute_op_flags`, and the categories after `system`
/// (`execute_op_far_return`, `execute_op_opmask`) and the RVV delegation are
/// unreachable. The harnesses replace the forward with its target and the
/// unreachable functions with these panics; a panic that were reachable
/// would fail the proof.
pub(crate) fn no_rv_vector(
    _: &mut SmirContext,
    _: &mut dyn SmirMemory,
    _: u32,
    _: u8,
    _: u64,
    _: &RvVectorState,
) {
    panic!("RVV delegation is not reachable from eBPF IR");
}

pub(crate) fn no_category(
    _: &SmirInterpreter,
    _: &mut SmirContext,
    _: &mut dyn SmirMemory,
    _: &SmirOp,
) -> Result<(), MemoryError> {
    panic!("operation category is not reachable from eBPF IR");
}
