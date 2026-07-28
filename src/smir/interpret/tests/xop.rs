//! End-to-end AMD XOP packed-bit lift, optimization, and interpretation tests.

use super::*;
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::ops::X86XopPackedBitKind;
use crate::smir::ir::types::VecElementType;
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::{OptLevel, optimize_function};

const CR0_PE: u64 = 1;
const CR0_TS: u64 = 1 << 3;
const CR0_AM: u64 = 1 << 18;
const CR4_OSXSAVE: u64 = 1 << 18;
const RFLAGS_AC: u64 = 1 << 18;
const INITIAL_FLAGS: u64 = 0x2 | 1 | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);

fn xop(map: u8, w: bool, vvvv: u8, opcode: u8, tail: &[u8]) -> Vec<u8> {
    let mut bytes = vec![
        0x8F,
        0xE0 | map,
        (u8::from(w) << 7) | (((!vvvv) & 0x0F) << 3),
        opcode,
    ];
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
        .expect("lift XOP packed-bit instruction");
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
    ctx
}

fn set_xmm(ctx: &mut SmirContext, index: usize, low: [u8; 16], upper_seed: u64) {
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!();
    };
    x86.xmm[index][0] = u64::from_le_bytes(low[..8].try_into().unwrap());
    x86.xmm[index][1] = u64::from_le_bytes(low[8..].try_into().unwrap());
    for (word, value) in x86.xmm[index][2..].iter_mut().enumerate() {
        *value = upper_seed ^ word as u64;
    }
}

fn low_xmm(ctx: &SmirContext, index: usize) -> [u8; 16] {
    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
        unreachable!();
    };
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&x86.xmm[index][0].to_le_bytes());
    bytes[8..].copy_from_slice(&x86.xmm[index][1].to_le_bytes());
    bytes
}

fn shape(opcode: u8) -> (X86XopPackedBitKind, usize) {
    match opcode {
        0x90 | 0xC0 => (X86XopPackedBitKind::Rotate, 1),
        0x91 | 0xC1 => (X86XopPackedBitKind::Rotate, 2),
        0x92 | 0xC2 => (X86XopPackedBitKind::Rotate, 4),
        0x93 | 0xC3 => (X86XopPackedBitKind::Rotate, 8),
        0x94 => (X86XopPackedBitKind::LogicalShift, 1),
        0x95 => (X86XopPackedBitKind::LogicalShift, 2),
        0x96 => (X86XopPackedBitKind::LogicalShift, 4),
        0x97 => (X86XopPackedBitKind::LogicalShift, 8),
        0x98 => (X86XopPackedBitKind::ArithmeticShift, 1),
        0x99 => (X86XopPackedBitKind::ArithmeticShift, 2),
        0x9A => (X86XopPackedBitKind::ArithmeticShift, 4),
        0x9B => (X86XopPackedBitKind::ArithmeticShift, 8),
        _ => unreachable!("test enumerates assigned packed-bit opcodes"),
    }
}

fn reference(
    source: [u8; 16],
    counts: Option<[u8; 16]>,
    immediate: Option<u8>,
    element_bytes: usize,
    kind: X86XopPackedBitKind,
) -> [u8; 16] {
    let bits = (element_bytes * 8) as u32;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1_u64 << bits) - 1
    };
    let mut output = [0_u8; 16];
    for offset in (0..16).step_by(element_bytes) {
        let mut lane = [0_u8; 8];
        lane[..element_bytes].copy_from_slice(&source[offset..offset + element_bytes]);
        let value = u64::from_le_bytes(lane);
        let signed_count =
            immediate.unwrap_or_else(|| counts.expect("variable count")[offset]) as i8;
        let amount = u32::from(signed_count.unsigned_abs()) & (bits - 1);
        let value = match (kind, signed_count.is_negative()) {
            (X86XopPackedBitKind::Rotate, false) => {
                if bits == 64 {
                    value.rotate_left(amount)
                } else {
                    ((value << amount) | (value >> ((bits - amount) & (bits - 1)))) & mask
                }
            }
            (X86XopPackedBitKind::Rotate, true) => {
                if bits == 64 {
                    value.rotate_right(amount)
                } else {
                    ((value >> amount) | (value << ((bits - amount) & (bits - 1)))) & mask
                }
            }
            (X86XopPackedBitKind::LogicalShift, false)
            | (X86XopPackedBitKind::ArithmeticShift, false) => (value << amount) & mask,
            (X86XopPackedBitKind::LogicalShift, true) => value >> amount,
            (X86XopPackedBitKind::ArithmeticShift, true) => {
                let signed = if bits == 64 {
                    value as i64
                } else {
                    ((value << (64 - bits)) as i64) >> (64 - bits)
                };
                ((signed >> amount) as u64) & mask
            }
        };
        output[offset..offset + element_bytes]
            .copy_from_slice(&value.to_le_bytes()[..element_bytes]);
    }
    output
}

fn assert_zero_upper(ctx: &SmirContext, index: usize, label: &str) {
    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
        unreachable!();
    };
    assert_eq!(x86.xmm[index][2..], [0; 14], "{label}: bits 1023:128");
}

#[test]
fn all_variable_cells_and_operand_orders_match_signed_count_semantics_at_o0_o1_o2() {
    let source = [
        0x81, 0x7E, 0x34, 0x92, 0x78, 0x56, 0xBC, 0x9A, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23,
        0x81,
    ];
    let counts = [
        0, 1, 0xFF, 7, 0xF9, 15, 0xF1, 0x80, 31, 0xE1, 63, 0xC1, 127, 0x81, 8, 0xF8,
    ];

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for opcode in 0x90..=0x9B {
            let (kind, element_bytes) = shape(opcode);
            let expected = reference(source, Some(counts), None, element_bytes, kind);
            for w in [false, true] {
                let mut ctx = enabled_context();
                set_xmm(&mut ctx, 2, [0xCC; 16], 0xD200_0000_0000_0000);
                if w {
                    set_xmm(&mut ctx, 4, source, 0x4400_0000_0000_0000);
                    set_xmm(&mut ctx, 3, counts, 0x3300_0000_0000_0000);
                } else {
                    set_xmm(&mut ctx, 3, source, 0x3300_0000_0000_0000);
                    set_xmm(&mut ctx, 4, counts, 0x4400_0000_0000_0000);
                }
                let bytes = xop(9, w, 4, opcode, &[0xD3]);
                let exit = execute(&bytes, level, &mut ctx, &mut FlatMemory::new(0x100));
                assert!(
                    matches!(exit, BlockResult::Exit(ExitReason::Halt)),
                    "opcode={opcode:#04x}, W={w}, {level:?}: {exit:?}"
                );
                assert_eq!(
                    low_xmm(&ctx, 2),
                    expected,
                    "opcode={opcode:#04x}, W={w}, {level:?}"
                );
                assert_zero_upper(&ctx, 2, "variable XOP");
                assert_eq!(ctx.flags.materialized.to_rflags(), INITIAL_FLAGS);
            }
        }
    }
}

#[test]
fn immediate_rotate_counts_cover_signed_extremes_and_modulo_widths() {
    let source = [
        0x81, 0x7E, 0x34, 0x92, 0x78, 0x56, 0xBC, 0x9A, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23,
        0x81,
    ];
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for opcode in 0xC0..=0xC3 {
            let (kind, element_bytes) = shape(opcode);
            for immediate in [0, 1, 7, 8, 15, 0x7F, 0x80, 0xFF] {
                let mut ctx = enabled_context();
                set_xmm(&mut ctx, 2, source, 0x2200_0000_0000_0000);
                set_xmm(&mut ctx, 3, [0xCC; 16], 0x3300_0000_0000_0000);
                let bytes = xop(8, false, 0, opcode, &[0xDA, immediate]);
                let exit = execute(&bytes, level, &mut ctx, &mut FlatMemory::new(0x100));
                assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
                assert_eq!(
                    low_xmm(&ctx, 3),
                    reference(source, None, Some(immediate), element_bytes, kind),
                    "opcode={opcode:#04x}, imm={immediate:#04x}, {level:?}"
                );
                assert_zero_upper(&ctx, 3, "immediate XOP");
                assert_eq!(ctx.flags.materialized.to_rflags(), INITIAL_FLAGS);
            }
        }
    }
}

#[test]
fn register_aliases_snapshot_both_sources_before_destination_commit() {
    let source = [
        0x81, 0x7E, 0x34, 0x92, 0x78, 0x56, 0xBC, 0x9A, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23,
        0x81,
    ];
    let counts = [
        1, 0xFF, 7, 0xF9, 15, 0xF1, 31, 0xE1, 63, 0xC1, 127, 0x81, 8, 0xF8, 0, 0x80,
    ];
    for (name, w, vvvv, modrm, destination) in [
        ("destination aliases source W=0", false, 4, 0xDB, 3),
        ("destination aliases count W=0", false, 4, 0xE3, 4),
        ("destination aliases source W=1", true, 4, 0xE3, 4),
        ("destination aliases count W=1", true, 4, 0xDB, 3),
    ] {
        let mut ctx = enabled_context();
        set_xmm(&mut ctx, 3, source, 0x3300_0000_0000_0000);
        set_xmm(&mut ctx, 4, counts, 0x4400_0000_0000_0000);
        let expected = reference(
            if w { counts } else { source },
            Some(if w { source } else { counts }),
            None,
            4,
            X86XopPackedBitKind::LogicalShift,
        );
        let bytes = xop(9, w, vvvv, 0x96, &[modrm]);
        let exit = execute(&bytes, OptLevel::O2, &mut ctx, &mut FlatMemory::new(0x100));
        assert!(
            matches!(exit, BlockResult::Exit(ExitReason::Halt)),
            "{name}"
        );
        assert_eq!(low_xmm(&ctx, destination), expected, "{name}");
        assert_zero_upper(&ctx, destination, name);
    }
}

#[test]
fn xop_guard_is_dynamic_precise_and_noncommitting_for_every_failed_condition() {
    for (name, mutate) in [
        ("feature absent", 0_u8),
        ("protected mode absent", 1),
        ("strict long mode absent", 2),
        ("virtual-8086 mode", 3),
        ("OSXSAVE absent", 4),
        ("XCR0.XMM absent", 5),
        ("XCR0.YMM absent", 6),
        ("CR0.TS set", 7),
    ] {
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut ctx = enabled_context();
            set_xmm(&mut ctx, 2, [0xA5; 16], 0x2200_0000_0000_0000);
            set_xmm(&mut ctx, 3, [0x81; 16], 0x3300_0000_0000_0000);
            let before = {
                let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                    unreachable!();
                };
                x86.xmm[2]
            };
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!();
            };
            match mutate {
                0 => x86.xop = false,
                1 => x86.cr0 &= !CR0_PE,
                2 => x86.cs_l = false,
                3 => x86.rflags |= crate::isa::x86_64::flags::bits::VM,
                4 => x86.cr4 &= !CR4_OSXSAVE,
                5 => x86.xcr0 &= !(1 << 1),
                6 => x86.xcr0 &= !(1 << 2),
                7 => x86.cr0 |= CR0_TS,
                _ => unreachable!(),
            }
            let bytes = xop(8, false, 0, 0xC0, &[0xD3, 1]);
            let exit = execute(&bytes, level, &mut ctx, &mut FlatMemory::new(0x100));
            assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
                ),
                "{name}, {level:?}: {exit:?}"
            );
            let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                unreachable!();
            };
            assert_eq!(x86.xmm[2], before, "{name}, {level:?}: destination");
            assert_eq!(
                ctx.flags.materialized.to_rflags(),
                INITIAL_FLAGS,
                "{name}, {level:?}: flags"
            );
        }
    }
}

#[test]
fn memory_alignment_and_canonicality_faults_precede_memory_and_destination_commit() {
    let source = [
        0x81, 0x7E, 0x34, 0x92, 0x78, 0x56, 0xBC, 0x9A, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23,
        0x81,
    ];
    let counts = [1_u8; 16];
    let register = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let bytes = xop(9, false, 4, 0x94, &[0x08]);

    for (name, address, ac_enabled, expected_exit) in [
        ("aligned", 0x100, false, None),
        ("misaligned AC disabled", 0x101, false, None),
        ("misaligned AC enabled", 0x101, true, Some("ac")),
        (
            "noncanonical start",
            0x0000_8000_0000_0000,
            true,
            Some("gp"),
        ),
        (
            "canonical range crossing",
            0x0000_7FFF_FFFF_FFF8,
            true,
            Some("gp"),
        ),
    ] {
        let mut ctx = enabled_context();
        ctx.write_vreg(register, address);
        set_xmm(&mut ctx, 1, [0xCC; 16], 0x1100_0000_0000_0000);
        set_xmm(&mut ctx, 4, counts, 0x4400_0000_0000_0000);
        if ac_enabled {
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!();
            };
            x86.cr0 |= CR0_AM;
            x86.cpl = 3;
            x86.rflags |= RFLAGS_AC;
            ctx.flags.materialized.ac = true;
        }
        let before = {
            let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                unreachable!();
            };
            x86.xmm[1]
        };
        let mut memory = FlatMemory::new(0x400);
        if address < 0x200 {
            memory.write(address, &source).unwrap();
        }
        let exit = execute(&bytes, OptLevel::O2, &mut ctx, &mut memory);
        match expected_exit {
            None => {
                assert!(
                    matches!(exit, BlockResult::Exit(ExitReason::Halt)),
                    "{name}"
                );
                assert_eq!(
                    low_xmm(&ctx, 1),
                    reference(
                        source,
                        Some(counts),
                        None,
                        1,
                        X86XopPackedBitKind::LogicalShift,
                    ),
                    "{name}"
                );
                assert_zero_upper(&ctx, 1, name);
            }
            Some("ac") => assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::AlignmentCheck { addr: 0x1000 })
                ),
                "{name}: {exit:?}"
            ),
            Some("gp") => assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::GeneralProtection {
                        addr: 0x1000,
                        error_code: 0,
                    })
                ),
                "{name}: {exit:?}"
            ),
            _ => unreachable!(),
        }
        if expected_exit.is_some() {
            let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                unreachable!();
            };
            assert_eq!(x86.xmm[1], before, "{name}: destination");
        }
    }

    // A stack-default address selects #SS(0), also before the out-of-bounds read.
    let mut ctx = enabled_context();
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rsp)), 0x0000_8000_0000_0000);
    set_xmm(&mut ctx, 1, [0xCC; 16], 0x1100_0000_0000_0000);
    set_xmm(&mut ctx, 4, counts, 0x4400_0000_0000_0000);
    let stack_bytes = xop(9, false, 4, 0x94, &[0x0C, 0x24]);
    let exit = execute(
        &stack_bytes,
        OptLevel::O2,
        &mut ctx,
        &mut FlatMemory::new(0x100),
    );
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::StackSegment {
            addr: 0x1000,
            error_code: 0,
        })
    ));
}

#[test]
fn malformed_xop_ir_operands_fail_closed_without_destination_commit() {
    for (name, count, elem) in [
        (
            "negative immediate",
            SrcOperand::Imm(-1),
            VecElementType::I8,
        ),
        (
            "oversized immediate",
            SrcOperand::Imm(256),
            VecElementType::I8,
        ),
        ("floating element", SrcOperand::Imm(1), VecElementType::F32),
    ] {
        let mut ctx = enabled_context();
        set_xmm(&mut ctx, 1, [0xCC; 16], 0x1100_0000_0000_0000);
        set_xmm(&mut ctx, 2, [0x81; 16], 0x2200_0000_0000_0000);
        let before = {
            let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                unreachable!();
            };
            x86.xmm[1]
        };
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::X86XopPackedBit {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                count,
                elem,
                kind: X86XopPackedBitKind::Rotate,
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let function = builder.finish();
        let exit = SmirInterpreter::new().execute_block(
            &mut ctx,
            &mut FlatMemory::new(0x100),
            &function.blocks[0],
        );
        assert!(
            matches!(
                exit,
                BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
            ),
            "{name}: {exit:?}"
        );
        let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[1], before, "{name}: destination");
    }
}

#[test]
fn malformed_xop_alignment_ir_fails_closed_without_address_observation() {
    for alignment in [0, 1, 8, 32] {
        let mut ctx = enabled_context();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0xFFFF_FFFF_FFFF_FFFF);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::X86CheckAlignmentAc {
                addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
                alignment,
                stack_segment: false,
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let function = builder.finish();
        let exit = SmirInterpreter::new().execute_block(
            &mut ctx,
            &mut FlatMemory::new(0),
            &function.blocks[0],
        );
        assert!(
            matches!(
                exit,
                BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
            ),
            "alignment={alignment}: {exit:?}"
        );
    }
}
