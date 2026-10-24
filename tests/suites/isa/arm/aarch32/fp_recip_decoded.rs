//! Reciprocal estimate and step controls and complete aliased register effects.
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
    }
}

#[test]
fn reciprocal_estimate_flushes_inputs_and_reports_nan_zero_status() {
    check(
        [0xf3bb0542, 0xffbb0542],
        &[(2, 0x80000001_00000001), (3, 0x7f812345_00000000)],
        1 << 22,
        &[0xff800000_7f800000, 0x7fc00000_7f800000],
        0x83,
    );
}
#[test]
fn reciprocal_estimate_flushes_tiny_results_and_reports_underflow() {
    check(
        [0xf3bb0501, 0xffbb0501],
        &[(1, 0xff7fffff_7e800000)],
        0,
        &[0x80000000_00000000],
        8,
    );
}
#[test]
fn reciprocal_sqrt_negative_and_signaling_inputs_set_invalid() {
    check(
        [0xf3bb05c2, 0xffbb05c2],
        &[(2, 0xff800000_bf800000), (3, 0x7fc12345_7f812345)],
        2 << 22,
        &[0x7fc00000_7fc00000, 0x7fc00000_7fc00000],
        1,
    );
}
#[test]
fn reciprocal_step_rounds_product_before_subtraction() {
    check(
        [0xf2010f12, 0xef010f12],
        &[(1, 0x7f800000_3f800001), (2, 0x00000000_3ffffffe)],
        1 << 22,
        &[0x40000000_00000000],
        0x10,
    );
}
#[test]
fn reciprocal_sqrt_step_rounds_product_before_halved_subtraction() {
    check(
        [0xf2210f12, 0xef210f12],
        &[(1, 0x00000001_3f800001), (2, 0x7f800000_403fffff)],
        2 << 22,
        &[0x3fc00000_00000000],
        0x90,
    );
}
#[test]
fn reciprocal_step_accumulates_product_overflow_and_quiets_signaling_nan() {
    check(
        [0xf2010f12, 0xef010f12],
        &[(1, 0x7f812345_7f7fffff), (2, 0x3f800000_40000000)],
        3 << 22,
        &[0x7fc00000_ff800000],
        0x15,
    );
}
#[test]
fn reciprocal_step_zero_infinity_is_exact_with_no_invalid() {
    check(
        [0xf2010f12, 0xef010f12],
        &[(1, 0x80000000_7f800000), (2, 0xff800000_00000000)],
        0x9f,
        &[0x40000000_40000000],
        0,
    );
}
#[test]
fn integer_estimates_preserve_floating_status_and_controls() {
    check(
        [0xf3bb0401, 0xffbb0401],
        &[(1, 0x80000000_00000000)],
        (1 << 22) | 0x9f,
        &[0xff800000_ffffffff],
        0,
    );
}
#[test]
fn missing_fpscr_representation_keeps_reciprocal_operations_out_of_the_lifters() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::aarch32::Aarch32Lifter;
    use rax::smir::lift::thumb::ThumbLifter;
    use rax::smir::lift::{LiftContext, LiftError, SmirLifter};
    for words in [
        [0xf3bb0542_u32, 0xffbb0542],
        [0xf3bb05c2, 0xffbb05c2],
        [0xf2010f12, 0xef010f12],
        [0xf2210f12, 0xef210f12],
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
