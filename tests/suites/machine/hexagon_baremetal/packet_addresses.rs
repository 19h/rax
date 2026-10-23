//! V68 section 5.1: effective addresses inhabit the 32-bit byte address space.
use super::packet_memory::{LOW_MEMORY, lifted, machine, regs};
use rax::config::Endianness;
use rax::smir::ir::memory::SmirMemory;
use rax::smir::ir::types::{ArchReg, HexagonReg};
use rax::smir::optimize::OptLevel;
use rax::smir::{BlockResult, ExitReason};
use rax::vm::vcpu::{CpuState, VCpu};
use vm_memory::{Bytes, GuestAddress};

#[derive(Clone, Copy)]
enum Effect {
    Load(usize, bool),
    LoadPair,
    Store(usize, u64),
    Rmw,
}
use Effect::*;

fn contract(word: u32, effect: Effect) {
    for endian in [Endianness::Little, Endianness::Big] {
        let (mut cpu, memory, mut before) = machine(word, 0xfffffff0, 0x1000, endian);
        before.r[4] = 8;
        before.c[11] = 0xffffffc0;
        cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
        let mut expected_registers = before.r;
        let mut expected_memory = LOW_MEMORY;
        let write = match effect {
            Load(width, signed) => {
                expected_registers[3] = match width {
                    1 => {
                        if signed {
                            LOW_MEMORY[0] as i8 as i32 as u32
                        } else {
                            LOW_MEMORY[0] as u32
                        }
                    }
                    2 => {
                        let bytes = LOW_MEMORY[..2].try_into().unwrap();
                        let value = match endian {
                            Endianness::Little => u16::from_le_bytes(bytes),
                            Endianness::Big => u16::from_be_bytes(bytes),
                        };
                        if signed {
                            value as i16 as i32 as u32
                        } else {
                            value as u32
                        }
                    }
                    4 => match endian {
                        Endianness::Little => {
                            u32::from_le_bytes(LOW_MEMORY[..4].try_into().unwrap())
                        }
                        Endianness::Big => u32::from_be_bytes(LOW_MEMORY[..4].try_into().unwrap()),
                    },
                    _ => unreachable!(),
                };
                None
            }
            LoadPair => {
                let value = match endian {
                    Endianness::Little => u64::from_le_bytes(LOW_MEMORY),
                    Endianness::Big => u64::from_be_bytes(LOW_MEMORY),
                };
                expected_registers[2] = value as u32;
                expected_registers[3] = (value >> 32) as u32;
                None
            }
            Store(width, value) => Some((width, value)),
            Rmw => {
                let value = match endian {
                    Endianness::Little => u32::from_le_bytes(LOW_MEMORY[..4].try_into().unwrap()),
                    Endianness::Big => u32::from_be_bytes(LOW_MEMORY[..4].try_into().unwrap()),
                };
                Some((4, u64::from(value.wrapping_add(1))))
            }
        };
        if let Some((width, value)) = write {
            let bytes = match endian {
                Endianness::Little => value.to_le_bytes(),
                Endianness::Big => value.to_be_bytes(),
            };
            expected_memory[..width].copy_from_slice(match endian {
                Endianness::Little => &bytes[..width],
                Endianness::Big => &bytes[8 - width..],
            });
        }
        assert!(cpu.step_insn().unwrap().is_none());
        assert_eq!(regs(&cpu).r, expected_registers);
        let mut after = [0; 8];
        memory.read_slice(&mut after, GuestAddress(0x10)).unwrap();
        assert_eq!(after, expected_memory);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            if endian != Endianness::Little {
                continue;
            }
            let (state, mut memory, result) = lifted(&[word], &before, level);
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Halt)),
                "{word:x}, {level:?}: {result:?}"
            );
            for index in 0..32 {
                assert_eq!(
                    state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(index))),
                    expected_registers[index as usize] as u64
                );
            }
            memory.read(0x10, &mut after).unwrap();
            assert_eq!(after, expected_memory);
        }
    }
}

// LLVM 23 V68 encodings. Base+32, base+(8<<2), or GP+80 wraps to 0x10.
macro_rules! case {
    ($name:ident, $word:expr, $effect:expr) => {
        #[test]
        fn $name() {
            contract($word, $effect);
        }
    };
}
case!(byte_load_wraps, 0x9101c403, Load(1, true));
case!(half_load_wraps, 0x9141c203, Load(2, true));
case!(unsigned_half_load_wraps, 0x9161c203, Load(2, false));
case!(word_load_wraps, 0x9181c103, Load(4, false));
case!(double_load_wraps, 0x91c1c082, LoadPair);
case!(half_store_wraps, 0xa141c210, Store(2, 0x3344));
case!(word_store_wraps, 0xa181c208, Store(4, 0x11223344));
case!(double_store_wraps, 0xa1c1c204, Store(8, 0x5566778811223344));
case!(scaled_index_load_wraps, 0x3a81e403, Load(4, false));
case!(scaled_index_store_wraps, 0x3b81e402, Store(4, 0x11223344));
case!(global_pointer_load_wraps, 0x4980c283, Load(4, false));
case!(rmw_wraps, 0x3f41c401, Rmw);

#[test]
fn underflow_faults_name_the_32_bit_address_and_preserve_ram() {
    for (word, write) in [(0x9781ff03, false), (0xa781e2f8, true)] {
        let (mut cpu, _, before) = machine(word, 0x10, 0x1000, Endianness::Little);
        let error = cpu.step_insn().unwrap_err();
        assert!(
            matches!(
                error,
                rax::Error::GuestMemory(vm_memory::GuestMemoryError::InvalidGuestAddress(
                    GuestAddress(0xfffffff0)
                ))
            ),
            "{error:?}"
        );
        assert_eq!(regs(&cpu).r, before.r);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let (state, mut memory, result) = lifted(&[word], &before, level);
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::MemoryFault { addr:0xfffffff0, write:w }) if w==write),
                "{word:x}, {level:?}: {result:?}"
            );
            for index in 0..32 {
                assert_eq!(
                    state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(index))),
                    before.r[index as usize] as u64
                );
            }
            let mut after = [0; 32];
            memory.read(0x2000, &mut after).unwrap();
            assert_eq!(after, [0xa5; 32]);
        }
    }
}

#[test]
fn predicated_accesses_wrap_when_enabled_and_cancel_when_disabled() {
    for word in [0x4181c103, 0x4081c240] {
        for predicate in [0, 0xff] {
            let (mut cpu, memory, mut before) =
                machine(word, 0xfffffff0, 0x1000, Endianness::Little);
            before.p[0] = predicate;
            cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
            assert!(cpu.step_insn().unwrap().is_none());
            let expected = regs(&cpu);
            let mut expected_memory = [0; 8];
            memory
                .read_slice(&mut expected_memory, GuestAddress(0x10))
                .unwrap();
            assert_eq!(
                expected.r[3],
                if word == 0x4181c103 && predicate != 0 {
                    0x98badcfe
                } else {
                    before.r[3]
                }
            );
            assert_eq!(
                &expected_memory[..4],
                if word == 0x4081c240 && predicate != 0 {
                    &[0x44, 0x33, 0x22, 0x11]
                } else {
                    &LOW_MEMORY[..4]
                }
            );
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                let (state, mut memory, result) = lifted(&[word], &before, level);
                assert!(
                    matches!(result, BlockResult::Exit(ExitReason::Halt)),
                    "{result:?}"
                );
                assert_eq!(
                    state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(3))),
                    expected.r[3] as u64
                );
                let mut after = [0; 8];
                memory.read(0x10, &mut after).unwrap();
                assert_eq!(after, expected_memory);
            }
        }
    }
}
