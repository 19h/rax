//! Exhaustive strict-lift, canonical-interpreter, optimizer, and native-gate
//! coverage for the six legacy MMX/SSE packed conversion forms.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::optimize::{OptLevel, optimize_function};
use std::collections::HashMap;

const INITIAL_FLAGS: u64 = 0x2 | 0x08D5;
const MXCSR_DEFAULT: u32 = 0x1F80;
const MXCSR_IE: u32 = 1 << 0;
const MXCSR_PE: u32 = 1 << 5;
const MXCSR_DAZ: u32 = 1 << 6;
const MXCSR_IM: u32 = 1 << 7;
const MXCSR_PM: u32 = 1 << 12;

fn lift_nonstrict(bytes: &[u8]) -> Result<LiftResult, LiftError> {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    lifter.lift_insn(0x1000, bytes, &mut ctx)
}

fn conversion(result: &LiftResult) -> &SmirOp {
    result
        .ops
        .iter()
        .find(|op| {
            matches!(
                op.kind,
                OpKind::X86PackedIntToFp { .. } | OpKind::X86PackedFpToInt { .. }
            )
        })
        .expect("one MMX/SSE conversion operation")
}

fn enter_mmx_count(result: &LiftResult) -> usize {
    result
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op.kind,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                }
            )
        })
        .count()
}

fn execute(
    bytes: &[u8],
    level: OptLevel,
    ctx: &mut SmirContext,
    memory: &mut dyn SmirMemory,
) -> BlockResult {
    let lifted = lift_single(bytes).expect("strict MMX/SSE conversion lift");
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

fn context() -> SmirContext {
    let mut ctx = SmirContext::new_x86_64();
    ctx.pc = 0x1000;
    ctx.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
    ctx.flags.lazy = None;
    ctx
}

fn pack_i32(low: i32, high: i32) -> u64 {
    u64::from(low as u32) | (u64::from(high as u32) << 32)
}

fn pack_f32(low: u32, high: u32) -> u64 {
    u64::from(low) | (u64::from(high) << 32)
}

fn unpack_i32(value: u64) -> [i32; 2] {
    [value as u32 as i32, (value >> 32) as u32 as i32]
}

fn set_f32_source(ctx: &mut SmirContext, index: usize, low: u32, high: u32) {
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!();
    };
    x86.xmm[index][0] = pack_f32(low, high);
}

fn set_f64_source(ctx: &mut SmirContext, index: usize, low: u64, high: u64) {
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!();
    };
    x86.xmm[index][0] = low;
    x86.xmm[index][1] = high;
}

#[test]
fn every_modrm_rex_and_mandatory_prefix_shape_strictly_lifts() {
    // 2 prefix classes * 3 opcodes * 17 REX choices * 256 ModR/M values =
    // 26,112 complete shape probes. Eight trailing zero bytes satisfy every
    // SIB/displacement form and are not part of the decoded instruction.
    let mut probes = 0usize;
    for operand_size in [false, true] {
        for opcode in [0x2A, 0x2C, 0x2D] {
            for rex in std::iter::once(None).chain((0x40..=0x4F).map(Some)) {
                for modrm in 0u8..=u8::MAX {
                    let mut bytes = Vec::with_capacity(14);
                    if operand_size {
                        bytes.push(0x66);
                    }
                    if let Some(rex) = rex {
                        bytes.push(rex);
                    }
                    bytes.extend_from_slice(&[0x0F, opcode, modrm]);
                    bytes.extend_from_slice(&[0; 8]);
                    let lifted = lift_single(&bytes).unwrap_or_else(|error| {
                        panic!("valid MMX conversion {bytes:02X?}: {error:?}")
                    });
                    assert!(matches!(lifted.control_flow, ControlFlow::Fallthrough));
                    let op = conversion(&lifted);
                    let expected_prefix = if operand_size {
                        X86SsePrefix::OpSize
                    } else {
                        X86SsePrefix::None
                    };
                    assert_eq!(
                        op.x86_hint,
                        Some(X86OpHint::SseOp {
                            prefix: expected_prefix,
                            opcode,
                        }),
                        "{bytes:02X?}",
                    );
                    probes += 1;
                }
            }
        }
    }
    assert_eq!(probes, 2 * 3 * 17 * 256);
}

#[test]
fn exact_operands_widths_rex_extensions_and_mmx_transitions_are_lifted() {
    let cases: &[(&[u8], VecElementType, bool, bool)] = &[
        (&[0x0F, 0x2A, 0xC1], VecElementType::F32, true, true),
        (&[0x66, 0x0F, 0x2A, 0xC1], VecElementType::F64, true, true),
        (&[0x0F, 0x2C, 0xC1], VecElementType::F32, false, true),
        (&[0x66, 0x0F, 0x2C, 0xC1], VecElementType::F64, false, true),
        (&[0x0F, 0x2D, 0xC1], VecElementType::F32, false, false),
        (&[0x66, 0x0F, 0x2D, 0xC1], VecElementType::F64, false, false),
    ];
    for &(bytes, fp_elem, int_to_fp, truncate) in cases {
        let strict = lift_single(bytes).unwrap();
        let nonstrict = lift_nonstrict(bytes).unwrap();
        assert_eq!(strict.bytes_consumed, bytes.len());
        assert_eq!(nonstrict.bytes_consumed, bytes.len());
        assert_eq!(enter_mmx_count(&strict), 1);
        match &conversion(&strict).kind {
            OpKind::X86PackedIntToFp {
                dst,
                src,
                int_elem,
                fp_elem: actual_fp,
                signed,
                lanes,
                src_width,
                dst_width,
                zero_upper,
                round,
                suppress_exceptions,
                ..
            } if int_to_fp => {
                assert_eq!(*dst, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))));
                assert_eq!(*src, VReg::Arch(ArchReg::X86(X86Reg::Mm(1))));
                assert_eq!(*int_elem, VecElementType::I32);
                assert_eq!(*actual_fp, fp_elem);
                assert!(*signed);
                assert_eq!(*lanes, 2);
                assert_eq!(*src_width, VecWidth::V64);
                assert_eq!(
                    *dst_width,
                    if fp_elem == VecElementType::F32 {
                        VecWidth::V64
                    } else {
                        VecWidth::V128
                    }
                );
                assert!(!*zero_upper);
                assert_eq!(*round, FpRoundMode::Dynamic);
                assert!(!*suppress_exceptions);
            }
            OpKind::X86PackedFpToInt {
                dst,
                src,
                fp_elem: actual_fp,
                int_elem,
                signed,
                truncate: actual_truncate,
                lanes,
                src_width,
                dst_width,
                zero_upper,
                round,
                suppress_exceptions,
                ..
            } if !int_to_fp => {
                assert_eq!(*dst, VReg::Arch(ArchReg::X86(X86Reg::Mm(0))));
                assert_eq!(*src, VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))));
                assert_eq!(*actual_fp, fp_elem);
                assert_eq!(*int_elem, VecElementType::I32);
                assert!(*signed);
                assert_eq!(*actual_truncate, truncate);
                assert_eq!(*lanes, 2);
                assert_eq!(
                    *src_width,
                    if fp_elem == VecElementType::F32 {
                        VecWidth::V64
                    } else {
                        VecWidth::V128
                    }
                );
                assert_eq!(*dst_width, VecWidth::V64);
                assert!(!*zero_upper);
                assert_eq!(
                    *round,
                    if truncate {
                        FpRoundMode::RoundTowardZero
                    } else {
                        FpRoundMode::Dynamic
                    }
                );
                assert!(!*suppress_exceptions);
            }
            other => panic!("unexpected conversion for {bytes:02X?}: {other:?}"),
        }
        let op = conversion(&strict);
        assert!(op.kind.flags_written().is_empty());
        assert!(
            op.kind.has_side_effects(),
            "MXCSR status/trap is observable"
        );
        assert!(!op.is_jit_safe(), "unsuppressed #XM must fail closed");
    }

    // Independently assembled by LLVM MC. REX extends only the XMM operand;
    // the architectural MMX namespace remains MM0-MM7.
    let int_to_fp = lift_single(&[0x44, 0x0F, 0x2A, 0xC7]).unwrap();
    assert!(matches!(
        conversion(&int_to_fp).kind,
        OpKind::X86PackedIntToFp {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(8))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Mm(7))),
            ..
        }
    ));
    let ignored_b = lift_single(&[0x41, 0x0F, 0x2A, 0xC7]).unwrap();
    assert!(matches!(
        conversion(&ignored_b).kind,
        OpKind::X86PackedIntToFp {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Mm(7))),
            ..
        }
    ));
    let fp_to_int = lift_single(&[0x41, 0x0F, 0x2D, 0xF8]).unwrap();
    assert!(matches!(
        conversion(&fp_to_int).kind,
        OpKind::X86PackedFpToInt {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(7))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(8))),
            ..
        }
    ));
    let ignored_r = lift_single(&[0x44, 0x0F, 0x2D, 0xF8]).unwrap();
    assert!(matches!(
        conversion(&ignored_r).kind,
        OpKind::X86PackedFpToInt {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(7))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            ..
        }
    ));
}

#[test]
fn memory_width_alignment_addressing_and_state_transition_asymmetry_are_exact() {
    for (bytes, width, aligned, enters_mmx) in [
        (&[0x0F, 0x2A, 0x00][..], VecWidth::V64, false, true),
        (&[0x66, 0x0F, 0x2A, 0x00][..], VecWidth::V64, false, false),
        (&[0x0F, 0x2C, 0x00][..], VecWidth::V64, false, true),
        (&[0x66, 0x0F, 0x2C, 0x00][..], VecWidth::V128, true, true),
        (&[0x0F, 0x2D, 0x00][..], VecWidth::V64, false, true),
        (&[0x66, 0x0F, 0x2D, 0x00][..], VecWidth::V128, true, true),
    ] {
        let result = lift_single(bytes).unwrap();
        let load_index = result
            .ops
            .iter()
            .position(
                |op| matches!(op.kind, OpKind::VLoad { width: actual, .. } if actual == width),
            )
            .expect("exact memory width");
        let conversion_index = result
            .ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::X86PackedIntToFp { .. } | OpKind::X86PackedFpToInt { .. }
                )
            })
            .unwrap();
        assert!(load_index < conversion_index);
        let alignment_index = result
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }));
        assert_eq!(alignment_index.is_some(), aligned, "{bytes:02X?}");
        if let Some(index) = alignment_index {
            assert!(index < load_index);
        }
        assert_eq!(enter_mmx_count(&result), usize::from(enters_mmx));
        if enters_mmx {
            assert!(
                result
                    .ops
                    .iter()
                    .position(|op| matches!(
                        op.kind,
                        OpKind::X86X87Control {
                            kind: X86X87ControlKind::EnterMmx,
                            ..
                        }
                    ))
                    .unwrap()
                    > conversion_index
            );
        }
    }

    // FS:[addr32 EAX + ESI*2 + 1] retains both address truncation and the
    // segment-base addition before the aligned packed-double access.
    let fs_addr32 = lift_single(&[0x64, 0x67, 0x66, 0x0F, 0x2D, 0x44, 0x70, 0x01]).unwrap();
    assert!(fs_addr32.ops.iter().any(|op| matches!(
        &op.kind,
        OpKind::VLoad {
            addr: Address::X86Addr32(inner),
            width: VecWidth::V128,
            ..
        } if matches!(inner.as_ref(), Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
            ..
        })
    )));
}

#[test]
fn reserved_prefixes_fail_and_scalar_f2_f3_cells_remain_disjoint() {
    for opcode in [0x2A, 0x2C, 0x2D] {
        for operand_size in [false, true] {
            let mut lock = vec![0xF0];
            if operand_size {
                lock.push(0x66);
            }
            lock.extend_from_slice(&[0x0F, opcode, 0xC1]);
            assert!(matches!(
                lift_single(&lock),
                Err(LiftError::InvalidEncoding { .. })
            ));

            let mut rex2 = Vec::new();
            if operand_size {
                rex2.push(0x66);
            }
            rex2.extend_from_slice(&[0xD5, 0x80, opcode, 0xC1]);
            assert!(matches!(
                lift_single(&rex2),
                Err(LiftError::InvalidEncoding { .. })
            ));
        }
    }

    for bytes in [&[0xF3, 0x0F, 0x2A, 0xC1][..], &[0xF2, 0x0F, 0x2A, 0xC1][..]] {
        let result = lift_single(bytes).unwrap();
        assert!(
            result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86IntToFp { .. }))
        );
        assert_eq!(enter_mmx_count(&result), 0);
    }
    for bytes in [
        &[0xF3, 0x0F, 0x2C, 0xC1][..],
        &[0xF2, 0x0F, 0x2C, 0xC1][..],
        &[0xF3, 0x0F, 0x2D, 0xC1][..],
        &[0xF2, 0x0F, 0x2D, 0xC1][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(
            result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86FpToInt { .. }))
        );
        assert_eq!(enter_mmx_count(&result), 0);
    }

    for bytes in [&[0x0F, 0x2A][..], &[0x66, 0x0F, 0x2D][..]] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::Incomplete { .. })
        ));
    }
}

#[test]
fn int_to_fp_rounding_widths_upper_state_and_exceptions_match_mxcsr() {
    let rounding = [
        (0u32, [16_777_216.0f32, -16_777_216.0f32]),
        (1, [16_777_216.0, -16_777_218.0]),
        (2, [16_777_218.0, -16_777_216.0]),
        (3, [16_777_216.0, -16_777_216.0]),
    ];
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for (rc, expected) in rounding {
            let mut ctx = context();
            let upper = 0xA5A5_5A5A_F0F0_0F0F;
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!();
            };
            x86.mm[1] = pack_i32(16_777_217, -16_777_217);
            x86.xmm[0] = [upper; 16];
            x86.x87.tag_word = 0xFFFF;
            x86.mxcsr = (MXCSR_DEFAULT & !(3 << 13)) | (rc << 13);
            assert!(matches!(
                execute(
                    &[0x0F, 0x2A, 0xC1],
                    level,
                    &mut ctx,
                    &mut FlatMemory::new(1)
                ),
                BlockResult::Exit(ExitReason::Halt)
            ));
            let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                unreachable!();
            };
            assert_eq!(
                x86.xmm[0][0],
                pack_f32(expected[0].to_bits(), expected[1].to_bits()),
                "{level:?}, RC={rc}",
            );
            assert!(x86.xmm[0][1..].iter().all(|word| *word == upper));
            assert_eq!(x86.mxcsr & MXCSR_PE, MXCSR_PE);
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(ctx.flags.materialized.to_rflags(), INITIAL_FLAGS);
        }

        let mut ctx = context();
        let upper = 0xC3C3_3C3C_A5A5_5A5A;
        let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
            unreachable!();
        };
        x86.mm[1] = pack_i32(i32::MIN, i32::MAX);
        x86.xmm[0] = [upper; 16];
        assert!(matches!(
            execute(
                &[0x66, 0x0F, 0x2A, 0xC1],
                level,
                &mut ctx,
                &mut FlatMemory::new(1)
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[0][0], (-2_147_483_648.0f64).to_bits());
        assert_eq!(x86.xmm[0][1], 2_147_483_647.0f64.to_bits());
        assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        assert_eq!(x86.mxcsr & 0x3F, 0, "I32-to-F64 is exact");
    }

    // Unmasked precision commits MXCSR.PE but neither the destination nor the
    // following MMX-state transition.
    let mut ctx = context();
    let destination = [0x1122_3344_5566_7788; 16];
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!();
    };
    x86.mm[1] = pack_i32(16_777_217, -16_777_217);
    x86.xmm[0] = destination;
    x86.x87.tag_word = 0x1357;
    x86.mxcsr = MXCSR_DEFAULT & !MXCSR_PM;
    let exit = execute(
        &[0x0F, 0x2A, 0xC1],
        OptLevel::O2,
        &mut ctx,
        &mut FlatMemory::new(1),
    );
    assert!(
        matches!(
            exit,
            BlockResult::Exit(ExitReason::SimdFloatingPoint { addr: 0x1000 })
        ),
        "unexpected unmasked CVTPI2PS exit: {exit:?}"
    );
    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
        unreachable!();
    };
    assert_eq!(x86.xmm[0], destination);
    assert_eq!(x86.x87.tag_word, 0x1357);
    assert_eq!(x86.mxcsr & MXCSR_PE, MXCSR_PE);
}

#[test]
fn fp_to_int_rounding_truncation_special_values_daz_and_atomic_exceptions_are_exact() {
    let rounded = [(0u32, [2, -2]), (1, [1, -3]), (2, [2, -2]), (3, [1, -2])];
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for fp64 in [false, true] {
            for (rc, expected) in rounded {
                let mut ctx = context();
                if fp64 {
                    set_f64_source(&mut ctx, 1, 1.5f64.to_bits(), (-2.5f64).to_bits());
                } else {
                    set_f32_source(&mut ctx, 1, 1.5f32.to_bits(), (-2.5f32).to_bits());
                }
                let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                    unreachable!();
                };
                x86.mxcsr = (MXCSR_DEFAULT & !(3 << 13)) | (rc << 13);
                x86.mm[0] = u64::MAX;
                let bytes = if fp64 {
                    &[0x66, 0x0F, 0x2D, 0xC1][..]
                } else {
                    &[0x0F, 0x2D, 0xC1][..]
                };
                assert!(matches!(
                    execute(bytes, level, &mut ctx, &mut FlatMemory::new(1)),
                    BlockResult::Exit(ExitReason::Halt)
                ));
                let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                    unreachable!();
                };
                assert_eq!(unpack_i32(x86.mm[0]), expected, "{level:?}, RC={rc}");
                assert_eq!(x86.mxcsr & MXCSR_PE, MXCSR_PE);
                assert_eq!(x86.x87.tag_word, 0);
            }

            for rc in 0..=3u32 {
                let mut ctx = context();
                if fp64 {
                    set_f64_source(&mut ctx, 1, 1.5f64.to_bits(), (-2.5f64).to_bits());
                } else {
                    set_f32_source(&mut ctx, 1, 1.5f32.to_bits(), (-2.5f32).to_bits());
                }
                let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                    unreachable!();
                };
                x86.mxcsr = (MXCSR_DEFAULT & !(3 << 13)) | (rc << 13);
                let bytes = if fp64 {
                    &[0x66, 0x0F, 0x2C, 0xC1][..]
                } else {
                    &[0x0F, 0x2C, 0xC1][..]
                };
                execute(bytes, level, &mut ctx, &mut FlatMemory::new(1));
                let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                    unreachable!();
                };
                assert_eq!(unpack_i32(x86.mm[0]), [1, -2], "truncate RC={rc}");
            }
        }
    }

    for (fp64, quiet_nan, signaling_nan) in [
        (false, u64::from(0x7FC0_1234u32), u64::from(0x7F80_1234u32)),
        (true, 0x7FF8_0000_0000_1234, 0x7FF0_0000_0000_1234),
    ] {
        let mut ctx = context();
        if fp64 {
            set_f64_source(&mut ctx, 1, quiet_nan, signaling_nan);
        } else {
            set_f32_source(&mut ctx, 1, quiet_nan as u32, signaling_nan as u32);
        }
        let bytes = if fp64 {
            &[0x66, 0x0F, 0x2D, 0xC1][..]
        } else {
            &[0x0F, 0x2D, 0xC1][..]
        };
        execute(bytes, OptLevel::O2, &mut ctx, &mut FlatMemory::new(1));
        let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.mm[0], 0x8000_0000_8000_0000);
        assert_eq!(x86.mxcsr & MXCSR_IE, MXCSR_IE);
    }

    // DAZ substitutes signed zero before conversion. Without DAZ, the same
    // non-zero subnormal rounds to zero and reports precision.
    for daz in [false, true] {
        let mut ctx = context();
        set_f32_source(&mut ctx, 1, 0x0000_0001, 0x8000_0001);
        let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
            unreachable!();
        };
        x86.mxcsr = MXCSR_DEFAULT | if daz { MXCSR_DAZ } else { 0 };
        execute(
            &[0x0F, 0x2D, 0xC1],
            OptLevel::O2,
            &mut ctx,
            &mut FlatMemory::new(1),
        );
        let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.mm[0], 0);
        assert_eq!(x86.mxcsr & MXCSR_PE != 0, !daz);
    }

    for (unmasked, source, expected_status) in [
        (
            MXCSR_PM,
            pack_f32(1.5f32.to_bits(), (-2.5f32).to_bits()),
            MXCSR_PE,
        ),
        (
            MXCSR_IM,
            pack_f32(f32::INFINITY.to_bits(), f32::NEG_INFINITY.to_bits()),
            MXCSR_IE,
        ),
    ] {
        let mut ctx = context();
        let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
            unreachable!();
        };
        x86.xmm[1][0] = source;
        x86.mm[0] = 0x0123_4567_89AB_CDEF;
        x86.x87.tag_word = 0x2468;
        x86.mxcsr = MXCSR_DEFAULT & !unmasked;
        let exit = execute(
            &[0x0F, 0x2D, 0xC1],
            OptLevel::O2,
            &mut ctx,
            &mut FlatMemory::new(1),
        );
        assert!(
            matches!(
                exit,
                BlockResult::Exit(ExitReason::SimdFloatingPoint { addr: 0x1000 })
            ),
            "unexpected unmasked CVTPS2PI exit: {exit:?}"
        );
        let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.mm[0], 0x0123_4567_89AB_CDEF);
        assert_eq!(x86.x87.tag_word, 0x2468);
        assert_eq!(x86.mxcsr & expected_status, expected_status);
    }
}

#[test]
fn memory_forms_enforce_alignment_fault_atomicity_and_cvtpi2pd_no_mmx_transition() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut memory = FlatMemory::new(0x240);
    memory
        .write(0x81, &pack_i32(i32::MIN, i32::MAX).to_le_bytes())
        .unwrap();

    let mut ctx = context();
    ctx.write_vreg(rax, 0x81);
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!();
    };
    x86.x87.tag_word = 0x1357;
    x86.xmm[0] = [u64::MAX; 16];
    assert!(matches!(
        execute(
            &[0x66, 0x0F, 0x2A, 0x00],
            OptLevel::O2,
            &mut ctx,
            &mut memory
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
        unreachable!();
    };
    assert_eq!(x86.xmm[0][0], (-2_147_483_648.0f64).to_bits());
    assert_eq!(x86.xmm[0][1], 2_147_483_647.0f64.to_bits());
    assert_eq!(x86.x87.tag_word, 0x1357, "m64 CVTPI2PD does not enter MMX");

    let mut ctx = context();
    ctx.write_vreg(rax, 0x81);
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!();
    };
    x86.x87.tag_word = 0xFFFF;
    execute(&[0x0F, 0x2A, 0x00], OptLevel::O2, &mut ctx, &mut memory);
    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
        unreachable!();
    };
    assert_eq!(x86.x87.tag_word, 0, "m64 CVTPI2PS enters MMX");

    memory.write(0x80, &1.5f64.to_bits().to_le_bytes()).unwrap();
    memory
        .write(0x88, &(-2.5f64).to_bits().to_le_bytes())
        .unwrap();
    for (addr, expected) in [(0x80, Some([2, -2])), (0x81, None)] {
        let mut ctx = context();
        ctx.write_vreg(rax, addr);
        let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
            unreachable!();
        };
        x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
        x86.x87.tag_word = 0x369C;
        let exit = execute(
            &[0x66, 0x0F, 0x2D, 0x00],
            OptLevel::O2,
            &mut ctx,
            &mut memory,
        );
        let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
            unreachable!();
        };
        if let Some(expected) = expected {
            assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
            assert_eq!(unpack_i32(x86.mm[0]), expected);
            assert_eq!(x86.x87.tag_word, 0);
        } else {
            assert!(matches!(
                exit,
                BlockResult::Exit(ExitReason::GeneralProtection {
                    addr: 0x1000,
                    error_code: 0,
                })
            ));
            assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(x86.x87.tag_word, 0x369C);
        }
    }

    // A mapped-but-short/out-of-bounds source faults before conversion and
    // before the explicit MMX-state commit.
    let mut ctx = context();
    ctx.write_vreg(rax, 0x240);
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!();
    };
    x86.mm[0] = 0xABCD_EF01_2345_6789;
    x86.x87.tag_word = 0xFFFF;
    assert!(matches!(
        execute(
            &[0x66, 0x0F, 0x2C, 0x00],
            OptLevel::O2,
            &mut ctx,
            &mut memory
        ),
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: 0x240,
            write: false,
        })
    ));
    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
        unreachable!();
    };
    assert_eq!(x86.mm[0], 0xABCD_EF01_2345_6789);
    assert_eq!(x86.x87.tag_word, 0xFFFF);
}

#[test]
fn mixed_mmx_xmm_conversion_regions_remain_fail_closed_for_native_jit() {
    for bytes in [
        &[0x0F, 0x2A, 0xC1][..],
        &[0x66, 0x0F, 0x2A, 0xC1][..],
        &[0x0F, 0x2C, 0xC1][..],
        &[0x66, 0x0F, 0x2C, 0xC1][..],
        &[0x0F, 0x2D, 0xC1][..],
        &[0x66, 0x0F, 0x2D, 0xC1][..],
    ] {
        let lifted = lift_single(bytes).unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.set_terminator(Terminator::Branch { target: BlockId(0) });
        let mut function = builder.finish();
        function.blocks[0].ops = lifted.ops;
        assert!(
            !crate::smir::lower::runtime::is_native_clobber_safe_excluding(
                &function,
                &HashMap::new(),
                true,
            ),
            "mixed MMX/XMM conversion admitted before exact native lowering: {bytes:02X?}",
        );
    }
}
