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
fn scalar_round_zero_flushes_half_input_without_idc_or_inexact() {
    check(
        [0xeebd09c1; 2],
        &[(1, 0x12345678_dead0001)],
        1 << 19,
        &[0xcafebabe_00000000],
        0,
    );
}
#[test]
fn scalar_unsigned_live_rounding_flushes_negative_half_input() {
    check(
        [0xeebc0941; 2],
        &[(1, 0x12345678_dead8001)],
        (1 << 19) | (1 << 22),
        &[0xcafebabe_00000000],
        0,
    );
}
#[test]
fn scalar_directed_rounding_observes_fz16() {
    check(
        [0xfebe09c1; 2],
        &[(1, 0x12345678_dead0001)],
        1 << 19,
        &[0xcafebabe_00000000],
        0,
    );
}
#[test]
fn scalar_live_rounding_preserves_controls_and_full_destination() {
    for (mode, want) in [(0, 2), (1, 2), (2, 1), (3, 1)] {
        check(
            [0xeebd0941; 2],
            &[(1, 0x12345678_dead3e00)],
            (mode << 22) | (1 << 26),
            &[0xcafebabe_00000000 | want],
            0x10,
        );
    }
}
#[test]
fn scalar_explicit_rounding_modes_and_signedness_are_independent_of_live_rm() {
    for (word, want) in [
        (0xfebc09c1, 3),
        (0xfebd09c1, 2),
        (0xfebe09c1, 3),
        (0xfebf09c1, 2),
        (0xfebc0941, 3),
        (0xfebd0941, 2),
        (0xfebe0941, 3),
        (0xfebf0941, 2),
    ] {
        check(
            [word; 2],
            &[(1, 0x12345678_dead4100)],
            3 << 22,
            &[0xcafebabe_00000000 | want],
            0x10,
        );
    }
}
#[test]
fn scalar_half_integer_conversion_ignores_ahp_and_saturates_infinity() {
    check(
        [0xeebd09c1; 2],
        &[(1, 0x12345678_dead7c00)],
        1 << 26,
        &[0xcafebabe_7fffffff],
        1,
    );
    check(
        [0xeebc09c1; 2],
        &[(1, 0x12345678_deadfc00)],
        1 << 26,
        &[0xcafebabe_00000000],
        1,
    );
}
#[test]
fn vector_signed_minimum_is_exact_without_invalid() {
    check(
        [0xf3b70142, 0xffb70142],
        &[(2, 0xf800_77ff_0000_8000), (3, 0xf800_77ff_0000_8000)],
        1 << 26,
        &[0x8000_7ff0_0000_0000, 0x8000_7ff0_0000_0000],
        0,
    );
}
#[test]
fn vector_fractional_integer_results_accumulate_inexact() {
    check(
        [0xf3b70142, 0xffb70142],
        &[(2, 0xc100_4100_be00_3e00), (3, 0xc100_4100_be00_3e00)],
        3 << 22,
        &[0xfffe_0002_fffe_0002, 0xfffe_0002_fffe_0002],
        0x10,
    );
}
#[test]
fn vector_unsigned_negative_rounds_before_saturation() {
    check(
        [0xf3b701c2, 0xffb701c2],
        &[(2, 0xb400_b400_b400_b400), (3, 0xb400_b400_b400_b400)],
        0,
        &[0, 0],
        0x10,
    );
}
#[test]
fn vector_directed_integer_conversion_observes_fz16_without_idc() {
    check(
        [0xf3b70242, 0xffb70242],
        &[(2, 0x0001_0001_8001_8001), (3, 0x0001_0001_8001_8001)],
        (1 << 19) | (1 << 24),
        &[0, 0],
        0,
    );
}
#[test]
fn vector_integer_saturation_drops_inexact_for_invalid_lanes() {
    check(
        [0xf3b70142, 0xffb70142],
        &[(2, 0x7c01_7e01_fc00_7800), (3, 0x7c01_7e01_fc00_7800)],
        0,
        &[0x0000_0000_8000_7fff, 0x0000_0000_8000_7fff],
        1,
    );
}
#[test]
fn scalar_half_integer_register_fields_and_aliases() {
    let mut count = 0;
    for thumb in [false, true] {
        for (base, want) in [
            (0xeebd09c0, 2),
            (0xeebc09c0, 2),
            (0xeebd0940, 2),
            (0xeebc0940, 2),
            (0xfebc09c0, 3),
            (0xfebd09c0, 2),
            (0xfebe09c0, 3),
            (0xfebf09c0, 2),
            (0xfebc0940, 3),
            (0xfebd0940, 2),
            (0xfebe0940, 3),
            (0xfebf0940, 2),
        ] {
            for d in 0..32 {
                for m in 0..32 {
                    let mut cpu = Armv7Cpu::new();
                    let mut memory = FlatMemory::new(4096, 0);
                    cpu.cpsr.t = thumb;
                    cpu.vfp.dregs.fill(0xcafebabe_deadbeef);
                    cpu.vfp.write_s_bits(m, 0xdead4100);
                    let mut expected = cpu.vfp.clone();
                    expected.write_s_bits(d, want);
                    let initial = 0xa8000000;
                    cpu.vfp.fpscr = Fpscr::from_bits(initial);
                    let cpsr = cpu.cpsr.to_u32();
                    let gprs = cpu.regs;
                    let raw = base
                        | ((u32::from(d) & 1) << 22)
                        | ((u32::from(d) >> 1) << 12)
                        | ((u32::from(m) & 1) << 5)
                        | (u32::from(m) >> 1);
                    let insn = if thumb {
                        ThumbDecoder::decode_32bit(raw)
                    } else {
                        Aarch32Decoder::decode(raw)
                    }
                    .unwrap();
                    assert!(
                        matches!(
                            Executor::new(&mut cpu, &mut memory).execute(&insn),
                            ExecResult::Continue
                        ),
                        "raw={raw:08x}"
                    );
                    assert_eq!(cpu.vfp.dregs, expected.dregs, "raw={raw:08x}");
                    assert_eq!(cpu.vfp.fpscr.bits(), initial | 0x10);
                    assert_eq!(cpu.cpsr.to_u32(), cpsr);
                    assert_eq!(cpu.regs, gprs);
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 24576);
}
#[test]
fn vector_half_integer_register_fields_and_aliases() {
    let mut count = 0;
    for thumb in [false, true] {
        for unsigned in [false, true] {
            for (mode, want) in [(0, 3), (1, 2), (2, 3), (3, 2)] {
                for q in [false, true] {
                    for d in (0..32).step_by(if q { 2 } else { 1 }) {
                        for m in (0..32).step_by(if q { 2 } else { 1 }) {
                            let mut cpu = Armv7Cpu::new();
                            let mut memory = FlatMemory::new(4096, 0);
                            cpu.cpsr.t = thumb;
                            cpu.vfp.dregs.fill(0xcafebabe_deadbeef);
                            let regs = if q { 2 } else { 1 };
                            for offset in 0..regs {
                                cpu.vfp.dregs[m + offset] = 0x4100_4100_4100_4100;
                            }
                            let mut expected = cpu.vfp.dregs;
                            for offset in 0..regs {
                                expected[d + offset] = 0x0001000100010001 * want;
                            }
                            let initial = 0xa8000000 | (1 << 26) | (3 << 22);
                            cpu.vfp.fpscr = Fpscr::from_bits(initial);
                            let cpsr = cpu.cpsr.to_u32();
                            let gprs = cpu.regs;
                            let raw = 0xf3b70000
                                | (mode << 8)
                                | ((unsigned as u32) << 7)
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
                            assert!(
                                matches!(
                                    Executor::new(&mut cpu, &mut memory).execute(&insn),
                                    ExecResult::Continue
                                ),
                                "raw={raw:08x}"
                            );
                            assert_eq!(cpu.vfp.dregs, expected, "raw={raw:08x}");
                            assert_eq!(cpu.vfp.fpscr.bits(), initial | 0x10);
                            assert_eq!(cpu.cpsr.to_u32(), cpsr);
                            assert_eq!(cpu.regs, gprs);
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 20480);
}
#[test]
fn missing_fpscr_representation_keeps_reciprocal_operations_out_of_the_lifters() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::aarch32::Aarch32Lifter;
    use rax::smir::lift::thumb::ThumbLifter;
    use rax::smir::lift::{LiftContext, LiftError, SmirLifter};
    for words in [
        [0xeebd09c1_u32, 0xeebd09c1],
        [0xeebc09c1_u32, 0xeebc09c1],
        [0xeebd0941_u32, 0xeebd0941],
        [0xeebc0941_u32, 0xeebc0941],
        [0xfebc09c1_u32, 0xfebc09c1],
        [0xfebd09c1_u32, 0xfebd09c1],
        [0xfebe09c1_u32, 0xfebe09c1],
        [0xfebf09c1_u32, 0xfebf09c1],
        [0xfebc0941_u32, 0xfebc0941],
        [0xfebd0941_u32, 0xfebd0941],
        [0xfebe0941_u32, 0xfebe0941],
        [0xfebf0941_u32, 0xfebf0941],
        [0xf3b70042, 0xffb70042],
        [0xf3b700c2, 0xffb700c2],
        [0xf3b70142, 0xffb70142],
        [0xf3b701c2, 0xffb701c2],
        [0xf3b70242, 0xffb70242],
        [0xf3b702c2, 0xffb702c2],
        [0xf3b70342, 0xffb70342],
        [0xf3b703c2, 0xffb703c2],
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
