//! Strict-lift and fetch-frontier coverage for AVX2 `VPBLENDD`.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::optimize::{OptLevel, optimize_function};

const INITIAL_FLAGS: u64 = 0x2 | 0x08D5;

fn lift_nonstrict(bytes: &[u8]) -> Result<LiftResult, LiftError> {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    lifter.lift_insn(0x1000, bytes, &mut ctx)
}

fn assert_incomplete(bytes: &[u8], need: usize) {
    let result = lift_single(bytes);
    let debug = format!("{result:?}");
    assert!(
        matches!(
            result,
            Err(LiftError::Incomplete {
                addr: 0x1000,
                have,
                need: actual_need,
            }) if have == bytes.len() && actual_need == need
        ),
        "{bytes:02X?}: {debug}"
    );
}

fn execute_vpblendd(
    bytes: &[u8],
    level: OptLevel,
    ctx: &mut SmirContext,
    memory: &mut dyn SmirMemory,
) -> BlockResult {
    let lifted = lift_single(bytes).expect("strict VPBLENDD lift");
    assert_eq!(lifted.bytes_consumed, bytes.len());
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = builder.finish();
    function.blocks[0].ops = lifted.ops;
    optimize_function(&mut function, level);
    SmirInterpreter::new().execute_block(ctx, memory, &function.blocks[0])
}

fn blend_dwords(first: &[u64; 16], second: &[u64; 16], lanes: u8, imm: u8) -> [u64; 16] {
    let mut result = [0_u64; 16];
    for lane in 0..lanes {
        let word = usize::from(lane / 2);
        let shift = u32::from(lane % 2) * 32;
        let source = if (imm >> lane) & 1 == 0 {
            first[word]
        } else {
            second[word]
        };
        result[word] |= ((source >> shift) & 0xFFFF_FFFF) << shift;
    }
    result
}

#[test]
fn vpblendd_strictly_lifts_register_memory_width_alias_and_extension_forms() {
    for (bytes, width, dst, src1, src2, selected_src1, selected_src2) in [
        (
            &[0xC4, 0xE3, 0x71, 0x02, 0xC2, 0x0A][..],
            VecWidth::V128,
            X86Reg::Xmm(0),
            X86Reg::Xmm(1),
            X86Reg::Xmm(2),
            &[0_u8, 2][..],
            &[1_u8, 3][..],
        ),
        (
            &[0xC4, 0xE3, 0x7D, 0x02, 0xC2, 0xA5][..],
            VecWidth::V256,
            X86Reg::Ymm(0),
            X86Reg::Ymm(0),
            X86Reg::Ymm(2),
            &[1_u8, 3, 4, 6][..],
            &[0_u8, 2, 5, 7][..],
        ),
        (
            &[0xC4, 0x43, 0x25, 0x02, 0xCA, 0x5A][..],
            VecWidth::V256,
            X86Reg::Ymm(9),
            X86Reg::Ymm(11),
            X86Reg::Ymm(10),
            &[0_u8, 2, 5, 7][..],
            &[1_u8, 3, 4, 6][..],
        ),
    ] {
        let result =
            lift_single(bytes).unwrap_or_else(|error| panic!("VPBLENDD {bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );

        let selected = |source: X86Reg| {
            result
                .ops
                .iter()
                .filter_map(|op| match op.kind {
                    OpKind::VExtractLane {
                        vec: VReg::Arch(ArchReg::X86(actual)),
                        lane,
                        elem: VecElementType::I32,
                        ..
                    } if actual == source => Some(lane),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(selected(src1), selected_src1, "{bytes:02X?}: source 1");
        assert_eq!(selected(src2), selected_src2, "{bytes:02X?}: source 2");
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                width: actual_width,
                ..
            } if actual_dst == dst && actual_width == width
        )));
    }

    let memory = lift_single(&[0x67, 0x64, 0xC4, 0xE3, 0x65, 0x02, 0x4C, 0x70, 0x01, 0x96])
        .expect("address-size and FS-relative VPBLENDD memory form");
    assert_eq!(memory.bytes_consumed, 10);
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V256,
            ..
        }
    )));
    assert!(
        !memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );
}

#[test]
fn vpblendd_w1_is_terminal_ud_at_the_opcode_frontier_in_both_lift_modes() {
    for bytes in [
        &[0xC4, 0xE3, 0xF1, 0x02][..],
        &[0xC4, 0xE3, 0xF5, 0x02, 0x84, 0x88, 0, 0, 0, 0xA5][..],
        &[0x67, 0x64, 0xC4, 0xE3, 0xF5, 0x02][..],
    ] {
        let expected_len = bytes.iter().position(|byte| *byte == 0x02).expect("opcode") + 1;
        let strict = lift_single(bytes)
            .unwrap_or_else(|error| panic!("strict W1 VPBLENDD {bytes:02X?}: {error:?}"));
        assert_invalid_opcode_trap(&strict, expected_len);
        let nonstrict = lift_nonstrict(bytes)
            .unwrap_or_else(|error| panic!("nonstrict W1 VPBLENDD {bytes:02X?}: {error:?}"));
        assert_invalid_opcode_trap(&nonstrict, expected_len);
    }
}

#[test]
fn vpblendd_w0_fetch_frontiers_require_prefix_opcode_modrm_address_and_immediate() {
    assert_incomplete(&[0xC4], 3);
    assert_incomplete(&[0xC4, 0xE3], 3);
    assert_incomplete(&[0xC4, 0xE3, 0x75], 4);
    assert_incomplete(&[0xC4, 0xE3, 0x75, 0x02], 5);
    assert_incomplete(&[0xC4, 0xE3, 0x75, 0x02, 0xC2], 6);
    assert_incomplete(&[0xC4, 0xE3, 0x75, 0x02, 0x84], 6);
    assert_incomplete(&[0xC4, 0xE3, 0x75, 0x02, 0x84, 0x88], 10);
    assert_incomplete(&[0xC4, 0xE3, 0x75, 0x02, 0x84, 0x88, 0, 0, 0, 0], 11);
}

#[test]
fn vpblendd_interpretation_matches_raw_lane_selection_at_o0_o1_o2() {
    let first = [
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0x8000_0000_7FFF_FFFF,
        0x7FC0_1234_FF80_0001,
        0xA5A5_A5A5_A5A5_A5A5,
        0x5A5A_5A5A_5A5A_5A5A,
        6,
        7,
        8,
        9,
        10,
        11,
        12,
        13,
        14,
        15,
    ];
    let second: [u64; 16] = [
        0xAAAA_AAAA_5555_5555,
        0x0000_0000_FFFF_FFFF,
        0x1357_9BDF_2468_ACE0,
        0xFFFF_FFFE_0000_0001,
        0xCCCC_CCCC_CCCC_CCCC,
        0xDDDD_DDDD_DDDD_DDDD,
        0xEEEE_EEEE_EEEE_EEEE,
        0xFFFF_FFFF_FFFF_FFFF,
        0,
        1,
        2,
        3,
        4,
        5,
        6,
        7,
    ];

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut context = SmirContext::new_x86_64();
        context.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
        context.flags.lazy = None;
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!("x86 context must contain x86 state");
        };
        x86.xmm[0] = first;
        x86.xmm[2] = second;
        let exit = execute_vpblendd(
            &[0xC4, 0xE3, 0x7D, 0x02, 0xC2, 0xA5],
            level,
            &mut context,
            &mut FlatMemory::new(1),
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[0], blend_dwords(&first, &second, 8, 0xA5));
        assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);

        let mut context = SmirContext::new_x86_64();
        context.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
        context.flags.lazy = None;
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!();
        };
        x86.xmm[1] = first;
        x86.xmm[2] = second;
        x86.xmm[0] = [u64::MAX; 16];
        let exit = execute_vpblendd(
            &[0xC4, 0xE3, 0x71, 0x02, 0xC2, 0xFA],
            level,
            &mut context,
            &mut FlatMemory::new(1),
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[0], blend_dwords(&first, &second, 4, 0x0A));
        assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);
    }
}

#[test]
fn vpblendd_unaligned_memory_is_exact_and_faults_before_destination_commit() {
    let first = [0x1111_1111_1111_1111_u64; 16];
    let second: [u64; 16] = [
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0x8000_0000_7FFF_FFFF,
        0x7FC0_1234_FF80_0001,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    let memory_bytes = second[..4]
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect::<Vec<_>>();

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x181);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!();
        };
        x86.xmm[1] = first;
        let mut memory = FlatMemory::new(0x400);
        memory.write(0x181, &memory_bytes).unwrap();
        let exit = execute_vpblendd(
            &[0xC4, 0xE3, 0x75, 0x02, 0x00, 0x5A],
            level,
            &mut context,
            &mut memory,
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[0], blend_dwords(&first, &second, 8, 0x5A));

        let sentinel = [0xCCCC_CCCC_CCCC_CCCC_u64; 16];
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x1F0);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!();
        };
        x86.xmm[0] = sentinel;
        x86.xmm[1] = first;
        let fault = execute_vpblendd(
            &[0xC4, 0xE3, 0x75, 0x02, 0x00, 0x5A],
            level,
            &mut context,
            &mut FlatMemory::new(0x200),
        );
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[0], sentinel, "{level:?}: fault atomicity");
    }
}
