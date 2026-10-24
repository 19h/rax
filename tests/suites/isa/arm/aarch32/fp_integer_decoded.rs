//! Instruction control selection and complete aliased register effects.
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
fn scalar_integer_to_float_uses_live_rounding_and_updates_status() {
    check(
        [0xeeb80ac1_u32; 2],
        &[(1, 0x12345678_01000001)],
        1 << 22,
        &[0xcafebabe_4b800001],
        0x10,
    );
}

#[test]
fn scalar_fixed_to_float_uses_nearest_despite_live_round_up() {
    check(
        [0xeeba0ac8; 2],
        &[(0, 0xcafebabe_01000001)],
        1 << 22,
        &[0xcafebabe_43800000],
        0x10,
    );
}

#[test]
fn scalar_float_to_integer_flushes_input_before_conversion() {
    check(
        [0xeebd0ac1; 2],
        &[(1, 0x12345678_00000001)],
        1 << 24,
        &[0xcafebabe_00000000],
        0x80,
    );
}

#[test]
fn signed_fixed_double_result_extends_across_the_destination() {
    check(
        [0xeebe0bc8; 2],
        &[(0, 0xbff4000000000000)],
        0,
        &[0xffff_ffff_fffe_c000_u64],
        0,
    );
}

#[test]
fn unsigned_fixed_double_result_clears_the_upper_destination() {
    check(
        [0xeebf0bc8; 2],
        &[(0, 0x3ff4000000000000)],
        0,
        &[0x14000],
        0,
    );
}

#[test]
fn simd_integer_to_float_uses_standard_controls_and_accumulates_lane_status() {
    check(
        [0xf3bb0642, 0xffbb0642],
        &[(2, 0xffffffff_01000001), (3, 0x01000003_80000000)],
        1 << 22,
        &[0xbf800000_4b800000, 0x4b800002_cf000000],
        0x10,
    );
}

#[test]
fn simd_float_to_integer_flushes_subnormal_lanes_with_fpscr_flush_clear() {
    check(
        [0xf3bb0742, 0xffbb0742],
        &[(2, 0x80000001_00000001), (3, 0x7fc00001_3fa00000)],
        2 << 22,
        &[0, 1],
        0x91,
    );
}

#[test]
fn simd_fixed_to_float_uses_nearest_and_accumulates_status() {
    check(
        [0xf2b00e52, 0xefb00e52],
        &[(2, 0xffffffff_01000001), (3, 0x01000003_80000000)],
        1 << 22,
        &[0xb7800000_43800000, 0x43800002_c7000000],
        0x10,
    );
}

#[test]
fn failed_scalar_condition_preserves_registers_and_fpscr() {
    let mut cpu = Armv7Cpu::new();
    let mut memory = FlatMemory::new(4096, 0);
    cpu.cpsr.z = false;
    cpu.vfp.dregs.fill(0xcafebabe_deadbeef);
    cpu.vfp.write_s_bits(2, 0x01000001);
    cpu.vfp.fpscr = Fpscr::from_bits(0xa8000004 | (1 << 22));
    let before = (cpu.vfp.dregs, cpu.vfp.fpscr.bits(), cpu.cpsr.to_u32());
    let insn = Aarch32Decoder::decode(0x0eb80ac1).unwrap();
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
fn missing_fpscr_representation_keeps_conversions_out_of_the_lifters() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::aarch32::Aarch32Lifter;
    use rax::smir::lift::thumb::ThumbLifter;
    use rax::smir::lift::{LiftContext, LiftError, SmirLifter};
    for words in [
        [0xeeb80ac1_u32; 2],
        [0xeebd0bc1; 2],
        [0xeeba0ac8; 2],
        [0xeebe0bc8; 2],
        [0xf3bb0642, 0xffbb0642],
        [0xf3bb0042, 0xffbb0042],
        [0xf2b00e52, 0xefb00e52],
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
