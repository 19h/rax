//! AVX10.2 MAP5 saturating-conversion execution tests.

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

const SENTINEL: [u64; 16] = [0xA5A5_5A5A_A5A5_5A5A; 16];

fn set_f32_lanes(value: &mut [u64; 16], lanes: &[f32]) {
    for (lane, input) in lanes.iter().enumerate() {
        SmirInterpreter::set_lane(value, lane as u8, 32, u64::from(input.to_bits()));
    }
}

fn set_f64_lanes(value: &mut [u64; 16], lanes: &[f64]) {
    for (lane, input) in lanes.iter().enumerate() {
        SmirInterpreter::set_lane(value, lane as u8, 64, input.to_bits());
    }
}

#[test]
fn lifted_saturating_byte_conversions_use_dword_slots_and_exact_status() {
    for (opcode, inputs, expected) in [
        (
            0x68,
            [-129.0, -128.9, 127.9, 128.0],
            [0x80u64, 0x80, 0x7F, 0x7F],
        ),
        (0x6A, [-1.0, -0.5, 255.9, 256.0], [0u64, 0, 0xFF, 0xFF]),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = SENTINEL;
            set_f32_lanes(&mut x86.xmm[2], &inputs);
            x86.mxcsr = 0x1F80;
        }
        let exit = execute_lifted_x86(
            &[0x62, 0xF5, 0x7D, 0x08, opcode, 0xCA],
            &mut ctx,
            &mut FlatMemory::new(0x100),
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for (lane, expected) in expected.into_iter().enumerate() {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane as u8, 32),
                    expected,
                    "opcode {opcode:#04x}, dword lane {lane}"
                );
            }
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
            assert_eq!(x86.mxcsr & 0x3F, (1 << 5) | 1);
        }
    }
}

#[test]
fn lifted_i32_i64_saturation_preserves_narrowing_and_widening_lane_geometry() {
    let mut narrowing = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut narrowing.arch_regs {
        x86.xmm[1] = SENTINEL;
        set_f64_lanes(&mut x86.xmm[2], &[-2_147_483_648.9, 2_147_483_648.0]);
        x86.mxcsr = 0x1F80;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF5, 0xFC, 0x08, 0x6D, 0xCA],
        &mut narrowing,
        &mut FlatMemory::new(0x100),
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &narrowing.arch_regs else {
        unreachable!()
    };
    assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 32), 0x8000_0000);
    assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 1, 32), 0x7FFF_FFFF);
    assert!(x86.xmm[1][1..].iter().all(|word| *word == 0));
    assert_eq!(x86.mxcsr & 0x3F, 1 | (1 << 5));

    let mut equal = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut equal.arch_regs {
        x86.xmm[1] = SENTINEL;
        set_f32_lanes(&mut x86.xmm[2], &[-0.5, 1.9, 4_294_967_296.0, f32::NAN]);
        x86.mxcsr = 0x1F80;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF5, 0x7C, 0x08, 0x6C, 0xCA],
        &mut equal,
        &mut FlatMemory::new(0x100),
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &equal.arch_regs else {
        unreachable!()
    };
    for (lane, expected) in [0, 1, 0xFFFF_FFFF, 0].into_iter().enumerate() {
        assert_eq!(
            SmirInterpreter::get_lane(&x86.xmm[1], lane as u8, 32),
            expected
        );
    }
    assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
    assert_eq!(x86.mxcsr & 0x3F, 1 | (1 << 5));

    let mut widening = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut widening.arch_regs {
        x86.xmm[1] = SENTINEL;
        set_f32_lanes(
            &mut x86.xmm[2],
            &[
                -9_223_372_036_854_775_808.0,
                -1.9,
                1.9,
                9_223_372_036_854_775_808.0,
            ],
        );
        x86.mxcsr = 0x1F80;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF5, 0x7D, 0x28, 0x6D, 0xCA],
        &mut widening,
        &mut FlatMemory::new(0x100),
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &widening.arch_regs else {
        unreachable!()
    };
    for (lane, expected) in [0x8000_0000_0000_0000, u64::MAX, 1, i64::MAX as u64]
        .into_iter()
        .enumerate()
    {
        assert_eq!(x86.xmm[1][lane], expected);
    }
    assert!(x86.xmm[1][4..].iter().all(|word| *word == 0));
    assert_eq!(x86.mxcsr & 0x3F, 1 | (1 << 5));
}

#[test]
fn saturating_conversion_helper_handles_nan_infinity_and_i64_u64_boundaries() {
    for (bits, format, int_bits, signed, expected, status) in [
        (u64::from(f32::NAN.to_bits()), X86_SIMD_F32, 8, true, 0, 1),
        (
            u64::from(f32::INFINITY.to_bits()),
            X86_SIMD_F32,
            8,
            true,
            0x7F,
            1,
        ),
        (
            u64::from(f32::NEG_INFINITY.to_bits()),
            X86_SIMD_F32,
            8,
            true,
            0x80,
            1,
        ),
        (
            (-9_223_372_036_854_775_808.0f64).to_bits(),
            X86_SIMD_F64,
            64,
            true,
            0x8000_0000_0000_0000,
            0,
        ),
        (
            9_223_372_036_854_775_808.0f64.to_bits(),
            X86_SIMD_F64,
            64,
            true,
            0x7FFF_FFFF_FFFF_FFFF,
            1,
        ),
        ((-1.0f64).to_bits(), X86_SIMD_F64, 64, false, 0, 1),
        (
            18_446_744_073_709_551_616.0f64.to_bits(),
            X86_SIMD_F64,
            64,
            false,
            u64::MAX,
            1,
        ),
    ] {
        let converted = SmirInterpreter::x86_simd_fp_to_int_sat(
            bits,
            format,
            int_bits,
            signed,
            FpRoundMode::RoundTowardZero,
        );
        assert_eq!(converted.bits, expected);
        assert_eq!(converted.status, status);
    }

    // The representable binary64 value immediately below 2^64 is 2^64-2048.
    let next_down = f64::from_bits(18_446_744_073_709_551_616.0f64.to_bits() - 1);
    let converted = SmirInterpreter::x86_simd_fp_to_int_sat(
        next_down.to_bits(),
        X86_SIMD_F64,
        64,
        false,
        FpRoundMode::RoundTowardZero,
    );
    assert_eq!(converted.bits, u64::MAX - 2047);
    assert_eq!(converted.status, 0);
}

#[test]
fn nontruncating_byte_helper_uses_pre_round_avx10_2_saturation_thresholds() {
    for (mode, signed_expected, unsigned_expected) in [
        (
            FpRoundMode::RoundNearest,
            [0x80, 0xFE, 2, 0x7F],
            [0, 0, 0xFE, 0xFF],
        ),
        (
            FpRoundMode::RoundDown,
            [0x80, 0xFE, 1, 0x7F],
            [0, 0, 0xFE, 0xFF],
        ),
        (
            FpRoundMode::RoundUp,
            [0x80, 0xFF, 2, 0x7F],
            [0, 1, 0xFF, 0xFF],
        ),
        (
            FpRoundMode::RoundTowardZero,
            [0x80, 0xFF, 1, 0x7F],
            [0, 0, 0xFE, 0xFF],
        ),
    ] {
        for (signed, inputs, expected) in [
            (true, [-128.5f32, -1.5, 1.5, 127.25], signed_expected),
            (false, [-0.75f32, 0.5, 254.5, 255.75], unsigned_expected),
        ] {
            for (lane, input) in inputs.into_iter().enumerate() {
                let converted = SmirInterpreter::x86_simd_fp_to_int_sat(
                    u64::from(input.to_bits()),
                    X86_SIMD_F32,
                    8,
                    signed,
                    mode,
                );
                assert_eq!(converted.bits, expected[lane]);
                let invalid = signed
                    && ((mode == FpRoundMode::RoundDown && lane == 0)
                        || (mode == FpRoundMode::RoundUp && lane == 3));
                assert_eq!(converted.status, if invalid { 1 } else { 1 << 5 });
            }
        }
    }

    for (input, expected) in [(-1.0f32, 0), (256.0, 0xFF)] {
        let converted = SmirInterpreter::x86_simd_fp_to_int_sat(
            u64::from(input.to_bits()),
            X86_SIMD_F32,
            8,
            false,
            FpRoundMode::RoundUp,
        );
        assert_eq!(converted.bits, expected);
        assert_eq!(converted.status, 1);
    }

    let next_more_negative = |value: f32| f32::from_bits(value.to_bits() + 1);
    let next_more_positive = |value: f32| f32::from_bits(value.to_bits() - 1);
    let next_up = |value: f32| f32::from_bits(value.to_bits() + 1);
    for (mode, input, signed, expected, status) in [
        (FpRoundMode::RoundNearest, -128.5, true, 0x80, 1 << 5),
        (FpRoundMode::RoundNearest, 127.5, true, 0x7F, 1),
        (FpRoundMode::RoundDown, -128.0, true, 0x80, 0),
        (
            FpRoundMode::RoundDown,
            next_more_negative(-128.0),
            true,
            0x80,
            1,
        ),
        (FpRoundMode::RoundUp, 127.0, true, 0x7F, 0),
        (FpRoundMode::RoundUp, next_up(127.0), true, 0x7F, 1),
        (FpRoundMode::RoundUp, -1.0, false, 0, 1),
        (
            FpRoundMode::RoundUp,
            next_more_positive(-1.0),
            false,
            0,
            1 << 5,
        ),
        (FpRoundMode::RoundUp, 256.0, false, 0xFF, 1),
    ] {
        let converted = SmirInterpreter::x86_simd_fp_to_int_sat(
            u64::from(input.to_bits()),
            X86_SIMD_F32,
            8,
            signed,
            mode,
        );
        assert_eq!(converted.bits, expected, "{mode:?}, {input:?}");
        assert_eq!(converted.status, status, "{mode:?}, {input:?}");
    }
}

#[test]
fn lifted_nontruncating_byte_conversion_resolves_mxcsr_and_embedded_rounding() {
    for (bytes, mxcsr, expected) in [
        (
            &[0x62, 0xF5, 0x7D, 0x08, 0x69, 0xCA][..],
            0x1F80 | (1 << 13),
            [0x80, 0xFE, 1, 0x7F],
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x58, 0x69, 0xCA][..],
            0,
            [0x80, 0xFF, 2, 0x7F],
        ),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            set_f32_lanes(&mut x86.xmm[2], &[-128.5, -1.5, 1.5, 127.25]);
            x86.mxcsr = mxcsr;
        }
        let exit = execute_lifted_x86(bytes, &mut ctx, &mut FlatMemory::new(0x100));
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for (lane, expected) in expected.into_iter().enumerate() {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane as u8, 32),
                    expected
                );
            }
            if mxcsr == 0 {
                assert_eq!(x86.mxcsr, 0, "embedded rounding must imply SAE");
            } else {
                assert_eq!(x86.mxcsr & 0x3F, 1 | (1 << 5));
            }
        }
    }
}

#[test]
fn lifted_saturating_conversion_masks_exceptions_and_commits_atomically() {
    for (zeroing, p2, inactive) in [(false, 0x09, 0xA5A5_5A5A), (true, 0x89, 0)] {
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = SENTINEL;
            set_f32_lanes(&mut x86.xmm[2], &[1.0, f32::NAN, 3.9, f32::INFINITY]);
            x86.k[1] = 0b0101;
            x86.mxcsr = 0x1F80 & !(1 << 7);
        }
        let exit = execute_lifted_x86(
            &[0x62, 0xF5, 0x7D, p2, 0x68, 0xCA],
            &mut ctx,
            &mut FlatMemory::new(0x100),
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 32), 1);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 1, 32), inactive);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 2, 32), 3);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 3, 32), inactive);
            assert_eq!(x86.mxcsr & 0x3F, 1 << 5, "zeroing={zeroing}");
        }
    }

    let mut unmasked = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut unmasked.arch_regs {
        x86.xmm[1] = SENTINEL;
        set_f32_lanes(&mut x86.xmm[2], &[f32::NAN, 1.0, 2.0, 3.0]);
        x86.mxcsr = 0x1F80 & !(1 << 7);
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF5, 0x7D, 0x08, 0x68, 0xCA],
        &mut unmasked,
        &mut FlatMemory::new(0x100),
    );
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &unmasked.arch_regs {
        assert_eq!(x86.xmm[1], SENTINEL);
        assert_eq!(x86.mxcsr & 0x3F, 1);
    }

    for (invalid_masked, expected_status) in [(false, 1), (true, 1 | (1 << 5))] {
        let mut mixed = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut mixed.arch_regs {
            x86.xmm[1] = SENTINEL;
            set_f32_lanes(&mut x86.xmm[2], &[f32::NAN, 1.5, 0.0, 0.0]);
            x86.mxcsr = (0x1F80 & !(1 << 12)) & if invalid_masked { u32::MAX } else { !(1 << 7) };
        }
        let exit = execute_lifted_x86(
            &[0x62, 0xF5, 0x7D, 0x08, 0x68, 0xCA],
            &mut mixed,
            &mut FlatMemory::new(0x100),
        );
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        if let ArchRegState::X86_64(x86) = &mixed.arch_regs {
            assert_eq!(x86.xmm[1], SENTINEL);
            assert_eq!(x86.mxcsr & 0x3F, expected_status);
        }
    }

    let mut sae = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut sae.arch_regs {
        x86.xmm[1] = SENTINEL;
        set_f32_lanes(&mut x86.xmm[2], &[f32::NAN, f32::INFINITY, -129.0, 1.9]);
        x86.mxcsr = 0;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF5, 0x7D, 0x18, 0x68, 0xCA],
        &mut sae,
        &mut FlatMemory::new(0x100),
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &sae.arch_regs {
        assert_eq!(x86.mxcsr & 0x3F, 0);
        for (lane, expected) in [0, 0x7F, 0x80, 1].into_iter().enumerate() {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], lane as u8, 32),
                expected
            );
        }
    }
}

#[test]
fn lifted_saturating_conversion_honors_daz_and_masked_memory_fault_suppression() {
    for (mxcsr, expected_status) in [(0x1F80, 1 << 5), (0x1F80 | (1 << 6), 0)] {
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            set_f32_lanes(&mut x86.xmm[2], &[f32::from_bits(1), 0.0, 0.0, 0.0]);
            x86.mxcsr = mxcsr;
        }
        let exit = execute_lifted_x86(
            &[0x62, 0xF5, 0x7D, 0x08, 0x68, 0xCA],
            &mut ctx,
            &mut FlatMemory::new(0x100),
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 32), 0);
            assert_eq!(x86.mxcsr & 0x3F, expected_status);
        }
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    ctx.write_vreg(rax, 0x100);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = SENTINEL;
        x86.k[2] = 0;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF5, 0x7D, 0x1A, 0x68, 0x08],
        &mut ctx,
        &mut FlatMemory::new(0x100),
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1][0..2], SENTINEL[0..2]);
        assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = SENTINEL;
        x86.k[2] = 1;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF5, 0x7D, 0x1A, 0x68, 0x08],
        &mut ctx,
        &mut FlatMemory::new(0x100),
    );
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], SENTINEL);
    }

    let mut rounded = SmirContext::new_x86_64();
    rounded.write_vreg(rax, 0x40);
    if let ArchRegState::X86_64(x86) = &mut rounded.arch_regs {
        x86.mxcsr = 0x1F80 | (2 << 13);
    }
    let mut memory = FlatMemory::new(0x100);
    memory.write(0x40, &0.5f32.to_bits().to_le_bytes()).unwrap();
    let exit = execute_lifted_x86(
        &[0x62, 0xF5, 0x7D, 0x18, 0x6B, 0x08],
        &mut rounded,
        &mut memory,
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &rounded.arch_regs {
        for lane in 0..4 {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 32), 1);
        }
        assert_eq!(x86.mxcsr & 0x3F, 1 << 5);
        assert_eq!((x86.mxcsr >> 13) & 3, 2);
    }
}

#[test]
fn optimized_saturating_conversion_matches_o0_o1_o2() {
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::optimize::{OptLevel, optimize_function};

    let bytes = [0x62, 0xF5, 0x7D, 0x09, 0x68, 0xCA];
    let mut observed = Vec::new();
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut lifter = X86_64Lifter::strict();
        let mut lift_ctx = LiftContext::new(SourceArch::X86_64);
        let lifted = lifter.lift_insn(0x1000, &bytes, &mut lift_ctx).unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let mut function = builder.finish();
        function.blocks[0].ops = lifted.ops;
        optimize_function(&mut function, level);

        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = SENTINEL;
            set_f32_lanes(&mut x86.xmm[2], &[1.9, f32::NAN, 127.9, f32::INFINITY]);
            x86.k[1] = 0b0101;
            x86.mxcsr = 0x1F80;
        }
        let exit = SmirInterpreter::new().execute_block(
            &mut ctx,
            &mut FlatMemory::new(0x100),
            &function.blocks[0],
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            observed.push((x86.xmm[1], x86.mxcsr));
        }
    }
    assert_eq!(observed[0], observed[1]);
    assert_eq!(observed[0], observed[2]);
    assert_eq!(SmirInterpreter::get_lane(&observed[0].0, 0, 32), 1);
    assert_eq!(SmirInterpreter::get_lane(&observed[0].0, 2, 32), 0x7F);
    assert_eq!(observed[0].1 & 0x3F, 1 << 5);
}

#[test]
fn optimized_narrowing_and_widening_saturation_match_o0_o1_o2() {
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::optimize::{OptLevel, optimize_function};

    for (bytes, narrowing) in [
        (&[0x62, 0xF5, 0xFC, 0x08, 0x6D, 0xCA][..], true),
        (&[0x62, 0xF5, 0x7D, 0x28, 0x6D, 0xCA][..], false),
    ] {
        let mut observed = Vec::new();
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut lifter = X86_64Lifter::strict();
            let mut lift_ctx = LiftContext::new(SourceArch::X86_64);
            let lifted = lifter.lift_insn(0x1000, bytes, &mut lift_ctx).unwrap();
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.set_terminator(Terminator::Trap {
                kind: TrapKind::Halt,
            });
            let mut function = builder.finish();
            function.blocks[0].ops = lifted.ops;
            optimize_function(&mut function, level);

            let mut ctx = SmirContext::new_x86_64();
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1] = SENTINEL;
                if narrowing {
                    set_f64_lanes(&mut x86.xmm[2], &[-2_147_483_648.9, 2_147_483_648.0]);
                } else {
                    set_f32_lanes(&mut x86.xmm[2], &[-1.9, 1.9, f32::NAN, f32::INFINITY]);
                }
                x86.mxcsr = 0x1F80;
            }
            let exit = SmirInterpreter::new().execute_block(
                &mut ctx,
                &mut FlatMemory::new(0x100),
                &function.blocks[0],
            );
            assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
            let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                unreachable!()
            };
            observed.push((x86.xmm[1], x86.mxcsr));
        }
        assert_eq!(observed[0], observed[1]);
        assert_eq!(observed[0], observed[2]);
        assert_eq!(observed[0].1 & 0x3F, 1 | (1 << 5));
    }
}

#[test]
fn optimized_nontruncating_conversion_matches_o0_o1_o2() {
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::optimize::{OptLevel, optimize_function};

    let bytes = [0x62, 0xF5, 0x7D, 0x08, 0x69, 0xCA];
    let mut observed = Vec::new();
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut lifter = X86_64Lifter::strict();
        let mut lift_ctx = LiftContext::new(SourceArch::X86_64);
        let lifted = lifter.lift_insn(0x1000, &bytes, &mut lift_ctx).unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let mut function = builder.finish();
        function.blocks[0].ops = lifted.ops;
        optimize_function(&mut function, level);

        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            set_f32_lanes(&mut x86.xmm[2], &[-128.5, -1.5, 1.5, 127.25]);
            x86.mxcsr = 0x1F80 | (1 << 13);
        }
        let exit = SmirInterpreter::new().execute_block(
            &mut ctx,
            &mut FlatMemory::new(0x100),
            &function.blocks[0],
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            observed.push((x86.xmm[1], x86.mxcsr));
        }
    }
    assert_eq!(observed[0], observed[1]);
    assert_eq!(observed[0], observed[2]);
    for (lane, expected) in [0x80, 0xFE, 1, 0x7F].into_iter().enumerate() {
        assert_eq!(
            SmirInterpreter::get_lane(&observed[0].0, lane as u8, 32),
            expected
        );
    }
    assert_eq!(observed[0].1 & 0x3F, 1 | (1 << 5));
}
