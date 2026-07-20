//! Fault-precise helper-backed native descriptor-table lowering.

use super::*;
use crate::smir::ir::ops::{
    X86DescriptorTable, X86DescriptorTableLoadOp, X86DescriptorTableStoreOp,
};
use crate::smir::lower::{
    X86_GUEST_DESCRIPTOR_LOAD_FN_OFFSET, X86_GUEST_DESCRIPTOR_STORE_FN_OFFSET,
};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn store(addr: Address, table: X86DescriptorTable, requires_apx: bool) -> OpKind {
    OpKind::X86DescriptorTableStore(X86DescriptorTableStoreOp {
        addr,
        table,
        requires_apx,
    })
}

fn load(addr: Address, table: X86DescriptorTable, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86DescriptorTableLoad(X86DescriptorTableLoadOp {
        addr,
        table,
        requires_apx,
        next_pc,
    })
}

fn lower(
    kind: OpKind,
    mem_helpers: bool,
    fault_guards: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_descriptor_table_store_requires_both_guards_and_helper_channel() {
    let op = store(
        Address::Direct(x86(X86Reg::Rax)),
        X86DescriptorTable::Gdt,
        false,
    );
    assert!(matches!(
        lower(op.clone(), true, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower(op.clone(), false, true),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let (code, _) = lower(op, true, true).expect("guarded descriptor-table store lowering");
    assert!(
        code.windows(4).any(|window| {
            window == (X86_GUEST_DESCRIPTOR_STORE_FN_OFFSET as u32).to_le_bytes()
        }),
        "missing descriptor helper offset: {code:02X?}"
    );
    for offset in [X86_GUEST_CR4_OFFSET, X86_GUEST_CPL_OFFSET] {
        assert!(
            code.windows(4)
                .any(|window| window == (offset as u32).to_le_bytes()),
            "missing dynamic guard offset {offset}: {code:02X?}"
        );
    }
    assert!(
        !code
            .windows(3)
            .any(|window| window[..2] == [0x0F, 0x01] && (window[2] >> 3) & 7 <= 1),
        "lowering must not execute host SGDT/SIDT: {code:02X?}"
    );
}

#[test]
fn lower_descriptor_table_store_rejects_every_non_lifter_shape() {
    for malformed in [
        store(
            Address::Direct(VReg::virt(0)),
            X86DescriptorTable::Gdt,
            false,
        ),
        store(
            Address::Direct(VReg::Arch(ArchReg::Arm(crate::smir::ir::types::ArmReg::X(
                0,
            )))),
            X86DescriptorTable::Idt,
            false,
        ),
        store(
            Address::Direct(x86(X86Reg::R31)),
            X86DescriptorTable::Gdt,
            false,
        ),
        store(
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rax)),
                index: x86(X86Reg::Rcx),
                scale: 3,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            X86DescriptorTable::Idt,
            false,
        ),
    ] {
        assert!(matches!(
            lower(malformed, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        store(Address::Absolute(0x4000), X86DescriptorTable::Gdt, false),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(matches!(
        lowerer.lower_function(&hinted),
        Err(LowerError::InvalidOperand { .. })
    ));
}

#[test]
fn lower_descriptor_table_load_requires_guards_helpers_serializes_and_never_executes_host_load() {
    let op = load(
        Address::Direct(x86(X86Reg::Rax)),
        X86DescriptorTable::Gdt,
        false,
        0x1003,
    );
    assert!(matches!(
        lower(op.clone(), true, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower(op.clone(), false, true),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let (code, _) = lower(op, true, true).expect("guarded descriptor-table load lowering");
    assert!(
        code.windows(4)
            .any(|window| { window == (X86_GUEST_DESCRIPTOR_LOAD_FN_OFFSET as u32).to_le_bytes() }),
        "missing descriptor-load helper offset: {code:02X?}"
    );
    for offset in [X86_GUEST_CPL_OFFSET] {
        assert!(
            code.windows(4)
                .any(|window| window == (offset as u32).to_le_bytes()),
            "missing dynamic guard offset {offset}: {code:02X?}"
        );
    }
    assert!(
        code.windows(2).any(|window| window == [0x0F, 0xA2]),
        "successful LGDT/LIDT must serialize: {code:02X?}"
    );
    assert!(
        !code.windows(3).any(|window| {
            window[..2] == [0x0F, 0x01]
                && matches!((window[2] >> 3) & 7, 2 | 3)
                && window[2] >> 6 != 3
        }),
        "lowering must not execute host LGDT/LIDT: {code:02X?}"
    );
}

#[test]
fn lower_descriptor_table_load_rejects_every_non_lifter_shape_and_frontier() {
    for malformed in [
        load(
            Address::Direct(VReg::virt(0)),
            X86DescriptorTable::Gdt,
            false,
            0x1003,
        ),
        load(
            Address::Direct(VReg::Arch(ArchReg::Arm(crate::smir::ir::types::ArmReg::X(
                0,
            )))),
            X86DescriptorTable::Idt,
            false,
            0x1003,
        ),
        load(
            Address::Direct(x86(X86Reg::R31)),
            X86DescriptorTable::Gdt,
            false,
            0x1003,
        ),
        load(
            Address::Direct(x86(X86Reg::Rax)),
            X86DescriptorTable::Idt,
            false,
            0x1000,
        ),
        load(
            Address::Direct(x86(X86Reg::Rax)),
            X86DescriptorTable::Idt,
            false,
            0x1010,
        ),
    ] {
        assert!(matches!(
            lower(malformed, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct DescriptorContext {
    calls: u64,
    addr: u64,
    table: u32,
    ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn descriptor_helper(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    addr: u64,
    table: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut DescriptorContext) };
    context.calls += 1;
    context.addr = addr;
    context.table = table;
    context.ok
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute(
    kind: OpKind,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs, &mut DescriptorContext),
) -> (crate::smir::lower::runtime::GuestRegs, DescriptorContext) {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(kind, true, true).expect("lower descriptor-table store");
    let exec = ExecMem::new(&code).expect("map descriptor-table store");
    let mut state = GuestRegs::default();
    for (index, value) in state.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    state.rflags = 0x2 | 0x08D5 | (1 << 10);
    state.exit_pc = 0xDEAD_BEEF_DEAD_BEEF;
    state.apx_enabled = 1;
    state.descriptor_store_fn = descriptor_helper as usize as u64;
    let mut context = DescriptorContext {
        ok: 1,
        ..DescriptorContext::default()
    };
    state.ctx = (&mut context as *mut DescriptorContext) as u64;
    configure(&mut state, &mut context);
    exec.run(entry, &mut state);
    (state, context)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_load(
    kind: OpKind,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs, &mut DescriptorContext),
) -> (crate::smir::lower::runtime::GuestRegs, DescriptorContext) {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(kind, true, true).expect("lower descriptor-table load");
    let exec = ExecMem::new(&code).expect("map descriptor-table load");
    let mut state = GuestRegs::default();
    for (index, value) in state.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    state.rflags = 0x2 | 0x08D5 | (1 << 10);
    state.exit_pc = 0xDEAD_BEEF_DEAD_BEEF;
    state.apx_enabled = 1;
    state.descriptor_load_fn = descriptor_helper as usize as u64;
    let mut context = DescriptorContext {
        ok: 1,
        ..DescriptorContext::default()
    };
    state.ctx = (&mut context as *mut DescriptorContext) as u64;
    configure(&mut state, &mut context);
    exec.run(entry, &mut state);
    (state, context)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_descriptor_store_computes_stack_egpr_addresses_and_table_selector() {
    for (addr, table, requires_apx, expected_addr, expected_table) in [
        (
            Address::BaseOffset {
                base: x86(X86Reg::Rsp),
                offset: 0x28,
                disp_size: DispSize::Disp8,
            },
            X86DescriptorTable::Gdt,
            false,
            0xA500_0000_0000_002C,
            0,
        ),
        (
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R25)),
                index: x86(X86Reg::R26),
                scale: 8,
                disp: -16,
                disp_size: DispSize::Disp8,
            },
            X86DescriptorTable::Idt,
            true,
            (0x5000_u64).wrapping_add(4 * 8).wrapping_sub(16),
            1,
        ),
    ] {
        let (state, context) = execute(store(addr, table, requires_apx), |state, _| {
            state.gpr[4] = 0xA500_0000_0000_0004;
            state.gpr[25] = 0x5000;
            state.gpr[26] = 4;
        });
        assert_eq!(context.calls, 1);
        assert_eq!(context.addr, expected_addr);
        assert_eq!(context.table, expected_table);
        assert_eq!(state.exit_pc, 0xDEAD_BEEF_DEAD_BEEF);
        assert_eq!(state.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_descriptor_store_guards_and_helper_failure_are_noncommitting() {
    for (name, requires_apx, apx, cr4, cpl, helper_ok, calls) in [
        ("APX", true, 0, 0, 0, 1, 0),
        ("UMIP", false, 1, 1 << 11, 3, 1, 0),
        ("helper", false, 1, 0, 0, 0, 1),
    ] {
        let (state, context) = execute(
            store(
                Address::Direct(x86(X86Reg::Rbx)),
                X86DescriptorTable::Gdt,
                requires_apx,
            ),
            |state, context| {
                state.apx_enabled = apx;
                state.cr4 = cr4;
                state.cpl = cpl;
                context.ok = helper_ok;
            },
        );
        assert_eq!(context.calls, calls, "{name}");
        assert_eq!(state.exit_pc, 0x1000, "{name}");
        for (index, value) in state.gpr.iter().enumerate() {
            assert_eq!(*value, 0xA500_0000_0000_0000 | index as u64, "{name}");
        }
        assert_eq!(state.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_descriptor_load_computes_stack_and_egpr_addresses_selects_table_and_hands_off() {
    for (addr, table, requires_apx, next_pc, expected_addr, expected_table) in [
        (
            Address::BaseOffset {
                base: x86(X86Reg::Rsp),
                offset: 0x28,
                disp_size: DispSize::Disp8,
            },
            X86DescriptorTable::Gdt,
            false,
            0x1004,
            0xA500_0000_0000_002C,
            0,
        ),
        (
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R25)),
                index: x86(X86Reg::R26),
                scale: 8,
                disp: -16,
                disp_size: DispSize::Disp8,
            },
            X86DescriptorTable::Idt,
            true,
            0x1005,
            0x5000_u64.wrapping_add(4 * 8).wrapping_sub(16),
            1,
        ),
        (
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::R16)),
                index: Some(x86(X86Reg::Rcx)),
                scale: 2,
                disp: -16,
            })),
            X86DescriptorTable::Gdt,
            true,
            0x100f,
            0x7020,
            0,
        ),
    ] {
        let (state, context) =
            execute_load(load(addr, table, requires_apx, next_pc), |state, _| {
                state.gpr[4] = 0xA500_0000_0000_0004;
                state.gpr[1] = 0x20;
                state.gpr[16] = u64::MAX - 15;
                state.gpr[25] = 0x5000;
                state.gpr[26] = 4;
                state.fs_base = 0x7000;
            });
        assert_eq!(context.calls, 1);
        assert_eq!(context.addr, expected_addr);
        assert_eq!(context.table, expected_table);
        assert_eq!(state.exit_pc, next_pc);
        assert_eq!(state.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_descriptor_load_apx_cpl_and_helper_failures_are_noncommitting() {
    for (name, requires_apx, apx, cpl, helper_ok, calls) in [
        ("APX", true, 0, 3, 1, 0),
        ("CPL", false, 1, 3, 1, 0),
        ("helper", false, 1, 0, 0, 1),
    ] {
        let (state, context) = execute_load(
            load(
                Address::Direct(x86(X86Reg::Rbx)),
                X86DescriptorTable::Gdt,
                requires_apx,
                0x1003,
            ),
            |state, context| {
                state.apx_enabled = apx;
                state.cpl = cpl;
                context.ok = helper_ok;
            },
        );
        assert_eq!(context.calls, calls, "{name}");
        assert_eq!(state.exit_pc, 0x1000, "{name}");
        for (index, value) in state.gpr.iter().enumerate() {
            assert_eq!(*value, 0xA500_0000_0000_0000 | index as u64, "{name}");
        }
        assert_eq!(state.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    }
}
