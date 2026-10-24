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
fn quiet_addend_cannot_suppress_zero_times_infinity_invalid() {
    check(
        [0xfc220813; 2],
        &[
            (0, 0x7fc12345_7fc12345),
            (2, 0xdeadbeef_00000000),
            (3, 0xdeadbeef_7c007c00),
        ],
        1 << 8,
        &[0x7fc00000_7fc00000],
        1,
    );
}
#[test]
fn mixed_half_quiet_nan_uses_standard_default_nan() {
    check(
        [0xfc220813; 2],
        &[(0, 0), (2, 0xdeadbeef_7e017e01), (3, 0xdeadbeef_3c003c00)],
        0,
        &[0x7fc00000_7fc00000],
        0,
    );
}
#[test]
fn mixed_half_signaling_nan_accumulates_invalid() {
    check(
        [0xfc220813; 2],
        &[(0, 0), (2, 0xdeadbeef_7c017c01), (3, 0xdeadbeef_3c003c00)],
        0,
        &[0x7fc00000_7fc00000],
        1,
    );
}
#[test]
fn mixed_half_input_flush_uses_fz16_without_idc() {
    check(
        [0xfc220813; 2],
        &[(0, 0), (2, 0xdeadbeef_00010001), (3, 0xdeadbeef_3c003c00)],
        1 << 19,
        &[0],
        0,
    );
}
#[test]
fn mixed_single_addend_flush_uses_standard_fz32_and_idc() {
    check(
        [0xfc220813; 2],
        &[
            (0, 0x00000001_00000001),
            (2, 0xdeadbeef_00000000),
            (3, 0xdeadbeef_3c003c00),
        ],
        0,
        &[0],
        0x80,
    );
}
#[test]
fn mixed_final_rounding_is_nearest_even_and_sets_inexact() {
    check(
        [0xfc220813; 2],
        &[
            (0, 0x3f800000_3f800000),
            (2, 0xdeadbeef_00010001),
            (3, 0xdeadbeef_3c003c00),
        ],
        1 << 22,
        &[0x3f800000_3f800000],
        0x10,
    );
}
#[test]
fn mixed_same_signed_zero_keeps_its_sign() {
    check(
        [0xfc220813; 2],
        &[
            (0, 0x80000000_80000000),
            (2, 0xdeadbeef_80008000),
            (3, 0xdeadbeef_3c003c00),
        ],
        1 << 26,
        &[0x80000000_80000000],
        0,
    );
}
#[test]
fn mixed_indexed_s_source_selects_the_requested_half_lane() {
    check(
        [0xfe02081b; 2],
        &[
            (0, 0x3f800000_3f800000),
            (2, 0xdeadbeef_40003c00),
            (3, 0xdeadbeef_40003c00),
        ],
        0,
        &[0x40a00000_40400000],
        0,
    );
}
#[test]
fn mixed_indexed_d_source_selects_the_requested_half_lane_for_all_results() {
    check(
        [0xfe04087e; 2],
        &[
            (0, 0x3f800000_3f800000),
            (1, 0x3f800000_3f800000),
            (4, 0x44004200_40003c00),
            (6, 0x40003c00_3c003c00),
        ],
        0,
        &[0x40a00000_40400000, 0x41100000_40e00000],
        0,
    );
    check(
        [0xfe14087e; 2],
        &[
            (0, 0x3f800000_3f800000),
            (1, 0x3f800000_3f800000),
            (4, 0x44004200_40003c00),
            (6, 0x40003c00_3c003c00),
        ],
        0,
        &[0xc0400000_bf800000, 0xc0e00000_c0a00000],
        0,
    );
}
#[test]
fn mixed_q_destination_alias_keeps_original_first_operand_lanes() {
    check(
        [0xfc200854; 2],
        &[(0, 0x40003c00_40003c00), (1, 0), (4, 0x3c003c00_3c003c00)],
        0,
        &[0x40801e00_40403c00, 0x40000000_3f800000],
        0,
    );
}
#[test]
fn mixed_q_destination_alias_keeps_original_second_operand_lanes() {
    check(
        [0xfc240850; 2],
        &[(0, 0x40003c00_40003c00), (1, 0), (4, 0x3c003c00_3c003c00)],
        0,
        &[0x40801e00_40403c00, 0x40000000_3f800000],
        0,
    );
}
#[test]
fn mixed_q_destination_alias_keeps_both_original_operands() {
    check(
        [0xfc200850; 2],
        &[(0, 0x40003c00_40003c00), (1, 0)],
        0,
        &[0x40c01e00_40403c00, 0x40800000_3f800000],
        0,
    );
}
#[test]
fn mixed_high_source_register_fields_select_the_complete_d_indices() {
    check(
        [0xfc2408f5; 2],
        &[
            (0, 0x3f800000_3f800000),
            (1, 0x3f800000_3f800000),
            (4, 0x44004400_44004400),
            (5, 0x48004800_48004800),
            (20, 0x3c003c00_3c003c00),
            (21, 0x40004000_40004000),
        ],
        0x9f,
        &[0x40400000_40400000, 0x40400000_40400000],
        0,
    );
}
fn register_matrix(indexed: bool) {
    let mut count = 0;
    for thumb in [false, true] {
        for subtract in [false, true] {
            for q in [false, true] {
                for d in (0..32).step_by(if q { 2 } else { 1 }) {
                    for n in 0..32 {
                        for m in 0..(if indexed { if q { 8 } else { 16 } } else { 32 }) {
                            for index in 0..(if indexed { if q { 4 } else { 2 } } else { 1 }) {
                                let mut cpu = Armv7Cpu::new();
                                let mut memory = FlatMemory::new(4096, 0);
                                cpu.cpsr.t = thumb;
                                cpu.vfp.dregs.fill(0x3f800000_3f800000);
                                let mut expected = cpu.vfp.dregs;
                                let regs = if q { 2 } else { 1 };
                                let high = if indexed && index & 1 == 0 {
                                    0x3f800000
                                } else if subtract {
                                    0xc0210000
                                } else {
                                    0x40908000
                                };
                                for offset in 0..regs {
                                    expected[d + offset] = (high << 32) | 0x3f800000;
                                }
                                let initial = 0xa8000010 | (1 << 26) | (3 << 22);
                                cpu.vfp.fpscr = Fpscr::from_bits(initial);
                                let cpsr = cpu.cpsr.to_u32();
                                let gprs = cpu.regs;
                                let mut raw = if indexed {
                                    0xfe000810 | ((subtract as u32) << 20)
                                } else {
                                    0xfc200810 | ((subtract as u32) << 23)
                                };
                                raw |= ((q as u32) << 6)
                                    | (((d as u32) >> 4) << 22)
                                    | (((d as u32) & 15) << 12);
                                if q {
                                    raw |= (((n as u32) >> 4) << 7) | (((n as u32) & 15) << 16);
                                } else {
                                    raw |= (((n as u32) & 1) << 7) | (((n as u32) >> 1) << 16);
                                }
                                if indexed {
                                    if q {
                                        raw |= (m as u32)
                                            | (((index as u32) & 1) << 3)
                                            | (((index as u32) >> 1) << 5);
                                    } else {
                                        raw |= ((m as u32) >> 1)
                                            | (((m as u32) & 1) << 5)
                                            | ((index as u32) << 3);
                                    }
                                } else if q {
                                    raw |= (((m as u32) >> 4) << 5) | ((m as u32) & 15);
                                } else {
                                    raw |= (((m as u32) & 1) << 5) | ((m as u32) >> 1);
                                }
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
                                assert_eq!(cpu.vfp.dregs, expected, "raw={raw:08x}");
                                assert_eq!(cpu.vfp.fpscr.bits(), initial, "raw={raw:08x}");
                                assert_eq!(cpu.cpsr.to_u32(), cpsr);
                                assert_eq!(cpu.regs, gprs);
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 196608);
}
#[test]
fn mixed_vector_register_fields_and_aliases() {
    register_matrix(false);
}
#[test]
fn mixed_indexed_register_fields_and_aliases() {
    register_matrix(true);
}
#[test]
fn missing_fpscr_representation_keeps_reciprocal_operations_out_of_the_lifters() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::aarch32::Aarch32Lifter;
    use rax::smir::lift::thumb::ThumbLifter;
    use rax::smir::lift::{LiftContext, LiftError, SmirLifter};
    for words in [
        [0xfc220813_u32; 2],
        [0xfca20813_u32; 2],
        [0xfc200854_u32; 2],
        [0xfca00854_u32; 2],
        [0xfe02081b_u32; 2],
        [0xfe12081b_u32; 2],
        [0xfe04087e_u32; 2],
        [0xfe14087e_u32; 2],
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
