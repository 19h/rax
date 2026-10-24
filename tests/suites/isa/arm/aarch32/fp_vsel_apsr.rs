//! VSEL takes its condition from APSR while preserving FPSCR and raw data.
use rax::isa::arm::aarch32::vfp::Fpscr;
use rax::isa::arm::decoder::{Aarch32Decoder, ThumbDecoder};
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};
pub(super) fn word(fp: u32, cc: u32, d: u32, n: u32, m: u32) -> u32 {
    let (d, n, m) = if fp == 64 {
        (
            (d & 15) << 12 | (d >> 4) << 22,
            (n & 15) << 16 | (n >> 4) << 7,
            (m & 15) | (m >> 4) << 5,
        )
    } else {
        (
            (d >> 1) << 12 | (d & 1) << 22,
            (n >> 1) << 16 | (n & 1) << 7,
            (m >> 1) | (m & 1) << 5,
        )
    };
    0xfe000800
        | if fp == 16 {
            1 << 8
        } else if fp == 32 {
            2 << 8
        } else {
            3 << 8
        }
        | cc << 20
        | d
        | n
        | m
}
pub(super) fn set_flags(cpu: &mut Armv7Cpu, bits: u32) {
    cpu.cpsr.n = bits & (1 << 31) != 0;
    cpu.cpsr.z = bits & (1 << 30) != 0;
    cpu.cpsr.c = bits & (1 << 29) != 0;
    cpu.cpsr.v = bits & (1 << 28) != 0;
    cpu.cpsr.q = true;
}
fn take_n(cc: u32, flags: u32) -> bool {
    let n = flags & (1 << 31) != 0;
    let z = flags & (1 << 30) != 0;
    let v = flags & (1 << 28) != 0;
    match cc {
        0 => z,
        1 => v,
        2 => n == v,
        3 => !z && n == v,
        _ => unreachable!(),
    }
}
fn read(cpu: &Armv7Cpu, fp: u32, r: u8) -> u64 {
    if fp == 64 {
        cpu.vfp.read_d_bits(r)
    } else {
        u64::from(cpu.vfp.read_s_bits(r))
    }
}
fn check(cpu: &mut Armv7Cpu, fp: u32, cc: u32, d: u32, n: u32, m: u32, memory: &mut FlatMemory) {
    let selected = if take_n(cc, cpu.cpsr.to_u32()) { n } else { m };
    let result = read(cpu, fp, selected as u8);
    let mut expected = cpu.vfp.clone();
    if fp == 64 {
        expected.write_d_bits(d as u8, result);
    } else {
        expected.write_s_bits(
            d as u8,
            if fp == 16 {
                u32::from(result as u16)
            } else {
                result as u32
            },
        );
    }
    let raw = word(fp, cc, d, n, m);
    let insn = if cpu.cpsr.t {
        ThumbDecoder::decode_32bit(raw)
    } else {
        Aarch32Decoder::decode(raw)
    }
    .unwrap();
    let regs = cpu.regs;
    let cpsr = cpu.cpsr.to_u32();
    let fpscr = cpu.vfp.fpscr.bits();
    assert!(
        matches!(
            Executor::new(cpu, memory).execute(&insn),
            ExecResult::Continue
        ),
        "raw={raw:08x}"
    );
    assert_eq!(
        cpu.vfp.dregs, expected.dregs,
        "raw={raw:08x} APSR={cpsr:08x} FPSCR={fpscr:08x}"
    );
    assert_eq!(cpu.vfp.fpscr.bits(), fpscr);
    assert_eq!(cpu.cpsr.to_u32(), cpsr);
    assert_eq!(cpu.regs, regs);
}
#[test]
fn all_apsr_and_fpscr_flag_combinations_choose_from_apsr() {
    let mut count = 0;
    let mut memory = FlatMemory::new(4096, 0);
    for thumb in [false, true] {
        for fp in [16, 32, 64] {
            for cc in 0..4 {
                for apsr in 0..16 {
                    for fpscr in 0..16 {
                        let mut cpu = Armv7Cpu::new();
                        cpu.cpsr.t = thumb;
                        set_flags(&mut cpu, apsr << 28);
                        cpu.vfp.fpscr = Fpscr::from_bits(fpscr << 28 | 0x0ec8009f);
                        cpu.vfp.dregs.fill(0xcafebabe_deadbeef);
                        if fp == 64 {
                            cpu.vfp.write_d_bits(1, 0x7ff123456789abcd);
                            cpu.vfp.write_d_bits(2, 0x8000000000000001);
                        } else {
                            cpu.vfp.write_s_bits(2, 0x7f817c01);
                            cpu.vfp.write_s_bits(4, 0x80018001);
                        }
                        check(
                            &mut cpu,
                            fp,
                            cc,
                            0,
                            if fp == 64 { 1 } else { 2 },
                            if fp == 64 { 2 } else { 4 },
                            &mut memory,
                        );
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(count, 6144);
}
#[test]
fn all_vsel_register_fields_and_aliases_preserve_raw_data_and_state() {
    let mut count = 0;
    let mut memory = FlatMemory::new(4096, 0);
    for thumb in [false, true] {
        for fp in [16, 32, 64] {
            for cc in 0..4 {
                for d in 0..32 {
                    for n in 0..32 {
                        for m in 0..32 {
                            let mut cpu = Armv7Cpu::new();
                            cpu.cpsr.t = thumb;
                            let flags = ((d ^ n ^ m) & 15) << 28;
                            set_flags(&mut cpu, flags);
                            cpu.vfp.fpscr = Fpscr::from_bits((flags ^ 0xf0000000) | 0x0ec8009f);
                            cpu.vfp.dregs.fill(0xcafebabe_deadbeef);
                            if fp == 64 {
                                cpu.vfp.write_d_bits(n as u8, 0x7ff123456789abcd);
                                cpu.vfp.write_d_bits(m as u8, 0x8000000000000001);
                            } else {
                                cpu.vfp.write_s_bits(n as u8, 0x7f817c01);
                                cpu.vfp.write_s_bits(m as u8, 0x80018001);
                            }
                            check(&mut cpu, fp, cc, d, n, m, &mut memory);
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 786432);
}
#[test]
fn vsel_forged_condition_tags_and_executing_it_reject_without_state_changes() {
    use rax::isa::arm::decoder::Mnemonic;
    for thumb in [false, true] {
        let mut cpu = Armv7Cpu::new();
        cpu.cpsr.t = thumb;
        let raw = word(32, 2, 0, 2, 4);
        let mut insn = if thumb {
            ThumbDecoder::decode_32bit(raw)
        } else {
            Aarch32Decoder::decode(raw)
        }
        .unwrap();
        insn.mnemonic = Mnemonic::VSELEQ;
        let regs = cpu.regs;
        let dregs = cpu.vfp.dregs;
        let status = cpu.vfp.fpscr.bits();
        let mut memory = FlatMemory::new(4096, 0);
        assert!(matches!(
            Executor::new(&mut cpu, &mut memory).execute(&insn),
            ExecResult::Undefined
        ));
        assert_eq!(cpu.regs, regs);
        assert_eq!(cpu.vfp.dregs, dregs);
        assert_eq!(cpu.vfp.fpscr.bits(), status);
    }
    let mut cpu = Armv7Cpu::new();
    cpu.cpsr.t = true;
    cpu.cpsr.set_it_state(0, 8);
    cpu.cpsr.z = true;
    let insn = ThumbDecoder::decode_32bit(word(32, 0, 0, 2, 4)).unwrap();
    let mut memory = FlatMemory::new(4096, 0);
    assert!(matches!(
        Executor::new(&mut cpu, &mut memory).execute(&insn),
        ExecResult::Undefined
    ));
}
#[test]
fn vsel_does_not_gain_a_smir_lift_without_full_control_state() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::aarch32::Aarch32Lifter;
    use rax::smir::lift::thumb::ThumbLifter;
    use rax::smir::lift::{LiftContext, LiftError, SmirLifter};
    for fp in [16, 32, 64] {
        for cc in 0..4 {
            for thumb in [false, true] {
                let raw = word(fp, cc, 0, 2, 4);
                let bytes = if thumb {
                    let h = ((raw >> 16) as u16).to_le_bytes();
                    let l = (raw as u16).to_le_bytes();
                    [h[0], h[1], l[0], l[1]]
                } else {
                    raw.to_le_bytes()
                };
                let mut context = LiftContext::new(if thumb {
                    SourceArch::Thumb
                } else {
                    SourceArch::Aarch32
                });
                let mut lifter: Box<dyn SmirLifter> = if thumb {
                    Box::new(ThumbLifter::new())
                } else {
                    Box::new(Aarch32Lifter::new())
                };
                assert!(matches!(
                    lifter.lift_insn(0x1000, &bytes, &mut context),
                    Err(LiftError::Unsupported { addr: 0x1000, .. })
                ));
            }
        }
    }
}
