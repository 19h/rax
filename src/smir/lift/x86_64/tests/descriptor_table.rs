//! Strict lift, metadata, optimizer, and interpreter descriptor-table coverage.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::ops::{
    X86DescriptorTable, X86DescriptorTableLoadOp, X86DescriptorTableStoreOp,
};
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_store(result: &LiftResult) -> &X86DescriptorTableStoreOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86DescriptorTableStore(store) => store,
        other => panic!("expected one exact X86DescriptorTableStore op, got {other:?}"),
    }
}

fn exact_load(result: &LiftResult) -> &X86DescriptorTableLoadOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86DescriptorTableLoad(load) => load,
        other => panic!("expected one exact X86DescriptorTableLoad op, got {other:?}"),
    }
}

fn descriptor_block(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict descriptor-table lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

#[test]
fn lgdt_lidt_strictly_lift_long_mode_addresses_and_exact_handoffs() {
    for (bytes, table, expected_addr) in [
        (
            &[0x0F, 0x01, 0x10][..],
            X86DescriptorTable::Gdt,
            Address::Direct(x86_gpr(0)),
        ),
        (
            &[0x48, 0x0F, 0x01, 0x5C, 0x88, 0x7F],
            X86DescriptorTable::Idt,
            Address::BaseIndexScale {
                base: Some(x86_gpr(0)),
                index: x86_gpr(1),
                scale: 4,
                disp: 0x7F,
                disp_size: DispSize::Disp8,
            },
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        let load = exact_load(&result);
        assert_eq!(load.table, table);
        assert_eq!(format!("{:?}", load.addr), format!("{expected_addr:?}"));
        assert!(!load.requires_apx);
        assert_eq!(load.next_pc, 0x1000 + bytes.len() as u64);
    }

    let addr32 = lift_single(&[0x67, 0x0F, 0x01, 0x9C, 0x8D, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert!(matches!(
        &exact_load(&addr32).addr,
        Address::X86Addr32(inner)
            if matches!(
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

    let apx = lift_single(&[0xD5, 0xB3, 0x01, 0x14, 0xD1]).unwrap();
    assert!(matches!(
        exact_load(&apx),
        X86DescriptorTableLoadOp {
            addr: Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 8,
                ..
            },
            table: X86DescriptorTable::Gdt,
            requires_apx: true,
            next_pc: 0x1005,
        } if *base == x86_gpr(25) && *index == x86_gpr(26)
    ));
}

#[test]
fn lgdt_lidt_ignore_data_width_prefixes_reject_lock_and_preserve_fixed_aliases() {
    for bytes in [
        &[0x66, 0x0F, 0x01, 0x10][..],
        &[0x48, 0x0F, 0x01, 0x18],
        &[0xF2, 0x0F, 0x01, 0x10],
        &[0xF3, 0x0F, 0x01, 0x18],
        &[0x64, 0x0F, 0x01, 0x10],
    ] {
        assert_eq!(lift_single(bytes).unwrap().bytes_consumed, bytes.len());
    }
    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x01, 0x10]),
        Err(LiftError::InvalidEncoding { .. })
    ));

    let xgetbv = lift_single(&[0x0F, 0x01, 0xD0]).unwrap();
    assert!(matches!(
        xgetbv.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86XGetBv { .. },
            ..
        }]
    ));
    let vmrun = lift_single(&[0x0F, 0x01, 0xD8]).unwrap();
    assert!(matches!(
        vmrun.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));
}

#[test]
fn descriptor_table_load_metadata_tracks_faulting_read_and_implicit_state() {
    let op = &lift_single(&[0x0F, 0x01, 0x5C, 0x88, 0x08]).unwrap().ops[0];
    assert_eq!(op.kind.source_vregs(), vec![x86_gpr(1), x86_gpr(0)]);
    assert!(op.kind.dests().is_empty());
    assert!(op.kind.flags_read().is_empty());
    assert!(op.kind.flags_written().is_empty());
    assert!(op.kind.has_side_effects());
    assert!(op.kind.reads_memory());
    assert!(!op.kind.writes_memory());
    assert!(op.is_jit_safe());
}

#[test]
fn descriptor_table_load_interpreter_commits_exact_payload_and_preserves_flags() {
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
    for (bytes, table, limit, base) in [
        (
            &[0x0F, 0x01, 0x10][..],
            X86DescriptorTable::Gdt,
            0x1357_u16,
            0x0001_0000_0000_1234_u64,
        ),
        (
            &[0x0F, 0x01, 0x18],
            X86DescriptorTable::Idt,
            0x2468,
            0xFEDC_BA98_7654_3210,
        ),
    ] {
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(x86_gpr(0), 0x20);
        context.flags.materialized = flags;
        let mut memory = FlatMemory::new(0x40);
        let mut payload = [0u8; 10];
        payload[..2].copy_from_slice(&limit.to_le_bytes());
        payload[2..].copy_from_slice(&base.to_le_bytes());
        memory.load(0x20, &payload);

        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            &descriptor_block(bytes),
        );
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        match table {
            X86DescriptorTable::Gdt => {
                assert_eq!((x86.gdtr_limit, x86.gdtr_base), (limit, base));
            }
            X86DescriptorTable::Idt => {
                assert_eq!((x86.idtr_limit, x86.idtr_base), (limit, base));
            }
        }
        assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
        assert!(context.flags.lazy.is_none());
    }
}

#[test]
fn descriptor_table_load_interpreter_orders_guards_and_never_partially_commits() {
    let guarded = descriptor_block(&[0xD5, 0x91, 0x01, 0x10]);
    for (name, apx, cpl, expected) in [("APX", false, 3, 6), ("CPL", true, 3, 13)] {
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(x86_gpr(16), u64::MAX);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.apx_enabled = apx;
        x86.cpl = cpl;
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &guarded);
        assert!(
            matches!(
                (expected, result),
                (
                    6,
                    BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
                ) | (
                    13,
                    BlockResult::Exit(ExitReason::GeneralProtection {
                        addr: 0x1000,
                        error_code: 0
                    })
                )
            ),
            "{name}"
        );
    }

    let mut context = SmirContext::new_x86_64();
    context.write_vreg(x86_gpr(0), 3);
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.gdtr_limit = 0xBEEF;
    x86.gdtr_base = u64::MAX;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(8),
        &descriptor_block(&[0x0F, 0x01, 0x10]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!((x86.gdtr_limit, x86.gdtr_base), (0xBEEF, u64::MAX));
}

#[test]
fn descriptor_table_load_rejects_malformed_ir_and_survives_o2() {
    let malformed = [
        X86DescriptorTableLoadOp {
            addr: Address::Direct(VReg::virt(0)),
            table: X86DescriptorTable::Gdt,
            requires_apx: false,
            next_pc: 0x1003,
        },
        X86DescriptorTableLoadOp {
            addr: Address::Direct(x86_gpr(31)),
            table: X86DescriptorTable::Idt,
            requires_apx: false,
            next_pc: 0x1003,
        },
        X86DescriptorTableLoadOp {
            addr: Address::Direct(x86_gpr(0)),
            table: X86DescriptorTable::Gdt,
            requires_apx: false,
            next_pc: 0x1000,
        },
    ];
    for load in malformed {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(SmirOp::new(
            OpId(0),
            0x1000,
            OpKind::X86DescriptorTableLoad(load),
        ));
        block.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let result = SmirInterpreter::new().execute_block(
            &mut SmirContext::new_x86_64(),
            &mut FlatMemory::new(1),
            &block,
        );
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Undefined { .. })
        ));
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86DescriptorTableLoad(X86DescriptorTableLoadOp {
            addr: Address::Direct(x86_gpr(0)),
            table: X86DescriptorTable::Gdt,
            requires_apx: false,
            next_pc: 0x1003,
        }),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);
    assert!(matches!(
        function.blocks[0].ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86DescriptorTableLoad(..),
            ..
        }]
    ));
}

#[test]
fn sgdt_sidt_strictly_lift_memory_only_state_backed_addresses() {
    for (bytes, table, expected_addr) in [
        (
            &[0x0F, 0x01, 0x00][..],
            X86DescriptorTable::Gdt,
            Address::Direct(x86_gpr(0)),
        ),
        (
            &[0x0F, 0x01, 0x4C, 0x88, 0x7F],
            X86DescriptorTable::Idt,
            Address::BaseIndexScale {
                base: Some(x86_gpr(0)),
                index: x86_gpr(1),
                scale: 4,
                disp: 0x7F,
                disp_size: DispSize::Disp8,
            },
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        let store = exact_store(&result);
        assert_eq!(store.table, table);
        assert_eq!(format!("{:?}", store.addr), format!("{expected_addr:?}"));
        assert!(!store.requires_apx);
    }

    let addr32 = lift_single(&[0x67, 0x0F, 0x01, 0x8C, 0x8D, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert!(matches!(
        &exact_store(&addr32).addr,
        Address::X86Addr32(inner)
            if matches!(
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

    let apx = lift_single(&[0xD5, 0xB3, 0x01, 0x04, 0xD1]).unwrap();
    assert!(matches!(
        exact_store(&apx),
        X86DescriptorTableStoreOp {
            addr: Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 8,
                ..
            },
            table: X86DescriptorTable::Gdt,
            requires_apx: true,
        } if *base == x86_gpr(25) && *index == x86_gpr(26)
    ));
}

#[test]
fn sgdt_sidt_ignore_data_width_prefixes_but_reject_lock() {
    for bytes in [
        &[0x66, 0x0F, 0x01, 0x00][..],
        &[0x48, 0x0F, 0x01, 0x08],
        &[0xF2, 0x0F, 0x01, 0x00],
        &[0xF3, 0x0F, 0x01, 0x08],
        &[0x64, 0x0F, 0x01, 0x00],
    ] {
        assert_eq!(lift_single(bytes).unwrap().bytes_consumed, bytes.len());
    }
    for bytes in [&[0xF0, 0x0F, 0x01, 0x00][..]] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn sgdt_sidt_do_not_intercept_group7_fixed_register_encodings() {
    let sgx = lift_single(&[0x0F, 0x01, 0xC0]).unwrap();
    assert!(matches!(
        sgx.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));

    let vmcall = lift_single(&[0x0F, 0x01, 0xC1]).unwrap();
    assert!(vmcall.ops.is_empty());
    assert!(matches!(vmcall.control_flow, ControlFlow::Fallthrough));

    let monitor = lift_single(&[0x0F, 0x01, 0xC8]).unwrap();
    assert!(matches!(
        monitor.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86MonitorMwait(_),
            ..
        }]
    ));

    let clac = lift_single(&[0x0F, 0x01, 0xCA]).unwrap();
    assert!(matches!(
        clac.ops.as_slice(),
        [SmirOp {
            kind: OpKind::SetAC { value: false },
            ..
        }]
    ));
}

#[test]
fn descriptor_table_store_metadata_tracks_address_and_faulting_write() {
    let op = &lift_single(&[0x0F, 0x01, 0x4C, 0x88, 0x08]).unwrap().ops[0];
    assert_eq!(op.kind.source_vregs(), vec![x86_gpr(1), x86_gpr(0)]);
    assert!(op.kind.dests().is_empty());
    assert!(op.kind.flags_read().is_empty());
    assert!(op.kind.flags_written().is_empty());
    assert!(op.kind.has_side_effects());
    assert!(!op.kind.reads_memory());
    assert!(op.kind.writes_memory());
    assert!(op.is_jit_safe());
}

#[test]
fn descriptor_table_interpreter_stores_exact_10_byte_payload_and_preserves_flags() {
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
    for (bytes, table, limit, base) in [
        (
            &[0x0F, 0x01, 0x00][..],
            X86DescriptorTable::Gdt,
            0x1357,
            0x0123_4567_89AB_CDEF,
        ),
        (
            &[0x0F, 0x01, 0x08],
            X86DescriptorTable::Idt,
            0x2468,
            0xFEDC_BA98_7654_3210,
        ),
    ] {
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(x86_gpr(0), 0x20);
        context.flags.materialized = flags;
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        match table {
            X86DescriptorTable::Gdt => {
                x86.gdtr_limit = limit;
                x86.gdtr_base = base;
            }
            X86DescriptorTable::Idt => {
                x86.idtr_limit = limit;
                x86.idtr_base = base;
            }
        }
        let mut memory = FlatMemory::new(0x40);
        memory.load(0x1F, &[0xA5; 12]);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            &descriptor_block(bytes),
        );
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        let mut observed = [0u8; 12];
        memory.read(0x1F, &mut observed).unwrap();
        let mut expected = [0xA5; 12];
        expected[1..3].copy_from_slice(&limit.to_le_bytes());
        expected[3..11].copy_from_slice(&base.to_le_bytes());
        assert_eq!(observed, expected);
        assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
        assert!(context.flags.lazy.is_none());
    }
}

#[test]
fn descriptor_table_interpreter_orders_apx_then_umip_before_memory() {
    let block = descriptor_block(&[0xD5, 0x91, 0x01, 0x00]);
    for (name, apx, cr4, cpl, expected) in [
        ("APX", false, 1 << 11, 3, 6),
        ("UMIP", true, 1 << 11, 3, 13),
    ] {
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(x86_gpr(16), u64::MAX);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.apx_enabled = apx;
        x86.cr4 = cr4;
        x86.cpl = cpl;
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &block);
        assert!(
            matches!(
                (expected, result),
                (
                    6,
                    BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
                ) | (
                    13,
                    BlockResult::Exit(ExitReason::GeneralProtection {
                        addr: 0x1000,
                        error_code: 0
                    })
                )
            ),
            "{name}"
        );
    }
}

#[test]
fn descriptor_table_interpreter_fault_is_noncommitting_and_o2_preserves_op() {
    let mut context = SmirContext::new_x86_64();
    context.write_vreg(x86_gpr(0), 3);
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.gdtr_limit = 0xBEEF;
    x86.gdtr_base = u64::MAX;
    let mut memory = FlatMemory::new(8);
    memory.load(0, &[0xCC; 8]);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        &descriptor_block(&[0x0F, 0x01, 0x00]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
    ));
    let mut observed = [0u8; 8];
    memory.read(0, &mut observed).unwrap();
    assert_eq!(observed, [0xCC; 8]);

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86DescriptorTableStore(X86DescriptorTableStoreOp {
            addr: Address::Direct(x86_gpr(0)),
            table: X86DescriptorTable::Gdt,
            requires_apx: false,
        }),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);
    assert!(matches!(
        function.blocks[0].ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86DescriptorTableStore(..),
            ..
        }]
    ));
}
