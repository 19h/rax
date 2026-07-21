//! Strict lift, canonical interpretation, and optimizer coverage for
//! `MOV Sreg,r/m` (`8E /r`).

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, X86SystemSegmentCache};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::ops::{X86SystemSelector, X86SystemSelectorLoadOp, X86SystemSelectorSource};
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_load(result: &LiftResult) -> &X86SystemSelectorLoadOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86SystemSelectorLoad(load) => load,
        other => panic!("expected one exact X86SystemSelectorLoad op, got {other:?}"),
    }
}

fn selector_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict MOV Sreg,r/m lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn data_descriptor(
    base: u64,
    raw_limit: u32,
    type_: u8,
    dpl: u8,
    present: bool,
    system: bool,
    granularity: bool,
) -> [u8; 8] {
    assert!(raw_limit <= 0xF_FFFF);
    let mut raw = u64::from(raw_limit & 0xFFFF)
        | ((base & 0xFFFF) << 16)
        | (((base >> 16) & 0xFF) << 32)
        | (u64::from(type_ & 0xF) << 40)
        | (u64::from(!system) << 44)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (u64::from((raw_limit >> 16) & 0xF) << 48)
        | (1 << 52)
        | (1 << 54)
        | (((base >> 24) & 0xFF) << 56);
    if granularity {
        raw |= 1 << 55;
    }
    raw.to_le_bytes()
}

fn protected_context(selector: u64, cpl: u8) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.cs_l = true;
    x86.efer = 1 << 10;
    x86.cpl = cpl;
    x86.gdtr_base = 0x2000;
    x86.gdtr_limit = 0x1F;
    context.write_vreg(x86_gpr(0), selector);
    context
}

fn execute_register(
    field: u8,
    selector: u64,
    cpl: u8,
    descriptor: Option<[u8; 8]>,
) -> (BlockResult, SmirContext, FlatMemory) {
    let mut context = protected_context(selector, cpl);
    let mut memory = FlatMemory::with_base(0x2000, 0x100);
    if let Some(descriptor) = descriptor {
        memory.load(0x10, &descriptor);
    }
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        &selector_block(&[0x8E, 0xC0 | (field << 3)]),
    );
    (result, context, memory)
}

#[test]
fn mov_sreg_rm_strictly_lifts_all_legal_register_selectors_and_ignores_r_extensions() {
    for (field, selector) in [
        (0, X86SystemSelector::Es),
        (2, X86SystemSelector::Ss),
        (3, X86SystemSelector::Ds),
        (4, X86SystemSelector::Fs),
        (5, X86SystemSelector::Gs),
    ] {
        for (prefix, source) in [(&[][..], 0), (&[0x66][..], 0), (&[0x4D][..], 8)] {
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(&[0x8E, 0xC0 | (field << 3)]);
            let result = lift_single(&bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
            assert!(matches!(
                exact_load(&result),
                X86SystemSelectorLoadOp {
                    selector: got_selector,
                    source: X86SystemSelectorSource::Register { src },
                    requires_apx: false,
                    next_pc,
                } if *got_selector == selector
                    && *src == x86_gpr(source)
                    && *next_pc == 0x1000 + bytes.len() as u64
            ));
        }
    }
}

#[test]
fn mov_sreg_rm_rex2_map0_exhaustively_ignores_r_fields_and_extends_only_source() {
    for payload in 0_u8..=0x7F {
        let bytes = [0xD5, payload, 0x8E, 0xE0]; // MOV FS,r/m
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("REX2 payload {payload:#04x}: {error:?}"));
        let source =
            (if payload & 0x10 != 0 { 16 } else { 0 }) | (if payload & 0x01 != 0 { 8 } else { 0 });
        assert!(matches!(
            exact_load(&result),
            X86SystemSelectorLoadOp {
                selector: X86SystemSelector::Fs,
                source: X86SystemSelectorSource::Register { src },
                requires_apx: true,
                next_pc: 0x1004,
            } if *src == x86_gpr(source)
        ));
    }
}

#[test]
fn mov_sreg_rm_lifts_exact_memory_width_address_and_stack_fault_metadata() {
    for (bytes, width, stack_segment) in [
        (&[0x8E, 0x18][..], MemWidth::B2, false),
        (&[0x48, 0x8E, 0x5C, 0x24, 0x08], MemWidth::B8, true),
        (&[0x3E, 0x48, 0x8E, 0x5C, 0x24, 0x08], MemWidth::B8, false),
        (&[0x36, 0x8E, 0x18], MemWidth::B2, true),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            exact_load(&result),
            X86SystemSelectorLoadOp {
                selector: X86SystemSelector::Ds,
                source: X86SystemSelectorSource::Memory {
                    width: got_width,
                    stack_segment: got_stack,
                    ..
                },
                next_pc,
                ..
            } if *got_width == width
                && *got_stack == stack_segment
                && *next_pc == 0x1000 + bytes.len() as u64
        ));
    }

    let apx = lift_single(&[0xD5, 0x33, 0x8E, 0x64, 0xD1, 0x7F]).unwrap();
    assert!(matches!(
        exact_load(&apx),
        X86SystemSelectorLoadOp {
            selector: X86SystemSelector::Fs,
            source: X86SystemSelectorSource::Memory {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 8,
                    disp: 0x7F,
                    ..
                },
                width: MemWidth::B2,
                ..
            },
            requires_apx: true,
            ..
        } if *base == x86_gpr(25) && *index == x86_gpr(26)
    ));
}

#[test]
fn mov_sreg_rm_invalid_selectors_are_explicit_ud_and_lock_is_decode_invalid() {
    for modrm in [0xC8, 0xF0, 0xF8, 0x08] {
        let result = lift_single(&[0x8E, modrm]).unwrap();
        assert!(result.ops.is_empty());
        assert_eq!(result.bytes_consumed, 2);
        assert!(matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
    assert!(matches!(
        lift_single(&[0xF0, 0x8E, 0xC0]),
        Err(LiftError::InvalidEncoding { .. })
    ));

    // The raw selector field is sufficient to guarantee #UD. Apparent RIP-
    // relative, SIB, and disp8 forms therefore do not require their absent
    // address bytes before reaching the architectural trap frontier.
    for bytes in [&[0x8E, 0x0D][..], &[0x8E, 0x34], &[0x8E, 0x7D]] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, 2, "{bytes:02x?}");
        assert!(matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
}

#[test]
fn mov_sreg_rm_interpreter_loads_every_cache_sets_accessed_and_ss_shadow() {
    let base = 0x1234_5000;
    let raw_limit = 0xA_BCDE;
    for (field, selector) in [
        (0, X86SystemSelector::Es),
        (2, X86SystemSelector::Ss),
        (3, X86SystemSelector::Ds),
        (4, X86SystemSelector::Fs),
        (5, X86SystemSelector::Gs),
    ] {
        let descriptor = data_descriptor(base, raw_limit, 0x2, 0, true, false, true);
        let (result, context, mut memory) = execute_register(field, 0x10, 0, Some(descriptor));
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{selector:?}"
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        let (visible, cache) = match selector {
            X86SystemSelector::Es => (x86.es_selector, &x86.es_cache),
            X86SystemSelector::Ss => (x86.ss_selector, &x86.ss_cache),
            X86SystemSelector::Ds => (x86.ds_selector, &x86.ds_cache),
            X86SystemSelector::Fs => (x86.fs_selector, &x86.fs_cache),
            X86SystemSelector::Gs => (x86.gs_selector, &x86.gs_cache),
            _ => unreachable!(),
        };
        assert_eq!(visible, 0x10, "{selector:?}");
        assert_eq!(cache.base, base, "{selector:?}");
        assert_eq!(cache.limit, (raw_limit << 12) | 0xFFF, "{selector:?}");
        assert_eq!(cache.type_, 0x3, "{selector:?}");
        assert!(
            cache.present && cache.s && cache.g && cache.avl,
            "{selector:?}"
        );
        assert!(!cache.unusable, "{selector:?}");
        assert_eq!(x86.interrupt_inhibit, selector == X86SystemSelector::Ss);
        if selector == X86SystemSelector::Fs {
            assert_eq!(x86.fs_base, base);
        }
        if selector == X86SystemSelector::Gs {
            assert_eq!(x86.gs_base, base);
        }
        let mut accessed = [0_u8; 8];
        memory.read(0x2010, &mut accessed).unwrap();
        assert_ne!(u64::from_le_bytes(accessed) & (1 << 40), 0);
    }
}

#[test]
fn mov_sreg_rm_interpreter_fault_matrix_is_ordered_and_noncommitting() {
    struct Case {
        name: &'static str,
        field: u8,
        selector: u64,
        cpl: u8,
        descriptor: [u8; 8],
        vector: u8,
    }
    for case in [
        Case {
            name: "data RPL exceeds DPL",
            field: 3,
            selector: 0x13,
            cpl: 0,
            descriptor: data_descriptor(0, 0xFFFF, 0x2, 0, true, false, false),
            vector: 13,
        },
        Case {
            name: "unreadable code",
            field: 3,
            selector: 0x10,
            cpl: 0,
            descriptor: data_descriptor(0, 0xFFFF, 0x8, 0, true, false, false),
            vector: 13,
        },
        Case {
            name: "system descriptor",
            field: 3,
            selector: 0x10,
            cpl: 0,
            descriptor: data_descriptor(0, 0xFFFF, 0x2, 0, true, true, false),
            vector: 13,
        },
        Case {
            name: "data not present",
            field: 3,
            selector: 0x10,
            cpl: 0,
            descriptor: data_descriptor(0, 0xFFFF, 0x2, 0, false, false, false),
            vector: 11,
        },
        Case {
            name: "stack not writable",
            field: 2,
            selector: 0x10,
            cpl: 0,
            descriptor: data_descriptor(0, 0xFFFF, 0x0, 0, true, false, false),
            vector: 13,
        },
        Case {
            name: "stack not present",
            field: 2,
            selector: 0x10,
            cpl: 0,
            descriptor: data_descriptor(0, 0xFFFF, 0x2, 0, false, false, false),
            vector: 12,
        },
    ] {
        let (result, context, mut memory) =
            execute_register(case.field, case.selector, case.cpl, Some(case.descriptor));
        match (case.vector, result) {
            (13, BlockResult::Exit(ExitReason::GeneralProtection { error_code, .. })) => {
                assert_eq!(error_code, 0x10, "{}", case.name)
            }
            (11, BlockResult::Exit(ExitReason::SegmentNotPresent { error_code, .. })) => {
                assert_eq!(error_code, 0x10, "{}", case.name)
            }
            (12, BlockResult::Exit(ExitReason::StackSegment { error_code, .. })) => {
                assert_eq!(error_code, 0x10, "{}", case.name)
            }
            (_, other) => panic!("{}: wrong result {other:?}", case.name),
        }
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.ds_selector, 0, "{}", case.name);
        assert_eq!(x86.ss_selector, 0, "{}", case.name);
        assert!(!x86.interrupt_inhibit, "{}", case.name);
        let mut raw = [0_u8; 8];
        memory.read(0x2010, &mut raw).unwrap();
        assert_eq!(raw, case.descriptor, "{}", case.name);
    }
}

#[test]
fn mov_sreg_rm_null_real_vm86_and_long_mode_ss_rules_are_exact() {
    let (result, context, _) = execute_register(3, 3, 0, None);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.ds_selector, 3);
    assert!(x86.ds_cache.unusable);

    for (cpl, selector, allowed) in [(0, 0_u64, true), (2, 2, true), (3, 3, false)] {
        let (result, context, _) = execute_register(2, selector, cpl, None);
        assert_eq!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            allowed,
            "cpl={cpl} selector={selector}"
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.interrupt_inhibit, allowed);
    }

    for (virtual_8086, expected_dpl) in [(false, 0), (true, 3)] {
        let mut context = SmirContext::new_x86_64();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = u64::from(virtual_8086);
        x86.rflags = if virtual_8086 {
            crate::isa::x86_64::flags::bits::VM
        } else {
            0
        };
        x86.cpl = expected_dpl;
        context.write_vreg(x86_gpr(0), 0x1234);
        let mut memory = FlatMemory::new(1);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            &selector_block(&[0x8E, 0xE0]),
        );
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.fs_selector, 0x1234);
        assert_eq!(x86.fs_base, 0x1_2340);
        assert_eq!(x86.fs_cache.dpl, expected_dpl);
        assert!(!x86.fs_cache.unusable);
    }
}

#[test]
fn mov_sreg_rm_memory_w_reads_eight_bytes_and_faults_without_commit() {
    for (bytes, succeeds) in [(&[0x8E, 0x18][..], true), (&[0x48, 0x8E, 0x18], false)] {
        let mut context = protected_context(0, 0);
        context.write_vreg(x86_gpr(0), 0x3000);
        let mut memory = FlatMemory::with_base(0x2000, 0x1002);
        memory.load(0x1000, &[0, 0]);
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &selector_block(bytes));
        assert_eq!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            succeeds,
            "{bytes:02X?}: {result:?}"
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.ds_selector, 0);
        if !succeeds {
            assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault { addr: 0x3008, .. })
            ));
        }
    }
}

#[test]
fn mov_sreg_rm_metadata_and_optimizer_preserve_descriptor_side_effects() {
    let lifted = lift_single(&[0x8E, 0x18]).unwrap();
    let op = &lifted.ops[0];
    assert_eq!(op.kind.source_vregs(), vec![x86_gpr(0)]);
    assert!(op.kind.dests().is_empty());
    assert!(op.kind.reads_memory());
    assert!(op.kind.writes_memory());
    assert!(op.kind.has_side_effects());
    assert!(op.is_jit_safe());

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, lifted.ops[0].kind.clone());
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        optimize_function(&mut function, level);
        assert!(matches!(
            function.entry_block().unwrap().ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86SystemSelectorLoad(_),
                ..
            }]
        ));
    }

    // A malformed CS load remains fail-closed even when injected manually.
    let mut context = protected_context(0, 0);
    let mut block = selector_block(&[0x8E, 0xC0]);
    let OpKind::X86SystemSelectorLoad(load) = &mut block.ops[0].kind else {
        unreachable!()
    };
    load.selector = X86SystemSelector::Cs;
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &block);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.es_cache, X86SystemSegmentCache::default());
}
