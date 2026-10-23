use rax::isa::arm::aarch32::vfp::{self, Fpscr};
#[test]
fn minmax_preserves_equal_nonzero_values() {
    for bits in [0x3f800000_u32, 0xbf800000, 0x7f800000, 0xff800000, 1] {
        let mut fpscr = Fpscr::from_bits(0);
        for op in [vfp::vmaxnm_f32, vfp::vminnm_f32] {
            assert_eq!(
                op(f32::from_bits(bits), f32::from_bits(bits), &mut fpscr).to_bits(),
                bits
            );
            assert_eq!(fpscr.bits(), 0);
        }
    }
    for bits in [
        0x3ff0000000000000_u64,
        0xbff0000000000000,
        0x7ff0000000000000,
        0xfff0000000000000,
        1,
    ] {
        let mut fpscr = Fpscr::from_bits(0);
        for op in [vfp::vmaxnm_f64, vfp::vminnm_f64] {
            assert_eq!(
                op(f64::from_bits(bits), f64::from_bits(bits), &mut fpscr).to_bits(),
                bits
            );
            assert_eq!(fpscr.bits(), 0);
        }
    }
}
#[test]
fn minmax_numeric_quiet_nan_and_signaling_nan_rules() {
    for op in [vfp::vmaxnm_f32, vfp::vminnm_f32] {
        for (a, b, expected, status) in [
            (0x7fc01234, 0x3f800000, 0x3f800000, 0),
            (0x3f800000, 0x7fc01234, 0x3f800000, 0),
            (0x7f801234, 0x3f800000, 0x7fc01234, 1),
            (0x7fc01234, 0xff801234, 0xffc01234, 1),
            (0xffc04321, 0x7fc01234, 0xffc04321, 0),
        ] {
            let mut fpscr = Fpscr::from_bits(0);
            assert_eq!(
                (
                    op(f32::from_bits(a), f32::from_bits(b), &mut fpscr).to_bits(),
                    fpscr.bits()
                ),
                (expected, status)
            );
        }
    }
}
#[test]
fn minmax_signed_zeros_and_flush_inputs() {
    for (op, expected) in [
        (vfp::vmaxnm_f32 as fn(f32, f32, &mut Fpscr) -> f32, 0),
        (vfp::vminnm_f32, 0x80000000),
    ] {
        for (a, b) in [(0, 0x80000000), (0x80000000, 0)] {
            let mut fpscr = Fpscr::from_bits(0);
            assert_eq!(
                op(f32::from_bits(a), f32::from_bits(b), &mut fpscr).to_bits(),
                expected
            );
            let mut fpscr = Fpscr::from_bits(1 << 24);
            let value = op(f32::from_bits(a | 1), f32::from_bits(b | 1), &mut fpscr);
            assert_eq!(
                (value.to_bits(), fpscr.bits()),
                (expected, (1 << 24) | 0x80)
            );
        }
    }
}
#[test]
fn minmax_default_nan_and_sticky_status() {
    for op in [vfp::vmaxnm_f64, vfp::vminnm_f64] {
        let mut fpscr = Fpscr::from_bits((1 << 25) | (1 << 4));
        let value = op(
            f64::from_bits(0xfff8000000001234),
            f64::from_bits(0x7ff8000000004321),
            &mut fpscr,
        );
        assert_eq!(
            (value.to_bits(), fpscr.bits()),
            (0x7ff8000000000000, (1 << 25) | (1 << 4))
        );
    }
}
#[test]
fn compare_flushes_subnormals_and_preserves_controls() {
    let mut fpscr = Fpscr::from_bits((1 << 24) | 0x10);
    vfp::vcmp_f32(f32::from_bits(1), 0.0, &mut fpscr);
    assert_eq!(fpscr.bits(), 0x60000000 | (1 << 24) | 0x90);
    let mut fpscr = Fpscr::from_bits((1 << 24) | 0x10);
    vfp::vcmp_f64(f64::from_bits(0x8000000000000001), 0.0, &mut fpscr);
    assert_eq!(fpscr.bits(), 0x60000000 | (1 << 24) | 0x90);
}
#[test]
fn compare_nan_exception_modes_and_input_flush_both_apply() {
    for (signal, expected) in [(false, 0), (true, 1)] {
        let mut fpscr = Fpscr::from_bits(1 << 24);
        vfp::vcmp_f32_with_exception(
            f32::from_bits(0x7fc01234),
            f32::from_bits(1),
            signal,
            &mut fpscr,
        );
        assert_eq!(fpscr.bits(), 0x30000000 | (1 << 24) | 0x80 | expected);
    }
}

fn neon(word: u32, first: u32, second: u32, pairwise: bool) -> (u64, u32) {
    use rax::isa::arm::decoder::Aarch32Decoder;
    use rax::isa::arm::execution::FlatMemory;
    use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};
    let mut cpu = Armv7Cpu::new();
    let mut memory = FlatMemory::new(0x1000, 0);
    let repeated = |bits: u32| u64::from(bits) | (u64::from(bits) << 32);
    let pair = u64::from(first) | (u64::from(second) << 32);
    cpu.vfp
        .write_d_bits(1, if pairwise { pair } else { repeated(first) });
    cpu.vfp
        .write_d_bits(2, if pairwise { pair } else { repeated(second) });
    cpu.vfp.fpscr = Fpscr::from_bits(0xa8400000);
    let instruction = Aarch32Decoder::decode(word).unwrap();
    assert!(matches!(
        Executor::new(&mut cpu, &mut memory).execute(&instruction),
        ExecResult::Continue
    ));
    (cpu.vfp.read_d_bits(0), cpu.vfp.fpscr.bits())
}

#[test]
fn neon_minmax_and_pairwise_preserve_values_use_standard_controls_and_raise_flags() {
    for (word, pairwise, maximum) in [
        (0xf2010f02, false, true),
        (0xf2210f02, false, false),
        (0xf3010f02, true, true),
        (0xf3210f02, true, false),
    ] {
        for (a, b, expected, flags) in [
            (0x3f800000, 0x3f800000, 0x3f800000_u32, 0),
            (0xbf800000, 0xbf800000, 0xbf800000, 0),
            (0x7fc01234, 0x3f800000, 0x7fc00000, 0),
            (0x7f801234, 0x3f800000, 0x7fc00000, 1),
            (1, 0, 0, 0x80),
            (0x80000000, 0, if maximum { 0 } else { 0x80000000 }, 0),
        ] {
            assert_eq!(
                neon(word, a, b, pairwise),
                (
                    u64::from(expected) | (u64::from(expected) << 32),
                    0xa8400000 | flags
                ),
                "word={word:x} a={a:x} b={b:x}"
            );
        }
    }
}

#[test]
fn neon_comparisons_flush_inputs_and_distinguish_quiet_and_signaling_nan_modes() {
    for (word, a, b, mask, flags) in [
        (0xf2010e02, 1, 0, 0xffffffff_u32, 0x80),
        (0xf2010e02, 0x7fc01234, 0, 0, 0),
        (0xf2010e02, 0x7f801234, 0, 0, 1),
        (0xf3010e02, 0x7fc01234, 0, 0, 1),
        (0xf3210e02, 0x7fc01234, 0, 0, 1),
        (0xf3010e12, 0xbf800000, 0x3f800000, 0xffffffff, 0),
    ] {
        assert_eq!(
            neon(word, a, b, false),
            (
                u64::from(mask) | (u64::from(mask) << 32),
                0xa8400000 | flags
            ),
            "word={word:x}"
        );
    }
}

#[test]
fn neon_absolute_difference_uses_standard_rounding_and_accumulates_exceptions() {
    for (a, b, expected, flags) in [
        (0x3f800000, 0xb3800000, 0x3f800000_u32, 0x10),
        (1, 0, 0, 0x80),
        (0x7f800000, 0x7f800000, 0x7fc00000, 1),
        (0x7fc01234, 0, 0x7fc00000, 0),
    ] {
        assert_eq!(
            neon(0xf3210d02, a, b, false),
            (
                u64::from(expected) | (u64::from(expected) << 32),
                0xa8400000 | flags
            )
        );
    }
}
