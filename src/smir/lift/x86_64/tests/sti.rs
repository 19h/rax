//! Strict lift, metadata, optimizer, and canonical-interpreter coverage for
//! x86 STI.

use super::*;
use crate::isa::x86_64::flags;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

fn sti_kind(result: &LiftResult) -> (bool, u64) {
    assert_eq!(result.ops.len(), 1);
    match result.ops[0].kind {
        OpKind::X86Sti {
            requires_apx,
            next_pc,
        } => (requires_apx, next_pc),
        ref other => panic!("expected one exact X86Sti op, got {other:?}"),
    }
}

fn sti_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict STI lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn execute_sti(
    bytes: &[u8],
    configure: impl FnOnce(&mut SmirContext),
) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    configure(&mut context);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &sti_block(bytes),
    );
    (result, context)
}

#[test]
fn sti_strictly_lifts_to_one_exact_control_flag_operation() {
    let result = lift_single(&[0xFB]).expect("STI must strictly lift");
    assert_eq!(result.bytes_consumed, 1);
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert_eq!(sti_kind(&result), (false, 0x1001));
    assert_eq!(result.ops[0].guest_pc, 0x1000);
}

#[test]
fn sti_ignores_non_lock_legacy_rex_and_every_rex2_payload_field() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
        0x66, 0x67, // operand/address size
        0x40, 0x41, 0x42, 0x44, 0x48, 0x4F, // representative REX
        0xF2, 0xF3, // repeat prefixes
    ] {
        let bytes = [prefix, 0xFB];
        let result = lift_single(&bytes).expect("architecturally ignored STI prefix");
        assert_eq!(result.bytes_consumed, bytes.len(), "prefix {prefix:#04x}");
        assert_eq!(sti_kind(&result), (false, 0x1002));
    }

    for payload in 0x00_u8..=0x7F {
        let bytes = [0xD5, payload, 0xFB];
        let result = lift_single(&bytes).expect("REX2 map-0 STI");
        assert_eq!(result.bytes_consumed, 3, "payload {payload:#04x}");
        assert_eq!(sti_kind(&result), (true, 0x1003));
    }
}

#[test]
fn sti_rejects_lock_and_legacy_rex_immediately_before_rex2() {
    for bytes in [&[0xF0, 0xFB][..], &[0xF0, 0xD5, 0x00, 0xFB]] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
    assert!(matches!(
        lift_single(&[0x48, 0xD5, 0x00, 0xFB]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}

#[test]
fn sti_metadata_is_operand_free_status_neutral_stateful_and_jit_safe() {
    let op = lift_single(&[0xFB]).unwrap().ops.remove(0);
    assert!(op.kind.source_vregs().is_empty());
    assert!(op.kind.dests().is_empty());
    assert!(op.kind.flags_read().is_empty());
    assert!(op.kind.flags_written().is_empty());
    assert!(op.kind.has_side_effects());
    assert!(!op.kind.reads_memory());
    assert!(!op.kind.writes_memory());
    assert!(op.kind.is_jit_safe());
    assert!(op.is_jit_safe());
}

#[test]
fn sti_interpreter_matches_if_vif_routing_and_exact_shadow_creation() {
    struct Case {
        name: &'static str,
        cr0: u64,
        cr4: u64,
        cpl: u8,
        initial: u64,
        set: u64,
        inhibit: bool,
    }
    let cases = [
        Case {
            name: "real-if-zero",
            cr0: 0,
            cr4: 0,
            cpl: 3,
            initial: 0x2,
            set: flags::bits::IF,
            inhibit: true,
        },
        Case {
            name: "real-if-already-one",
            cr0: 0,
            cr4: 0,
            cpl: 3,
            initial: flags::bits::IF | 0x2,
            set: flags::bits::IF,
            inhibit: false,
        },
        Case {
            name: "protected-cpl0",
            cr0: 1,
            cr4: 0,
            cpl: 0,
            initial: 0x2,
            set: flags::bits::IF,
            inhibit: true,
        },
        Case {
            name: "protected-cpl3-iopl3-vip-ignored",
            cr0: 1,
            cr4: 1 << 1,
            cpl: 3,
            initial: flags::bits::IOPL_MASK | flags::bits::VIP | 0x2,
            set: flags::bits::IF,
            inhibit: true,
        },
        Case {
            name: "protected-pvi",
            cr0: 1,
            cr4: 1 << 1,
            cpl: 3,
            initial: 0x2,
            set: flags::bits::VIF,
            inhibit: false,
        },
        Case {
            name: "virtual-8086-vme",
            cr0: 1,
            cr4: 1,
            cpl: 3,
            initial: flags::bits::VM | 0x2,
            set: flags::bits::VIF,
            inhibit: false,
        },
    ];

    for case in cases {
        let (result, context) = execute_sti(&[0xFB], |context| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = case.cr0;
            x86.cr4 = case.cr4;
            x86.cpl = case.cpl;
            x86.rflags = case.initial;
            x86.interrupt_inhibit = true;
            context.flags.materialized = MaterializedFlags {
                cf: true,
                df: true,
                ..Default::default()
            };
            context.flags.set_lazy_add(1, 2, 3, OpWidth::W64);
        });
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{}",
            case.name
        );
        let ArchRegState::X86_64(x86) = context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.rflags, case.initial | case.set, "{}", case.name);
        assert_eq!(x86.interrupt_inhibit, case.inhibit, "{}", case.name);
        assert!(context.flags.lazy.is_some(), "{}: status flags", case.name);
        assert!(context.flags.materialized.cf, "{}: CF", case.name);
        assert!(context.flags.materialized.df, "{}: DF", case.name);
    }
}

#[test]
fn sti_interpreter_gp_faults_are_precise_noncommitting_and_end_prior_shadow() {
    for (name, cr4, cpl, rflags) in [
        ("protected-cpl3", 0, 3, 0x2),
        ("protected-cpl2-pvi-does-not-apply", 1 << 1, 2, 0x2),
        ("protected-pvi-vip", 1 << 1, 3, flags::bits::VIP | 0x2),
        ("virtual-8086-without-vme", 0, 3, flags::bits::VM | 0x2),
        (
            "virtual-8086-vme-vip",
            1,
            3,
            flags::bits::VM | flags::bits::VIP | 0x2,
        ),
    ] {
        let (result, context) = execute_sti(&[0xFB], |context| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = 1;
            x86.cr4 = cr4;
            x86.cpl = cpl;
            x86.rflags = rflags;
            x86.interrupt_inhibit = true;
            context.flags.set_lazy_add(u64::MAX, 1, 0, OpWidth::W64);
        });
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::GeneralProtection {
                    addr: 0x1000,
                    error_code: 0
                })
            ),
            "{name}"
        );
        let ArchRegState::X86_64(x86) = context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.rflags, rflags, "{name}");
        assert!(!x86.interrupt_inhibit, "{name}");
        assert!(context.flags.lazy.is_some(), "{name}: status flags");
    }
}

#[test]
fn sti_rex2_apx_fault_precedes_privilege_and_non_x86_contexts_fail_closed() {
    let initial = flags::bits::VIP | 0x2;
    let (result, context) = execute_sti(&[0xD5, 0x00, 0xFB], |context| {
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.apx_enabled = false;
        x86.cr0 = 1;
        x86.cpl = 3;
        x86.rflags = initial;
        x86.interrupt_inhibit = true;
    });
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined {
            addr: 0x1000,
            opcode: 0
        })
    ));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rflags, initial);
    assert!(!x86.interrupt_inhibit);

    let mut non_x86 = SmirContext::new_aarch64();
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut non_x86,
            &mut FlatMemory::new(1),
            &sti_block(&[0xFB])
        ),
        BlockResult::Exit(ExitReason::Undefined {
            addr: 0x1000,
            opcode: 0
        })
    ));
}

#[test]
fn sti_o2_preserves_order_effect_and_fault_frontier() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for index in 0..3_u64 {
        builder.push_op(
            0x1000 + index,
            OpKind::X86Sti {
                requires_apx: false,
                next_pc: 0x1001 + index,
            },
        );
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let original = builder.finish();
    let mut optimized = original.clone();
    optimize_function(&mut optimized, OptLevel::O2);

    assert_eq!(
        optimized
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86Sti { .. }))
            .count(),
        3
    );

    for function in [&original, &optimized] {
        let mut context = SmirContext::new_x86_64();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 1;
        x86.cpl = 0;
        x86.rflags = 0x2;
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            function.entry_block().unwrap(),
        );
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Return { to: 0 })
        ));
        let ArchRegState::X86_64(x86) = context.arch_regs else {
            unreachable!()
        };
        assert_ne!(x86.rflags & flags::bits::IF, 0);
        // Only the first STI transitions IF from zero; later STI instructions
        // consume that shadow and do not create a new one.
        assert!(!x86.interrupt_inhibit);
    }
}
