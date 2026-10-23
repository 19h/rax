//! V68 sections 3.3.1 and 6.1.3: old predicate reads and packet auto-AND.

#[path = "../../../support/hexagon.rs"]
mod lifter_stack;

use rax::config::{Endianness, HexagonIsa};
use rax::isa::hexagon::HexagonVcpu;
use rax::smir::ir::memory::SmirMemory;
use rax::smir::ir::types::{ArchReg, FunctionId, HexagonReg, SourceArch};
use rax::smir::ir::{FunctionBuilder, Terminator, TrapKind};
use rax::smir::optimize::{OptLevel, optimize_function};
use rax::smir::{
    BlockResult, ExitReason, FlatMemory, HexagonLifter, LiftContext, SmirContext, SmirInterpreter,
    SmirLifter,
};
use rax::vm::vcpu::{CpuState, HexagonRegisters, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn seed() -> HexagonRegisters {
    let mut regs = HexagonRegisters::default();
    regs.set_pc(0x1000);
    regs.r[1] = 1;
    regs.r[2] = 2;
    regs.r[3] = 1;
    regs.r[4] = 0xcccccccc;
    regs.r[5] = 0x2000;
    regs
}
fn direct(words: &[u32], regs: &HexagonRegisters) -> HexagonRegisters {
    let mem = Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x4000)]).unwrap());
    for (i, word) in words.iter().enumerate() {
        mem.write_obj(word.to_le(), GuestAddress(0x1000 + i as u64 * 4))
            .unwrap();
    }
    mem.write_obj(0x12345678u32.to_le(), GuestAddress(0x2000))
        .unwrap();
    let mut cpu = HexagonVcpu::new(0, mem, HexagonIsa::V68, Endianness::Little);
    cpu.set_state(&CpuState::hexagon(regs.clone())).unwrap();
    let packets = words
        .iter()
        .filter(|word| matches!((*word >> 14) & 3, 0 | 3))
        .count();
    for _ in 0..packets {
        assert!(cpu.step_insn().unwrap().is_none());
    }
    let CpuState::Hexagon(state) = cpu.get_state().unwrap() else {
        unreachable!()
    };
    state.regs
}
fn lifted(words: &[u32], regs: &HexagonRegisters, level: OptLevel) -> SmirContext {
    lifter_stack::run_on_large_lifter_stack("Hexagon predicate contract", || {
        lifted_inner(words, regs, level)
    })
}

fn lifted_inner(words: &[u32], regs: &HexagonRegisters, level: OptLevel) -> SmirContext {
    let mut lifter = HexagonLifter::default_isa();
    let mut lift = LiftContext::new(SourceArch::Hexagon);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (i, word) in words.iter().enumerate() {
        let result = lifter
            .lift_insn(0x1000 + i as u64 * 4, &word.to_le_bytes(), &mut lift)
            .unwrap();
        for op in result.ops {
            builder.push_op(op.guest_pc, op.kind);
        }
    }
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = builder.finish();
    optimize_function(&mut function, level);
    let mut context = SmirContext::new_hexagon();
    for n in 0..32 {
        context.write_arch_reg(
            ArchReg::Hexagon(HexagonReg::R(n)),
            regs.r[n as usize] as u64,
        );
    }
    for n in 0..4 {
        context.write_arch_reg(
            ArchReg::Hexagon(HexagonReg::P(n)),
            regs.p[n as usize] as u64,
        );
    }
    let mut memory = FlatMemory::new(0x4000);
    SmirMemory::write(&mut memory, 0x2000, &0x12345678u32.to_le_bytes()).unwrap();
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        function.entry_block().unwrap(),
    );
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Halt)),
        "{level:?}: {result:?}"
    );
    context
}
fn compare_all(words: &[u32], regs: &HexagonRegisters, expected_predicate: u8, expected_r4: u32) {
    let result = direct(words, regs);
    assert_eq!(result.p[0], expected_predicate);
    assert_eq!(result.r[4], expected_r4);
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let result = lifted(words, regs, level);
        assert_eq!(
            result.read_arch_reg(ArchReg::Hexagon(HexagonReg::P(0))),
            expected_predicate as u64,
            "{level:?}"
        );
        assert_eq!(
            result.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(4))),
            expected_r4 as u64,
            "{level:?}"
        );
    }
}
#[test]
fn compare_writes_auto_and_without_including_the_old_predicate() {
    for pred in 0..4u8 {
        for (a, b, c) in [(1, 2, 1), (1, 1, 2), (1, 1, 1), (1, 2, 3)] {
            for previous in [0, 0xff] {
                let mut regs = seed();
                regs.r[1] = a;
                regs.r[2] = b;
                regs.r[3] = c;
                regs.p[pred as usize] = previous;
                let words = [0xf2014200 | pred as u32, 0xf201c300 | pred as u32];
                let expected = if a == b && a == c { 0xff } else { 0 };
                assert_eq!(direct(&words, &regs).p[pred as usize], expected);
                for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                    assert_eq!(
                        lifted(&words, &regs, level)
                            .read_arch_reg(ArchReg::Hexagon(HexagonReg::P(pred))),
                        expected as u64
                    );
                }
            }
        }
    }
}
#[test]
fn ordinary_predicate_read_observes_the_old_packet_value() {
    let mut regs = seed();
    regs.p[0] = 0xff;
    compare_all(&[0xf2014200, 0x4185c004], &regs, 0, 0x12345678);
}
#[test]
fn dotnew_read_observes_the_new_value_instead_of_the_old_value() {
    let mut regs = seed();
    regs.r[2] = 1;
    regs.p[0] = 0;
    compare_all(&[0xf2014200, 0x4385c004], &regs, 0xff, 0x12345678);
}
#[test]
fn dotnew_aggregate_cancels_an_unmapped_load_after_false_then_true_compares() {
    let mut regs = seed();
    regs.p[0] = 0xff;
    regs.r[5] = 0x5000;
    compare_all(&[0xf2014200, 0xf2014300, 0x4385c004], &regs, 0, 0xcccccccc);
}
#[test]
fn predicate_register_transfers_auto_and_full_bytes() {
    let mut regs = seed();
    regs.p[0] = 0xff;
    regs.p[1] = 0xf0;
    regs.p[2] = 0x0f;
    compare_all(&[0x6b214100, 0x6b22c200], &regs, 0, 0xcccccccc);
}
#[test]
fn predicate_aggregation_resets_between_packets() {
    compare_all(&[0xf201c200, 0xf201c100], &seed(), 0xff, 0xcccccccc);
}

#[test]
fn opaque_predicate_consumer_after_a_producer_defers_to_direct_execution() {
    let mut regs = seed();
    regs.r[2] = 1;
    regs.r[6] = 4;
    regs.r[7] = 5;
    // LLVM 23, V68: { p0=cmp.eq(r1,r2); if(p0.new) r4=add(r6,r7) }.
    let words = [0xf2014200, 0xfb06e704];
    assert_eq!(direct(&words, &regs).r[4], 9);
    lifter_stack::run_on_large_lifter_stack("opaque predicate admission", || {
        let mut lifter = HexagonLifter::default_isa();
        let mut context = LiftContext::new(SourceArch::Hexagon);
        lifter
            .lift_insn(0x1000, &words[0].to_le_bytes(), &mut context)
            .unwrap();
        let result = lifter.lift_insn(0x1004, &words[1].to_le_bytes(), &mut context);
        assert!(
            matches!(result, Err(rax::smir::LiftError::Unsupported { ref mnemonic, .. })
            if mnemonic == "opaque predicate consumer after a packet producer")
        );
    });
    // Without a preceding producer, an ordinary predicate consumer remains
    // supported and observes its architectural input byte.
    regs.p[0] = 0xff;
    compare_all(&[0xfb06c704], &regs, 0xff, 9);
}

// LLVM 23, hexagonv68: a compare plus a compound compare/.new jump.
// Each row records the predicate, branch sense, and compound operand form.
const COMPOUND_PACKETS: [([u32; 2], u8, bool, bool); 8] = [
    ([0x75034000, 0x1401e210], 0, true, true),
    ([0x75034000, 0x1441e210], 0, false, true),
    ([0x75034001, 0x1401f210], 1, true, true),
    ([0x75034001, 0x1441f210], 1, false, true),
    ([0xf2014200, 0x1003e010], 0, true, false),
    ([0xf2014200, 0x1043e010], 0, false, false),
    ([0xf2014201, 0x1203e010], 1, true, false),
    ([0xf2014201, 0x1243e010], 1, false, false),
];

fn compound_seed(
    pred: u8,
    register_compare: bool,
    first: bool,
    second: bool,
    old: u8,
) -> HexagonRegisters {
    let mut regs = seed();
    let (register_result, immediate_result) = if register_compare {
        (second, first)
    } else {
        (first, second)
    };
    regs.r[2] = if register_result { 1 } else { 2 };
    regs.r[3] = if immediate_result { 0 } else { 1 };
    regs.p[pred as usize] = old;
    regs
}

#[test]
fn compound_compare_jump_uses_the_aggregate_new_predicate() {
    for (words, pred, sense, register_compare) in COMPOUND_PACKETS {
        for first in [false, true] {
            for second in [false, true] {
                for old in [0, 0xff] {
                    let regs = compound_seed(pred, register_compare, first, second, old);
                    let result = direct(&words, &regs);
                    assert_eq!(
                        result.p[pred as usize],
                        if first && second { 0xff } else { 0 }
                    );
                    assert_eq!(
                        result.pc(),
                        if (first && second) == sense {
                            0x1020
                        } else {
                            0x1008
                        },
                        "{words:x?}"
                    );
                }
            }
        }
    }
}

#[test]
fn lifted_compound_jump_condition_uses_the_aggregate_at_every_opt_level() {
    lifter_stack::run_on_large_lifter_stack("compound predicate condition", || {
        for (words, pred, sense, register_compare) in COMPOUND_PACKETS {
            for first in [false, true] {
                for second in [false, true] {
                    for old in [0, 0xff] {
                        let regs = compound_seed(pred, register_compare, first, second, old);
                        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                            let mut lifter = HexagonLifter::default_isa();
                            let mut context = LiftContext::new(SourceArch::Hexagon);
                            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
                            let mut condition = None;
                            for (index, word) in words.into_iter().enumerate() {
                                let lifted = lifter
                                    .lift_insn(
                                        0x1000 + index as u64 * 4,
                                        &word.to_le_bytes(),
                                        &mut context,
                                    )
                                    .unwrap();
                                for op in lifted.ops {
                                    builder.push_op(op.guest_pc, op.kind);
                                }
                                if let rax::smir::ControlFlow::CondBranchReg { cond, .. } =
                                    lifted.control_flow
                                {
                                    condition = Some(cond);
                                }
                            }
                            // Preserve the frontend's branch condition through optimization.
                            builder.push_op(
                                0x1004,
                                rax::smir::ir::ops::OpKind::Mov {
                                    dst: rax::smir::ir::types::VReg::Arch(ArchReg::Hexagon(
                                        HexagonReg::R(30),
                                    )),
                                    src: rax::smir::ir::types::SrcOperand::Reg(
                                        condition.expect("branch condition"),
                                    ),
                                    width: rax::smir::ir::types::OpWidth::W32,
                                },
                            );
                            builder.set_terminator(Terminator::Trap {
                                kind: TrapKind::Halt,
                            });
                            let mut function = builder.finish();
                            optimize_function(&mut function, level);
                            let mut state = SmirContext::new_hexagon();
                            for n in 0..32 {
                                state.write_arch_reg(
                                    ArchReg::Hexagon(HexagonReg::R(n)),
                                    regs.r[n as usize] as u64,
                                );
                            }
                            for n in 0..4 {
                                state.write_arch_reg(
                                    ArchReg::Hexagon(HexagonReg::P(n)),
                                    regs.p[n as usize] as u64,
                                );
                            }
                            let result = SmirInterpreter::new().execute_block(
                                &mut state,
                                &mut FlatMemory::new(0),
                                function.entry_block().unwrap(),
                            );
                            assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
                            assert_eq!(
                                state.read_arch_reg(ArchReg::Hexagon(HexagonReg::P(pred))),
                                if first && second { 0xff } else { 0 }
                            );
                            assert_eq!(
                                state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(30))),
                                u64::from((first && second) == sense),
                                "{words:x?} {level:?}"
                            );
                        }
                    }
                }
            }
        }
    });
}
