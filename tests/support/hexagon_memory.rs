//! Flat-memory scalar Hexagon execution fixtures shared by packet contracts.
use rax::config::{Endianness, HexagonIsa};
use rax::error::{Error, MemoryAccessKind};
use rax::isa::hexagon::HexagonVcpu;
use rax::vm::vcpu::{CpuState, HexagonRegisters, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

pub(crate) const LOW_MEMORY: [u8; 8] = [0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10];

#[path = "hexagon.rs"]
mod lifter_stack;

pub(crate) fn assert_alignment(
    error: Error,
    pc: u64,
    address: u64,
    alignment: u32,
    access: MemoryAccessKind,
) {
    assert!(
        matches!(error, Error::AlignmentFault { pc:p, address:a, alignment:n, access:k }
        if p == pc && a == address && n == alignment && k == access),
        "{error:?}"
    );
}
pub(crate) fn machine(
    word: u32,
    address: u32,
    pc: u32,
    endian: Endianness,
) -> (HexagonVcpu, Arc<GuestMemoryMmap>, HexagonRegisters) {
    let memory = Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x4000)]).unwrap());
    let code = match endian {
        Endianness::Little => word.to_le_bytes(),
        Endianness::Big => word.to_be_bytes(),
    };
    memory.write_slice(&code, GuestAddress(pc as u64)).unwrap();
    memory
        .write_slice(&[0xa5; 32], GuestAddress(0x2000))
        .unwrap();
    memory.write_slice(&LOW_MEMORY, GuestAddress(0x10)).unwrap();
    let mut regs = HexagonRegisters::default();
    regs.set_pc(pc);
    regs.r[1] = address;
    regs.r[2] = 0x11223344;
    regs.r[3] = 0x55667788;
    let mut cpu = HexagonVcpu::new(0, memory.clone(), HexagonIsa::V68, endian);
    cpu.set_state(&CpuState::hexagon(regs.clone())).unwrap();
    (cpu, memory, regs)
}
pub(crate) fn regs(cpu: &HexagonVcpu) -> HexagonRegisters {
    let CpuState::Hexagon(state) = cpu.get_state().unwrap() else {
        unreachable!()
    };
    state.regs
}
pub(crate) fn lifted(
    words: &[u32],
    registers: &HexagonRegisters,
    level: rax::smir::optimize::OptLevel,
) -> (
    rax::smir::SmirContext,
    rax::smir::FlatMemory,
    rax::smir::BlockResult,
) {
    use rax::smir::ir::memory::SmirMemory;
    use rax::smir::ir::types::{ArchReg, FunctionId, HexagonReg, SourceArch};
    use rax::smir::ir::{FunctionBuilder, Terminator, TrapKind};
    use rax::smir::{HexagonLifter, LiftContext, SmirLifter};
    lifter_stack::run_on_large_lifter_stack("Hexagon alignment contract", || {
        let mut lifter = HexagonLifter::default_isa();
        let mut context = LiftContext::new(SourceArch::Hexagon);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        for (index, word) in words.iter().enumerate() {
            let lifted = lifter
                .lift_insn(0x1000 + index as u64 * 4, &word.to_le_bytes(), &mut context)
                .unwrap();
            for op in lifted.ops {
                builder.push_op(op.guest_pc, op.kind);
            }
        }
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let mut function = builder.finish();
        rax::smir::optimize::optimize_function(&mut function, level);
        let mut state = rax::smir::SmirContext::new_hexagon();
        for n in 0..32 {
            state.write_arch_reg(
                ArchReg::Hexagon(HexagonReg::R(n)),
                registers.r[n as usize] as u64,
            );
        }
        for n in 0..4 {
            state.write_arch_reg(
                ArchReg::Hexagon(HexagonReg::P(n)),
                registers.p[n as usize] as u64,
            );
        }
        state.write_arch_reg(
            ArchReg::Hexagon(HexagonReg::Gp),
            u64::from(registers.c[11] & !0x3f),
        );
        let mut memory = rax::smir::FlatMemory::new(0x4000);
        memory.write(0x2000, &[0xa5; 32]).unwrap();
        memory.write(0x10, &LOW_MEMORY).unwrap();
        let result = rax::smir::SmirInterpreter::new().execute_block(
            &mut state,
            &mut memory,
            function.entry_block().unwrap(),
        );
        (state, memory, result)
    })
}
