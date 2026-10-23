//! V68 section 10.9: an extended immediate is a complete, unscaled byte offset.
use super::packet_memory::{assert_alignment, lifted, machine, regs};
use rax::config::{Endianness, HexagonIsa};
use rax::error::MemoryAccessKind;
use rax::isa::hexagon::decode::{AddrMode, DecodedInsn, decode};
use rax::smir::ir::memory::SmirMemory;
use rax::smir::ir::types::{ArchReg, HexagonReg};
use rax::smir::optimize::OptLevel;
use rax::smir::{BlockResult, ExitReason};
use rax::vm::vcpu::{HexagonRegisters, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

#[derive(Clone, Copy)]
enum Effect {
    Register(u8, u32),
    Pair(u32, u32),
    Store(usize, u64),
}
use Effect::*;

fn address(instruction: DecodedInsn) -> AddrMode {
    match instruction {
        DecodedInsn::Load { addr, .. }
        | DecodedInsn::Store { addr, .. }
        | DecodedInsn::LoadAlign { addr, .. }
        | DecodedInsn::LoadUnpack { addr, .. }
        | DecodedInsn::StoreNew { addr, .. } => addr,
        other => panic!("expected scalar memory access, got {other:?}"),
    }
}

fn write_packet(memory: &Arc<GuestMemoryMmap>, words: &[u32], endian: Endianness) {
    for (index, word) in words.iter().enumerate() {
        let bytes = match endian {
            Endianness::Little => word.to_le_bytes(),
            Endianness::Big => word.to_be_bytes(),
        };
        memory
            .write_slice(&bytes, GuestAddress(0x1000 + index as u64 * 4))
            .unwrap();
    }
}

fn expected(
    effect: Effect,
    before: &HexagonRegisters,
    endian: Endianness,
) -> (HexagonRegisters, [u8; 32]) {
    let mut registers = before.clone();
    let mut memory = [0xa5; 32];
    match effect {
        Register(index, value) => registers.r[index as usize] = value,
        Pair(low, high) => {
            registers.r[2] = low;
            registers.r[3] = high;
        }
        Store(width, value) => {
            let bytes = match endian {
                Endianness::Little => value.to_le_bytes(),
                Endianness::Big => value.to_be_bytes(),
            };
            let data = match endian {
                Endianness::Little => &bytes[..width],
                Endianness::Big => &bytes[8 - width..],
            };
            memory[16..16 + width].copy_from_slice(data);
        }
    }
    (registers, memory)
}

fn contract(word: u32, scale: u8, effect: Effect) {
    for extender in [None, Some(0)] {
        let decoded = decode(word, extender, HexagonIsa::V68);
        assert_eq!(decoded.used_ext, extender.is_some());
        let offset = if extender.is_some() { 16 } else { 16 << scale };
        assert!(
            matches!(address(decoded.insn), AddrMode::Offset { base:1, offset:n } if n==offset)
        );
    }
    let words = [0x00004000, word];
    for endian in [Endianness::Little, Endianness::Big] {
        let (mut cpu, memory, before) = machine(words[0], 0x2000, 0x1000, endian);
        write_packet(&memory, &words, endian);
        let (registers, expected_memory) = expected(effect, &before, endian);
        assert!(cpu.step_insn().unwrap().is_none());
        assert_eq!(regs(&cpu).r, registers.r, "{word:x}, {endian:?}");
        assert_eq!(cpu.current_pc(), 0x1008);
        assert_eq!(cpu.instruction_count(), 1);
        let mut after = [0; 32];
        memory.read_slice(&mut after, GuestAddress(0x2000)).unwrap();
        assert_eq!(after, expected_memory);
        // The generic Hexagon SMIR data path uses little-endian memory. The
        // direct CPU covers both guest data byte orders above.
        if endian == Endianness::Little {
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                let (state, mut memory, result) = lifted(&words, &before, level);
                assert!(
                    matches!(result, BlockResult::Exit(ExitReason::Halt)),
                    "{result:?}"
                );
                for index in 0..32 {
                    assert_eq!(
                        state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(index))),
                        registers.r[index as usize] as u64,
                        "{word:x}, {level:?}, r{index}"
                    );
                }
                memory.read(0x2000, &mut after).unwrap();
                assert_eq!(after, expected_memory, "{word:x}, {level:?}");
            }
        }
    }
}

// Fixed LLVM 23 V68 encodings of native-width accesses at R1 + ##16.
macro_rules! case {
    ($name:ident, $word:expr, $scale:expr, $effect:expr) => {
        #[test]
        fn $name() {
            contract($word, $scale, $effect);
        }
    };
}
case!(signed_byte_control, 0x9101c203, 0, Register(3, 0xffffffa5));
case!(unsigned_byte_control, 0x9121c203, 0, Register(3, 0xa5));
case!(signed_half, 0x9141c203, 1, Register(3, 0xffffa5a5));
case!(unsigned_half, 0x9161c203, 1, Register(3, 0xa5a5));
case!(word_load, 0x9181c203, 2, Register(3, 0xa5a5a5a5));
case!(double_load, 0x91c1c202, 3, Pair(0xa5a5a5a5, 0xa5a5a5a5));
case!(byte_store_control, 0xa101c210, 0, Store(1, 0x44));
case!(half_store, 0xa141c210, 1, Store(2, 0x3344));
case!(high_half_store, 0xa161c210, 1, Store(2, 0x1122));
case!(word_store, 0xa181c210, 2, Store(4, 0x11223344));
case!(double_store, 0xa1c1c210, 3, Store(8, 0x5566778811223344));
case!(unpack_two, 0x9061c203, 1, Register(3, 0x00a500a5));
case!(unpack_four, 0x90a1c202, 2, Pair(0x00a500a5, 0x00a500a5));
case!(half_fifo_load, 0x9041c202, 1, Pair(0x77881122, 0xa5a55566));

#[test]
fn new_value_store_uses_the_extended_byte_offset() {
    let words = [0x78004543, 0x00004000, 0xa1a1d210];
    for endian in [Endianness::Little, Endianness::Big] {
        let (mut cpu, memory, before) = machine(words[0], 0x2000, 0x1000, endian);
        write_packet(&memory, &words, endian);
        assert!(cpu.step_insn().unwrap().is_none());
        assert_eq!(regs(&cpu).r[3], 42);
        let (_, expected_memory) = expected(Store(4, 42), &before, endian);
        let mut after = [0; 32];
        memory.read_slice(&mut after, GuestAddress(0x2000)).unwrap();
        assert_eq!(after, expected_memory);
        if endian == Endianness::Little {
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                let (state, mut memory, result) = lifted(&words, &before, level);
                assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
                assert_eq!(state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(3))), 42);
                memory.read(0x2000, &mut after).unwrap();
                assert_eq!(after, expected_memory, "{level:?}");
            }
        }
    }
}

#[test]
fn negative_and_high_extended_bits_remain_unscaled() {
    for (words, base) in [
        ([0x0fff7fff, 0x9181c603], 0x2020),
        ([0x00004040, 0x9181c203], 0x1000),
    ] {
        assert!(
            matches!(address(decode(words[1], Some(if base == 0x2020 { 0x03ffffff } else { 0x40 }), HexagonIsa::V68).insn), AddrMode::Offset { base:1, offset:n } if n == 0x2010_i32 - base as i32)
        );
        let (mut cpu, memory, before) = machine(words[0], base, 0x1000, Endianness::Little);
        write_packet(&memory, &words, Endianness::Little);
        cpu.step_insn().unwrap();
        assert_eq!(regs(&cpu).r[3], 0xa5a5a5a5);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let (state, _, result) = lifted(&words, &before, level);
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Halt)),
                "{result:?}"
            );
            assert_eq!(
                state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(3))),
                0xa5a5a5a5
            );
        }
    }
}

#[test]
fn an_extended_unaligned_access_reports_the_unscaled_address() {
    for (word, width, access) in [
        (0x9141c023, 2, MemoryAccessKind::Read),
        (0x9181c023, 4, MemoryAccessKind::Read),
        (0x91c1c022, 8, MemoryAccessKind::Read),
        (0xa141c201, 2, MemoryAccessKind::Write),
        (0xa161c201, 2, MemoryAccessKind::Write),
        (0xa181c201, 4, MemoryAccessKind::Write),
        (0xa1c1c201, 8, MemoryAccessKind::Write),
    ] {
        let words = [0x00004000, word];
        let (mut cpu, memory, before) = machine(words[0], 0x2000, 0x1000, Endianness::Little);
        write_packet(&memory, &words, Endianness::Little);
        assert_alignment(cpu.step_insn().unwrap_err(), 0x1000, 0x2001, width, access);
        assert_eq!(regs(&cpu).r, before.r);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let (state, _, result) = lifted(&words, &before, level);
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::AlignmentFault { pc:0x1000, address:0x2001, alignment:n, access:k }) if n==width && k==access),
                "{result:?}"
            );
            assert_eq!(state.pc, 0x1000);
            for index in 0..32 {
                assert_eq!(
                    state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(index))),
                    before.r[index as usize] as u64
                );
            }
        }
    }
}

#[test]
fn an_absolute_access_consumes_its_extender_before_the_next_packet() {
    // Absolute load, store, and cancelled predicated load. All three consume
    // their extender regardless of whether a memory access takes place.
    for word in [0x4980c203, 0x4880c210, 0x9f88e083] {
        absolute_access_then_literal(word);
    }
}

fn absolute_access_then_literal(word: u32) {
    let words = [0x00004080, word, 0x7800c024];
    let (mut cpu, memory, before) = machine(words[0], 0x2000, 0x1000, Endianness::Little);
    write_packet(&memory, &words, Endianness::Little);
    cpu.step_insn().unwrap();
    cpu.step_insn().unwrap();
    assert_eq!(regs(&cpu).r[4], 1);
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let (state, _, result) = lifted(&words, &before, level);
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(4))),
            1,
            "{level:?}"
        );
    }
}
