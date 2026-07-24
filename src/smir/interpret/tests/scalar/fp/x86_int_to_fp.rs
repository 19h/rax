//! Precise scalar x86 integer-to-floating-point exception-state tests.

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
fn lifted_scalar_int_to_fp_commits_masked_precision_and_overflow_status() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut memory = FlatMemory::new(1);

    for (name, bytes, input, expected_bits, expected_status) in [
        (
            "signed binary32 precision",
            &[0x62, 0xF1, 0x6E, 0x08, 0x2A, 0xC8][..],
            16_777_217u64,
            16_777_216.0f32.to_bits() as u64,
            1u32 << 5,
        ),
        (
            "unsigned binary64 precision",
            &[0x62, 0xF1, 0xEF, 0x08, 0x7B, 0xC8][..],
            9_007_199_254_740_993u64,
            9_007_199_254_740_992.0f64.to_bits(),
            1u32 << 5,
        ),
        (
            "signed binary16 overflow and precision",
            &[0x62, 0xF5, 0x6E, 0x08, 0x2A, 0xC8][..],
            70_000u64,
            0x7C00,
            (1u32 << 3) | (1u32 << 5),
        ),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        ctx.pc = 0x1000;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F80 | 1;
            x86.xmm[2] = [0xA5A5_5A5A_C3C3_3C3C; 16];
        }
        ctx.write_vreg(rax, input);
        let result = execute_lifted_x86(bytes, &mut ctx, &mut memory);
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{name}"
        );
        let lane_width = if name.contains("binary16") {
            16
        } else if name.contains("binary32") {
            32
        } else {
            64
        };
        assert_eq!(
            SmirInterpreter::get_lane(&xmm(&ctx, 1), 0, lane_width),
            expected_bits,
            "{name}: result"
        );
        assert_eq!(
            mxcsr(&ctx) & 0x3F,
            1 | expected_status,
            "{name}: sticky status"
        );
    }
}

#[test]
fn lifted_scalar_int_to_fp_unmasked_exception_is_precise_and_noncommitting() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let original = [0x0123_4567_89AB_CDEF; 16];
    let mut memory = FlatMemory::new(1);

    for (name, bytes, input, initial_mxcsr, expected_status) in [
        (
            "precision",
            &[0x62, 0xF1, 0x6E, 0x08, 0x2A, 0xC8][..],
            16_777_217u64,
            0x1F80 & !(1 << 12),
            1u32 << 5,
        ),
        (
            "overflow",
            &[0x62, 0xF5, 0x6E, 0x08, 0x2A, 0xC8][..],
            70_000u64,
            0x1F80 & !(1 << 10),
            (1u32 << 3) | (1u32 << 5),
        ),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        ctx.pc = 0x1000;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = initial_mxcsr;
            x86.xmm[1] = original;
            x86.xmm[2] = [0xF0E1_D2C3_B4A5_9687; 16];
        }
        ctx.write_vreg(rax, input);
        let result = execute_lifted_x86(bytes, &mut ctx, &mut memory);
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
            "{name}: status"
        );
    }
}

#[test]
fn lifted_scalar_int_to_fp_sae_suppresses_status_and_unmasked_traps() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut memory = FlatMemory::new(1);

    for (name, bytes, input, expected_bits, lane_width) in [
        (
            "binary32 precision {rd-sae}",
            &[0x62, 0xF1, 0x6E, 0x38, 0x2A, 0xC8][..],
            16_777_217u64,
            16_777_216.0f32.to_bits() as u64,
            32,
        ),
        (
            "binary16 overflow {rz-sae}",
            &[0x62, 0xF5, 0x6E, 0x78, 0x2A, 0xC8][..],
            70_000u64,
            0x7BFF,
            16,
        ),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = (0x1F80 & !((1 << 10) | (1 << 12))) | 1;
            x86.xmm[2] = [0xCAFE_BABE_DEAD_BEEF; 16];
        }
        ctx.write_vreg(rax, input);
        let result = execute_lifted_x86(bytes, &mut ctx, &mut memory);
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{name}"
        );
        assert_eq!(
            SmirInterpreter::get_lane(&xmm(&ctx, 1), 0, lane_width),
            expected_bits,
            "{name}: result"
        );
        assert_eq!(mxcsr(&ctx) & 0x3F, 1, "{name}: prior status only");
    }
}

#[test]
fn lifted_scalar_binary64_w0_is_exact_and_ignores_attempted_embedded_rounding() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let merge = [0xA5A5_5A5A_C3C3_3C3C; 16];
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mxcsr = (0x1F80 & !(1 << 12)) | (1 << 13);
        x86.xmm[1] = [u64::MAX; 16];
        x86.xmm[2] = merge;
    }
    ctx.write_vreg(rax, 0xDEAD_BEEF_FFFF_FFFF);

    // EVEX.b=1/L'L=2 is architecturally ignored for W0 VCVTUSI2SD.
    let result = execute_lifted_x86(&[0x62, 0xF1, 0x6F, 0x58, 0x7B, 0xC8], &mut ctx, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let result_vector = xmm(&ctx, 1);
    assert_eq!(result_vector[0], 4_294_967_295.0f64.to_bits());
    assert_eq!(result_vector[1], merge[1]);
    assert!(result_vector[2..].iter().all(|word| *word == 0));
    assert_eq!(mxcsr(&ctx) & (1 << 5), 0, "no Precision exception");
}

#[test]
fn synthetic_invalid_scalar_int_to_fp_ir_fails_closed_without_committing() {
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    let merge = VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)));
    let src = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let original = [0x1357_9BDF_2468_ACE0; 16];

    let invalid = [
        OpKind::X86IntToFp {
            dst,
            merge,
            src,
            elem: VecElementType::I32,
            int_width: OpWidth::W32,
            signed: true,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: true,
            zero_upper: true,
        },
        OpKind::X86IntToFp {
            dst,
            merge,
            src,
            elem: VecElementType::F32,
            int_width: OpWidth::W16,
            signed: true,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: true,
            zero_upper: true,
        },
        OpKind::X86IntToFp {
            dst,
            merge,
            src,
            elem: VecElementType::F32,
            int_width: OpWidth::W32,
            signed: true,
            round: FpRoundMode::RoundNearestTiesAway,
            suppress_exceptions: true,
            zero_upper: true,
        },
    ];

    for kind in invalid {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let function = builder.finish();
        let mut ctx = SmirContext::new_x86_64();
        ctx.pc = 0x1000;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F81;
            x86.xmm[1] = original;
        }
        ctx.write_vreg(src, u64::MAX);
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
