//! Exhaustive legacy MOVQ2DQ/MOVDQ2Q lifting and canonical semantics.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

const INITIAL_FLAGS: u64 = 0x2 | 0x08D5;

fn transfer(result: &LiftResult) -> &SmirOp {
    result
        .ops
        .iter()
        .find(|op| matches!(op.kind, OpKind::X86MovdQ { .. }))
        .expect("one MMX/XMM transfer")
}

fn enter_mmx_count(result: &LiftResult) -> usize {
    result
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op.kind,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                }
            )
        })
        .count()
}

fn execute(bytes: &[u8], level: OptLevel, ctx: &mut SmirContext) -> BlockResult {
    let lifted = lift_single(bytes).expect("strict MMX/XMM transfer lift");
    assert_eq!(lifted.bytes_consumed, bytes.len());
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = builder.finish();
    function.blocks[0].ops = lifted.ops;
    optimize_function(&mut function, level);
    SmirInterpreter::new().execute_block(ctx, &mut FlatMemory::new(1), &function.blocks[0])
}

fn context() -> SmirContext {
    let mut ctx = SmirContext::new_x86_64();
    ctx.pc = 0x1000;
    ctx.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
    ctx.flags.lazy = None;
    ctx
}

#[test]
fn every_register_modrm_and_rex_shape_strictly_lifts_with_exact_register_files() {
    // 2 directions * 17 REX choices * 64 register ModR/M values = 2,176
    // complete encodings. REX.R extends only MOVQ2DQ's XMM destination;
    // REX.B extends only MOVDQ2Q's XMM source.
    let mut probes = 0usize;
    for rep in [0xF2, 0xF3] {
        for rex in std::iter::once(None).chain((0x40..=0x4F).map(Some)) {
            for reg in 0u8..8 {
                for rm in 0u8..8 {
                    let mut bytes = vec![rep];
                    if let Some(rex) = rex {
                        bytes.push(rex);
                    }
                    bytes.extend_from_slice(&[0x0F, 0xD6, 0xC0 | reg << 3 | rm]);
                    let result = lift_single(&bytes)
                        .unwrap_or_else(|error| panic!("lift {bytes:02X?}: {error:?}"));
                    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
                    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
                    assert_eq!(result.ops.len(), 2, "{bytes:02X?}");
                    assert_eq!(enter_mmx_count(&result), 1, "{bytes:02X?}");

                    let rex = rex.unwrap_or(0x40);
                    let (expected_dst, expected_src, expected_prefix) = if rep == 0xF3 {
                        (
                            VReg::Arch(ArchReg::X86(X86Reg::Xmm(reg | ((rex >> 2) & 1) << 3))),
                            VReg::Arch(ArchReg::X86(X86Reg::Mm(rm))),
                            X86SsePrefix::Rep,
                        )
                    } else {
                        (
                            VReg::Arch(ArchReg::X86(X86Reg::Mm(reg))),
                            VReg::Arch(ArchReg::X86(X86Reg::Xmm(rm | (rex & 1) << 3))),
                            X86SsePrefix::Repne,
                        )
                    };
                    assert!(matches!(
                        transfer(&result).kind,
                        OpKind::X86MovdQ {
                            dst,
                            src,
                            width: OpWidth::W64,
                            zero_upper: false,
                        } if dst == expected_dst && src == expected_src
                    ));
                    assert_eq!(
                        transfer(&result).x86_hint,
                        Some(X86OpHint::SseOp {
                            prefix: expected_prefix,
                            opcode: 0xD6,
                        }),
                        "{bytes:02X?}",
                    );
                    probes += 1;
                }
            }
        }
    }
    assert_eq!(probes, 2 * 17 * 64);
}

#[test]
fn memory_reserved_prefix_rex2_and_truncation_shapes_fail_closed() {
    // Both instructions are register-only. Eight trailing bytes satisfy every
    // possible SIB/disp32 parse so rejection cannot be an incomplete-input
    // artifact.
    let mut memory_probes = 0usize;
    for rep in [0xF2, 0xF3] {
        for rex in std::iter::once(None).chain((0x40..=0x4F).map(Some)) {
            for modrm in 0u8..0xC0 {
                let mut bytes = vec![rep];
                if let Some(rex) = rex {
                    bytes.push(rex);
                }
                bytes.extend_from_slice(&[0x0F, 0xD6, modrm]);
                bytes.extend_from_slice(&[0; 8]);
                assert!(
                    matches!(lift_single(&bytes), Err(LiftError::InvalidEncoding { .. })),
                    "memory form was not rejected: {bytes:02X?}"
                );
                memory_probes += 1;
            }
        }
    }
    assert_eq!(memory_probes, 2 * 17 * 0xC0);

    for bytes in [
        &[0xF0, 0xF3, 0x0F, 0xD6, 0xC1][..],
        &[0xF0, 0xF2, 0x0F, 0xD6, 0xC1],
        &[0x66, 0xF3, 0x0F, 0xD6, 0xC1],
        &[0xF3, 0x66, 0x0F, 0xD6, 0xC1],
        &[0x66, 0xF2, 0x0F, 0xD6, 0xC1],
        &[0xF2, 0x66, 0x0F, 0xD6, 0xC1],
        &[0xF3, 0xD5, 0x80, 0xD6, 0xC1],
        &[0xF2, 0xD5, 0x80, 0xD6, 0xC1],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "reserved encoding was not rejected: {bytes:02X?}"
        );
    }
    for bytes in [&[0xF3, 0x0F, 0xD6][..], &[0xF2, 0x0F, 0xD6][..]] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::Incomplete { .. })
        ));
    }
    let bare = lift_single(&[0x0F, 0xD6, 0xC1]).unwrap();
    assert_eq!(bare.bytes_consumed, 2);
    assert!(bare.ops.is_empty());
    assert!(matches!(
        bare.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));

    // Address-size and segment prefixes are inert for ModR/M.mod=3 but remain
    // part of the exact instruction length.
    let prefixed = [0x64, 0x67, 0xF3, 0x44, 0x0F, 0xD6, 0xF8];
    let result = lift_single(&prefixed).unwrap();
    assert_eq!(result.bytes_consumed, prefixed.len());
    assert!(matches!(
        transfer(&result).kind,
        OpKind::X86MovdQ {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(15))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
            ..
        }
    ));
}

#[test]
fn canonical_interpreter_preserves_flags_other_registers_and_upper_backing_at_all_levels() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut movq2dq = context();
        let upper = 0xA5A5_5A5A_F0F0_0F0F;
        let untouched_mm = [0x0123_4567_89AB_CDEF, 1, 2, 3, 4, 5, 6, 7];
        let ArchRegState::X86_64(x86) = &mut movq2dq.arch_regs else {
            unreachable!();
        };
        x86.mm = untouched_mm;
        x86.xmm[15] = [upper; 16];
        x86.x87.tag_word = 0xFFFF;
        assert!(matches!(
            execute(&[0xF3, 0x44, 0x0F, 0xD6, 0xF8], level, &mut movq2dq),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let ArchRegState::X86_64(x86) = &movq2dq.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[15][0], untouched_mm[0], "{level:?}");
        assert_eq!(x86.xmm[15][1], 0, "{level:?}");
        assert!(x86.xmm[15][2..].iter().all(|word| *word == upper));
        assert_eq!(x86.mm, untouched_mm);
        assert_eq!(x86.x87.tag_word, 0);
        assert_eq!(
            movq2dq.flags.materialized.to_rflags(),
            INITIAL_FLAGS,
            "{level:?}"
        );

        let mut movdq2q = context();
        let source = [
            0x8877_6655_4433_2211,
            0xDEAD_BEEF_CAFE_BABE,
            2,
            3,
            4,
            5,
            6,
            7,
            8,
            9,
            10,
            11,
            12,
            13,
            14,
            15,
        ];
        let ArchRegState::X86_64(x86) = &mut movdq2q.arch_regs else {
            unreachable!();
        };
        x86.xmm[14] = source;
        x86.mm = untouched_mm;
        x86.x87.tag_word = 0xFFFF;
        assert!(matches!(
            execute(&[0xF2, 0x41, 0x0F, 0xD6, 0xFE], level, &mut movdq2q),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let ArchRegState::X86_64(x86) = &movdq2q.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.mm[7], source[0], "{level:?}");
        assert_eq!(&x86.mm[..7], &untouched_mm[..7]);
        assert_eq!(x86.xmm[14], source);
        assert_eq!(x86.x87.tag_word, 0);
        assert_eq!(
            movdq2q.flags.materialized.to_rflags(),
            INITIAL_FLAGS,
            "{level:?}"
        );
    }
}
