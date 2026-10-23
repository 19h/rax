use rax::isa::arm::aarch32::vfp::{self, Fpscr};
use rax::isa::arm::decoder::Aarch32Decoder;
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};

macro_rules! contract32 {
    ($name:ident, $helper:ident, $acc:expr, $a:expr, $b:expr, $control:expr, $bits:expr, $flags:expr) => {
        #[test]
        fn $name() {
            let mut fpscr = Fpscr::from_bits($control);
            let value = vfp::$helper(
                f32::from_bits($acc),
                f32::from_bits($a),
                f32::from_bits($b),
                &mut fpscr,
            );
            assert_eq!((value.to_bits(), fpscr.bits()), ($bits, $control | $flags));
        }
    };
}
contract32!(
    infinite_accumulator_is_not_overflow,
    vfma_f32,
    0x7f800000,
    0x3f800000,
    0x3f800000,
    0,
    0x7f800000,
    0
);
contract32!(
    quiet_nan_is_not_invalid,
    vfma_f32,
    0x7fc01234,
    0x3f800000,
    0x3f800000,
    0,
    0x7fc01234,
    0
);
contract32!(
    quiet_addend_does_not_hide_zero_times_infinity,
    vfma_f32,
    0xffc01234,
    0,
    0x7f800000,
    0,
    0x7fc00000,
    1
);
contract32!(
    signaling_product_nan_precedes_quiet_addend,
    vfma_f32,
    0x7fc01234,
    0xff801234,
    0x3f800000,
    0,
    0xffc01234,
    1
);
contract32!(
    first_nan_priority_starts_with_addend,
    vfma_f32,
    0xffc04321,
    0x7fc01234,
    0x3f800000,
    0,
    0xffc04321,
    0
);
contract32!(
    default_nan_discards_payload,
    vfma_f32,
    0xffc04321,
    0x3f800000,
    0x3f800000,
    1 << 25,
    0x7fc00000,
    0
);
contract32!(
    guest_rounding_applies_once,
    vfma_f32,
    0x3f800000,
    0x33800000,
    0x3f800000,
    1 << 22,
    0x3f800001,
    0x10
);
contract32!(
    fused_residual_survives,
    vfma_f32,
    0xbf800000,
    0x3f800001,
    0x3f7ffffe,
    0,
    0xa8800000,
    0
);
contract32!(
    flush_inputs_retains_input_denormal_status,
    vfma_f32,
    0x3f800000,
    1,
    0x3f800000,
    1 << 24,
    0x3f800000,
    0x80
);
contract32!(
    flush_exact_tiny_output_omits_inexact,
    vfma_f32,
    0,
    0x00800000,
    0x3f000000,
    1 << 24,
    0,
    8
);
contract32!(
    subtract_nan_negates_first_operand_before_processing,
    vfms_f32,
    0x3f800000,
    0x7fc01234,
    0x3f800000,
    0,
    0xffc01234,
    0
);
contract32!(
    negated_addend_sign_is_processed_before_nan,
    vfnms_f32,
    0x7fc01234,
    0x3f800000,
    0x3f800000,
    0,
    0xffc01234,
    0
);

#[test]
fn binary64_rounding_status_and_fused_residual() {
    for (acc, a, b, control, bits, flags) in [
        (
            0x3ff0000000000000,
            0x3ca0000000000000,
            0x3ff0000000000000,
            1 << 22,
            0x3ff0000000000001,
            0x10,
        ),
        (
            0xbff0000000000000,
            0x3ff0000000000001,
            0x3feffffffffffffe,
            0,
            0xb970000000000000,
            0,
        ),
        (0, 0x0010000000000000, 0x3fe0000000000000, 1 << 24, 0, 8),
    ] {
        let mut fpscr = Fpscr::from_bits(control);
        let actual = vfp::vfma_f64(
            f64::from_bits(acc),
            f64::from_bits(a),
            f64::from_bits(b),
            &mut fpscr,
        );
        assert_eq!((actual.to_bits(), fpscr.bits()), (bits, control | flags));
    }
}

#[test]
fn decoded_fused_forms_use_guest_rounding_and_accumulator() {
    for (word, acc, a, b, bits) in [
        (0xeea00a81, 0x3f800000, 0x33800000, 0x3f800000, 0x3f800001),
        (0xeea00ac1, 0x3f800000, 0xb3800000, 0x3f800000, 0x3f800001),
        (0xee900ac1, 0xbf800000, 0xb3800000, 0x3f800000, 0x3f800001),
        (0xee900a81, 0xbf800000, 0x33800000, 0x3f800000, 0x3f800001),
    ] {
        let mut cpu = Armv7Cpu::new();
        let mut memory = FlatMemory::new(0x1000, 0);
        cpu.vfp.write_s_bits(0, acc);
        cpu.vfp.write_s_bits(1, a);
        cpu.vfp.write_s_bits(2, b);
        cpu.vfp.fpscr = Fpscr::from_bits(1 << 22);
        let instruction = Aarch32Decoder::decode(word).unwrap();
        assert!(matches!(
            Executor::new(&mut cpu, &mut memory).execute(&instruction),
            ExecResult::Continue
        ));
        assert_eq!(
            (cpu.vfp.read_s_bits(0), cpu.vfp.fpscr.bits()),
            (bits, (1 << 22) | 0x10)
        );
    }
}

#[test]
fn decoded_binary64_fused_forms_obey_guest_rounding() {
    for (word, acc, a) in [
        (0xeea10b02, 0x3ff0000000000000, 0x3ca0000000000000),
        (0xeea10b42, 0x3ff0000000000000, 0xbca0000000000000),
        (0xee910b42, 0xbff0000000000000, 0xbca0000000000000),
        (0xee910b02, 0xbff0000000000000, 0x3ca0000000000000),
    ] {
        let mut cpu = Armv7Cpu::new();
        let mut memory = FlatMemory::new(0x1000, 0);
        cpu.vfp.write_d_bits(0, acc);
        cpu.vfp.write_d_bits(1, a);
        cpu.vfp.write_d_bits(2, 0x3ff0000000000000);
        cpu.vfp.fpscr = Fpscr::from_bits(1 << 22);
        let instruction = Aarch32Decoder::decode(word).unwrap();
        assert!(matches!(
            Executor::new(&mut cpu, &mut memory).execute(&instruction),
            ExecResult::Continue
        ));
        assert_eq!(
            (cpu.vfp.read_d_bits(0), cpu.vfp.fpscr.bits()),
            (0x3ff0000000000001, (1 << 22) | 0x10)
        );
    }
}

#[test]
fn unrepresented_fused_and_selection_state_stays_out_of_smir_admission() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::aarch32::Aarch32Lifter;
    use rax::smir::lift::{LiftContext, LiftError, SmirLifter};
    for word in [
        0xeea00a81_u32,
        0xeea00ac1,
        0xee900ac1,
        0xee900a81,
        0xeea10b02,
        0xeea10b42,
        0xee910b42,
        0xee910b02,
        0xfe800a81,
        0xfe800ac1,
    ] {
        let mut lifter = Aarch32Lifter::new();
        let mut context = LiftContext::new(SourceArch::Aarch32);
        assert!(matches!(
            lifter.lift_insn(0x1000, &word.to_le_bytes(), &mut context),
            Err(LiftError::Unsupported { addr: 0x1000, .. })
        ));
    }
}
