//! Precise scalar x86 floating-point precision-conversion state tests.

use super::*;

fn mxcsr(ctx: &SmirContext) -> u32 {
    match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.mxcsr,
        _ => unreachable!(),
    }
}

fn xmm(ctx: &SmirContext, register: usize) -> [u64; 16] {
    match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.xmm[register],
        _ => unreachable!(),
    }
}

#[test]
fn lifted_scalar_fp_convert_commits_masked_status_and_exact_lane_state() {
    let mut memory = FlatMemory::new(1);
    for (name, bytes, source, width, expected, expected_status) in [
        (
            "binary64 to binary32 precision",
            [0x62, 0xF1, 0xEF, 0x08, 0x5A, 0xCB],
            (1.0f64 + 2.0f64.powi(-24)).to_bits(),
            64,
            u64::from(1.0f32.to_bits()),
            1u32 << 5,
        ),
        (
            "binary32 to binary16 overflow",
            [0x62, 0xF5, 0x6C, 0x08, 0x1D, 0xCB],
            u64::from(f32::MAX.to_bits()),
            32,
            0x7C00,
            (1u32 << 3) | (1u32 << 5),
        ),
        (
            "binary16 to binary64 denormal",
            [0x62, 0xF5, 0x6E, 0x08, 0x5A, 0xCB],
            1,
            16,
            2.0f64.powi(-24).to_bits(),
            1u32 << 1,
        ),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        ctx.pc = 0x1000;
        let merge = [0x0123_4567_89AB_CDEF; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F80 | 1;
            x86.xmm[1] = [0xA5A5_5A5A_C3C3_3C3C; 16];
            x86.xmm[2] = merge;
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, width, source);
        }
        let result = execute_lifted_x86(&bytes, &mut ctx, &mut memory);
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{name}"
        );
        let actual = xmm(&ctx, 1);
        let result_width = if name.contains("binary16") && name.contains("to binary64") {
            64
        } else if name.contains("to binary16") {
            16
        } else {
            32
        };
        assert_eq!(
            SmirInterpreter::get_lane(&actual, 0, result_width),
            expected,
            "{name}: scalar result"
        );
        for bit in result_width..128 {
            assert_eq!(
                SmirInterpreter::get_lane(&actual, bit as u8, 1),
                SmirInterpreter::get_lane(&merge, bit as u8, 1),
                "{name}: merge bit {bit}"
            );
        }
        assert!(actual[2..].iter().all(|word| *word == 0), "{name}: upper");
        assert_eq!(mxcsr(&ctx) & 0x3F, 1 | expected_status, "{name}: status");
    }
}

#[test]
fn lifted_scalar_fp_convert_unmasked_exception_is_precise_and_noncommitting() {
    let original = [0x1357_9BDF_2468_ACE0; 16];
    let mut memory = FlatMemory::new(1);
    for (name, bytes, source, width, initial_mxcsr, expected_status) in [
        (
            "invalid",
            [0x62, 0xF1, 0xEF, 0x08, 0x5A, 0xCB],
            0x7FF0_1234_5678_9ABC,
            64,
            0x1F80 & !(1 << 7),
            1u32,
        ),
        (
            "underflow",
            [0x62, 0xF5, 0x6C, 0x08, 0x1D, 0xCB],
            u64::from(f32::MIN_POSITIVE.to_bits()),
            32,
            0x1F80 & !(1 << 11),
            (1u32 << 4) | (1u32 << 5),
        ),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        ctx.pc = 0x1000;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = initial_mxcsr;
            x86.xmm[1] = original;
            x86.xmm[2] = [0xF0E1_D2C3_B4A5_9687; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, width, source);
        }
        let result = execute_lifted_x86(&bytes, &mut ctx, &mut memory);
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::SimdFloatingPoint { addr: 0x1000 })
            ),
            "{name}: {result:?}"
        );
        assert_eq!(xmm(&ctx, 1), original, "{name}: destination");
        assert_eq!(
            mxcsr(&ctx) & expected_status,
            expected_status,
            "{name}: sticky status"
        );
    }
}

#[test]
fn lifted_scalar_fp_convert_sae_and_inactive_masks_suppress_exceptions() {
    let mut memory = FlatMemory::new(1);

    // VCVTSD2SH {rz-sae}: invalid is quieted in the result while SAE leaves
    // prior status unchanged even when Invalid is unmasked.
    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mxcsr = (0x1F80 & !(1 << 7)) | (1 << 5);
        x86.xmm[2] = [0x0123_4567_89AB_CDEF; 16];
        x86.xmm[3][0] = 0x7FF0_1234_5678_9ABC;
    }
    let result = execute_lifted_x86(&[0x62, 0xF5, 0xEF, 0x78, 0x5A, 0xCB], &mut ctx, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(mxcsr(&ctx) & 0x3F, 1 << 5);
    assert_eq!(xmm(&ctx, 1)[0] as u16 & 0x7E00, 0x7E00);

    for (zeroing, expected_low) in [(false, 0xBEEF), (true, 0)] {
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = (0x1F80 & !(1 << 7)) | (1 << 5);
            x86.k[1] = 0;
            x86.xmm[1] = [0xA5A5_5A5A_C3C3_BEEF; 16];
            x86.xmm[2] = [0x0123_4567_89AB_CDEF; 16];
            x86.xmm[3][0] = 0x7FF0_1234_5678_9ABC;
        }
        let p2 = if zeroing { 0x89 } else { 0x09 };
        let result = execute_lifted_x86(&[0x62, 0xF5, 0xEF, p2, 0x5A, 0xCB], &mut ctx, &mut memory);
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(xmm(&ctx, 1)[0] as u16, expected_low);
        assert_eq!(xmm(&ctx, 1)[0] >> 16, 0x0123_4567_89AB_CDEF >> 16);
        assert_eq!(
            mxcsr(&ctx) & 0x3F,
            1 << 5,
            "inactive source is not evaluated"
        );
    }
}

#[test]
fn synthetic_invalid_scalar_fp_convert_ir_fails_closed_without_committing() {
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    let merge = VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)));
    let src = VReg::Arch(ArchReg::X86(X86Reg::Xmm(3)));
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let original = [0x1357_9BDF_2468_ACE0; 16];

    for (from, to, round) in [
        (
            VecElementType::F32,
            VecElementType::F32,
            FpRoundMode::Dynamic,
        ),
        (
            VecElementType::I32,
            VecElementType::F64,
            FpRoundMode::Dynamic,
        ),
        (
            VecElementType::F64,
            VecElementType::F32,
            FpRoundMode::RoundNearestTiesAway,
        ),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::X86FpConvert {
                dst,
                merge,
                src,
                mask: Some(mask),
                from,
                to,
                mask_zeroing: true,
                round,
                suppress_exceptions: true,
                zero_upper: true,
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let function = builder.finish();
        let mut ctx = SmirContext::new_x86_64();
        ctx.pc = 0x1000;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F81;
            x86.k[1] = 0;
            x86.xmm[1] = original;
            x86.xmm[2] = [u64::MAX; 16];
            x86.xmm[3][0] = 0x3FF8_0000_0000_0000;
        }
        let result = SmirInterpreter::new().execute_block(
            &mut ctx,
            &mut FlatMemory::new(1),
            &function.blocks[0],
        );
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
        ));
        assert_eq!(xmm(&ctx, 1), original);
        assert_eq!(mxcsr(&ctx), 0x1F81);
    }
}
