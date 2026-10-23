use rax::isa::arm::aarch32::vfp::{self, Fpscr};
use rax::isa::arm::decoder::Aarch32Decoder;
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, Executor};

#[test]
fn unrepresented_vfp_state_is_rejected_by_the_a32_lifter() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::aarch32::Aarch32Lifter;
    use rax::smir::lift::{LiftContext, LiftError, SmirLifter};

    for word in [0xee30_0a20_u32, 0xee80_0a20] {
        let mut lifter = Aarch32Lifter::new();
        let mut context = LiftContext::new(SourceArch::Aarch32);
        assert!(matches!(
            lifter.lift_insn(0x1000, &word.to_le_bytes(), &mut context),
            Err(LiftError::Unsupported { addr: 0x1000, .. })
        ));
    }
}

macro_rules! case {
    ($helper:ident, $decoded:ident, $operation:ident, $code:expr, $a:expr, $b:expr, $control:expr, $result:expr, $status:expr) => {
        #[test]
        fn $helper() {
            let mut fpscr = Fpscr::from_bits($control);
            let value = vfp::$operation(f32::from_bits($a), f32::from_bits($b), &mut fpscr);
            assert_eq!(
                (value.to_bits(), fpscr.bits()),
                ($result, $control | $status)
            );
        }
        #[test]
        fn $decoded() {
            let mut cpu = Armv7Cpu::new();
            let mut memory = FlatMemory::new(0x1000, 0);
            cpu.vfp.write_s_bits(0, $a);
            cpu.vfp.write_s_bits(1, $b);
            cpu.vfp.fpscr = Fpscr::from_bits($control);
            let instruction = Aarch32Decoder::decode($code).unwrap();
            let result = Executor::new(&mut cpu, &mut memory).execute(&instruction);
            assert!(
                matches!(
                    result,
                    rax::isa::arm::aarch32::instructions::ExecResult::Continue
                ),
                "{result:?}"
            );
            assert_eq!(
                (cpu.vfp.read_s_bits(0), cpu.vfp.fpscr.bits()),
                ($result, $control | $status)
            );
        }
    };
}
case!(
    infinity_helper,
    infinity_decoded,
    vadd_f32,
    0xee300a20,
    0x7f800000,
    0x3f800000,
    0,
    0x7f800000,
    0
);
case!(
    quiet_nan_helper,
    quiet_nan_decoded,
    vadd_f32,
    0xee300a20,
    0x7fc00000,
    0x3f800000,
    0,
    0x7fc00000,
    0
);
case!(
    divide_zero_helper,
    divide_zero_decoded,
    vdiv_f32,
    0xee800a20,
    0x3f800000,
    0,
    0,
    0x7f800000,
    2
);
case!(
    invalid_zero_helper,
    invalid_zero_decoded,
    vdiv_f32,
    0xee800a20,
    0,
    0,
    0,
    0x7fc00000,
    1
);
case!(
    round_up_helper,
    round_up_decoded,
    vadd_f32,
    0xee300a20,
    0x3f800000,
    0x33800000,
    0x00400000,
    0x3f800001,
    0x10
);
case!(
    flush_input_helper,
    flush_input_decoded,
    vadd_f32,
    0xee300a20,
    1,
    0,
    0x01000000,
    0,
    0x80
);

macro_rules! double_case {
    ($name:ident, $operation:ident, $a:expr, $b:expr, $control:expr, $result:expr, $status:expr) => {
        #[test]
        fn $name() {
            let mut fpscr = Fpscr::from_bits($control);
            let value = vfp::$operation(f64::from_bits($a), f64::from_bits($b), &mut fpscr);
            assert_eq!(
                (value.to_bits(), fpscr.bits()),
                ($result, $control | $status)
            );
        }
    };
}
double_case!(
    infinity_double,
    vadd_f64,
    0x7ff0000000000000,
    0x3ff0000000000000,
    0,
    0x7ff0000000000000,
    0
);
double_case!(
    quiet_nan_double,
    vadd_f64,
    0x7ff8000000000000,
    0x3ff0000000000000,
    0,
    0x7ff8000000000000,
    0
);
double_case!(
    divide_zero_double,
    vdiv_f64,
    0x3ff0000000000000,
    0,
    0,
    0x7ff0000000000000,
    2
);
double_case!(
    invalid_zero_double,
    vdiv_f64,
    0,
    0,
    0,
    0x7ff8000000000000,
    1
);
double_case!(
    round_up_double,
    vadd_f64,
    0x3ff0000000000000,
    0x3ca0000000000000,
    0x00400000,
    0x3ff0000000000001,
    0x10
);
double_case!(flush_input_double, vadd_f64, 1, 0, 0x01000000, 0, 0x80);

#[test]
fn rounding_modes_and_signs_apply_to_binary32_and_binary64() {
    for mode in 0..4 {
        for negative in [false, true] {
            let control = mode << 22;
            let round_away = if negative { mode == 2 } else { mode == 1 };
            let sign32 = if negative { 1 << 31 } else { 0 };
            let sign64 = if negative { 1 << 63 } else { 0 };
            let mut fpscr = Fpscr::from_bits(control);
            let result = vfp::vadd_f32(
                f32::from_bits(sign32 | 0x3f800000),
                f32::from_bits(sign32 | 0x33800000),
                &mut fpscr,
            );
            assert_eq!(
                result.to_bits(),
                sign32 | (0x3f800000 + u32::from(round_away))
            );
            assert_eq!(fpscr.bits(), control | 0x10);
            let mut fpscr = Fpscr::from_bits(control);
            let result = vfp::vadd_f64(
                f64::from_bits(sign64 | 0x3ff0000000000000),
                f64::from_bits(sign64 | 0x3ca0000000000000),
                &mut fpscr,
            );
            assert_eq!(
                result.to_bits(),
                sign64 | (0x3ff0000000000000 + u64::from(round_away))
            );
            assert_eq!(fpscr.bits(), control | 0x10);
        }
    }
}

#[test]
fn nan_priority_payload_and_input_flush_follow_arm_rules() {
    for (a, b, control, expected, status) in [
        (0xffc12345, 0x3f800000, 0, 0xffc12345, 0),
        (0xffc12345, 0x7f812345, 0, 0x7fc12345, 1),
        (0xff812345, 0x7f854321, 0, 0xffc12345, 1),
        (0xffc12345, 0x3f800000, 1 << 25, 0x7fc00000, 0),
        (0xff812345, 0x3f800000, 1 << 25, 0x7fc00000, 1),
        (0x7fc12345, 1, 1 << 24, 0x7fc12345, 0x80),
    ] {
        let mut fpscr = Fpscr::from_bits(control);
        let result = vfp::vadd_f32(f32::from_bits(a), f32::from_bits(b), &mut fpscr);
        assert_eq!(
            (result.to_bits(), fpscr.bits()),
            (expected, control | status)
        );
    }
    let mut fpscr = Fpscr::default();
    let result = vfp::vsub_f32(1.0, f32::from_bits(0x7fc12345), &mut fpscr);
    assert_eq!(
        result.to_bits(),
        0x7fc12345,
        "NaN propagation precedes operand negation"
    );
}

#[test]
fn finite_overflow_obeys_rounding_and_reports_inexact() {
    for mode in 0..4 {
        for negative in [false, true] {
            let sign = if negative { 1 << 31 } else { 0 };
            let to_infinity = mode == 0 || (mode == 1 && !negative) || (mode == 2 && negative);
            let mut fpscr = Fpscr::from_bits(mode << 22);
            let result = vfp::vmul_f32(f32::from_bits(sign | 0x7f7fffff), 2.0, &mut fpscr);
            assert_eq!(
                result.to_bits(),
                sign | if to_infinity { 0x7f800000 } else { 0x7f7fffff }
            );
            assert_eq!(fpscr.bits(), (mode << 22) | 0x14);
        }
    }
}

#[test]
fn underflow_is_detected_before_rounding_and_fz_omits_inexact() {
    for (a, b, control, expected, status) in [
        (0x00800000, 0x3f000000, 0, 0x00400000, 0),
        (0x00800000, 0x3f000000, 1 << 24, 0, 8),
        (0x00800000, 0x3f7fffff, 0, 0x00800000, 0x18),
        (0x00800000, 0x3f7fffff, 1 << 24, 0, 8),
        (1, 0x3f800000, 0, 1, 0),
        (1, 0x3f800000, 1 << 24, 0, 0x80),
    ] {
        let mut fpscr = Fpscr::from_bits(control);
        let result = vfp::vmul_f32(f32::from_bits(a), f32::from_bits(b), &mut fpscr);
        assert_eq!(
            (result.to_bits(), fpscr.bits()),
            (expected, control | status)
        );
    }
    let mut fpscr = Fpscr::default();
    assert_eq!(
        vfp::vdiv_f32(f32::from_bits(1), 2.0, &mut fpscr).to_bits(),
        0
    );
    assert_eq!(fpscr.bits(), 0x18);
}

#[test]
fn square_root_classifies_inputs_and_rounds_without_host_arithmetic() {
    for (input, control, expected, status) in [
        (0x40800000, 0, 0x40000000, 0),
        (0x40000000, 0, 0x3fb504f3, 0x10),
        (0x40000000, 1 << 22, 0x3fb504f4, 0x10),
        (0xbf800000, 0, 0x7fc00000, 1),
        (0x80000000, 0, 0x80000000, 0),
        (0x7fc12345, 0, 0x7fc12345, 0),
    ] {
        let mut fpscr = Fpscr::from_bits(control);
        let result = vfp::vsqrt_f32(f32::from_bits(input), &mut fpscr);
        assert_eq!(
            (result.to_bits(), fpscr.bits()),
            (expected, control | status)
        );
    }
}

#[test]
fn cumulative_status_and_unrelated_fpscr_fields_are_preserved() {
    let before = 0xa8000085;
    let mut fpscr = Fpscr::from_bits(before);
    vfp::vdiv_f32(1.0, 3.0, &mut fpscr);
    assert_eq!(fpscr.bits(), before | 0x10);
}

#[test]
fn nonfused_negation_occurs_before_the_final_add_rounding() {
    let mut fpscr = Fpscr::from_bits(1 << 22);
    let result = vfp::vnmla_f32(1.0, f32::from_bits(0x33800000), 1.0, &mut fpscr);
    assert_eq!(
        (result.to_bits(), fpscr.bits()),
        (0xbf800000, (1 << 22) | 0x10)
    );
    let mut fpscr = Fpscr::default();
    let result = vfp::vmla_f32(
        f32::NEG_INFINITY,
        f32::from_bits(0x7f7fffff),
        2.0,
        &mut fpscr,
    );
    assert_eq!((result.to_bits(), fpscr.bits()), (0x7fc00000, 0x15));
}

#[test]
fn neon_uses_standard_controls_and_retains_the_guest_control_register() {
    for (a, b, expected, status) in [
        (0x3f800000_u32, 0x33800000_u32, 0x3f800000_u32, 0x10),
        (0x7fc12345, 0x3f800000, 0x7fc00000, 0),
        (1, 0, 0, 0x80),
    ] {
        let mut cpu = Armv7Cpu::new();
        let mut memory = FlatMemory::new(0x1000, 0);
        let control = (2 << 22) | (1 << 27);
        cpu.vfp.fpscr = Fpscr::from_bits(control);
        cpu.vfp.write_d_bits(1, u64::from(a) | (u64::from(a) << 32));
        cpu.vfp.write_d_bits(2, u64::from(b) | (u64::from(b) << 32));
        let instruction = Aarch32Decoder::decode(0xf2010d02).unwrap();
        assert!(matches!(
            Executor::new(&mut cpu, &mut memory).execute(&instruction),
            rax::isa::arm::ExecResult::Continue
        ));
        assert_eq!(
            cpu.vfp.read_d_bits(0),
            u64::from(expected) | (u64::from(expected) << 32)
        );
        assert_eq!(cpu.vfp.fpscr.bits(), control | status);
    }
}
