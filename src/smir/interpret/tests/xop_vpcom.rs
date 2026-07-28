//! End-to-end AMD XOP VPCOM lift, optimization, and interpretation tests.

use super::*;
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::{OptLevel, optimize_function};

const CR0_PE: u64 = 1;
const CR0_TS: u64 = 1 << 3;
const CR0_AM: u64 = 1 << 18;
const CR4_OSXSAVE: u64 = 1 << 18;
const RFLAGS_AC: u64 = 1 << 18;
const INITIAL_FLAGS: u64 = 0x2 | 1 | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);
const OPCODES: &[(u8, usize, bool)] = &[
    (0xCC, 1, true),
    (0xCD, 2, true),
    (0xCE, 4, true),
    (0xCF, 8, true),
    (0xEC, 1, false),
    (0xED, 2, false),
    (0xEE, 4, false),
    (0xEF, 8, false),
];

fn encoding(opcode: u8, vvvv: u8, tail: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x8F, 0xE8, ((!vvvv) & 0x0F) << 3, opcode];
    bytes.extend_from_slice(tail);
    bytes
}

fn execute(
    bytes: &[u8],
    level: OptLevel,
    ctx: &mut SmirContext,
    memory: &mut dyn SmirMemory,
) -> BlockResult {
    let mut lifter = X86_64Lifter::strict();
    let mut lift_ctx = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, bytes, &mut lift_ctx)
        .expect("lift VPCOM");
    assert_eq!(result.bytes_consumed, bytes.len());
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = builder.finish();
    function.blocks[0].ops = result.ops;
    optimize_function(&mut function, level);
    SmirInterpreter::new().execute_block(ctx, memory, &function.blocks[0])
}

fn enabled_context() -> SmirContext {
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
    ctx.flags.lazy = None;
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!();
    };
    x86.xop = true;
    x86.cr0 = CR0_PE;
    x86.cr4 = CR4_OSXSAVE;
    x86.xcr0 = 0b110;
    x86.cs_l = true;
    x86.rflags = 0x2;
    x86.cpl = 0;
    x86.mxcsr = 0x5F80;
    for (register, value) in x86.xmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0x807F_FF00_0123_FEDC_u64.rotate_left((register * 11 + word * 19) as u32)
        });
    }
    ctx
}

fn lane(words: &[u64; 16], offset: usize, element_bytes: usize) -> u64 {
    let word = offset / 8;
    let byte = offset % 8;
    let mut raw = words[word] >> (byte * 8);
    if byte + element_bytes > 8 {
        raw |= words[word + 1] << ((8 - byte) * 8);
    }
    if element_bytes == 8 {
        raw
    } else {
        raw & ((1_u64 << (element_bytes * 8)) - 1)
    }
}

fn signed(value: u64, bits: u32) -> i64 {
    if bits == 64 {
        value as i64
    } else {
        let shift = 64 - bits;
        ((value << shift) as i64) >> shift
    }
}

fn reference(
    source1: [u64; 16],
    source2: [u64; 16],
    element_bytes: usize,
    signed_elements: bool,
    immediate: u8,
) -> [u64; 16] {
    let predicate = immediate & 7;
    let bits = (element_bytes * 8) as u32;
    let mut result = [0_u64; 16];
    for offset in (0..16).step_by(element_bytes) {
        let left = lane(&source1, offset, element_bytes);
        let right = lane(&source2, offset, element_bytes);
        let value = match predicate {
            0 if signed_elements => signed(left, bits) < signed(right, bits),
            1 if signed_elements => signed(left, bits) <= signed(right, bits),
            2 if signed_elements => signed(left, bits) > signed(right, bits),
            3 if signed_elements => signed(left, bits) >= signed(right, bits),
            0 => left < right,
            1 => left <= right,
            2 => left > right,
            3 => left >= right,
            4 => left == right,
            5 => left != right,
            6 => false,
            7 => true,
            _ => unreachable!(),
        };
        if value {
            let lane_mask = if bits == 64 {
                u64::MAX
            } else {
                (1_u64 << bits) - 1
            };
            result[offset / 8] |= lane_mask << ((offset % 8) * 8);
        }
    }
    result
}

#[test]
fn register_vpcom_matches_reference_at_every_optimization_level_and_alias() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for &(opcode, element_bytes, signed_elements) in OPCODES {
            for immediate in 0..8 {
                for (destination, source1, source2) in [(1, 2, 3), (2, 2, 3), (3, 2, 3)] {
                    let bytes = encoding(
                        opcode,
                        source1,
                        &[0xC0 | (destination << 3) | source2, 0xA0 | immediate],
                    );
                    let mut ctx = enabled_context();
                    let before = match &ctx.arch_regs {
                        ArchRegState::X86_64(x86) => x86.xmm,
                        _ => unreachable!(),
                    };
                    let expected = reference(
                        before[usize::from(source1)],
                        before[usize::from(source2)],
                        element_bytes,
                        signed_elements,
                        immediate,
                    );
                    let exit = execute(&bytes, level, &mut ctx, &mut FlatMemory::new(0x100));
                    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
                    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                        unreachable!();
                    };
                    assert_eq!(
                        x86.xmm[usize::from(destination)],
                        expected,
                        "opcode={opcode:#04x}, imm={immediate}, {level:?}"
                    );
                    assert_eq!(x86.mxcsr, 0x5F80);
                    assert_eq!(x86.rflags, 0x2);
                    assert_eq!(ctx.flags.materialized.to_rflags(), INITIAL_FLAGS);
                }
            }
        }
    }
}

#[test]
fn memory_vpcom_matches_reference_and_constant_predicates_still_fault() {
    let memory_value = [
        0x807F_FF00_0123_FEDC,
        0x8000_7FFF_FFFF_0001,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for &(opcode, element_bytes, signed_elements) in OPCODES {
            for immediate in 0..8 {
                let bytes = encoding(opcode, 2, &[0x08, 0xF0 | immediate]);
                let mut ctx = enabled_context();
                ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x100);
                let before = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.xmm,
                    _ => unreachable!(),
                };
                let expected = reference(
                    before[2],
                    memory_value,
                    element_bytes,
                    signed_elements,
                    immediate,
                );
                let mut memory = FlatMemory::new(0x400);
                for (word, value) in memory_value[..2].iter().enumerate() {
                    memory
                        .write(0x100 + (word * 8) as u64, &value.to_le_bytes())
                        .unwrap();
                }
                let exit = execute(&bytes, level, &mut ctx, &mut memory);
                assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
                let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                    unreachable!();
                };
                assert_eq!(x86.xmm[1], expected);
            }
        }
    }

    for predicate in [6, 7] {
        let bytes = encoding(0xCC, 2, &[0x08, predicate]);
        let mut ctx = enabled_context();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x10_000);
        let before = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.xmm[1],
            _ => unreachable!(),
        };
        let exit = execute(&bytes, OptLevel::O2, &mut ctx, &mut FlatMemory::new(0x100));
        assert!(
            matches!(exit, BlockResult::Exit(ExitReason::MemoryFault { .. })),
            "predicate={predicate}: {exit:?}"
        );
        let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[1], before);
    }
}

#[test]
fn vpcom_dynamic_guard_and_alignment_faults_are_precise_and_noncommitting() {
    let bytes = encoding(0xCC, 2, &[0x08, 0xA5]);
    for (name, configure, expected) in [
        ("CPUID.XOP=0", 0_u8, "replay"),
        ("CR0.TS=1", 1_u8, "replay"),
        ("enabled #AC", 2_u8, "ac"),
    ] {
        let mut ctx = enabled_context();
        ctx.write_vreg(
            VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            if configure == 2 { 0x10_001 } else { 0x10_000 },
        );
        {
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!();
            };
            match configure {
                0 => x86.xop = false,
                1 => x86.cr0 |= CR0_TS,
                2 => {
                    x86.cr0 |= CR0_AM;
                    x86.cpl = 3;
                    x86.rflags |= RFLAGS_AC;
                    ctx.flags.materialized.ac = true;
                }
                _ => unreachable!(),
            }
        }
        let before = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.xmm[1],
            _ => unreachable!(),
        };
        let exit = execute(&bytes, OptLevel::O2, &mut ctx, &mut FlatMemory::new(0x100));
        match expected {
            "replay" => assert!(matches!(
                exit,
                BlockResult::Exit(ExitReason::Undefined {
                    addr: 0x1000,
                    opcode: 0
                })
            )),
            "ac" => assert!(matches!(
                exit,
                BlockResult::Exit(ExitReason::AlignmentCheck { addr: 0x1000 })
            )),
            _ => unreachable!(),
        }
        let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[1], before, "{name}");
        assert_eq!(x86.mxcsr, 0x5F80, "{name}");
    }
}
