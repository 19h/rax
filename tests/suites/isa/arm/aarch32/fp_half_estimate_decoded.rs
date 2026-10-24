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
fn half_estimate_nan_and_signed_zero_accumulate_only_required_flags() {
    check(
        [0xf3b70501, 0xffb70501],
        &[(1, 0x7c01_8000_0000_7e01)],
        0,
        &[0x7e00_fc00_7c00_7e00],
        3,
    );
}
#[test]
fn half_sqrt_estimate_negative_and_signaling_values_are_invalid() {
    check(
        [0xf3b70581, 0xffb70581],
        &[(1, 0xfc00_bc00_7e01_7c01)],
        0,
        &[0x7e00_7e00_7e00_7e00],
        1,
    );
}
#[test]
fn flushed_half_inputs_raise_divide_by_zero_without_input_denormal() {
    for words in [[0xf3b70501, 0xffb70501], [0xf3b70581, 0xffb70581]] {
        check(
            words,
            &[(1, 0x8001_0001_83ff_03ff)],
            1 << 19,
            &[0xfc00_7c00_fc00_7c00],
            2,
        );
    }
}
#[test]
fn half_estimate_overflow_does_not_follow_fz32_or_live_round_zero() {
    check(
        [0xf3b70501, 0xffb70501],
        &[(1, 0x8001_0001_8001_0001)],
        (1 << 24) | (3 << 22),
        &[0xfc00_7c00_fc00_7c00],
        0x14,
    );
}
#[test]
fn half_estimate_tiny_outputs_follow_fz16_and_keep_signed_zero() {
    check(
        [0xf3b70501, 0xffb70501],
        &[(1, 0xfbff_7bff_f400_7400)],
        1 << 19,
        &[0x8000_0000_8000_0000],
        8,
    );
}
#[test]
fn half_estimate_standard_controls_ignore_ahp_and_preserve_sticky_flags() {
    check(
        [0xf3b70501, 0xffb70501],
        &[(1, 0xffff_7fff_fc00_7c00)],
        (1 << 26) | (1 << 22) | 0x90,
        &[0x7e00_7e00_8000_0000],
        0,
    );
}
#[test]
fn half_estimate_q_vector_accumulates_across_both_d_registers() {
    check(
        [0xf3b70542, 0xffb70542],
        &[(2, 0x3c00_4000_4400_7c00), (3, 0x7c01_0000_8000_7e01)],
        0,
        &[0x3bfc_37fc_33fc_0000, 0x7e00_7c00_fc00_7e00],
        3,
    );
}
#[test]
fn half_estimate_full_register_fields_and_aliases() {
    let mut count = 0;
    for thumb in [false, true] {
        for sqrt in [false, true] {
            for q in [false, true] {
                for d in (0..32).step_by(if q { 2 } else { 1 }) {
                    for m in (0..32).step_by(if q { 2 } else { 1 }) {
                        let mut cpu = Armv7Cpu::new();
                        let mut mem = FlatMemory::new(4096, 0);
                        cpu.cpsr.t = thumb;
                        cpu.vfp.dregs.fill(0xfeedface_cafebabe);
                        let regs = if q { 2 } else { 1 };
                        for offset in 0..regs {
                            cpu.vfp.dregs[m + offset] = 0x3c00_3c00_3c00_3c00;
                        }
                        let initial = (1 << 26) | (3 << 22) | 0xa8000010;
                        cpu.vfp.fpscr = Fpscr::from_bits(initial);
                        let mut expected = cpu.vfp.dregs;
                        for offset in 0..regs {
                            expected[d + offset] = 0x3bfc_3bfc_3bfc_3bfc;
                        }
                        let raw = 0xf3b70500
                            | ((sqrt as u32) << 7)
                            | ((q as u32) << 6)
                            | (((d as u32) >> 4) << 22)
                            | (((d as u32) & 15) << 12)
                            | (((m as u32) >> 4) << 5)
                            | ((m as u32) & 15);
                        let insn = if thumb {
                            ThumbDecoder::decode_32bit((raw & 0x00ffffff) | 0xff000000)
                        } else {
                            Aarch32Decoder::decode(raw)
                        }
                        .unwrap();
                        let cpsr = cpu.cpsr.to_u32();
                        let gprs = cpu.regs;
                        assert!(
                            matches!(
                                Executor::new(&mut cpu, &mut mem).execute(&insn),
                                ExecResult::Continue
                            ),
                            "raw={raw:08x}"
                        );
                        assert_eq!(cpu.vfp.dregs, expected, "raw={raw:08x}");
                        assert_eq!(cpu.vfp.fpscr.bits(), initial);
                        assert_eq!(cpu.cpsr.to_u32(), cpsr);
                        assert_eq!(cpu.regs, gprs);
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(count, 5120);
}
#[test]
fn missing_fpscr_representation_keeps_reciprocal_operations_out_of_the_lifters() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::aarch32::Aarch32Lifter;
    use rax::smir::lift::thumb::ThumbLifter;
    use rax::smir::lift::{LiftContext, LiftError, SmirLifter};
    for words in [
        [0xf3b70501_u32, 0xffb70501],
        [0xf3b70581, 0xffb70581],
        [0xf3b70542, 0xffb70542],
        [0xf3b705c2, 0xffb705c2],
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
