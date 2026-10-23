//! V68 section 3.3.2: complete lifted packets commit or restore their entry state.
use super::packet_memory::{context, lifted, machine};
use rax::config::Endianness;
use rax::smir::ir::context::ArchRegState;
use rax::smir::ir::memory::SmirMemory;
use rax::smir::ir::types::{ArchReg, HexagonReg};
use rax::smir::optimize::OptLevel;
use rax::smir::{BlockResult, ExitReason};

fn memory_fault(result: BlockResult) {
    assert!(
        matches!(
            result,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ),
        "{result:?}"
    );
}

#[test]
fn later_store_fault_preserves_earlier_ram() {
    let (_, _, mut before) = machine(0xa1814200, 0x2000, 0x1000, Endianness::Little);
    before.r[4] = 0x5000;
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let (state, mut memory, result) = lifted(&[0xa1814200, 0xa184c200], &before, level);
        memory_fault(result);
        let mut bytes = [0; 32];
        memory.read(0x2000, &mut bytes).unwrap();
        assert_eq!(bytes, [0xa5; 32], "{level:?}");
        assert_eq!(state.pc, 0x1000);
        assert_eq!(state.insn_count, 0);
    }
}

#[test]
fn later_fault_preserves_earlier_gpr() {
    let (_, _, mut before) = machine(0x78004038, 0x2000, 0x1000, Endianness::Little);
    before.r[4] = 0x5000;
    before.r[24] = 0x77;
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let (state, _, result) = lifted(&[0x78004038, 0xa184c200], &before, level);
        memory_fault(result);
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(24))),
            0x77,
            "{level:?}"
        );
        assert_eq!(state.insn_count, 0);
    }
}

#[test]
fn a_later_packet_fault_keeps_the_previous_register_commit() {
    let (_, _, mut before) = machine(0x7800c038, 0x2000, 0x1000, Endianness::Little);
    before.r[4] = 0x5000;
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let (state, _, result) = lifted(&[0x7800c038, 0x78004058, 0xa184c200], &before, level);
        memory_fault(result);
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(24))),
            1,
            "{level:?}"
        );
        assert_eq!(state.pc, 0x1004);
        assert_eq!(state.insn_count, 1);
    }
}

#[test]
fn predicate_writes_roll_back_with_a_later_memory_fault() {
    let (_, _, mut before) = machine(0xf2014200, 0x2000, 0x1000, Endianness::Little);
    before.r[2] = before.r[1];
    before.r[4] = 0x5000;
    before.p[0] = 0;
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let (state, _, result) = lifted(&[0xf2014200, 0xa184c200], &before, level);
        memory_fault(result);
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::P(0))),
            0,
            "{level:?}"
        );
    }
}

#[test]
fn saturation_usr_flags_roll_back_with_the_packet() {
    let (_, _, mut before) = machine(0x8c8240b8, 0x2000, 0x1000, Endianness::Little);
    before.r[2] = 0x80000000;
    before.r[4] = 0x5000;
    before.c[8] = 0x400;
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let (state, _, result) = lifted(&[0x8c8240b8, 0xa184c200], &before, level);
        memory_fault(result);
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::Usr)),
            0x400,
            "{level:?}"
        );
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(24))),
            before.r[24] as u64
        );
    }
}

#[test]
fn a_faulting_postincrement_store_restores_its_base() {
    let (_, _, before) = machine(0xab81c208, 0x5000, 0x1000, Endianness::Little);
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let (state, _, result) = lifted(&[0xab81c208], &before, level);
        memory_fault(result);
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(1))),
            0x5000,
            "{level:?}"
        );
    }
}

#[test]
fn vector_results_roll_back_with_a_later_scalar_store_fault() {
    let (_, _, mut before) = machine(0x1fa241c0, 0x2000, 0x1000, Endianness::Little);
    before.r[4] = 0x5000;
    before.v[0] = [0x77777777; 32];
    before.v[1] = [0x01010101; 32];
    before.v[2] = [0x02020202; 32];
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let expected = context(&before);
        let (state, _, result) = lifted(&[0x1fa241c0, 0xa184c200], &before, level);
        memory_fault(result);
        let (ArchRegState::Hexagon(actual), ArchRegState::Hexagon(expected)) =
            (&state.arch_regs, &expected.arch_regs)
        else {
            unreachable!()
        };
        assert_eq!(actual.v, expected.v, "{level:?}");
    }
}

#[test]
fn alignment_failure_reports_and_restores_the_packet_start() {
    let (_, _, mut before) = machine(0x78004038, 0x2000, 0x1000, Endianness::Little);
    before.r[4] = 0x2001;
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let (state, _, result) = lifted(&[0x78004038, 0xa184c200], &before, level);
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::AlignmentFault {
                    pc: 0x1000,
                    address: 0x2001,
                    ..
                })
            ),
            "{result:?}"
        );
        assert_eq!(state.pc, 0x1000);
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(24))),
            before.r[24] as u64
        );
    }
}

#[test]
fn committed_memory_is_visible_to_the_following_packet() {
    let (_, _, before) = machine(0xa181c200, 0x2000, 0x1000, Endianness::Little);
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let (state, _, result) = lifted(&[0xa181c200, 0x9181c018], &before, level);
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(24))),
            before.r[2] as u64
        );
        assert_eq!(state.insn_count, 2);
        assert_eq!(state.pc, 0x1008);
    }
}

#[test]
fn noshuf_forwards_pending_store_bytes_before_publication() {
    let (_, _, mut before) = machine(0xa1814200, 0x2000, 0x1000, Endianness::Little);
    before.r[4] = 0x2000;
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let (state, _, result) = lifted(&[0xa1814200, 0x9184c018], &before, level);
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(24))),
            before.r[2] as u64
        );
        assert_eq!(state.insn_count, 1);
    }
}

#[path = "../../../support/hexagon.rs"]
mod lifter_stack;

#[test]
fn block_lifting_executes_the_packet_tail_before_an_early_branch_retires() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::ir::{SmirBlock, Terminator, TrapKind};
    use rax::smir::{FlatMemory, HexagonLifter, LiftContext, SmirInterpreter, SmirLifter};
    lifter_stack::run_on_large_lifter_stack("Hexagon early branch packet", || {
        let mut memory = FlatMemory::new(0x4000);
        memory.write(0x1000, &0x58004008_u32.to_le_bytes()).unwrap();
        memory.write(0x1004, &0x7800c038_u32.to_le_bytes()).unwrap();
        let mut lifter = HexagonLifter::default_isa();
        let mut lift = LiftContext::new(SourceArch::Hexagon);
        struct Instructions<'a>(&'a FlatMemory);
        impl rax::smir::lift::MemoryReader for Instructions<'_> {
            fn read(
                &self,
                addr: u64,
                size: usize,
            ) -> Result<Vec<u8>, rax::smir::ir::memory::MemoryError> {
                rax::smir::ir::memory::MemoryReader::read(self.0, addr, size)
            }
        }
        let block = lifter
            .lift_block(0x1000, &Instructions(&memory), &mut lift)
            .unwrap();
        let target = lift.get_or_create_block(0x1010);
        let mut interpreter = SmirInterpreter::new();
        interpreter.add_block(
            0x1010,
            SmirBlock {
                id: target,
                guest_pc: 0x1010,
                phis: vec![],
                ops: vec![],
                terminator: Terminator::Trap {
                    kind: TrapKind::Halt,
                },
                exec_count: 0,
            },
        );
        let mut state = rax::smir::SmirContext::new_hexagon();
        let result = interpreter.execute_block(&mut state, &mut memory, &block);
        assert!(
            matches!(result, BlockResult::Continue(0x1010)),
            "{result:?}"
        );
        assert_eq!(state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(24))), 1);
        assert_eq!(state.pc, 0x1010);
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::Pc)),
            0x1010
        );
        assert_eq!(state.insn_count, 1);
    });
}
