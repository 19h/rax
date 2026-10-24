//! Half precision-conversion and refinement controls and complete aliased register effects.
use rax::isa::arm::aarch32::vfp::Fpscr;
use rax::isa::arm::decoder::{Aarch32Decoder, ThumbDecoder};
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};

fn check(words: [u32; 2], inputs: &[(usize, u64)], control: u32, result: &[u64], status: u32) {
    for thumb in [false, true] {
        let mut cpu = Armv7Cpu::new();
        let mut memory = FlatMemory::new(4096, 0);
        cpu.cpsr.t = thumb;
        cpu.vfp.dregs.fill(0xcafebabe_deadbeef);
        for &(reg, bits) in inputs {
            cpu.vfp.dregs[reg] = bits;
        }
        let initial = control | 0xa800_0000;
        cpu.vfp.fpscr = Fpscr::from_bits(initial);
        let cpsr = cpu.cpsr.to_u32();
        let gprs = cpu.regs;
        let mut expected = cpu.vfp.dregs;
        expected[..result.len()].copy_from_slice(result);
        let insn = if thumb {
            ThumbDecoder::decode_32bit(words[1])
        } else {
            Aarch32Decoder::decode(words[0])
        }
        .unwrap();
        assert!(matches!(
            Executor::new(&mut cpu, &mut memory).execute(&insn),
            ExecResult::Continue
        ));
        assert_eq!(cpu.vfp.dregs, expected, "Thumb={thumb}");
        assert_eq!(cpu.vfp.fpscr.bits(), initial | status, "Thumb={thumb}");
        assert_eq!(cpu.cpsr.to_u32(), cpsr);
        assert_eq!(cpu.regs, gprs);
    }
}

#[test]
fn scalar_bottom_narrow_uses_live_rounding_and_preserves_top_storage_half() {
    check(
        [0xeeb30a41; 2],
        &[(1, 0x12345678_3f801000)],
        1 << 22,
        &[0xcafebabe_dead3c01],
        0x10,
    );
}
#[test]
fn scalar_top_narrow_quiet_nan_keeps_invalid_clear_and_preserves_bottom() {
    check(
        [0xeeb30ac1; 2],
        &[(1, 0x12345678_ffc12345)],
        0,
        &[0xcafebabe_fe09beef],
        0,
    );
}
#[test]
fn scalar_alternative_narrow_infinity_is_maximum_finite_invalid() {
    check(
        [0xeeb30a41; 2],
        &[(1, 0x12345678_7f800000)],
        1 << 26,
        &[0xcafebabe_dead7fff],
        1,
    );
}
#[test]
fn scalar_top_widen_quiets_signaling_nan_and_accumulates_invalid() {
    check(
        [0xeeb20ac1; 2],
        &[(1, 0x12345678_7c01beef)],
        1 << 19,
        &[0xcafebabe_7fc02000],
        1,
    );
}
#[test]
fn scalar_alternative_widen_exponent_31_is_finite_despite_dn() {
    check(
        [0xeeb20a41; 2],
        &[(1, 0x12345678_beef7c01)],
        (1 << 26) | (1 << 25) | (1 << 19),
        &[0xcafebabe_47802000],
        0,
    );
}
#[test]
fn scalar_integer_to_half_ignores_ahp_and_clears_upper_s_half() {
    check(
        [0xeeb809c1; 2],
        &[(1, 0x12345678_00010000)],
        1 << 26,
        &[0xcafebabe_00007c00],
        0x14,
    );
}
#[test]
fn scalar_unsigned_to_half_uses_live_round_zero() {
    check(
        [0xeeb80941; 2],
        &[(1, 0x12345678_0000ffff)],
        3 << 22,
        &[0xcafebabe_00007bff],
        0x10,
    );
}
#[test]
fn vector_widen_uses_standard_nan_control_but_ignores_fz16() {
    check(
        [0xf3b60702, 0xffb60702],
        &[(2, 0x7c00_7e01_7c01_0001)],
        (1 << 19) | (1 << 22),
        &[0x7fc00000_33800000, 0x7f800000_7fc00000],
        1,
    );
}
#[test]
fn high_source_vector_narrow_uses_m_bit_and_standard_controls() {
    check(
        [0xf3b60622, 0xffb60622],
        &[(18, 0x7fc12345_33800000), (19, 0x7f800000_00000001)],
        (1 << 19) | (1 << 22),
        &[0x7c00_0000_7e00_0001],
        0x80,
    );
}
#[test]
fn high_source_alternative_vector_widen_keeps_all_exponent_31_values_finite() {
    check(
        [0xf3b60722, 0xffb60722],
        &[(18, 0x7fff_7e01_7c01_7c00)],
        (1 << 19) | (1 << 26) | (1 << 22),
        &[0x47802000_47800000, 0x47ffe000_47c02000],
        0,
    );
}
#[test]
fn half_reciprocal_step_rounds_product_before_subtracting() {
    check(
        [0xf2120f54, 0xef120f54],
        &[
            (2, 0x3c01_3c01_3c01_3c01),
            (3, 0x3c01_3c01_3c01_3c01),
            (4, 0x3ffe_3ffe_3ffe_3ffe),
            (5, 0x3ffe_3ffe_3ffe_3ffe),
        ],
        (1 << 22) | (1 << 26),
        &[0, 0],
        0x10,
    );
}
#[test]
fn half_rsqrt_step_preserves_intermediate_product_overflow() {
    check(
        [0xf2320f54, 0xef320f54],
        &[
            (2, 0x7bff_7bff_7bff_7bff),
            (3, 0x7bff_7bff_7bff_7bff),
            (4, 0x4000_4000_4000_4000),
            (5, 0x4000_4000_4000_4000),
        ],
        1 << 26,
        &[0xfc00_fc00_fc00_fc00, 0xfc00_fc00_fc00_fc00],
        0x14,
    );
}
#[test]
fn half_refinement_quiet_nan_does_not_raise_invalid() {
    check(
        [0xf2120f54, 0xef120f54],
        &[
            (2, 0x7e01_7e01_7e01_7e01),
            (3, 0x7e01_7e01_7e01_7e01),
            (4, 0x3c00_3c00_3c00_3c00),
            (5, 0x3c00_3c00_3c00_3c00),
        ],
        0,
        &[0x7e00_7e00_7e00_7e00, 0x7e00_7e00_7e00_7e00],
        0,
    );
}
#[test]
fn precision_conversion_exhausts_register_fields_and_source_destination_aliases() {
    let mut count = 0;
    for thumb in [false, true] {
        for narrow in [false, true] {
            let destinations = if narrow { 32 } else { 16 };
            let sources = if narrow { 16 } else { 32 };
            for dst in 0..destinations {
                for src in 0..sources {
                    let d = if narrow { dst } else { dst * 2 };
                    let m = if narrow { src * 2 } else { src };
                    let base = if narrow { 0xf3b60600 } else { 0xf3b60700 };
                    let raw = base
                        | ((d & 15) << 12)
                        | (((d >> 4) & 1) << 22)
                        | (m & 15)
                        | (((m >> 4) & 1) << 5);
                    let mut cpu = Armv7Cpu::new();
                    let mut mem = FlatMemory::new(4096, 0);
                    cpu.cpsr.t = thumb;
                    cpu.vfp.dregs.fill(if narrow {
                        0x3f800000_3f800000
                    } else {
                        0x3c00_3c00_3c00_3c00
                    });
                    let control = 0xac480000;
                    cpu.vfp.fpscr = Fpscr::from_bits(control);
                    let cpsr = cpu.cpsr.to_u32();
                    let mut expected = cpu.vfp.dregs;
                    expected[d as usize] = if narrow {
                        0x3c00_3c00_3c00_3c00
                    } else {
                        0x3f800000_3f800000
                    };
                    if !narrow {
                        expected[d as usize + 1] = 0x3f800000_3f800000;
                    }
                    let insn = if thumb {
                        ThumbDecoder::decode_32bit((raw & 0x00ffffff) | 0xff000000)
                    } else {
                        Aarch32Decoder::decode(raw)
                    }
                    .unwrap();
                    assert!(
                        matches!(
                            Executor::new(&mut cpu, &mut mem).execute(&insn),
                            ExecResult::Continue
                        ),
                        "raw={raw:08x} Thumb={thumb}"
                    );
                    assert_eq!(cpu.vfp.dregs, expected, "raw={raw:08x} Thumb={thumb}");
                    assert_eq!(cpu.vfp.fpscr.bits(), control);
                    assert_eq!(cpu.cpsr.to_u32(), cpsr);
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 2048);
}
#[test]
fn missing_fpscr_representation_keeps_half_conversion_and_refinement_out_of_the_lifters() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::aarch32::Aarch32Lifter;
    use rax::smir::lift::thumb::ThumbLifter;
    use rax::smir::lift::{LiftContext, LiftError, SmirLifter};
    for words in [
        [0xeeb30a41_u32; 2],
        [0xeeb30ac1; 2],
        [0xeeb20a41; 2],
        [0xeeb20ac1; 2],
        [0xeeb809c1; 2],
        [0xeeb80941; 2],
        [0xf3b60622, 0xffb60622],
        [0xf3b60722, 0xffb60722],
        [0xf2120f54, 0xef120f54],
        [0xf2320f54, 0xef320f54],
    ] {
        for thumb in [false, true] {
            let mut lifter: Box<dyn SmirLifter> = if thumb {
                Box::new(ThumbLifter::new())
            } else {
                Box::new(Aarch32Lifter::new())
            };
            let arch = if thumb {
                SourceArch::Thumb
            } else {
                SourceArch::Aarch32
            };
            let mut context = LiftContext::new(arch);
            let bytes = if thumb {
                let high = ((words[1] >> 16) as u16).to_le_bytes();
                let low = (words[1] as u16).to_le_bytes();
                [high[0], high[1], low[0], low[1]]
            } else {
                words[0].to_le_bytes()
            };
            assert!(matches!(
                lifter.lift_insn(0x1000, &bytes, &mut context),
                Err(LiftError::Unsupported { addr: 0x1000, .. })
            ));
        }
    }
}
