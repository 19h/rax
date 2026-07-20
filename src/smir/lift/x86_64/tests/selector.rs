//! Strict lift, metadata, optimizer, and interpreter coverage for SLDT/STR.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::ops::{X86SystemSelector, X86SystemSelectorStoreOp, X86SystemSelectorTarget};
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_selector(result: &LiftResult) -> &X86SystemSelectorStoreOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86SystemSelectorStore(store) => store,
        other => panic!("expected one exact X86SystemSelectorStore op, got {other:?}"),
    }
}

fn selector_block(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict SLDT/STR lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn execute_register(
    bytes: &[u8],
    configure: impl FnOnce(&mut SmirContext),
) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    configure(&mut context);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &selector_block(bytes),
    );
    (result, context)
}

#[test]
fn selector_stores_strictly_lift_both_selectors_register_widths_and_rex_extensions() {
    for (bytes, selector, dst, width, requires_apx) in [
        (
            &[0x0F, 0x00, 0xC0][..],
            X86SystemSelector::Ldtr,
            0,
            OpWidth::W32,
            false,
        ),
        (
            &[0x66, 0x0F, 0x00, 0xCD],
            X86SystemSelector::Tr,
            5,
            OpWidth::W16,
            false,
        ),
        (
            &[0x4D, 0x0F, 0x00, 0xCF],
            X86SystemSelector::Tr,
            15,
            OpWidth::W64,
            false,
        ),
        (
            &[0xD5, 0x91, 0x00, 0xC7],
            X86SystemSelector::Ldtr,
            31,
            OpWidth::W32,
            true,
        ),
        (
            &[0xD5, 0x99, 0x00, 0xC8],
            X86SystemSelector::Tr,
            24,
            OpWidth::W64,
            true,
        ),
    ] {
        let result = lift_single(bytes).expect("SLDT/STR register form must strictly lift");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            exact_selector(&result),
            X86SystemSelectorStoreOp {
                selector: got_selector,
                target: X86SystemSelectorTarget::Register {
                    dst: got_dst,
                    width: got_width,
                },
                requires_apx: got_apx,
            } if *got_selector == selector
                && *got_dst == x86_gpr(dst)
                && *got_width == width
                && *got_apx == requires_apx
        ));
    }
}

#[test]
fn selector_stores_lift_fixed_two_byte_memory_addresses_and_apx_components() {
    let direct = lift_single(&[0x0F, 0x00, 0x08]).unwrap();
    assert!(matches!(
        exact_selector(&direct),
        X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Tr,
            target: X86SystemSelectorTarget::Memory {
                addr: Address::Direct(base),
            },
            requires_apx: false,
        } if *base == x86_gpr(0)
    ));

    let sib = lift_single(&[0x48, 0x0F, 0x00, 0x44, 0x88, 0x7F]).unwrap();
    assert!(matches!(
        &exact_selector(&sib).target,
        X86SystemSelectorTarget::Memory {
            addr: Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x7F,
                disp_size: DispSize::Disp8,
            }
        } if *base == x86_gpr(0) && *index == x86_gpr(1)
    ));

    let addr32 = lift_single(&[0x67, 0x0F, 0x00, 0x8C, 0x8D, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert!(matches!(
        &exact_selector(&addr32).target,
        X86SystemSelectorTarget::Memory {
            addr: Address::X86Addr32(inner),
        } if matches!(
            inner.as_ref(),
            Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x1234_5678,
                disp_size: DispSize::Disp32,
            } if *base == x86_gpr(5) && *index == x86_gpr(1)
        )
    ));

    let apx = lift_single(&[0xD5, 0xB3, 0x00, 0x0C, 0xD1]).unwrap();
    assert!(matches!(
        exact_selector(&apx),
        X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Tr,
            target: X86SystemSelectorTarget::Memory {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 8,
                    ..
                },
            },
            requires_apx: true,
        } if *base == x86_gpr(25) && *index == x86_gpr(26)
    ));
}

#[test]
fn selector_stores_honor_prefixes_reject_lock_and_leave_other_group6_unsupported() {
    let rex_w_wins = lift_single(&[0x66, 0x48, 0x0F, 0x00, 0xC0]).unwrap();
    assert!(matches!(
        exact_selector(&rex_w_wins).target,
        X86SystemSelectorTarget::Register {
            width: OpWidth::W64,
            ..
        }
    ));
    for prefix in [0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x40, 0xF2, 0xF3] {
        let bytes = [prefix, 0x0F, 0x00, 0xC8];
        assert_eq!(lift_single(&bytes).unwrap().bytes_consumed, bytes.len());
    }
    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x00, 0xC0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
    for modrm in [0xD0, 0xD8, 0xE0, 0xE8] {
        assert!(matches!(
            lift_single(&[0x0F, 0x00, modrm]),
            Err(LiftError::Unsupported { .. })
        ));
    }
}

#[test]
fn selector_store_metadata_exposes_register_and_faulting_memory_effects() {
    let register = &lift_single(&[0x66, 0x0F, 0x00, 0xCD]).unwrap().ops[0];
    assert!(register.kind.source_vregs().is_empty());
    assert_eq!(register.kind.dests(), vec![x86_gpr(5)]);
    assert!(register.kind.flags_read().is_empty());
    assert!(register.kind.flags_written().is_empty());
    assert!(register.kind.has_side_effects());
    assert!(!register.kind.reads_memory());
    assert!(!register.kind.writes_memory());
    assert!(register.is_jit_safe());

    let memory = &lift_single(&[0x0F, 0x00, 0x4C, 0x48, 0x08]).unwrap().ops[0];
    assert_eq!(memory.kind.source_vregs(), vec![x86_gpr(1), x86_gpr(0)]);
    assert!(memory.kind.dests().is_empty());
    assert!(memory.kind.has_side_effects());
    assert!(!memory.kind.reads_memory());
    assert!(memory.kind.writes_memory());
    assert!(memory.is_jit_safe());
}

#[test]
fn selector_store_interpreter_commits_exact_widths_selectors_and_preserves_flags() {
    let incoming = 0xA5A5_5A5A_DEAD_BEEF;
    let flags = MaterializedFlags {
        cf: true,
        zf: false,
        sf: true,
        of: true,
        pf: false,
        af: true,
        df: true,
        ac: true,
    };
    for (bytes, dst, selector, expected, apx) in [
        (
            &[0x66, 0x0F, 0x00, 0xC3][..],
            3,
            0x1357_u16,
            (incoming & !0xFFFF) | 0x1357,
            false,
        ),
        (&[0x0F, 0x00, 0xCB], 3, 0x2468, 0x2468, false),
        (&[0x48, 0x0F, 0x00, 0xCB], 3, 0x2468, 0x2468, false),
        (&[0xD5, 0x91, 0x00, 0xC7], 31, 0xBEEF, 0xBEEF, true),
    ] {
        let (result, context) = execute_register(bytes, |context| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = 1;
            x86.ldtr_selector = selector;
            x86.tr_selector = selector;
            x86.apx_enabled = apx;
            context.flags.materialized = flags;
            context.write_vreg(x86_gpr(dst), incoming);
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(context.read_vreg(x86_gpr(dst)), expected, "{bytes:02X?}");
        assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
        assert!(context.flags.lazy.is_none());
    }
}

#[test]
fn selector_store_interpreter_fault_order_is_apx_mode_umip_then_memory() {
    let bytes = [0xD5, 0x91, 0x00, 0xC7];
    let sentinel = 0x3131_3131_3131_3131;
    for (name, apx, cr0, rflags, cr4, cpl, expected_undefined) in [
        ("APX", false, 0, 1 << 17, 1 << 11, 3, true),
        ("real mode", true, 0, 0, 1 << 11, 3, true),
        ("VM86", true, 1, 1 << 17, 1 << 11, 3, true),
        ("UMIP", true, 1, 0, 1 << 11, 3, false),
    ] {
        let (fault, context) = execute_register(&bytes, |context| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.apx_enabled = apx;
            x86.cr0 = cr0;
            x86.rflags = rflags;
            x86.cr4 = cr4;
            x86.cpl = cpl;
            x86.ldtr_selector = 0x1234;
            context.write_vreg(x86_gpr(31), sentinel);
        });
        if expected_undefined {
            assert!(
                matches!(fault, BlockResult::Exit(ExitReason::Undefined { .. })),
                "{name}"
            );
        } else {
            assert!(
                matches!(
                    fault,
                    BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
                ),
                "{name}"
            );
        }
        assert_eq!(context.read_vreg(x86_gpr(31)), sentinel, "{name}");
    }

    let block = selector_block(&[0x0F, 0x00, 0x08]);
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.tr_selector = 0xBEEF;
    context.write_vreg(x86_gpr(0), 0x2001);
    let mut memory = FlatMemory::with_base(0x2000, 4);
    memory.load(0, &[0xA5; 4]);
    let result = SmirInterpreter::new().execute_block(&mut context, &mut memory, &block);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let mut observed = [0; 4];
    memory.read(0x2000, &mut observed).unwrap();
    assert_eq!(observed, [0xA5, 0xEF, 0xBE, 0xA5]);

    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr4 = 1 << 11;
    x86.cpl = 3;
    memory.load(0, &[0x5A; 4]);
    let fault = SmirInterpreter::new().execute_block(&mut context, &mut memory, &block);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::GeneralProtection { .. })
    ));
    memory.read(0x2000, &mut observed).unwrap();
    assert_eq!(observed, [0x5A; 4]);
}

#[test]
fn selector_store_interpreter_rejects_malformed_target_and_o2_retains_effects() {
    let malformed = OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
        selector: X86SystemSelector::Ldtr,
        target: X86SystemSelectorTarget::Register {
            dst: x86_gpr(16),
            width: OpWidth::W64,
        },
        requires_apx: false,
    });
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, malformed);
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let function = builder.finish();
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.ldtr_selector = 0x1234;
    context.write_vreg(x86_gpr(16), 0xA5A5);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        function.entry_block().unwrap(),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { .. })
    ));
    assert_eq!(context.read_vreg(x86_gpr(16)), 0xA5A5);

    let mut builder = FunctionBuilder::new(FunctionId(1), 0x2000);
    builder.push_op(
        0x2000,
        OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Ldtr,
            target: X86SystemSelectorTarget::Register {
                dst: x86_gpr(0),
                width: OpWidth::W32,
            },
            requires_apx: false,
        }),
    );
    builder.push_op(
        0x2003,
        OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Tr,
            target: X86SystemSelectorTarget::Memory {
                addr: Address::Direct(x86_gpr(3)),
            },
            requires_apx: false,
        }),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);
    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86SystemSelectorStore(..)))
            .count(),
        2
    );
}
