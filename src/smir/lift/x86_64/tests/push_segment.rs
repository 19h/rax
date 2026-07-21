//! Strict lift, canonical interpretation, optimization, and metadata coverage
//! for long-mode `PUSH FS` (`0F A0`) and `PUSH GS` (`0F A8`).

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::ops::{X86SystemSelector, X86SystemSelectorStoreOp, X86SystemSelectorTarget};
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_push(result: &LiftResult) -> &X86SystemSelectorStoreOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86SystemSelectorStore(store) => store,
        other => panic!("expected one selector-store stack op, got {other:?}"),
    }
}

fn push_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict PUSH FS/GS lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn configure_long_mode(context: &mut SmirContext, rsp: u64, fs: u16, gs: u16) {
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.efer = 1 << 10;
    x86.cs_l = true;
    x86.gpr[4] = rsp;
    x86.fs_selector = fs;
    x86.gs_selector = gs;
}

#[test]
fn push_fs_gs_strictly_lift_exact_stack_width_and_rex_w_precedence() {
    for (bytes, selector, width) in [
        (&[0x0F, 0xA0][..], X86SystemSelector::Fs, MemWidth::B8),
        (&[0x66, 0x0F, 0xA8][..], X86SystemSelector::Gs, MemWidth::B2),
        (&[0x48, 0x0F, 0xA0][..], X86SystemSelector::Fs, MemWidth::B8),
        (
            &[0x66, 0x48, 0x0F, 0xA8][..],
            X86SystemSelector::Gs,
            MemWidth::B8,
        ),
        (
            &[0xF3, 0x67, 0x64, 0x0F, 0xA0][..],
            X86SystemSelector::Fs,
            MemWidth::B8,
        ),
        (
            &[0x66, 0x47, 0x0F, 0xA8][..],
            X86SystemSelector::Gs,
            MemWidth::B2,
        ),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            exact_push(&result),
            X86SystemSelectorStoreOp {
                selector: got_selector,
                target: X86SystemSelectorTarget::Stack {
                    stack_pointer,
                    width: got_width,
                },
                requires_apx: false,
            } if *got_selector == selector
                && *stack_pointer == x86_gpr(4)
                && *got_width == width
        ));
    }
}

#[test]
fn push_fs_gs_rex2_map1_exhaustively_ignores_non_w_payload_and_requires_apx() {
    for payload in 0x80_u8..=0xFF {
        for (legacy_prefix, selector, opcode) in [
            (&[][..], X86SystemSelector::Fs, 0xA0),
            (&[0x66][..], X86SystemSelector::Gs, 0xA8),
        ] {
            let mut bytes = legacy_prefix.to_vec();
            bytes.extend_from_slice(&[0xD5, payload, opcode]);
            let result = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("REX2 payload {payload:#04x}: {error:?}"));
            let expected_width = if legacy_prefix.is_empty() || payload & 0x08 != 0 {
                MemWidth::B8
            } else {
                MemWidth::B2
            };
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(matches!(
                exact_push(&result),
                X86SystemSelectorStoreOp {
                    selector: got_selector,
                    target: X86SystemSelectorTarget::Stack {
                        stack_pointer,
                        width,
                    },
                    requires_apx: true,
                } if *got_selector == selector
                    && *stack_pointer == x86_gpr(4)
                    && *width == expected_width
            ));
        }
    }
}

#[test]
fn push_fs_gs_reject_lock_and_invalid_rex2_order() {
    for bytes in [
        &[0xF0, 0x0F, 0xA0][..],
        &[0xF0, 0xD5, 0x80, 0xA8],
        &[0x48, 0xD5, 0x80, 0xA0],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn push_fs_gs_interpreter_commits_exact_width_only_after_store() {
    for (bytes, selector, width) in [
        (&[0x0F, 0xA0][..], 0x1357_u16, 8_usize),
        (&[0x66, 0x0F, 0xA8][..], 0xBEEF, 2),
        (&[0x66, 0x48, 0x0F, 0xA0][..], 0x1357, 8),
    ] {
        let initial_rsp = 0x2010;
        let mut context = SmirContext::new_x86_64();
        configure_long_mode(&mut context, initial_rsp, 0x1357, 0xBEEF);
        let initial_flags = 0x08D7;
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.rflags = initial_flags;

        let mut memory = FlatMemory::with_base(0x2000, 0x20);
        memory.load(0, &[0xA5; 0x20]);
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &push_block(bytes));
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));

        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.gpr[4], initial_rsp - width as u64, "{bytes:02X?}");
        assert_eq!(x86.rflags, initial_flags, "{bytes:02X?}");
        let mut observed = [0_u8; 8];
        memory
            .read(initial_rsp - width as u64, &mut observed[..width])
            .unwrap();
        assert_eq!(
            &observed[..width],
            &u64::from(selector).to_le_bytes()[..width],
            "{bytes:02X?}"
        );
    }
}

#[test]
fn push_fs_gs_interpreter_faults_are_precise_and_noncommitting() {
    let initial_rsp = 0x3008;
    let mut context = SmirContext::new_x86_64();
    configure_long_mode(&mut context, initial_rsp, 0xCAFE, 0xBEEF);
    let mut memory = FlatMemory::with_base(0x3000, 7);
    memory.load(0, &[0x5A; 7]);

    let fault =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &push_block(&[0x0F, 0xA0]));
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[4], initial_rsp);
    let mut observed = [0_u8; 7];
    memory.read(0x3000, &mut observed).unwrap();
    assert_eq!(observed, [0x5A; 7]);

    for (name, bytes, rsp) in [
        (
            "B8 lower canonical boundary",
            &[0x0F, 0xA0][..],
            0x0000_8000_0000_0004_u64,
        ),
        (
            "B8 upper canonical boundary",
            &[0x0F, 0xA0][..],
            0xFFFF_8000_0000_0004,
        ),
        ("B8 64-bit wrap", &[0x0F, 0xA0][..], 4),
        (
            "B2 lower canonical boundary",
            &[0x66, 0x0F, 0xA0][..],
            0x0000_8000_0000_0001,
        ),
        (
            "B2 upper canonical boundary",
            &[0x66, 0x0F, 0xA0][..],
            0xFFFF_8000_0000_0001,
        ),
        ("B2 64-bit wrap", &[0x66, 0x0F, 0xA0][..], 1),
    ] {
        configure_long_mode(&mut context, rsp, 0xCAFE, 0xBEEF);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            &push_block(bytes),
        );
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::StackSegment {
                    addr: 0x1000,
                    error_code: 0,
                })
            ),
            "{name}: {result:?}"
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.gpr[4], rsp, "{name}");
    }
}

#[test]
fn push_fs_gs_interpreter_apx_mode_and_shape_guards_are_noncommitting() {
    let initial_rsp = 0x4010;
    let mut context = SmirContext::new_x86_64();
    configure_long_mode(&mut context, initial_rsp, 0x1357, 0x2468);
    let mut memory = FlatMemory::with_base(0x4000, 0x20);
    memory.load(0, &[0xA5; 0x20]);

    let apx = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        &push_block(&[0xD5, 0x80, 0xA0]),
    );
    assert!(matches!(
        apx,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[4], initial_rsp);

    let mut legacy_mode = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut legacy_mode.arch_regs else {
        unreachable!()
    };
    x86.gpr[4] = initial_rsp;
    x86.fs_selector = 0x1357;
    let mode = SmirInterpreter::new().execute_block(
        &mut legacy_mode,
        &mut memory,
        &push_block(&[0x0F, 0xA0]),
    );
    assert!(matches!(
        mode,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
    let ArchRegState::X86_64(x86) = &legacy_mode.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[4], initial_rsp);

    let mut malformed = FunctionBuilder::new(FunctionId(0), 0x1000);
    malformed.push_op(
        0x1000,
        OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Cs,
            target: X86SystemSelectorTarget::Stack {
                stack_pointer: x86_gpr(4),
                width: MemWidth::B8,
            },
            requires_apx: false,
        }),
    );
    malformed.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let malformed = malformed.finish();
    configure_long_mode(&mut context, initial_rsp, 0x1357, 0x2468);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        malformed.entry_block().unwrap(),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
}

#[test]
fn push_fs_gs_metadata_and_optimizer_preserve_atomic_stack_effect() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for (bytes, selector, width, requires_apx) in [
            (
                &[0x0F, 0xA0][..],
                X86SystemSelector::Fs,
                MemWidth::B8,
                false,
            ),
            (
                &[0x66, 0x0F, 0xA8][..],
                X86SystemSelector::Gs,
                MemWidth::B2,
                false,
            ),
            (
                &[0x66, 0xD5, 0x88, 0xA8][..],
                X86SystemSelector::Gs,
                MemWidth::B8,
                true,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            let kind = &lifted.ops[0].kind;
            assert_eq!(kind.source_vregs(), vec![x86_gpr(4)]);
            assert_eq!(kind.dests(), vec![x86_gpr(4)]);
            assert!(kind.writes_memory());
            assert!(kind.has_side_effects());

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind.clone());
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            optimize_function(&mut function, level);
            assert!(matches!(
                function.blocks[0].ops.as_slice(),
                [SmirOp {
                    kind: OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
                        selector: got_selector,
                        target: X86SystemSelectorTarget::Stack {
                            stack_pointer,
                            width: got_width,
                        },
                        requires_apx: got_apx,
                    }),
                    ..
                }] if *got_selector == selector
                    && *stack_pointer == x86_gpr(4)
                    && *got_width == width
                    && *got_apx == requires_apx
            ));
        }
    }
}
