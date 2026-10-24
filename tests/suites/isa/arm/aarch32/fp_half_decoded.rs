//! Half-precision instruction controls and complete aliased register effects.
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
fn scalar_half_add_uses_live_rounding_and_clears_upper_s_half() {
    check(
        [0xee310902; 2],
        &[(1, 0x12345678_dead3c00), (2, 0x12345678_dead1000)],
        1 << 22,
        &[0xcafebabe_00003c01],
        0x10,
    );
}
#[test]
fn scalar_half_fma_retains_exact_subnormal_residual() {
    check(
        [0xeea10902; 2],
        &[
            (0, 0xcafebabe_deadbc00),
            (1, 0x12345678_dead3c01),
            (2, 0x12345678_dead3bfe),
        ],
        0,
        &[0xcafebabe_00008010],
        0,
    );
}
#[test]
fn scalar_half_mla_rounds_the_product_first() {
    check(
        [0xee010902; 2],
        &[
            (0, 0xcafebabe_deadbc00),
            (1, 0x12345678_dead3c01),
            (2, 0x12345678_dead3bfe),
        ],
        0,
        &[0xcafebabe_00000000],
        0x10,
    );
}
#[test]
fn scalar_half_sqrt_uses_live_round_up() {
    check(
        [0xeeb109c1; 2],
        &[(1, 0x12345678_dead4000)],
        1 << 22,
        &[0xcafebabe_00003da9],
        0x10,
    );
}
#[test]
fn scalar_half_maxnum_keeps_exact_subnormal_without_underflow() {
    check(
        [0xfe810902; 2],
        &[(1, 0x12345678_dead0001), (2, 0x12345678_dead0000)],
        0,
        &[0xcafebabe_00000001],
        0,
    );
}
#[test]
fn vector_half_add_uses_standard_controls_and_half_input_flush() {
    check(
        [0xf2120d44, 0xef120d44],
        &[
            (2, 0x0001_7c01_7e01_3c00),
            (3, 0x4000_7bff_8001_8000),
            (4, 0x0000_3c00_3c00_1000),
            (5, 0xc000_0000_0000_8000),
        ],
        (1 << 22) | (1 << 19),
        &[0x0000_7e00_7e00_3c00, 0x0000_7bff_0000_8000],
        0x11,
    );
}
#[test]
fn vector_half_exact_rounding_uses_nearest_even_and_standard_nan() {
    check(
        [0xf3b604c2, 0xffb604c2],
        &[(2, 0xc180_c080_4180_4080), (3, 0x7e01_7c01_0001_7bff)],
        (1 << 22) | (1 << 19),
        &[0xc200_c000_4200_4000, 0x7e00_7e00_0000_7bff],
        0x11,
    );
}
#[test]
fn vector_half_zero_compare_observes_fz16_and_nan_status() {
    check(
        [0xf3b50542, 0xffb50542],
        &[(2, 0x7c01_8001_7e01_0001), (3, 0x3c00_bc00_8000_0000)],
        (1 << 22) | (1 << 19),
        &[0x0000_ffff_0000_ffff, 0x0000_0000_ffff_ffff],
        1,
    );
}
#[test]
fn vector_single_zero_compare_uses_standard_flush_and_status() {
    check(
        [0xf3b90542, 0xffb90542],
        &[(2, 0x7fc12345_00000001), (3, 0x7f812345_bf800000)],
        1 << 22,
        &[0x00000000_ffffffff, 0],
        0x81,
    );
}
#[test]
fn vector_half_binary_compare_observes_fz16_and_keeps_nzcv() {
    check(
        [0xf2120e44, 0xef120e44],
        &[
            (2, 0x7c01_7e01_8001_0001),
            (3, 0x3c00_3c00_3c00_3c00),
            (4, 0),
            (5, 0x3c00_3c00_3c00_3c00),
        ],
        1 << 19,
        &[0x0000_0000_ffff_ffff, u64::MAX],
        1,
    );
}
#[test]
fn vector_half_legacy_max_uses_nan_propagation_and_fz16() {
    check(
        [0xf2110f02, 0xef110f02],
        &[(1, 0x0001_8001_7e01_7c01), (2, 0)],
        1 << 19,
        &[0x0000_0000_7e00_7e00],
        1,
    );
}
#[test]
fn vector_half_absdiff_rounds_in_half_precision() {
    check(
        [0xf3320d44, 0xff320d44],
        &[
            (2, 0x3c00_3c00_3c00_3c00),
            (3, 0x3c00_3c00_3c00_3c00),
            (4, 0x1000_1000_1000_1000),
            (5, 0x1000_1000_1000_1000),
        ],
        1 << 22,
        &[0x3bff_3bff_3bff_3bff, 0x3bff_3bff_3bff_3bff],
        0,
    );
}
#[test]
fn scalar_half_immediate_clears_the_upper_s_half() {
    check([0xeeb70900; 2], &[], 0, &[0xcafebabe_00003c00], 0);
}
#[test]
fn scalar_half_select_copies_nan_bits_without_quieting_and_clears_upper_s_half() {
    let inputs = [(1, 0x12345678_dead7c01), (2, 0x12345678_dead3c00)];
    check([0xfe010902; 2], &inputs, 1 << 30, &[0xcafebabe_00007c01], 0);
    check([0xfe010902; 2], &inputs, 0, &[0xcafebabe_00003c00], 0);
}
#[test]
fn missing_fpscr_representation_keeps_half_data_processing_out_of_the_lifters() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::aarch32::Aarch32Lifter;
    use rax::smir::lift::thumb::ThumbLifter;
    use rax::smir::lift::{LiftContext, LiftError, SmirLifter};
    for words in [
        [0xeeb70900_u32; 2],
        [0xfe010902; 2],
        [0xee310902_u32; 2],
        [0xeea10902; 2],
        [0xee010902; 2],
        [0xeeb109c1; 2],
        [0xfe810902; 2],
        [0xf2120d44, 0xef120d44],
        [0xf3b604c2, 0xffb604c2],
        [0xf3b50542, 0xffb50542],
        [0xf3b90542, 0xffb90542],
        [0xf2120e44, 0xef120e44],
        [0xf2110f02, 0xef110f02],
        [0xf3320d44, 0xff320d44],
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

#[test]
fn reserved_half_register_move_rejects_every_register_field_and_keeps_valid_neighbors() {
    use rax::isa::arm::decoder::Mnemonic;
    let mut count = 0;
    for thumb in [false, true] {
        for d in 0..32u32 {
            for m in 0..32u32 {
                let raw =
                    0xeeb00940 | ((d >> 1) << 12) | ((d & 1) << 22) | (m >> 1) | ((m & 1) << 5);
                for size in [2, 3, 1] {
                    let word = (raw & !0x300) | size << 8;
                    let insn = if thumb {
                        ThumbDecoder::decode_32bit(word)
                    } else {
                        Aarch32Decoder::decode(word)
                    }
                    .unwrap();
                    assert_eq!(
                        insn.mnemonic,
                        if size == 1 {
                            Mnemonic::UNDEFINED
                        } else {
                            Mnemonic::VMOV
                        },
                        "word={word:08x} Thumb={thumb}"
                    );
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 6144);
}
#[test]
fn forged_reserved_half_move_keeps_all_cpu_state() {
    use rax::isa::arm::ExecutionState;
    use rax::isa::arm::decoder::{DecodedInsn, Mnemonic};
    for thumb in [false, true] {
        let mut cpu = Armv7Cpu::new();
        let mut mem = FlatMemory::new(4096, 0);
        cpu.cpsr.t = thumb;
        cpu.vfp.dregs.fill(0xcafebabe_deadbeef);
        cpu.vfp.dregs[1] = 0x12345678_00003c00;
        cpu.vfp.fpscr = Fpscr::from_bits(0xa840009f);
        let before = (
            cpu.regs,
            cpu.cpsr.to_u32(),
            cpu.vfp.dregs,
            cpu.vfp.fpscr.bits(),
        );
        let insn = DecodedInsn::new(
            Mnemonic::VMOV,
            if thumb {
                ExecutionState::Thumb
            } else {
                ExecutionState::Aarch32
            },
            0xeeb00960,
            4,
        );
        assert!(matches!(
            Executor::new(&mut cpu, &mut mem).execute(&insn),
            ExecResult::Undefined
        ));
        assert_eq!(
            (
                cpu.regs,
                cpu.cpsr.to_u32(),
                cpu.vfp.dregs,
                cpu.vfp.fpscr.bits()
            ),
            before
        );
    }
}
