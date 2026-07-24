//! Exact x86 FMA3 interpretation and MXCSR boundary tests.

use super::*;
use crate::smir::ir::ops::X86FmaOp;

const SRC1: X86Reg = X86Reg::Xmm(0);
const SRC2: X86Reg = X86Reg::Xmm(1);
const SRC3: X86Reg = X86Reg::Xmm(2);
const DST: X86Reg = X86Reg::Xmm(3);

fn reg(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn fma(
    elem: VecElementType,
    kind: X86FmaKind,
    order: X86FmaOrder,
    round: FpRoundMode,
    lanes: u8,
    mask: Option<VReg>,
) -> X86FmaOp {
    X86FmaOp {
        dst: reg(DST),
        src1: reg(SRC1),
        src2: reg(SRC2),
        src3: reg(SRC3),
        mask,
        elem,
        kind,
        order,
        round,
        lanes,
    }
}

fn execute(ctx: &mut SmirContext, op: OpKind) -> BlockResult {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, op);
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let function = builder.finish();
    SmirInterpreter::new().execute_block(ctx, &mut FlatMemory::new(1), &function.blocks[0])
}

fn set_lane(ctx: &mut SmirContext, register: X86Reg, lane: u8, width: u32, bits: u64) {
    let X86Reg::Xmm(index) = register else {
        unreachable!("test register is XMM")
    };
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!("test context is x86-64")
    };
    SmirInterpreter::set_lane(&mut x86.xmm[index as usize], lane, width, bits);
}

fn lane(ctx: &SmirContext, register: X86Reg, lane: u8, width: u32) -> u64 {
    let X86Reg::Xmm(index) = register else {
        unreachable!("test register is XMM")
    };
    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
        unreachable!("test context is x86-64")
    };
    SmirInterpreter::get_lane(&x86.xmm[index as usize], lane, width)
}

fn set_mxcsr(ctx: &mut SmirContext, mxcsr: u32) {
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!("test context is x86-64")
    };
    x86.mxcsr = mxcsr;
}

fn mxcsr(ctx: &SmirContext) -> u32 {
    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
        unreachable!("test context is x86-64")
    };
    x86.mxcsr
}

#[test]
fn x86_fma_rounds_binary32_and_binary64_from_mxcsr_or_embedded_control() {
    // 1 + 2^-p is exactly halfway between 1 and the next representable value.
    for (elem, width, half_ulp, one, next) in [
        (
            VecElementType::F32,
            32,
            0x3380_0000,
            0x3F80_0000,
            0x3F80_0001,
        ),
        (
            VecElementType::F64,
            64,
            0x3CA0_0000_0000_0000,
            0x3FF0_0000_0000_0000,
            0x3FF0_0000_0000_0001,
        ),
    ] {
        for (rc, expected) in [(0u32, one), (1, one), (2, next), (3, one)] {
            let mut ctx = SmirContext::new_x86_64();
            set_mxcsr(&mut ctx, 0x1F80 | (rc << 13));
            set_lane(&mut ctx, SRC1, 0, width, one);
            set_lane(&mut ctx, SRC2, 0, width, one);
            set_lane(&mut ctx, SRC3, 0, width, half_ulp);
            assert!(matches!(
                execute(
                    &mut ctx,
                    OpKind::X86Fma(fma(
                        elem,
                        X86FmaKind::Add,
                        X86FmaOrder::Order132,
                        FpRoundMode::Dynamic,
                        1,
                        None,
                    )),
                ),
                BlockResult::Exit(ExitReason::Halt)
            ));
            assert_eq!(lane(&ctx, DST, 0, width), expected, "{elem:?}, RC={rc}");
            assert_eq!(mxcsr(&ctx) & 0x3F, 1 << 5, "{elem:?}, RC={rc}");
        }

        let mut ctx = SmirContext::new_x86_64();
        let original_mxcsr = 0x1F80 | (1 << 13); // MXCSR requests round-down.
        set_mxcsr(&mut ctx, original_mxcsr);
        set_lane(&mut ctx, SRC1, 0, width, one);
        set_lane(&mut ctx, SRC2, 0, width, one);
        set_lane(&mut ctx, SRC3, 0, width, half_ulp);
        assert!(matches!(
            execute(
                &mut ctx,
                OpKind::X86Fma(fma(
                    elem,
                    X86FmaKind::Add,
                    X86FmaOrder::Order132,
                    FpRoundMode::RoundUp,
                    1,
                    None,
                )),
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        assert_eq!(lane(&ctx, DST, 0, width), next);
        assert_eq!(mxcsr(&ctx), original_mxcsr, "embedded rounding is SAE");
    }
}

#[test]
fn x86_fma_embedded_rounding_uses_sae_masked_underflow_response_for_ftz() {
    let mut ctx = SmirContext::new_x86_64();
    // FTZ=1, UM=0. SAE makes the arithmetic behave as if UM were set while
    // leaving both the architectural mask and sticky flags unchanged.
    let original_mxcsr = (0x1F80 & !(1 << 11)) | (1 << 15);
    set_mxcsr(&mut ctx, original_mxcsr);
    set_lane(&mut ctx, SRC1, 0, 32, 0x0080_0000); // minimum normal
    set_lane(&mut ctx, SRC2, 0, 32, 0);
    set_lane(&mut ctx, SRC3, 0, 32, 0x3F00_0000); // 0.5

    assert!(matches!(
        execute(
            &mut ctx,
            OpKind::X86Fma(fma(
                VecElementType::F32,
                X86FmaKind::Add,
                X86FmaOrder::Order132,
                FpRoundMode::RoundNearest,
                1,
                None,
            )),
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(lane(&ctx, DST, 0, 32), 0);
    assert_eq!(mxcsr(&ctx), original_mxcsr);
}

#[test]
fn lifted_evex_fma3_executes_nondefault_rc_with_sae_and_scalar_merge() {
    let mut ctx = SmirContext::new_x86_64();
    let original_mxcsr = 0x1F80 | (2 << 13); // Dynamic mode would round up.
    set_mxcsr(&mut ctx, original_mxcsr);
    set_lane(&mut ctx, X86Reg::Xmm(2), 0, 32, 0x3F80_0000);
    set_lane(&mut ctx, X86Reg::Xmm(2), 1, 32, 0xDEAD_BEEF);
    set_lane(&mut ctx, X86Reg::Xmm(1), 0, 32, 0x3F80_0000);
    set_lane(&mut ctx, X86Reg::Xmm(3), 0, 32, 0x3380_0000);

    // EVEX.b=1 and RC=01 select round-down. This encoding was previously
    // rejected because RC was incorrectly interpreted as a vector width.
    assert!(matches!(
        execute_lifted_x86(
            &[0x62, 0xF2, 0x75, 0x38, 0x99, 0xD3],
            &mut ctx,
            &mut FlatMemory::new(1),
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(lane(&ctx, X86Reg::Xmm(2), 0, 32), 0x3F80_0000);
    assert_eq!(lane(&ctx, X86Reg::Xmm(2), 1, 32), 0xDEAD_BEEF);
    assert_eq!(mxcsr(&ctx), original_mxcsr);
    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
        unreachable!()
    };
    assert!(x86.xmm[2][2..].iter().all(|word| *word == 0));
}

#[test]
fn x86_fma_retains_the_single_round_fused_residual() {
    for (elem, width, first, second, accumulator, expected) in [
        (
            VecElementType::F32,
            32,
            0x3F80_0001,
            0x3F7F_FFFE,
            0xBF80_0000,
            0xA880_0000,
        ),
        (
            VecElementType::F64,
            64,
            0x3FF0_0000_0000_0001,
            0x3FEF_FFFF_FFFF_FFFE,
            0xBFF0_0000_0000_0000,
            0xB970_0000_0000_0000,
        ),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        set_lane(&mut ctx, SRC1, 0, width, first);
        set_lane(&mut ctx, SRC2, 0, width, accumulator);
        set_lane(&mut ctx, SRC3, 0, width, second);
        assert!(matches!(
            execute(
                &mut ctx,
                OpKind::X86Fma(fma(
                    elem,
                    X86FmaKind::Add,
                    X86FmaOrder::Order132,
                    FpRoundMode::Dynamic,
                    1,
                    None,
                )),
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        assert_eq!(lane(&ctx, DST, 0, width), expected, "{elem:?}");
        assert_eq!(mxcsr(&ctx) & 0x3F, 0);
    }
}

#[test]
fn x86_fma_binary64_round_nearest_matches_independent_fused_oracle() {
    for (name, first, second, accumulator) in [
        (
            "accumulator-dominant alignment headroom",
            0x3FFF_FFFF_FFFF_FFFF,
            0x3FFF_FFFF_FFFF_FFFF,
            0x416F_FFFF_FFFF_FFFF,
        ),
        (
            "product-dominant alignment headroom",
            0x3FFF_FFFF_FFFF_FFFF,
            0x3FFF_FFFF_FFFF_FFFF,
            0x3B4F_FFFF_FFFF_FFFF,
        ),
    ] {
        let expected = f64::from_bits(first)
            .mul_add(f64::from_bits(second), f64::from_bits(accumulator))
            .to_bits();
        let actual = SmirInterpreter::x86_fma_boundary(
            first,
            second,
            accumulator,
            X86_SIMD_F64,
            false,
            false,
            FpRoundMode::RoundNearest,
            0x1F80,
        );
        assert_eq!(actual.bits, expected, "{name}");
    }

    let mut state = 0xD1B5_4A32_D192_ED03u64;
    let mut next_finite = || loop {
        state = state
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0xBF58_476D_1CE4_E5B9);
        if state & 0x7FF0_0000_0000_0000 != 0x7FF0_0000_0000_0000 {
            return state;
        }
    };

    for case in 0..100_000 {
        let first = next_finite();
        let second = next_finite();
        let accumulator = next_finite();
        let expected = f64::from_bits(first)
            .mul_add(f64::from_bits(second), f64::from_bits(accumulator))
            .to_bits();
        let actual = SmirInterpreter::x86_fma_boundary(
            first,
            second,
            accumulator,
            X86_SIMD_F64,
            false,
            false,
            FpRoundMode::RoundNearest,
            0x1F80,
        );
        assert_eq!(
            actual.bits, expected,
            "case {case}: a={first:016X} b={second:016X} c={accumulator:016X} status={:02X}",
            actual.status
        );
    }
}

#[test]
fn x86_fma_nan_priority_follows_the_132_213_231_mnemonic_order() {
    let numeric = 1.0f32.to_bits() as u64;
    let qnan1 = 0x7FC0_0011;
    let qnan2 = 0x7FC0_0022;
    let qnan3 = 0x7FC0_0033;
    for (order, sources, expected) in [
        (X86FmaOrder::Order132, [numeric, qnan2, qnan3], qnan3),
        (X86FmaOrder::Order213, [qnan1, numeric, qnan3], qnan1),
        (X86FmaOrder::Order231, [qnan1, numeric, qnan3], qnan3),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        for (register, bits) in [SRC1, SRC2, SRC3].into_iter().zip(sources) {
            set_lane(&mut ctx, register, 0, 32, bits);
        }
        assert!(matches!(
            execute(
                &mut ctx,
                OpKind::X86Fma(fma(
                    VecElementType::F32,
                    X86FmaKind::NegativeMultiplySub,
                    order,
                    FpRoundMode::Dynamic,
                    1,
                    None,
                )),
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        assert_eq!(lane(&ctx, DST, 0, 32), expected, "{order:?}");
        assert_eq!(mxcsr(&ctx) & 0x3F, 0);
    }
}

#[test]
fn x86_fma_quiet_nan_preempts_non_nan_invalid_product_classification() {
    for (nan, expected, invalid) in [
        (0x7FC0_0042u64, 0x7FC0_0042u64, false),
        (0x7F80_0042, 0x7FC0_0042, true),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        // Order132 computes src1 * src3 + src2: 0 * infinity + NaN.
        set_lane(&mut ctx, SRC1, 0, 32, 0);
        set_lane(&mut ctx, SRC2, 0, 32, nan);
        set_lane(&mut ctx, SRC3, 0, 32, f32::INFINITY.to_bits() as u64);
        assert!(matches!(
            execute(
                &mut ctx,
                OpKind::X86Fma(fma(
                    VecElementType::F32,
                    X86FmaKind::Add,
                    X86FmaOrder::Order132,
                    FpRoundMode::Dynamic,
                    1,
                    None,
                )),
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        assert_eq!(lane(&ctx, DST, 0, 32), expected);
        assert_eq!(mxcsr(&ctx) & 1, u32::from(invalid));
    }
}

#[test]
fn x86_fma_unmasked_exception_is_precise_and_embedded_rounding_is_sae() {
    let sentinel = [0xA5A5_A5A5_A5A5_A5A5; 16];
    let mut ctx = SmirContext::new_x86_64();
    set_mxcsr(&mut ctx, 0x1F00); // Invalid-operation exception unmasked.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = sentinel;
    }
    set_lane(&mut ctx, SRC1, 0, 32, 0);
    set_lane(&mut ctx, SRC2, 0, 32, 1.0f32.to_bits() as u64);
    set_lane(&mut ctx, SRC3, 0, 32, f32::INFINITY.to_bits() as u64);
    assert!(matches!(
        execute(
            &mut ctx,
            OpKind::X86Fma(fma(
                VecElementType::F32,
                X86FmaKind::Add,
                X86FmaOrder::Order132,
                FpRoundMode::Dynamic,
                1,
                None,
            )),
        ),
        BlockResult::Exit(ExitReason::SimdFloatingPoint { addr: 0x1000 })
    ));
    assert_eq!(mxcsr(&ctx), 0x1F01);
    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.xmm[3], sentinel);

    let mut ctx = SmirContext::new_x86_64();
    set_mxcsr(&mut ctx, 0x1F00);
    set_lane(&mut ctx, SRC1, 0, 32, 0);
    set_lane(&mut ctx, SRC2, 0, 32, 1.0f32.to_bits() as u64);
    set_lane(&mut ctx, SRC3, 0, 32, f32::INFINITY.to_bits() as u64);
    assert!(matches!(
        execute(
            &mut ctx,
            OpKind::X86Fma(fma(
                VecElementType::F32,
                X86FmaKind::Add,
                X86FmaOrder::Order132,
                FpRoundMode::RoundNearest,
                1,
                None,
            )),
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(lane(&ctx, DST, 0, 32), 0xFFC0_0000);
    assert_eq!(mxcsr(&ctx), 0x1F00);
}

#[test]
fn x86_fma_reports_precomputation_before_postcomputation_exceptions() {
    let run = |mxcsr_value: u32| {
        let sentinel = [0xB6B6_B6B6_B6B6_B6B6; 16];
        let mut ctx = SmirContext::new_x86_64();
        set_mxcsr(&mut ctx, mxcsr_value);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3] = sentinel;
        }
        // Lane 0 has a pre-computation invalid operation; lane 1 has a
        // post-computation overflow and inexact result.
        set_lane(&mut ctx, SRC1, 0, 32, 0);
        set_lane(&mut ctx, SRC3, 0, 32, f32::INFINITY.to_bits() as u64);
        set_lane(&mut ctx, SRC1, 1, 32, 0x7F7F_FFFF);
        set_lane(&mut ctx, SRC3, 1, 32, 2.0f32.to_bits() as u64);
        assert!(matches!(
            execute(
                &mut ctx,
                OpKind::X86Fma(fma(
                    VecElementType::F32,
                    X86FmaKind::Add,
                    X86FmaOrder::Order132,
                    FpRoundMode::Dynamic,
                    4,
                    None,
                )),
            ),
            BlockResult::Exit(ExitReason::SimdFloatingPoint { addr: 0x1000 })
        ));
        let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.xmm[3], sentinel);
        mxcsr(&ctx) & 0x3F
    };

    let invalid_and_overflow_unmasked = 0x1F80 & !(1 << 7) & !(1 << 10);
    assert_eq!(run(invalid_and_overflow_unmasked), 1 << 0);

    let overflow_unmasked = 0x1F80 & !(1 << 10);
    assert_eq!(run(overflow_unmasked), (1 << 0) | (1 << 3) | (1 << 5));
}

#[test]
fn x86_fma_writemask_suppresses_inactive_lane_arithmetic_and_status() {
    let mask = reg(X86Reg::K(1));
    let mut ctx = SmirContext::new_x86_64();
    ctx.write_vreg(mask, 1);
    set_lane(&mut ctx, SRC1, 0, 32, 2.0f32.to_bits() as u64);
    set_lane(&mut ctx, SRC2, 0, 32, 1.0f32.to_bits() as u64);
    set_lane(&mut ctx, SRC3, 0, 32, 3.0f32.to_bits() as u64);
    // Every inactive lane would report a different exception if evaluated.
    set_lane(&mut ctx, SRC1, 1, 32, 0);
    set_lane(&mut ctx, SRC3, 1, 32, f32::INFINITY.to_bits() as u64);
    set_lane(&mut ctx, SRC1, 2, 32, 0x7F80_0001); // signaling NaN
    set_lane(&mut ctx, SRC1, 3, 32, 1); // denormal operand
    assert!(matches!(
        execute(
            &mut ctx,
            OpKind::X86Fma(fma(
                VecElementType::F32,
                X86FmaKind::Add,
                X86FmaOrder::Order132,
                FpRoundMode::Dynamic,
                4,
                Some(mask),
            )),
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(lane(&ctx, DST, 0, 32), 7.0f32.to_bits() as u64);
    assert!((1..4).all(|index| lane(&ctx, DST, index, 32) == 0));
    assert_eq!(mxcsr(&ctx) & 0x3F, 0);
}

#[test]
fn x86_fma_models_daz_ftz_overflow_underflow_precision_and_zero_sign() {
    let run = |elem: VecElementType,
               width: u32,
               mxcsr_value: u32,
               first: u64,
               second: u64,
               accumulator: u64| {
        let mut ctx = SmirContext::new_x86_64();
        set_mxcsr(&mut ctx, mxcsr_value);
        set_lane(&mut ctx, SRC1, 0, width, first);
        set_lane(&mut ctx, SRC2, 0, width, accumulator);
        set_lane(&mut ctx, SRC3, 0, width, second);
        assert!(matches!(
            execute(
                &mut ctx,
                OpKind::X86Fma(fma(
                    elem,
                    X86FmaKind::Add,
                    X86FmaOrder::Order132,
                    FpRoundMode::Dynamic,
                    1,
                    None,
                )),
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        (lane(&ctx, DST, 0, width), mxcsr(&ctx) & 0x3F)
    };

    let run32 = |mxcsr, first, second, accumulator| {
        run(VecElementType::F32, 32, mxcsr, first, second, accumulator)
    };
    assert_eq!(run32(0x1F80, 1, 0x3F80_0000, 0), (1, 1 << 1));
    assert_eq!(run32(0x1FC0, 1, 0x3F80_0000, 0), (0, 0));
    assert_eq!(
        run32(0x9F80, 0x0080_0000, 0x3F00_0000, 0),
        (0, (1 << 4) | (1 << 5))
    );
    assert_eq!(
        run32(0x1F80, 0x7F7F_FFFF, 0x4000_0000, 0),
        (0x7F80_0000, (1 << 3) | (1 << 5))
    );
    assert_eq!(
        run32(0x3F80, 0x3F80_0000, 0x3F80_0000, 0xBF80_0000),
        (0x8000_0000, 0)
    );
    assert_eq!(run32(0x1F80, 0x3F80_0000, 0x3F80_0000, 0xBF80_0000), (0, 0));

    let run64 = |mxcsr, first, second, accumulator| {
        run(VecElementType::F64, 64, mxcsr, first, second, accumulator)
    };
    assert_eq!(run64(0x1F80, 1, 0x3FF0_0000_0000_0000, 0), (1, 1 << 1));
    assert_eq!(run64(0x1FC0, 1, 0x3FF0_0000_0000_0000, 0), (0, 0));
    assert_eq!(
        run64(0x9F80, 0x0010_0000_0000_0000, 0x3FE0_0000_0000_0000, 0,),
        (0, (1 << 4) | (1 << 5))
    );
    assert_eq!(
        run64(0x1F80, 0x7FEF_FFFF_FFFF_FFFF, 0x4000_0000_0000_0000, 0,),
        (0x7FF0_0000_0000_0000, (1 << 3) | (1 << 5))
    );
    assert_eq!(
        run64(
            0x3F80,
            0x3FF0_0000_0000_0000,
            0x3FF0_0000_0000_0000,
            0xBFF0_0000_0000_0000,
        ),
        (0x8000_0000_0000_0000, 0)
    );
    assert_eq!(
        run64(
            0x1F80,
            0x3FF0_0000_0000_0000,
            0x3FF0_0000_0000_0000,
            0xBFF0_0000_0000_0000,
        ),
        (0, 0)
    );
}

#[test]
fn x86_fma_alternating_kinds_select_addition_and_subtraction_by_lane() {
    for (kind, expected) in [
        (X86FmaKind::AddSub, [5.0f32, 7.0, 5.0, 7.0]),
        (X86FmaKind::SubAdd, [7.0f32, 5.0, 7.0, 5.0]),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        for lane_index in 0..4 {
            set_lane(&mut ctx, SRC1, lane_index, 32, 2.0f32.to_bits() as u64);
            set_lane(&mut ctx, SRC2, lane_index, 32, 1.0f32.to_bits() as u64);
            set_lane(&mut ctx, SRC3, lane_index, 32, 3.0f32.to_bits() as u64);
        }
        assert!(matches!(
            execute(
                &mut ctx,
                OpKind::X86Fma(fma(
                    VecElementType::F32,
                    kind,
                    X86FmaOrder::Order132,
                    FpRoundMode::Dynamic,
                    4,
                    None,
                )),
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        for (lane_index, expected) in expected.into_iter().enumerate() {
            assert_eq!(
                lane(&ctx, DST, lane_index as u8, 32),
                expected.to_bits() as u64
            );
        }
    }
}

#[test]
fn x86_fp16_fma_nan_priority_uses_mnemonic_arithmetic_order() {
    let mut ctx = SmirContext::new_x86_64();
    set_lane(&mut ctx, SRC1, 0, 16, 0x3C00);
    set_lane(&mut ctx, SRC2, 0, 16, 0x7E22);
    set_lane(&mut ctx, SRC3, 0, 16, 0x7E33);
    assert!(matches!(
        execute(
            &mut ctx,
            OpKind::X86FP16Fma {
                dst: reg(DST),
                src1: reg(SRC1),
                src2: reg(SRC2),
                src3: reg(SRC3),
                mask: None,
                kind: X86FmaKind::Add,
                order: X86FmaOrder::Order132,
                round: FpRoundMode::Dynamic,
                lanes: 1,
            },
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(lane(&ctx, DST, 0, 16), 0x7E33);
    assert_eq!(mxcsr(&ctx) & 0x3F, 0);
}

#[test]
fn malformed_x86_fma_ir_fails_closed_and_remains_optimizer_observable() {
    let malformed = fma(
        VecElementType::F16,
        X86FmaKind::Add,
        X86FmaOrder::Order132,
        FpRoundMode::Dynamic,
        1,
        None,
    );
    assert!(!malformed.shape_valid());
    assert!(OpKind::X86Fma(malformed).has_side_effects());
    assert_eq!(
        malformed.source_vregs(),
        vec![reg(SRC1), reg(SRC2), reg(SRC3)]
    );

    let sentinel = [0xCCCC_CCCC_CCCC_CCCC; 16];
    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = sentinel;
    }
    assert!(matches!(
        execute(&mut ctx, OpKind::X86Fma(malformed)),
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.xmm[3], sentinel);
}
