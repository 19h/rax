//! Cross-operation x86 SIMD NaN/denormal exception-priority tests.

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

type BinaryCore = fn(u64, u64, X86SimdFpFormat, FpRoundMode, u32) -> X86SimdFpResult;

#[test]
fn arithmetic_nan_precedence_suppresses_same_lane_denormal_status_for_every_core() {
    let cores: [(&str, BinaryCore); 4] = [
        ("add", SmirInterpreter::x86_simd_fp_add),
        ("sub", SmirInterpreter::x86_simd_fp_sub),
        ("mul", SmirInterpreter::x86_simd_fp_mul),
        ("div", SmirInterpreter::x86_simd_fp_div),
    ];
    let denormal = 1u64;
    let qnan = 0x7FC1_2345u64;
    let snan = 0x7F81_2345u64;
    let quieted_snan = 0x7FC1_2345u64;

    for (name, core) in cores {
        for mxcsr in [0x1F80, 0x1F80 | (1 << 6)] {
            for (nan, expected, invalid) in [(qnan, qnan, 0), (snan, quieted_snan, 1)] {
                let first_nan = core(
                    nan,
                    denormal,
                    X86_SIMD_F32,
                    FpRoundMode::RoundNearest,
                    mxcsr,
                );
                assert_eq!(first_nan.bits, expected, "{name} first NaN");
                assert_eq!(
                    first_nan.status, invalid,
                    "{name} first NaN must suppress src2 DE, MXCSR={mxcsr:#06X}"
                );

                let second_nan = core(
                    denormal,
                    nan,
                    X86_SIMD_F32,
                    FpRoundMode::RoundNearest,
                    mxcsr,
                );
                assert_eq!(second_nan.bits, expected, "{name} second NaN");
                assert_eq!(
                    second_nan.status, invalid,
                    "{name} second NaN must suppress src1 DE, MXCSR={mxcsr:#06X}"
                );
            }
        }
    }
}

#[test]
fn minmax_nan_precedence_selects_src2_without_denormal_status() {
    for (format, denormals, nans) in [
        (
            X86_SIMD_F32,
            [1u64, 0x8000_0001],
            [0x7FC1_2345u64, 0x7F81_2345],
        ),
        (
            X86_SIMD_F64,
            [1u64, 0x8000_0000_0000_0001],
            [0x7FF8_2468_ACE0_1357u64, 0x7FF0_2468_ACE0_1357],
        ),
    ] {
        let (sign, _, _, _) = SmirInterpreter::x86_simd_fp_masks(format);
        for min in [true, false] {
            for daz in [false, true] {
                let mxcsr = 0x1F80 | (u32::from(daz) << 6);
                for denormal in denormals {
                    let selected_src2 = if daz { denormal & sign } else { denormal };
                    for nan in nans {
                        let first_nan =
                            SmirInterpreter::x86_simd_fp_min_max(nan, denormal, format, mxcsr, min);
                        assert_eq!(
                            first_nan.bits, selected_src2,
                            "MIN/MAX selects DAZ-transformed src2; DAZ={daz}"
                        );
                        assert_eq!(
                            first_nan.status, 1,
                            "MIN/MAX NaN must suppress src2 DE; DAZ={daz}"
                        );

                        let second_nan =
                            SmirInterpreter::x86_simd_fp_min_max(denormal, nan, format, mxcsr, min);
                        assert_eq!(second_nan.bits, nan, "MIN/MAX selects NaN src2");
                        assert_eq!(
                            second_nan.status, 1,
                            "MIN/MAX NaN must suppress src1 DE; DAZ={daz}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn packed_arithmetic_nan_precedence_is_lane_local_when_status_is_aggregated() {
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        // Lane 0 returns QNaN and must suppress its same-lane denormal. Lane 1
        // has no NaN and independently contributes DE to the packed result.
        SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 32, 0x7FC1_2345);
        SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 32, 1);
        SmirInterpreter::set_lane(&mut x86.xmm[1], 1, 32, u64::from(1.0f32.to_bits()));
        SmirInterpreter::set_lane(&mut x86.xmm[2], 1, 32, 1);
        x86.mxcsr = 0x1F80;
    }

    let exit = execute_lifted_x86(&[0x0F, 0x59, 0xCA], &mut context, &mut FlatMemory::new(1));
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 32), 0x7FC1_2345);
    assert_eq!(x86.mxcsr & 0x3F, 1 << 1);
}
