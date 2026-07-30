//! AMD XOP `VPERMIL2PS`/`VPERMIL2PD` lifting and interpretation contracts.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::optimize::{OptLevel, optimize_function};

const INITIAL_FLAGS: u64 = 0x2 | 0x08D5;
const INITIAL_MXCSR: u32 = 0x9FC0;
const CR0_PE: u64 = 1;
const CR0_AM: u64 = 1 << 18;
const CR4_OSXSAVE: u64 = 1 << 18;
const RFLAGS_AC: u64 = 1 << 18;

fn vpermil2_encoding(
    opcode: u8,
    w: bool,
    l: bool,
    dst: u8,
    src1: u8,
    rm: u8,
    is4: u8,
    low: u8,
) -> [u8; 6] {
    assert!(matches!(opcode, 0x48 | 0x49));
    assert!(dst < 8 && src1 < 16 && rm < 8 && is4 < 16 && low < 16);
    [
        0xC4,
        0xE3,
        ((!src1 & 0x0F) << 3) | (u8::from(w) << 7) | (u8::from(l) << 2) | 1,
        opcode,
        0xC0 | (dst << 3) | rm,
        (is4 << 4) | low,
    ]
}

fn vpermil2_vec(reg: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(reg),
        VecWidth::V256 => X86Reg::Ymm(reg),
        _ => unreachable!("VPERMIL2 admits only 128-bit and 256-bit vectors"),
    }))
}

fn lift_at(pc: u64, bytes: &[u8]) -> Result<LiftResult, LiftError> {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    lifter.lift_insn(pc, bytes, &mut ctx)
}

fn enable_vpermil2(ctx: &mut SmirContext) {
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!();
    };
    x86.xop = true;
    x86.cr0 = CR0_PE;
    x86.cr4 = CR4_OSXSAVE;
    x86.xcr0 = 0b110;
    x86.cs_l = true;
}

fn execute_vpermil2(
    bytes: &[u8],
    level: OptLevel,
    ctx: &mut SmirContext,
    memory: &mut dyn SmirMemory,
) -> BlockResult {
    let lifted = lift_single(bytes).expect("strict VPERMIL2 lift");
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

fn lane(value: &[u64; 16], bits: u32, index: u8) -> u64 {
    match bits {
        32 => (value[usize::from(index / 2)] >> (u32::from(index & 1) * 32)) & 0xFFFF_FFFF,
        64 => value[usize::from(index)],
        _ => unreachable!(),
    }
}

fn set_lane(value: &mut [u64; 16], bits: u32, index: u8, lane_value: u64) {
    match bits {
        32 => {
            let word = &mut value[usize::from(index / 2)];
            let shift = u32::from(index & 1) * 32;
            *word = (*word & !(0xFFFF_FFFF_u64 << shift)) | ((lane_value & 0xFFFF_FFFF) << shift);
        }
        64 => value[usize::from(index)] = lane_value,
        _ => unreachable!(),
    }
}

fn vector_from_lanes(bits: u32, values: &[u64]) -> [u64; 16] {
    let mut result = [0_u64; 16];
    for (index, value) in values.iter().copied().enumerate() {
        set_lane(&mut result, bits, index as u8, value);
    }
    result
}

fn vpermil2_reference(
    src1: &[u64; 16],
    src2: &[u64; 16],
    selector: &[u64; 16],
    bits: u32,
    lanes: u8,
    m2z: u8,
) -> [u64; 16] {
    let block_lanes = (128 / bits) as u8;
    let mut result = [0_u64; 16];
    for output_lane in 0..lanes {
        let control = lane(selector, bits, output_lane);
        let selected = if bits == 32 {
            (control & 7) as u8
        } else {
            ((control >> 1) & 3) as u8
        };
        let block = (output_lane / block_lanes) * block_lanes;
        let selected_value = if selected < block_lanes {
            lane(src1, bits, block + selected)
        } else {
            lane(src2, bits, block + selected - block_lanes)
        };
        let m = control & 8 != 0;
        let zero = match m2z & 3 {
            0 | 1 => false,
            2 => m,
            3 => !m,
            _ => unreachable!(),
        };
        set_lane(
            &mut result,
            bits,
            output_lane,
            if zero { 0 } else { selected_value },
        );
    }
    result
}

fn tables(bits: u32) -> ([u64; 16], [u64; 16], [u64; 16]) {
    if bits == 32 {
        (
            vector_from_lanes(
                32,
                &[
                    0x7FC0_1234,
                    0xFF80_0001,
                    0x8000_0000,
                    0x0000_0001,
                    0x3F80_0000,
                    0xBF80_0000,
                    0x7F80_0000,
                    0xFFFF_FFFF,
                ],
            ),
            vector_from_lanes(
                32,
                &[
                    0x0123_4567,
                    0x89AB_CDEF,
                    0x1357_9BDF,
                    0x2468_ACE0,
                    0xAAAA_5555,
                    0x5555_AAAA,
                    0xDEAD_BEEF,
                    0xCAFE_BABE,
                ],
            ),
            vector_from_lanes(32, &[0x00, 0x09, 0x12, 0x1B, 0x24, 0x2D, 0x36, 0x3F]),
        )
    } else {
        (
            vector_from_lanes(
                64,
                &[
                    0x7FF8_0000_0000_1234,
                    0xFFF0_0000_0000_0001,
                    0x8000_0000_0000_0000,
                    0x0000_0000_0000_0001,
                ],
            ),
            vector_from_lanes(
                64,
                &[
                    0x0123_4567_89AB_CDEF,
                    0xFEDC_BA98_7654_3210,
                    0x1357_9BDF_2468_ACE0,
                    0xAAAA_5555_5555_AAAA,
                ],
            ),
            vector_from_lanes(64, &[0x00, 0x0B, 0x14, 0x1F]),
        )
    }
}

#[test]
fn vex_vpermil2_strictly_lifts_every_opcode_w_l_and_immediate_value() {
    for opcode in [0x48, 0x49] {
        let elem = if opcode == 0x48 {
            VecElementType::I32
        } else {
            VecElementType::I64
        };
        for w in [false, true] {
            for l in [false, true] {
                let width = if l { VecWidth::V256 } else { VecWidth::V128 };
                for imm in 0_u8..=u8::MAX {
                    let bytes = vpermil2_encoding(opcode, w, l, 1, 2, 4, imm >> 4, imm & 0x0F);
                    let lifted = lift_single(&bytes)
                        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
                    assert!(matches!(lifted.control_flow, ControlFlow::Fallthrough));
                    assert!(matches!(
                        lifted.ops.first().map(|op| &op.kind),
                        Some(OpKind::X86RequireXop)
                    ));
                    assert!(
                        lifted
                            .ops
                            .iter()
                            .all(|op| op.kind.flags_written().is_empty()),
                        "{bytes:02X?}"
                    );

                    let expected_src2 = vpermil2_vec(if w { imm >> 4 } else { 4 }, width);
                    let expected_selector = vpermil2_vec(if w { 4 } else { imm >> 4 }, width);
                    assert!(lifted.ops.iter().any(|op| matches!(
                        op.kind,
                        OpKind::VPermute {
                            src1,
                            src2: Some(src2),
                            elem: actual_elem,
                            width: actual_width,
                            overwrite_table: false,
                            ..
                        } if src1 == vpermil2_vec(2, width)
                            && src2 == expected_src2
                            && actual_elem == elem
                            && actual_width == width
                    )));
                    assert!(lifted.ops.iter().any(|op| match op.kind {
                        OpKind::VAnd {
                            src1,
                            width: actual_width,
                            ..
                        } if opcode == 0x48 => {
                            src1 == expected_selector && actual_width == width
                        }
                        OpKind::VShift {
                            src,
                            shift: ShiftOp::Lsr,
                            elem: VecElementType::I64,
                            amount: SrcOperand::Imm(1),
                            ..
                        } if opcode == 0x49 => src == expected_selector,
                        _ => false,
                    }));
                    assert!(matches!(
                        lifted.ops.last().map(|op| &op.kind),
                        Some(OpKind::VMov {
                            dst,
                            width: actual_width,
                            ..
                        }) if *dst == vpermil2_vec(1, width) && *actual_width == width
                    ));
                }
            }
        }
    }
}

#[test]
fn vex_vpermil2_extends_all_register_fields_and_swaps_only_rm_and_srs() {
    for (p1, expected_src2, expected_selector) in [(0x2D, 12, 11), (0xAD, 11, 12)] {
        // dest=ymm9 (VEX.R), src1=ymm10, r/m=ymm12 (VEX.B), SRS=ymm11.
        let bytes = [0xC4, 0x43, p1, 0x48, 0xCC, 0xB3];
        let lifted = lift_single(&bytes).expect("high-register VPERMIL2 form");
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VPermute {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(10))),
                src2: Some(src2),
                ..
            } if src2 == vpermil2_vec(expected_src2, VecWidth::V256)
        )));
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VAnd {
                src1,
                width: VecWidth::V256,
                ..
            } if src1 == vpermil2_vec(expected_selector, VecWidth::V256)
        )));
        assert!(matches!(
            lifted.ops.last().map(|op| &op.kind),
            Some(OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                ..
            })
        ));
    }
}

#[test]
fn vex_vpermil2_memory_uses_full_rip_and_w_selected_role() {
    let pc = 0x1_0000_2000_u64;
    for opcode in [0x48, 0x49] {
        for w in [false, true] {
            let p1 = 0x6D | (u8::from(w) << 7);
            let bytes = [0xC4, 0xE3, p1, opcode, 0x0D, 0x20, 0x00, 0x00, 0x00, 0x33];
            let lifted = lift_at(pc, &bytes).expect("RIP-relative VPERMIL2 memory form");
            assert_eq!(lifted.bytes_consumed, bytes.len());
            assert!(matches!(lifted.ops[0].kind, OpKind::X86RequireXop));
            let (loaded, addr) = lifted
                .ops
                .iter()
                .find_map(|op| match &op.kind {
                    OpKind::VLoad {
                        dst,
                        addr,
                        width: VecWidth::V256,
                    } => Some((*dst, addr)),
                    _ => None,
                })
                .expect("full-width VPERMIL2 memory load");
            let load_index = lifted
                .ops
                .iter()
                .position(|op| matches!(op.kind, OpKind::VLoad { .. }))
                .expect("VPERMIL2 load index");
            assert!(matches!(
                lifted.ops[load_index - 1].kind,
                OpKind::X86CheckAlignmentAc {
                    access_size: 32,
                    alignment: 16,
                    stack_segment: false,
                    natural_alignment: false,
                    ..
                }
            ));
            assert_eq!(
                lifted.ops[load_index].x86_hint,
                Some(X86OpHint::VecAlign(X86VecAlign::Aligned))
            );
            assert!(matches!(
                addr,
                Address::PcRel {
                    offset: 0x20,
                    base: Some(base),
                    ..
                } if *base == pc + bytes.len() as u64
            ));
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VPermute {
                    src2: Some(src2),
                    ..
                } if src2 == if w {
                    vpermil2_vec(3, VecWidth::V256)
                } else {
                    loaded
                }
            )));
            let expected_selector = if w {
                loaded
            } else {
                vpermil2_vec(3, VecWidth::V256)
            };
            assert!(lifted.ops.iter().any(|op| match op.kind {
                OpKind::VAnd { src1, .. } if opcode == 0x48 => {
                    src1 == expected_selector
                }
                OpKind::VShift {
                    src,
                    shift: ShiftOp::Lsr,
                    ..
                } if opcode == 0x49 => src == expected_selector,
                _ => false,
            }));
        }
    }

    let addr32 = lift_single(&[0x67, 0x64, 0xC4, 0xE3, 0x6D, 0x48, 0x4C, 0x91, 0x20, 0x30])
        .expect("address-size and FS-relative VPERMIL2 memory form");
    assert_eq!(addr32.bytes_consumed, 10);
    assert!(matches!(addr32.ops[0].kind, OpKind::X86RequireXop));
    assert!(addr32.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V256,
            ..
        }
    )));
    assert!(addr32.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86CheckAlignmentAc {
            access_size: 32,
            alignment: 16,
            stack_segment: false,
            natural_alignment: false,
            ..
        }
    )));
}

#[test]
fn vex_vpermil2_memory_alignment_guard_tracks_width_and_stack_segment_selection() {
    for (name, bytes, access_size, stack_segment) in [
        (
            "RBP default SS",
            &[0xC4, 0xE3, 0x69, 0x48, 0x4D, 0x00, 0x30][..],
            16,
            true,
        ),
        (
            "DS overrides RBP",
            &[0x3E, 0xC4, 0xE3, 0x69, 0x48, 0x4D, 0x00, 0x30][..],
            16,
            false,
        ),
        (
            "SS overrides RAX",
            &[0x36, 0xC4, 0xE3, 0x69, 0x48, 0x08, 0x30][..],
            16,
            true,
        ),
        (
            "FS overrides RBP",
            &[0x64, 0xC4, 0xE3, 0x6D, 0x48, 0x4D, 0x00, 0x30][..],
            32,
            false,
        ),
        (
            "RSP SIB default SS",
            &[0xC4, 0xE3, 0x6D, 0x49, 0x0C, 0x24, 0x30][..],
            32,
            true,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(matches!(lifted.ops[0].kind, OpKind::X86RequireXop));
        assert!(
            lifted
                .ops
                .iter()
                .enumerate()
                .all(|(index, op)| op.id == OpId(index as u16)),
            "{name}: operation IDs"
        );
        let load_index = lifted
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::VLoad { .. }))
            .unwrap_or_else(|| panic!("{name}: missing VLoad"));
        assert_eq!(
            lifted.ops[load_index].x86_hint,
            Some(X86OpHint::VecAlign(X86VecAlign::Aligned)),
            "{name}"
        );
        assert!(
            matches!(
                lifted.ops[load_index - 1].kind,
                OpKind::X86CheckAlignmentAc {
                    access_size: actual_size,
                    alignment: 16,
                    stack_segment: actual_stack_segment,
                    natural_alignment: false,
                    ..
                } if actual_size == access_size && actual_stack_segment == stack_segment
            ),
            "{name}"
        );
    }
}

#[test]
fn vex_vpermil2_feature_guard_precedes_semantics_and_rejects_without_commit() {
    let bytes = vpermil2_encoding(0x48, false, true, 1, 2, 3, 4, 3);
    let lifted = lift_single(&bytes).expect("register VPERMIL2");
    assert!(matches!(lifted.ops[0].kind, OpKind::X86RequireXop));
    assert!(
        lifted.ops[1..]
            .iter()
            .all(|op| !matches!(op.kind, OpKind::X86RequireXop))
    );

    for (name, configure) in [
        ("CPUID.XOP absent", 0_u8),
        ("CR0.PE clear", 1),
        ("CR0.TS set", 2),
        ("CR4.OSXSAVE clear", 3),
        ("XCR0.XMM clear", 4),
        ("XCR0.YMM clear", 5),
        ("CS.L clear", 6),
        ("VM set", 7),
    ] {
        let mut context = SmirContext::new_x86_64();
        enable_vpermil2(&mut context);
        let sentinel = [0xCCCC_CCCC_CCCC_CCCC_u64; 16];
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!();
        };
        x86.xmm[1] = sentinel;
        x86.xmm[2] = [0x1111_1111_1111_1111; 16];
        x86.xmm[3] = [0x2222_2222_2222_2222; 16];
        x86.xmm[4] = [0; 16];
        match configure {
            0 => x86.xop = false,
            1 => x86.cr0 &= !CR0_PE,
            2 => x86.cr0 |= 1 << 3,
            3 => x86.cr4 &= !CR4_OSXSAVE,
            4 => x86.xcr0 &= !(1 << 1),
            5 => x86.xcr0 &= !(1 << 2),
            6 => x86.cs_l = false,
            7 => x86.rflags |= crate::isa::x86_64::flags::bits::VM,
            _ => unreachable!(),
        }
        let exit = execute_vpermil2(&bytes, OptLevel::O2, &mut context, &mut FlatMemory::new(1));
        assert!(
            matches!(
                exit,
                BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
            ),
            "{name}: {exit:?}"
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[1], sentinel, "{name}");
    }
}

#[test]
fn vex_vpermil2_rejects_wrong_mandatory_prefix_and_reports_frontiers_exactly() {
    for pp in [0_u8, 2, 3] {
        let p1 = 0x68 | pp;
        assert!(matches!(
            lift_single(&[0xC4, 0xE3, p1, 0x48, 0xCC, 0x30]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }

    for (bytes, need) in [
        (&[0xC4, 0xE3, 0x69, 0x48][..], 5),
        (&[0xC4, 0xE3, 0x69, 0x48, 0xCC][..], 6),
        (&[0xC4, 0xE3, 0x69, 0x48, 0x04][..], 6),
        (&[0xC4, 0xE3, 0x69, 0x48, 0x04, 0x25][..], 10),
        (&[0xC4, 0xE3, 0x69, 0x48, 0x04, 0x25, 0, 0, 0, 0][..], 11),
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::Incomplete {
                addr: 0x1000,
                have,
                need: actual_need,
            }) if have == bytes.len() && actual_need == need
        ));
    }
}

#[test]
fn vex_vpermil2_interpretation_matches_block_local_raw_reference_at_all_levels() {
    for opcode in [0x48, 0x49] {
        let bits = if opcode == 0x48 { 32 } else { 64 };
        let (first, second, selector) = tables(bits);
        for w in [false, true] {
            for l in [false, true] {
                let lanes = if l { 256 / bits } else { 128 / bits } as u8;
                for m2z in 0..=3 {
                    let bytes = vpermil2_encoding(opcode, w, l, 1, 2, 4, 3, m2z);
                    let expected = vpermil2_reference(&first, &second, &selector, bits, lanes, m2z);
                    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                        let mut context = SmirContext::new_x86_64();
                        enable_vpermil2(&mut context);
                        context.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
                        context.flags.lazy = None;
                        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                            unreachable!();
                        };
                        x86.xmm[1] = [u64::MAX; 16];
                        x86.xmm[2] = first;
                        if w {
                            x86.xmm[3] = second;
                            x86.xmm[4] = selector;
                        } else {
                            x86.xmm[3] = selector;
                            x86.xmm[4] = second;
                        }
                        x86.mxcsr = INITIAL_MXCSR;
                        let exit =
                            execute_vpermil2(&bytes, level, &mut context, &mut FlatMemory::new(1));
                        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
                        let ArchRegState::X86_64(x86) = &context.arch_regs else {
                            unreachable!();
                        };
                        assert_eq!(x86.xmm[1], expected, "{bytes:02X?}, {level:?}, M2Z={m2z}");
                        assert_eq!(x86.mxcsr, INITIAL_MXCSR);
                        assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);
                    }
                }
            }
        }
    }
}

#[test]
fn vex_vpermil2_aliases_preserve_all_sources_until_destination_commit() {
    let register_bank = [
        vector_from_lanes(32, &[0, 1, 2, 3, 4, 5, 6, 7]),
        vector_from_lanes(32, &[8, 9, 10, 11, 12, 13, 14, 15]),
        vector_from_lanes(32, &[0x00, 0x09, 0x12, 0x1B, 0x24, 0x2D, 0x36, 0x3F]),
        vector_from_lanes(
            32,
            &[
                0xAAAA_0000,
                0xBBBB_0001,
                0xCCCC_0002,
                0xDDDD_0003,
                0xEEEE_0004,
                0xFFFF_0005,
                0x1111_0006,
                0x2222_0007,
            ],
        ),
    ];
    for (dst, src1, rm, is4) in [
        (2, 2, 4, 3),
        (4, 2, 4, 3),
        (3, 2, 4, 3),
        (1, 2, 2, 3),
        (1, 2, 4, 2),
        (2, 2, 2, 2),
    ] {
        for w in [false, true] {
            let bytes = vpermil2_encoding(0x48, w, true, dst, src1, rm, is4, 3);
            let first = register_bank[usize::from(src1 - 1)];
            let second_reg = if w { is4 } else { rm };
            let selector_reg = if w { rm } else { is4 };
            let second = register_bank[usize::from(second_reg - 1)];
            let selector = register_bank[usize::from(selector_reg - 1)];
            let expected = vpermil2_reference(&first, &second, &selector, 32, 8, 3);
            for level in [OptLevel::O0, OptLevel::O2] {
                let mut context = SmirContext::new_x86_64();
                enable_vpermil2(&mut context);
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!();
                };
                x86.xmm[1..=4].copy_from_slice(&register_bank);
                let exit = execute_vpermil2(&bytes, level, &mut context, &mut FlatMemory::new(1));
                assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
                let ArchRegState::X86_64(x86) = &context.arch_regs else {
                    unreachable!();
                };
                assert_eq!(
                    x86.xmm[usize::from(dst)],
                    expected,
                    "{bytes:02X?}, {level:?}"
                );
            }
        }
    }
}

#[test]
fn vex_vpermil2_unaligned_memory_is_exact_and_faults_before_destination_commit() {
    let (first, second, selector) = tables(32);
    for w in [false, true] {
        let bytes = [0xC4, 0xE3, 0x6D | (u8::from(w) << 7), 0x48, 0x08, 0x33];
        let memory_vector = if w { selector } else { second };
        let memory_bytes = memory_vector[..4]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        let expected = vpermil2_reference(&first, &second, &selector, 32, 8, 3);

        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut context = SmirContext::new_x86_64();
            enable_vpermil2(&mut context);
            context.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x181);
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!();
            };
            x86.xmm[2] = first;
            x86.xmm[3] = if w { second } else { selector };
            let mut memory = FlatMemory::new(0x400);
            memory.write(0x181, &memory_bytes).unwrap();
            let exit = execute_vpermil2(&bytes, level, &mut context, &mut memory);
            assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                unreachable!();
            };
            assert_eq!(x86.xmm[1], expected, "W={}, {level:?}", u8::from(w));

            let sentinel = [0xCCCC_CCCC_CCCC_CCCC_u64; 16];
            let mut context = SmirContext::new_x86_64();
            enable_vpermil2(&mut context);
            context.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x1F0);
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!();
            };
            x86.xmm[1] = sentinel;
            x86.xmm[2] = first;
            x86.xmm[3] = if w { second } else { selector };
            let fault = execute_vpermil2(&bytes, level, &mut context, &mut FlatMemory::new(0x200));
            assert!(matches!(
                fault,
                BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
            ));
            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                unreachable!();
            };
            assert_eq!(x86.xmm[1], sentinel, "W={}, {level:?}", u8::from(w));

            let mut context = SmirContext::new_x86_64();
            enable_vpermil2(&mut context);
            context.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x181);
            context.flags.materialized.ac = true;
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!();
            };
            x86.cr0 |= CR0_AM;
            x86.cpl = 3;
            x86.rflags |= RFLAGS_AC;
            x86.xmm[1] = sentinel;
            x86.xmm[2] = first;
            x86.xmm[3] = if w { second } else { selector };
            let mut memory = FlatMemory::new(0x400);
            memory.write(0x181, &memory_bytes).unwrap();
            let fault = execute_vpermil2(&bytes, level, &mut context, &mut memory);
            assert!(matches!(
                fault,
                BlockResult::Exit(ExitReason::AlignmentCheck { addr: 0x1000 })
            ));
            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                unreachable!();
            };
            assert_eq!(x86.xmm[1], sentinel, "W={}, {level:?}", u8::from(w));
        }
    }
}
