//! End-to-end AMD XOP/TBM lift, optimize, and interpretation parity.

use super::*;
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::ops::X86TbmKind;
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::{OptLevel, optimize_function};

const CF: u64 = 1 << 0;
const PF: u64 = 1 << 2;
const AF: u64 = 1 << 4;
const ZF: u64 = 1 << 6;
const SF: u64 = 1 << 7;
const OF: u64 = 1 << 11;
const INITIAL_FLAGS: u64 = 0x2 | PF | AF | ZF | OF;

fn map9_bytes(w: bool, opcode: u8, group: u8) -> [u8; 5] {
    [
        0x8F,
        0xE9,
        if w { 0xF8 } else { 0x78 },
        opcode,
        0xC1 | (group << 3),
    ]
}

fn execute(bytes: &[u8], level: OptLevel, ctx: &mut SmirContext) -> BlockResult {
    let mut lifter = X86_64Lifter::strict();
    let mut lift_ctx = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, bytes, &mut lift_ctx)
        .expect("lift XOP/TBM instruction");
    assert_eq!(result.bytes_consumed, bytes.len());

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = builder.finish();
    function.blocks[0].ops = result.ops;
    optimize_function(&mut function, level);
    SmirInterpreter::new().execute_block(ctx, &mut FlatMemory::new(0x3000), &function.blocks[0])
}

fn context(tbm: bool, source: u64) -> SmirContext {
    let mut ctx = SmirContext::new_x86_64();
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0xA5A5_5A5A_DEAD_BEEF);
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rcx)), source);
    ctx.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
    ctx.flags.lazy = None;
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!();
    };
    x86.tbm = tbm;
    x86.cr0 = 1;
    x86.cs_l = true;
    x86.rflags = 0x2;
    ctx
}

fn reference(kind: X86TbmKind, src: u64, mask: u64) -> (u64, bool) {
    let src = src & mask;
    let incremented = src.wrapping_add(1) & mask;
    let decremented = src.wrapping_sub(1) & mask;
    let result = match kind {
        X86TbmKind::Blcfill => src & incremented,
        X86TbmKind::Blci => src | !incremented,
        X86TbmKind::Blcic => !src & incremented,
        X86TbmKind::Blcmsk => src ^ incremented,
        X86TbmKind::Blcs => src | incremented,
        X86TbmKind::Blsfill => src | decremented,
        X86TbmKind::Blsic => !src | decremented,
        X86TbmKind::T1mskc => !src | incremented,
        X86TbmKind::Tzmsk => !src & decremented,
    } & mask;
    let carry = if matches!(
        kind,
        X86TbmKind::Blsfill | X86TbmKind::Blsic | X86TbmKind::Tzmsk
    ) {
        src == 0
    } else {
        src == mask
    };
    (result, carry)
}

#[test]
fn all_map9_tbm_semantics_match_amd_pseudocode_at_o0_o1_o2() {
    let cases = [
        (0x01, 1, X86TbmKind::Blcfill),
        (0x02, 6, X86TbmKind::Blci),
        (0x01, 5, X86TbmKind::Blcic),
        (0x02, 1, X86TbmKind::Blcmsk),
        (0x01, 3, X86TbmKind::Blcs),
        (0x01, 2, X86TbmKind::Blsfill),
        (0x01, 6, X86TbmKind::Blsic),
        (0x01, 7, X86TbmKind::T1mskc),
        (0x01, 4, X86TbmKind::Tzmsk),
    ];

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for w in [false, true] {
            let mask = if w { u64::MAX } else { u64::from(u32::MAX) };
            for &(opcode, group, kind) in &cases {
                for source in [0, 1, 2, 0x7E, 0x7F, mask - 1, mask] {
                    let bytes = map9_bytes(w, opcode, group);
                    let mut ctx = context(true, source);
                    let exit = execute(&bytes, level, &mut ctx);
                    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));

                    let (expected, carry) = reference(kind, source, mask);
                    assert_eq!(
                        ctx.read_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
                        expected,
                        "{kind:?}, W={}, src={source:#018x}, {level:?}",
                        u8::from(w)
                    );
                    assert_eq!(
                        ctx.read_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rcx))),
                        source,
                        "{kind:?}: source"
                    );
                    ctx.flags.materialize_all();
                    let flags = ctx.flags.materialized.to_rflags();
                    let expected_defined = (u64::from(carry) * CF)
                        | (u64::from(expected == 0) * ZF)
                        | (u64::from(expected & (1 << (if w { 63 } else { 31 })) != 0) * SF);
                    assert_eq!(
                        flags & (CF | ZF | SF | OF),
                        expected_defined,
                        "{kind:?}, W={}, src={source:#018x}, {level:?}",
                        u8::from(w)
                    );
                    assert_eq!(flags & (PF | AF), INITIAL_FLAGS & (PF | AF));
                }
            }
        }
    }
}

#[test]
fn immediate_bextr_lifted_semantics_cover_width_and_control_edges() {
    let source = 0x0FED_CBA9_8765_4321_u64;
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for (w, control, expected) in [
            (false, 0x0804_u32, 0x32_u64),
            (false, 0x0820, 0),
            (true, 0x0804, 0x32),
            (true, 0x0840, 0),
            (true, 0x4004, 0x00FE_DCBA_9876_5432),
        ] {
            let mut bytes = vec![0x8F, 0xEA, if w { 0xF8 } else { 0x78 }, 0x10, 0xC1];
            bytes.extend_from_slice(&control.to_le_bytes());
            let mut ctx = context(true, source);
            let exit = execute(&bytes, level, &mut ctx);
            assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
            assert_eq!(
                ctx.read_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
                expected,
                "W={}, control={control:#06x}, {level:?}",
                u8::from(w)
            );
        }
    }
}

#[test]
fn dynamic_tbm_guard_exits_before_register_or_memory_side_effects() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for (feature, protected_mode, long_mode, virtual_8086) in [
            (false, true, true, false),
            (true, false, true, false),
            (true, true, false, false),
            (true, true, true, true),
        ] {
            let mut ctx = context(feature, 0xFFFF_FFFD);
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!();
            };
            x86.cr0 = u64::from(protected_mode);
            x86.cs_l = long_mode;
            if virtual_8086 {
                x86.rflags |= crate::isa::x86_64::flags::bits::VM;
            }
            let before_rax = ctx.read_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)));
            let exit = execute(&map9_bytes(false, 0x02, 6), level, &mut ctx);
            assert!(matches!(
                exit,
                BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
            ));
            assert_eq!(
                ctx.read_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
                before_rax
            );
            assert_eq!(ctx.flags.materialized.to_rflags(), INITIAL_FLAGS);
        }

        // BLCFILL EAX,dword ptr [0xFFFF_F000]. The disabled feature #UD must
        // precede the deliberately out-of-bounds memory source.
        let bytes = [0x8F, 0xE9, 0x78, 0x01, 0x0C, 0x25, 0x00, 0xF0, 0xFF, 0xFF];
        let mut ctx = context(false, 0);
        let exit = execute(&bytes, level, &mut ctx);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
        ));
    }
}
