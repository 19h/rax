use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

fn assert_reserved_nop_result(result: &LiftResult, bytes: &[u8], requires_apx: bool) {
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    if requires_apx {
        assert!(
            matches!(
                result.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::X86RequireApx,
                    guest_pc: 0x1000,
                    ..
                }]
            ),
            "{bytes:02X?}: {:?}",
            result.ops
        );
    } else {
        assert!(result.ops.is_empty(), "{bytes:02X?}: {:?}", result.ops);
    }
    assert!(result.branch_targets.is_empty(), "{bytes:02X?}");
    assert!(
        matches!(result.control_flow, ControlFlow::Fallthrough),
        "{bytes:02X?}"
    );
}

#[test]
fn reserved_nop_0f19_strictly_lifts_every_modrm_register_form() {
    for modrm in 0xC0..=0xFF {
        let bytes = [0x0F, 0x19, modrm];
        let result =
            lift_single(&bytes).unwrap_or_else(|error| panic!("0F 19 {modrm:02X}: {error:?}"));
        assert_reserved_nop_result(&result, &bytes, false);
    }
}

#[test]
fn reserved_nop_0f19_consumes_prefixes_rex2_and_complete_address_forms() {
    let cases: &[(&[u8], bool)] = &[
        (&[0x66, 0x0F, 0x19, 0xC0], false),
        (&[0x67, 0x0F, 0x19, 0xC0], false),
        (&[0xF2, 0x0F, 0x19, 0xC0], false),
        (&[0xF3, 0x0F, 0x19, 0xC0], false),
        (&[0x2E, 0x0F, 0x19, 0xC0], false),
        (&[0x48, 0x0F, 0x19, 0xC0], false),
        (&[0x0F, 0x19, 0x00], false),
        (&[0x0F, 0x19, 0x7F, 0x80], false),
        (&[0x0F, 0x19, 0x80, 0x78, 0x56, 0x34, 0x12], false),
        (&[0x0F, 0x19, 0x04, 0x25, 0x78, 0x56, 0x34, 0x12], false),
        (&[0x0F, 0x19, 0x05, 0x78, 0x56, 0x34, 0x12], false),
        (
            &[0x67, 0x0F, 0x19, 0x04, 0x25, 0x78, 0x56, 0x34, 0x12],
            false,
        ),
        (
            &[0x4F, 0x0F, 0x19, 0x84, 0x7F, 0x78, 0x56, 0x34, 0x12],
            false,
        ),
        (&[0xD5, 0x80, 0x19, 0xC0], true),
        (
            &[0xD5, 0xFF, 0x19, 0x84, 0x7F, 0x78, 0x56, 0x34, 0x12],
            true,
        ),
        (&[0x66, 0x67, 0xF3, 0x2E, 0xD5, 0x80, 0x19, 0xC0], true),
    ];

    for &(bytes, requires_apx) in cases {
        let result = lift_single(bytes)
            .unwrap_or_else(|error| panic!("reserved NOP {bytes:02X?}: {error:?}"));
        assert_reserved_nop_result(&result, bytes, requires_apx);
    }
}

#[test]
fn every_rex2_empty_hint_form_gets_one_dynamic_apx_guard() {
    for opcode in [0x0D, 0x18, 0x19, 0x1A, 0x1B, 0x1E, 0x1F] {
        for payload in 0x80_u8..=0xFF {
            for modrm in 0xC0_u8..=0xFF {
                let bytes = [0xD5, payload, opcode, modrm];
                let result = lift_single(&bytes)
                    .unwrap_or_else(|error| panic!("REX2 empty hint {bytes:02X?}: {error:?}"));
                assert_reserved_nop_result(&result, &bytes, true);
            }
        }
    }
}

#[test]
fn reserved_nop_0f19_reports_exact_incomplete_address_boundaries() {
    for (bytes, have, need) in [
        (&[0x0F, 0x19][..], 0, 1),
        (&[0x0F, 0x19, 0x04][..], 1, 2),
        (&[0x0F, 0x19, 0x04, 0x25][..], 2, 6),
        (&[0x0F, 0x19, 0x80, 0x78, 0x56][..], 3, 5),
        (&[0xD5, 0x80, 0x19][..], 0, 1),
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::Incomplete {
                    have: got_have,
                    need: got_need,
                    ..
                }) if got_have == have && got_need == need
            ),
            "{bytes:02X?}: expected have={have}, need={need}"
        );
    }
}

#[test]
fn lock_reserved_nop_0f19_is_an_explicit_ud_without_operand_fetch() {
    for bytes in [&[0xF0, 0x0F, 0x19][..], &[0xF0, 0xD5, 0x80, 0x19][..]] {
        let result = lift_single(bytes)
            .unwrap_or_else(|error| panic!("LOCK reserved NOP {bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(result.ops.is_empty(), "{bytes:02X?}");
        assert!(result.branch_targets.is_empty(), "{bytes:02X?}");
        assert!(
            matches!(
                result.control_flow,
                ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode
                }
            ),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn lock_every_empty_hint_form_is_an_explicit_ud_without_operand_fetch() {
    for opcode in [0x0D, 0x18, 0x19, 0x1A, 0x1B, 0x1E, 0x1F] {
        for bytes in [vec![0xF0, 0x0F, opcode], vec![0xF0, 0xD5, 0x80, opcode]] {
            let result = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("LOCK empty hint {bytes:02X?}: {error:?}"));
            assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
            assert!(result.ops.is_empty(), "{bytes:02X?}");
            assert!(result.branch_targets.is_empty(), "{bytes:02X?}");
            assert!(
                matches!(
                    result.control_flow,
                    ControlFlow::Trap {
                        kind: TrapKind::InvalidOpcode
                    }
                ),
                "{bytes:02X?}"
            );
        }
    }
}

#[test]
fn reserved_nop_0f19_keeps_the_following_instruction_in_the_strict_function() {
    // Reserved NOP with a SIB+disp32 form; ADD RAX,1; RET.
    let code = vec![
        0x0F, 0x19, 0x04, 0x25, 0x78, 0x56, 0x34, 0x12, 0x48, 0x83, 0xC0, 0x01, 0xC3,
    ];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let function = lifter
        .lift_function(0x1800, &TestMemory::new(0x1800, code), &mut context)
        .expect("reserved NOP must not create an interpreter frontier");

    assert_eq!(function.blocks.len(), 1);
    let block = &function.blocks[0];
    assert_eq!(block.guest_pc, 0x1800);
    assert!(!block.ops.is_empty());
    assert!(block.ops.iter().any(|op| op.guest_pc == 0x1808));
    assert!(
        block.ops.iter().all(|op| op.guest_pc != 0x1800),
        "the empty reserved NOP must contribute no SMIR operation: {:?}",
        block.ops
    );
    assert!(matches!(block.terminator, Terminator::Return { .. }));
}

#[test]
fn rex2_reserved_nop_guard_metadata_interpretation_and_o2_are_exact() {
    let op = lift_single(&[0xD5, 0x80, 0x19, 0xC0])
        .expect("REX2 reserved NOP")
        .ops
        .remove(0);
    assert!(matches!(op.kind, OpKind::X86RequireApx));
    assert!(op.kind.source_vregs().is_empty());
    assert!(op.kind.dests().is_empty());
    assert!(op.kind.flags_read().is_empty());
    assert!(op.kind.flags_written().is_empty());
    assert!(op.kind.has_side_effects());
    assert!(!op.kind.reads_memory());
    assert!(!op.kind.writes_memory());
    assert!(op.kind.is_jit_safe());
    assert!(op.is_jit_safe());

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for pc in [0x1000, 0x1004, 0x1008] {
        builder.push_op(pc, OpKind::X86RequireApx);
    }
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let original = builder.finish();
    let mut optimized = original.clone();
    optimize_function(&mut optimized, OptLevel::O2);
    assert_eq!(
        optimized
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86RequireApx))
            .count(),
        3
    );

    for function in [&original, &optimized] {
        for enabled in [false, true] {
            let mut context = SmirContext::new_x86_64();
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.apx_enabled = enabled;
            x86.gpr[0] = 0xA5A5_5A5A_DEAD_BEEF;
            x86.rflags = 0x0CD7;
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut FlatMemory::new(1),
                function.entry_block().unwrap(),
            );
            if enabled {
                assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
            } else {
                assert!(matches!(
                    result,
                    BlockResult::Exit(ExitReason::Undefined {
                        addr: 0x1000,
                        opcode: 0
                    })
                ));
            }
            let ArchRegState::X86_64(x86) = context.arch_regs else {
                unreachable!()
            };
            assert_eq!(x86.gpr[0], 0xA5A5_5A5A_DEAD_BEEF);
            assert_eq!(x86.rflags, 0x0CD7);
        }
    }

    let mut non_x86 = SmirContext::new_aarch64();
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut non_x86,
            &mut FlatMemory::new(1),
            original.entry_block().unwrap()
        ),
        BlockResult::Exit(ExitReason::Undefined {
            addr: 0x1000,
            opcode: 0
        })
    ));
}
