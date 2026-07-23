//! Canonical AMD SSE4A interpretation coverage.

use super::*;
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::ops::{OpKind, X86Sse4aBitfieldKind};
use crate::smir::ir::types::{Address, ArchReg, MemWidth, VReg, VirtualId, X86Reg};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::{OptLevel, optimize_function};

const CR0_EM: u64 = 1 << 2;
const CR0_TS: u64 = 1 << 3;
const CR4_OSFXSR: u64 = 1 << 9;
const INITIAL_FLAGS: u64 = 0x2 | 0x08D5 | (1 << 10);

fn execute(bytes: &[u8], level: OptLevel, ctx: &mut SmirContext) -> BlockResult {
    execute_with_memory(bytes, level, ctx, &mut FlatMemory::new(0x1000))
}

fn execute_with_memory(
    bytes: &[u8],
    level: OptLevel,
    ctx: &mut SmirContext,
    memory: &mut FlatMemory,
) -> BlockResult {
    let mut lifter = X86_64Lifter::strict();
    let mut lift_ctx = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, bytes, &mut lift_ctx)
        .expect("lift SSE4A bitfield instruction");
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
        unreachable!("x86 context must contain x86 state");
    };
    x86.sse4a = true;
    x86.cr0 = 1;
    x86.cr4 = CR4_OSFXSR;
    ctx
}

#[test]
fn lifted_sse4a_forms_match_low_qword_semantics_at_o0_o1_o2() {
    struct Case {
        name: &'static str,
        bytes: &'static [u8],
        dst: usize,
        source: usize,
        dst_value: [u64; 2],
        source_value: [u64; 2],
        expected_low: u64,
    }

    let cases = [
        Case {
            name: "EXTRQ immediate",
            bytes: &[0x66, 0x0F, 0x78, 0xC1, 0xC8, 0xC4],
            dst: 1,
            source: 1,
            dst_value: [0xFEDC_BA98_7654_3210, 0x1112_1314_1516_1718],
            source_value: [0; 2],
            expected_low: 0x21,
        },
        Case {
            name: "EXTRQ register",
            bytes: &[0x66, 0x0F, 0x79, 0xD3],
            dst: 2,
            source: 3,
            dst_value: [0xFEDC_BA98_7654_3210, 0x2122_2324_2526_2728],
            source_value: [0xFFFF_FFFF_FFFF_100C, 0x3132_3334_3536_3738],
            expected_low: 0x654,
        },
        Case {
            name: "INSERTQ immediate",
            bytes: &[0xF2, 0x0F, 0x78, 0xE5, 0x08, 0x10],
            dst: 4,
            source: 5,
            dst_value: [0xFFFF_0000_FFFF_0000, 0x4142_4344_4546_4748],
            source_value: [0xA5, 0x5152_5354_5556_5758],
            expected_low: 0xFFFF_0000_FFA5_0000,
        },
        Case {
            name: "INSERTQ register",
            bytes: &[0xF2, 0x0F, 0x79, 0xF7],
            dst: 6,
            source: 7,
            dst_value: [0x0123_4567_89AB_CDEF, 0x6162_6364_6566_6768],
            source_value: [0xE7, (32 << 8) | 8],
            expected_low: 0x0123_45E7_89AB_CDEF,
        },
        Case {
            name: "INSERTQ register alias",
            bytes: &[0xF2, 0x0F, 0x79, 0xC9],
            dst: 1,
            source: 1,
            dst_value: [0x0123_4567_89AB_CDEF, 0xFFFF_FFFF_FFFF_2008],
            source_value: [0; 2],
            expected_low: 0x0123_45EF_89AB_CDEF,
        },
        Case {
            name: "EXTRQ encoded length zero",
            bytes: &[0x66, 0x0F, 0x78, 0xC1, 0x00, 0x00],
            dst: 1,
            source: 1,
            dst_value: [0x8877_6655_4433_2211, 0x7172_7374_7576_7778],
            source_value: [0; 2],
            expected_low: 0x8877_6655_4433_2211,
        },
        Case {
            name: "EXTRQ extended XMM",
            bytes: &[0x66, 0x45, 0x0F, 0x79, 0xCA],
            dst: 9,
            source: 10,
            dst_value: [0x8877_6655_4433_2211, 0x8182_8384_8586_8788],
            source_value: [(4 << 8) | 8, 0x9192_9394_9596_9798],
            expected_low: 0x21,
        },
    ];

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for case in &cases {
            let mut ctx = enabled_context();
            let destination_tail: [u64; 14] =
                std::array::from_fn(|index| 0xA100_0000_0000_0000 | index as u64);
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!();
            };
            x86.xmm[case.dst][..2].copy_from_slice(&case.dst_value);
            x86.xmm[case.dst][2..].copy_from_slice(&destination_tail);
            if case.source != case.dst {
                x86.xmm[case.source][..2].copy_from_slice(&case.source_value);
            }

            let exit = execute(case.bytes, level, &mut ctx);
            assert!(
                matches!(exit, BlockResult::Exit(ExitReason::Halt)),
                "{} {level:?}: {exit:?}",
                case.name
            );
            let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                unreachable!();
            };
            assert_eq!(
                x86.xmm[case.dst][0], case.expected_low,
                "{} {level:?}",
                case.name
            );
            assert_eq!(
                x86.xmm[case.dst][1], case.dst_value[1],
                "{} deterministic upper qword {level:?}",
                case.name
            );
            assert_eq!(
                x86.xmm[case.dst][2..],
                destination_tail,
                "{} upper vector state {level:?}",
                case.name,
            );
            assert_eq!(
                ctx.flags.materialized.to_rflags(),
                INITIAL_FLAGS,
                "{} flags {level:?}",
                case.name
            );
        }
    }
}

#[test]
fn sse4a_guard_exits_before_destination_or_flag_commit_for_every_dynamic_fault() {
    for (name, enabled, cr0, cr4) in [
        ("feature absent", false, 1, CR4_OSFXSR),
        ("CR0.EM", true, 1 | CR0_EM, CR4_OSFXSR),
        ("CR0.TS", true, 1 | CR0_TS, CR4_OSFXSR),
        ("CR4.OSFXSR absent", true, 1, 0),
    ] {
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut ctx = enabled_context();
            let original = [0xFEDC_BA98_7654_3210, 0x1112_1314_1516_1718];
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!();
            };
            x86.sse4a = enabled;
            x86.cr0 = cr0;
            x86.cr4 = cr4;
            x86.xmm[1][..2].copy_from_slice(&original);

            let exit = execute(&[0x66, 0x0F, 0x78, 0xC1, 8, 4], level, &mut ctx);
            assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
                ),
                "{name} {level:?}: {exit:?}"
            );
            let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                unreachable!();
            };
            assert_eq!(x86.xmm[1][..2], original, "{name} {level:?}");
            assert_eq!(
                ctx.flags.materialized.to_rflags(),
                INITIAL_FLAGS,
                "{name} {level:?}"
            );
        }
    }
}

#[test]
fn malformed_sse4a_controls_fail_closed_without_destination_commit() {
    for (name, length, index) in [
        ("unpaired", Some(8), None),
        ("length out of range", Some(64), Some(0)),
        ("index out of range", Some(8), Some(64)),
    ] {
        let mut ctx = enabled_context();
        let original = [0xFEDC_BA98_7654_3210, 0x1112_1314_1516_1718];
        let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
            unreachable!();
        };
        x86.xmm[1][..2].copy_from_slice(&original);

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::X86Sse4aBitfield {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                source: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                kind: X86Sse4aBitfieldKind::Extract,
                length,
                index,
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let function = builder.finish();
        let exit = SmirInterpreter::new().execute_block(
            &mut ctx,
            &mut FlatMemory::new(0x1000),
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
        assert_eq!(x86.xmm[1][..2], original, "{name}");
        assert_eq!(ctx.flags.materialized.to_rflags(), INITIAL_FLAGS, "{name}");
    }
}

#[test]
fn lifted_sse4a_movnt_stores_exact_scalar_width_at_o0_o1_o2() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for (name, bytes, source, expected) in [
            (
                "MOVNTSS",
                &[0xF3, 0x0F, 0x2B, 0x08][..],
                1_usize,
                &[0x88, 0x77, 0x66, 0x55][..],
            ),
            (
                "MOVNTSD extended XMM",
                &[0xF2, 0x44, 0x0F, 0x2B, 0x08][..],
                9,
                &[0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11][..],
            ),
        ] {
            let mut ctx = enabled_context();
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!();
            };
            x86.gpr[0] = 0x200;
            x86.xmm[source][0] = 0x1122_3344_5566_7788;
            x86.xmm[source][1] = 0xA1A2_A3A4_A5A6_A7A8;
            let before_xmm = x86.xmm;
            let mut memory = FlatMemory::new(0x1000);
            memory.write(0x200, &[0xCC; 8]).unwrap();

            let exit = execute_with_memory(bytes, level, &mut ctx, &mut memory);
            assert!(
                matches!(exit, BlockResult::Exit(ExitReason::Halt)),
                "{name} {level:?}: {exit:?}"
            );
            let mut stored = [0u8; 8];
            memory.read(0x200, &mut stored).unwrap();
            assert_eq!(&stored[..expected.len()], expected, "{name} {level:?}");
            if expected.len() == 4 {
                assert_eq!(&stored[4..], &[0xCC; 4], "{name} {level:?}: adjacent bytes");
            }
            let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                unreachable!();
            };
            assert_eq!(x86.xmm, before_xmm, "{name} {level:?}: XMM source");
            assert_eq!(
                ctx.flags.materialized.to_rflags(),
                INITIAL_FLAGS,
                "{name} {level:?}: flags"
            );
        }
    }
}

#[test]
fn sse4a_movnt_guard_faults_before_memory_write_at_every_optimization_level() {
    for (name, enabled, cr0, cr4) in [
        ("feature absent", false, 1, CR4_OSFXSR),
        ("CR0.EM", true, 1 | CR0_EM, CR4_OSFXSR),
        ("CR0.TS", true, 1 | CR0_TS, CR4_OSFXSR),
        ("CR4.OSFXSR absent", true, 1, 0),
    ] {
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut ctx = enabled_context();
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!();
            };
            x86.sse4a = enabled;
            x86.cr0 = cr0;
            x86.cr4 = cr4;
            x86.gpr[0] = 0x200;
            x86.xmm[1][0] = 0x1122_3344_5566_7788;
            let before_xmm = x86.xmm;
            let mut memory = FlatMemory::new(0x1000);
            memory.write(0x200, &[0xCC; 8]).unwrap();

            let exit = execute_with_memory(&[0xF3, 0x0F, 0x2B, 0x08], level, &mut ctx, &mut memory);
            assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
                ),
                "{name} {level:?}: {exit:?}"
            );
            let mut stored = [0u8; 8];
            memory.read(0x200, &mut stored).unwrap();
            assert_eq!(stored, [0xCC; 8], "{name} {level:?}: memory");
            let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                unreachable!();
            };
            assert_eq!(x86.xmm, before_xmm, "{name} {level:?}: XMM");
            assert_eq!(
                ctx.flags.materialized.to_rflags(),
                INITIAL_FLAGS,
                "{name} {level:?}: flags"
            );
        }
    }
}

#[test]
fn malformed_sse4a_movnt_ops_fail_closed_before_memory_access() {
    for (name, src, width) in [
        ("virtual source", VReg::Virtual(VirtualId(0)), MemWidth::B4),
        (
            "unencodable XMM",
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(16))),
            MemWidth::B8,
        ),
        (
            "invalid width",
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            MemWidth::B2,
        ),
    ] {
        let mut ctx = enabled_context();
        let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
            unreachable!();
        };
        x86.gpr[0] = 0x200;
        let mut memory = FlatMemory::new(0x1000);
        memory.write(0x200, &[0xCC; 8]).unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::X86Sse4aMovntStore {
                src,
                addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
                width,
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let function = builder.finish();

        let exit = SmirInterpreter::new().execute_block(&mut ctx, &mut memory, &function.blocks[0]);
        assert!(
            matches!(
                exit,
                BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
            ),
            "{name}: {exit:?}"
        );
        let mut stored = [0u8; 8];
        memory.read(0x200, &mut stored).unwrap();
        assert_eq!(stored, [0xCC; 8], "{name}: memory");
    }
}
