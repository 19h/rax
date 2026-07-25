//! Exact MXCSR and fail-closed tests for SSE3/AVX FP add-sub/horizontal IR.

use super::*;

fn packed_f32(bits: [u32; 4], upper: u64) -> VecValue {
    let mut vector = [upper; 16];
    for (lane, bits) in bits.into_iter().enumerate() {
        SmirInterpreter::set_lane(&mut vector, lane as u8, 32, u64::from(bits));
    }
    vector
}

#[test]
fn lifted_haddps_accrues_denormal_and_precision_status_and_preserves_legacy_upper_state() {
    let upper = 0xA55A_3CC3_F00F_9669;
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        // Pair 0: 0.5 + min-subnormal rounds to 0.5 and reports DE|PE.
        // Pair 1: min-normal - min-subnormal is the exact largest subnormal.
        x86.xmm[1] = packed_f32([0x3F00_0000, 0x0000_0001, 0x8000_0001, 0x0080_0000], upper);
        x86.xmm[3] = packed_f32([0x3EAA_AAAB, 0x0000_0000, 0x8000_0000, 0x3F80_0000], 0);
        x86.mxcsr = 0x1F80;
    }

    let exit = execute_lifted_x86(
        &[0xF2, 0x0F, 0x7C, 0xCB],
        &mut context,
        &mut FlatMemory::new(1),
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    for (lane, expected) in [0x3F00_0000u64, 0x007F_FFFF, 0x3EAA_AAAB, 0x3F80_0000]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            SmirInterpreter::get_lane(&x86.xmm[1], lane as u8, 32),
            expected,
            "lane {lane}"
        );
    }
    assert!(x86.xmm[1][2..].iter().all(|word| *word == upper));
    assert_eq!(x86.mxcsr, 0x1F80 | (1 << 1) | (1 << 5));
}

#[test]
fn lifted_haddps_unmasked_precision_traps_before_any_destination_write() {
    let mut context = SmirContext::new_x86_64();
    let before = packed_f32(
        [0x3F80_0000, 0x3380_0000, 0x0000_0000, 0x0000_0000],
        0x0123_4567_89AB_CDEF,
    );
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.xmm[0] = before;
        x86.xmm[1] = packed_f32([0; 4], 0);
        x86.mxcsr = 0x1F80 & !(1 << 12);
    }

    let exit = execute_lifted_x86(
        &[0xF2, 0x0F, 0x7C, 0xC1],
        &mut context,
        &mut FlatMemory::new(1),
    );
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.xmm[0], before);
    assert_eq!(x86.mxcsr, (0x1F80 & !(1 << 12)) | (1 << 5));
}

#[test]
fn lifted_haddps_reports_unmasked_precomputation_before_postcomputation_status() {
    let run = |mxcsr: u32| {
        let mut context = SmirContext::new_x86_64();
        let before = packed_f32(
            [
                f32::INFINITY.to_bits(),
                f32::NEG_INFINITY.to_bits(),
                0x7F7F_FFFF,
                0x7F7F_FFFF,
            ],
            0x0123_4567_89AB_CDEF,
        );
        if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
            x86.xmm[0] = before;
            x86.xmm[1] = packed_f32([0; 4], 0);
            x86.mxcsr = mxcsr;
        }

        let exit = execute_lifted_x86(
            &[0xF2, 0x0F, 0x7C, 0xC1],
            &mut context,
            &mut FlatMemory::new(1),
        );
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.xmm[0], before);
        x86.mxcsr & 0x3F
    };

    let invalid_and_overflow_unmasked = 0x1F80 & !(1 << 7) & !(1 << 10);
    assert_eq!(run(invalid_and_overflow_unmasked), 1 << 0);

    let overflow_unmasked = 0x1F80 & !(1 << 10);
    assert_eq!(run(overflow_unmasked), (1 << 0) | (1 << 3) | (1 << 5));
}

#[test]
fn paired_fp_ir_rejects_nonexistent_masks_widths_and_embedded_controls() {
    use crate::smir::optimize::{OptLevel, optimize_function};

    let destination = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let source1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    let source2 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)));
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(1)));

    for operation in [
        X86FpBinaryOp::AddSub,
        X86FpBinaryOp::HorizontalAdd,
        X86FpBinaryOp::HorizontalSub,
    ] {
        for (active_mask, lanes, round, suppress_exceptions) in [
            (Some(mask), 4, FpRoundMode::Dynamic, false),
            (None, 16, FpRoundMode::Dynamic, false),
            (None, 4, FpRoundMode::RoundNearest, true),
        ] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            let kind = OpKind::X86FpBinary {
                dst: destination,
                src1: source1,
                src2: source2,
                mask: active_mask,
                elem: VecElementType::F32,
                lanes,
                op: operation,
                round,
                suppress_exceptions,
            };
            assert!(kind.has_side_effects(), "malformed IR must survive DCE");
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Trap {
                kind: TrapKind::Halt,
            });
            let mut function = builder.finish();
            optimize_function(&mut function, OptLevel::O2);
            assert!(
                function.blocks[0]
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::X86FpBinary { .. })),
                "malformed IR must reach the fail-closed interpreter boundary"
            );
            let sentinel = [0xA55A_3CC3_F00F_9669; 16];
            let mut context = SmirContext::new_x86_64();
            if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
                x86.xmm[0] = sentinel;
                x86.mxcsr = 0x1F80;
            }
            let exit = SmirInterpreter::new().execute_block(
                &mut context,
                &mut FlatMemory::new(1),
                &function.blocks[0],
            );
            assert!(matches!(
                exit,
                BlockResult::Exit(ExitReason::Undefined { .. })
            ));
            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                unreachable!()
            };
            assert_eq!(x86.xmm[0], sentinel);
            assert_eq!(x86.mxcsr, 0x1F80);
        }
    }
}
