//! End-to-end AMD XOP VPCMOV lift, optimization, and interpretation tests.

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

fn vpcmov(w: bool, l: bool, vvvv: u8, tail: &[u8]) -> Vec<u8> {
    let mut bytes = vec![
        0x8F,
        0xE8,
        (u8::from(w) << 7) | (((!vvvv) & 0x0F) << 3) | (u8::from(l) << 2),
        0xA2,
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
        .expect("lift VPCMOV");
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
            0x0123_4567_89AB_CDEF_u64.rotate_left((register * 11 + word * 7) as u32)
        });
    }
    ctx
}

fn reference(
    src_true: [u64; 16],
    src_false: [u64; 16],
    mask: [u64; 16],
    width: VecWidth,
) -> [u64; 16] {
    let mut result = [0_u64; 16];
    for word in 0..(width.bytes() / 8) as usize {
        result[word] = (src_true[word] & mask[word]) | (src_false[word] & !mask[word]);
    }
    result
}

#[test]
fn register_forms_match_reference_at_o0_o1_o2_for_widths_roles_and_aliases() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for l in [false, true] {
            let width = if l { VecWidth::V256 } else { VecWidth::V128 };
            for w in [false, true] {
                for (name, destination, src_true, rm, selected) in [
                    ("distinct", 1, 2, 3, 4),
                    ("destination-true", 2, 2, 3, 4),
                    ("destination-rm", 3, 2, 3, 4),
                    ("destination-selected", 4, 2, 3, 4),
                    ("all operands", 2, 2, 2, 2),
                ] {
                    let bytes = vpcmov(
                        w,
                        l,
                        src_true,
                        &[0xC0 | (destination << 3) | rm, (selected << 4) | 0x0D],
                    );
                    let mut ctx = enabled_context();
                    let before = match &ctx.arch_regs {
                        ArchRegState::X86_64(x86) => x86.xmm,
                        _ => unreachable!(),
                    };
                    let (src_false, mask) = if w {
                        (before[usize::from(selected)], before[usize::from(rm)])
                    } else {
                        (before[usize::from(rm)], before[usize::from(selected)])
                    };
                    let expected = reference(before[usize::from(src_true)], src_false, mask, width);
                    let exit = execute(&bytes, level, &mut ctx, &mut FlatMemory::new(0x100));
                    assert!(
                        matches!(exit, BlockResult::Exit(ExitReason::Halt)),
                        "{name}, W={w}, L={l}, {level:?}: {exit:?}"
                    );
                    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                        unreachable!();
                    };
                    let mut expected_registers = before;
                    expected_registers[usize::from(destination)] = expected;
                    assert_eq!(
                        x86.xmm, expected_registers,
                        "{name}, W={w}, L={l}, {level:?}"
                    );
                    assert_eq!(x86.mxcsr, 0x5F80, "{name}, W={w}, L={l}, {level:?}");
                    assert_eq!(x86.rflags, 0x2, "{name}, W={w}, L={l}, {level:?}");
                    assert_eq!(
                        ctx.flags.materialized.to_rflags(),
                        INITIAL_FLAGS,
                        "{name}, W={w}, L={l}, {level:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn memory_forms_match_reference_for_both_w_roles_and_complete_transfer_widths() {
    let memory_value: [u64; 4] = [
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0x6996_F00F_3CC3_A55A,
        0x9669_0FF0_C33C_5AA5,
    ];
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for l in [false, true] {
            let width = if l { VecWidth::V256 } else { VecWidth::V128 };
            for w in [false, true] {
                // VPCMOV {X,Y}MM1,{X,Y}MM2,{[RAX],XMM4},{XMM4,[RAX]}.
                let bytes = vpcmov(w, l, 2, &[0x08, 0x4F]);
                let mut ctx = enabled_context();
                ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x100);
                let before = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.xmm,
                    _ => unreachable!(),
                };
                let mut memory_operand = [0_u64; 16];
                memory_operand[..4].copy_from_slice(&memory_value);
                let (src_false, mask) = if w {
                    (before[4], memory_operand)
                } else {
                    (memory_operand, before[4])
                };
                let expected = reference(before[2], src_false, mask, width);
                let mut memory = FlatMemory::new(0x400);
                for (word, value) in memory_value.iter().enumerate() {
                    memory
                        .write(0x100 + (word * 8) as u64, &value.to_le_bytes())
                        .unwrap();
                }
                let exit = execute(&bytes, level, &mut ctx, &mut memory);
                assert!(
                    matches!(exit, BlockResult::Exit(ExitReason::Halt)),
                    "W={w}, L={l}, {level:?}: {exit:?}"
                );
                let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                    unreachable!();
                };
                assert_eq!(x86.xmm[1], expected, "W={w}, L={l}, {level:?}");
                assert_eq!(x86.mxcsr, 0x5F80, "W={w}, L={l}, {level:?}");
                assert_eq!(
                    ctx.flags.materialized.to_rflags(),
                    INITIAL_FLAGS,
                    "W={w}, L={l}, {level:?}"
                );
            }
        }
    }
}

#[test]
fn dynamic_guard_and_alignment_faults_precede_memory_and_destination_commit() {
    let bytes = vpcmov(false, true, 2, &[0x08, 0x40]);
    for (name, configure, expected) in [
        ("CPUID.XOP=0", 0_u8, "ud"),
        ("CR0.TS=1", 1_u8, "ud"),
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
        let exit = execute(&bytes, OptLevel::O2, &mut ctx, &mut FlatMemory::new(0x400));
        match expected {
            "ud" => assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
                ),
                "{name}: {exit:?}"
            ),
            "ac" => assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::AlignmentCheck { addr: 0x1000 })
                ),
                "{name}: {exit:?}"
            ),
            _ => unreachable!(),
        }
        let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[1], before, "{name}: destination");
        assert_eq!(x86.mxcsr, 0x5F80, "{name}: MXCSR");
    }
}
