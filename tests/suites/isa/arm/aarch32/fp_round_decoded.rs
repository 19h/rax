//! Round-to-integral instruction controls and complete aliased register effects.
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
        let initial = control | 0xa800_0004;
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
fn scalar_quiet_nan_preserves_payload_without_invalid() {
    check(
        [0xfeb90a41, 0xfeb90a41],
        &[(1, 0x12345678_7fc12345)],
        0,
        &[0xcafebabe_7fc12345],
        0,
    );
}
#[test]
fn scalar_signaling_nan_quiets_and_uses_default_nan_control() {
    check(
        [0xfeb90a41, 0xfeb90a41],
        &[(1, 0x12345678_7f812345)],
        1 << 25,
        &[0xcafebabe_7fc00000],
        1,
    );
}
#[test]
fn scalar_exact_rounding_uses_live_round_up() {
    check(
        [0xeeb70a41; 2],
        &[(1, 0x12345678_40100000)],
        1 << 22,
        &[0xcafebabe_40400000],
        0x10,
    );
}
#[test]
fn vector_exact_rounding_uses_nearest_even_despite_live_round_up() {
    check(
        [0xf3ba04c2, 0xffba04c2],
        &[(2, 0x40300000_40100000), (3, 0xc0300000_c0100000)],
        1 << 22,
        &[0x40400000_40000000, 0xc0400000_c0000000],
        0x10,
    );
}
#[test]
fn vector_rounding_uses_standard_nan_and_flush_controls() {
    check(
        [0xf3ba0442, 0xffba0442],
        &[(2, 0x7f812345_7fc12345), (3, 0x80000001_00000001)],
        2 << 22,
        &[0x7fc00000_7fc00000, 0x80000000_00000000],
        0x81,
    );
}
#[test]
fn scalar_double_rounding_does_not_saturate_to_integer32() {
    check(
        [0xeeb60bc1; 2],
        &[(1, 0x42f0000000000008)],
        0,
        &[0x42f0000000000000],
        0,
    );
}
#[test]
fn scalar_input_flush_preserves_negative_zero_and_suppresses_inexact() {
    check(
        [0xeeb70a41; 2],
        &[(1, 0x12345678_80000001)],
        (1 << 24) | (2 << 22),
        &[0xcafebabe_80000000],
        0x80,
    );
}
#[test]
fn failed_scalar_condition_preserves_registers_and_fpscr() {
    let mut cpu = Armv7Cpu::new();
    let mut memory = FlatMemory::new(4096, 0);
    cpu.cpsr.z = false;
    cpu.vfp.dregs.fill(0xcafebabe_deadbeef);
    cpu.vfp.write_s_bits(2, 0x7f812345);
    cpu.vfp.fpscr = Fpscr::from_bits(0xa8000004 | (1 << 22));
    let before = (cpu.vfp.dregs, cpu.vfp.fpscr.bits(), cpu.cpsr.to_u32());
    let insn = Aarch32Decoder::decode(0x0eb70a41).unwrap();
    assert!(matches!(
        Executor::new(&mut cpu, &mut memory).execute(&insn),
        ExecResult::Continue
    ));
    assert_eq!(
        (cpu.vfp.dregs, cpu.vfp.fpscr.bits(), cpu.cpsr.to_u32()),
        before
    );
}

#[test]
fn missing_fpscr_representation_keeps_integral_rounding_out_of_the_lifters() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::aarch32::Aarch32Lifter;
    use rax::smir::lift::thumb::ThumbLifter;
    use rax::smir::lift::{LiftContext, LiftError, SmirLifter};
    for words in [
        [0xfeb90a41_u32; 2],
        [0xeeb70a41; 2],
        [0xeeb60bc1; 2],
        [0xf3ba04c2, 0xffba04c2],
        [0xf3ba0442, 0xffba0442],
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
