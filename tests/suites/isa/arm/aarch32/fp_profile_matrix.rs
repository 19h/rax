//! All scalar fixed scales/registers and SIMD conversion register fields.
use rax::isa::arm::aarch32::vfp::Fpscr;
use rax::isa::arm::decoder::{Aarch32Decoder, ThumbDecoder};
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};
fn execute(cpu: &mut Armv7Cpu, raw: u32, thumb: bool, expected: [u64; 32], status: u32) {
    cpu.cpsr.t = thumb;
    let before = cpu.vfp.fpscr.bits();
    let cpsr = cpu.cpsr.to_u32();
    let regs = cpu.regs;
    let mapped = if !thumb {
        raw
    } else if raw >> 24 == 0xf2 {
        raw ^ 0x1d000000
    } else if raw >> 24 == 0xf3 {
        raw ^ 0x0c000000
    } else {
        raw
    };
    let insn = if thumb {
        ThumbDecoder::decode_32bit(mapped)
    } else {
        Aarch32Decoder::decode(mapped)
    }
    .unwrap();
    let mut memory = FlatMemory::new(4096, 0);
    assert!(
        matches!(
            Executor::new(cpu, &mut memory).execute(&insn),
            ExecResult::Continue
        ),
        "raw={raw:08x} Thumb={thumb}"
    );
    assert_eq!(cpu.vfp.dregs, expected, "raw={raw:08x} Thumb={thumb}");
    assert_eq!(
        cpu.vfp.fpscr.bits(),
        before | status,
        "raw={raw:08x} Thumb={thumb}"
    );
    assert_eq!(cpu.cpsr.to_u32(), cpsr);
    assert_eq!(cpu.regs, regs);
}
#[test]
fn scalar_fixed_every_scale_width_sign_direction_and_register() {
    let mut count = 0;
    for thumb in [false, true] {
        for fp in [16, 32, 64] {
            for integer in [16, 32] {
                for signed in [false, true] {
                    for to_fixed in [false, true] {
                        for fraction in 1..=integer {
                            for d in 0u32..32 {
                                let immediate = integer - fraction;
                                let fields = if fp == 64 {
                                    (d & 15) << 12 | (d >> 4) << 22
                                } else {
                                    (d >> 1) << 12 | (d & 1) << 22
                                };
                                let raw = 0xeeba0840
                                    | ((if fp == 16 {
                                        1
                                    } else if fp == 32 {
                                        2
                                    } else {
                                        3
                                    }) << 8)
                                    | if integer == 32 { 1 << 7 } else { 0 }
                                    | if signed { 0 } else { 1 << 16 }
                                    | if to_fixed { 1 << 18 } else { 0 }
                                    | fields
                                    | ((immediate >> 1) & 15)
                                    | ((immediate & 1) << 5);
                                let half = match fp {
                                    16 => 0x3800,
                                    32 => 0x3f000000,
                                    64 => 0x3fe0000000000000,
                                    _ => unreachable!(),
                                };
                                let sign = if signed { 1u64 << (fp - 1) } else { 0 };
                                let fixed = if signed {
                                    (0u64).wrapping_sub(1u64 << (fraction - 1))
                                } else {
                                    1u64 << (fraction - 1)
                                };
                                let mut cpu = Armv7Cpu::new();
                                cpu.vfp.dregs.fill(0xcafebabe_deadbeef);
                                cpu.vfp.fpscr = Fpscr::from_bits(0xaec80080);
                                let input = if to_fixed { half | sign } else { fixed };
                                if fp == 64 {
                                    cpu.vfp.write_d_bits(d as u8, input);
                                } else {
                                    cpu.vfp.write_s_bits(
                                        d as u8,
                                        input as u32
                                            | if fp == 16 && to_fixed { 0xabcd0000 } else { 0 },
                                    );
                                }
                                let mut expected = cpu.vfp.dregs;
                                let result = if to_fixed { fixed } else { half | sign };
                                if fp == 64 {
                                    expected[d as usize] = result;
                                } else {
                                    let shift = (d & 1) * 32;
                                    expected[d as usize / 2] = (expected[d as usize / 2]
                                        & !(0xffffffffu64 << shift))
                                        | ((result & 0xffffffff) << shift);
                                }
                                execute(&mut cpu, raw, thumb, expected, 0);
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 36864);
}
fn repeated(raw: u64, width: u32) -> u64 {
    if width == 16 {
        (raw & 0xffff) * 0x0001000100010001
    } else {
        (raw & 0xffffffff) * 0x0000000100000001
    }
}
#[test]
fn simd_integer_and_fixed_all_fields_and_aliases() {
    let mut count = 0;
    for thumb in [false, true] {
        for width in [16, 32] {
            for fixed in [false, true] {
                for signed in [false, true] {
                    for to_fixed in [false, true] {
                        for q in [false, true] {
                            for d in 0u32..32 {
                                for m in 0u32..32 {
                                    if q && (d | m) & 1 != 0 {
                                        continue;
                                    }
                                    let dfield = (d & 15) << 12 | (d >> 4) << 22;
                                    let mfield = (m & 15) | (m >> 4) << 5;
                                    let raw = if fixed {
                                        0xf2800c10
                                            | if signed { 0 } else { 1 << 24 }
                                            | ((64 - 8) << 16)
                                            | if width == 32 { 1 << 9 } else { 0 }
                                            | if to_fixed { 1 << 8 } else { 0 }
                                    } else {
                                        0xf3b30600
                                            | if width == 16 { 1 << 18 } else { 2 << 18 }
                                            | if signed { 0 } else { 1 << 7 }
                                            | if to_fixed { 1 << 8 } else { 0 }
                                    } | dfield
                                        | mfield
                                        | if q { 1 << 6 } else { 0 };
                                    let float = if width == 16 { 0x3c00 } else { 0x3f800000 };
                                    let integer = if fixed { 256 } else { 1 };
                                    let input =
                                        repeated(if to_fixed { float } else { integer }, width);
                                    let output =
                                        repeated(if to_fixed { integer } else { float }, width);
                                    let mut cpu = Armv7Cpu::new();
                                    cpu.vfp.dregs.fill(0xcafebabe_deadbeef);
                                    cpu.vfp.fpscr = Fpscr::from_bits(0xaec80080);
                                    let regs = if q { 2 } else { 1 };
                                    for reg in 0..regs {
                                        cpu.vfp.dregs[(m + reg) as usize] = input;
                                    }
                                    let mut expected = cpu.vfp.dregs;
                                    for reg in 0..regs {
                                        expected[(d + reg) as usize] = output;
                                    }
                                    execute(&mut cpu, raw, thumb, expected, 0);
                                    count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 40960);
}
