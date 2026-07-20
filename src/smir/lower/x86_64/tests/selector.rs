//! Fault-precise helper-backed native lowering for SLDT/STR.

use super::*;
use crate::smir::ir::ops::{X86SystemSelector, X86SystemSelectorStoreOp, X86SystemSelectorTarget};
use crate::smir::lower::{
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CPL_OFFSET, X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET,
    X86_GUEST_RFLAGS_OFFSET, X86_GUEST_SYSTEM_SELECTOR_FN_OFFSET,
};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn register(selector: X86SystemSelector, index: u8, width: OpWidth, requires_apx: bool) -> OpKind {
    OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
        selector,
        target: X86SystemSelectorTarget::Register {
            dst: x86(X86Reg::gpr(index)),
            width,
        },
        requires_apx,
    })
}

fn memory(selector: X86SystemSelector, addr: Address, requires_apx: bool) -> OpKind {
    OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
        selector,
        target: X86SystemSelectorTarget::Memory { addr },
        requires_apx,
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
fn lower_selector_store_requires_guards_helpers_and_never_emits_host_sldt_str() {
    assert!(matches!(
        lower(
            register(X86SystemSelector::Ldtr, 0, OpWidth::W32, false),
            false,
            false,
        ),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let (register_code, _) = lower(
        register(X86SystemSelector::Tr, 15, OpWidth::W64, false),
        false,
        true,
    )
    .expect("guarded STR register lowering");
    let (apx_code, _) = lower(
        register(X86SystemSelector::Ldtr, 31, OpWidth::W32, true),
        false,
        true,
    )
    .expect("APX-guarded SLDT register lowering");

    let address = Address::Direct(x86(X86Reg::Rax));
    assert!(matches!(
        lower(
            memory(X86SystemSelector::Ldtr, address.clone(), false),
            false,
            true,
        ),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let (memory_code, _) = lower(memory(X86SystemSelector::Ldtr, address, false), true, true)
        .expect("guarded helper-backed SLDT memory lowering");

    for code in [&register_code, &apx_code, &memory_code] {
        assert!(
            !code
                .windows(3)
                .any(|window| { window[..2] == [0x0F, 0x00] && ((window[2] >> 3) & 7) <= 1 }),
            "guest selector store must not observe host LDTR/TR: {code:02X?}"
        );
        for offset in [
            X86_GUEST_CR0_OFFSET,
            X86_GUEST_RFLAGS_OFFSET,
            X86_GUEST_CR4_OFFSET,
            X86_GUEST_CPL_OFFSET,
            X86_GUEST_SYSTEM_SELECTOR_FN_OFFSET,
        ] {
            assert!(
                code.windows(4)
                    .any(|window| window == (offset as u32).to_le_bytes()),
                "missing selector-store state offset {offset}: {code:02X?}"
            );
        }
    }
    assert!(
        apx_code
            .windows(4)
            .any(|window| window == (X86_GUEST_APX_ENABLED_OFFSET as u32).to_le_bytes())
    );
}

#[test]
fn lower_selector_store_rejects_every_non_lifter_shape() {
    let wrap = |target, requires_apx| {
        OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Ldtr,
            target,
            requires_apx,
        })
    };
    for malformed in [
        wrap(
            X86SystemSelectorTarget::Register {
                dst: VReg::virt(0),
                width: OpWidth::W64,
            },
            false,
        ),
        wrap(
            X86SystemSelectorTarget::Register {
                dst: x86(X86Reg::Rax),
                width: OpWidth::W8,
            },
            false,
        ),
        register(X86SystemSelector::Tr, 0, OpWidth::W128, false),
        register(X86SystemSelector::Tr, 16, OpWidth::W64, false),
        memory(
            X86SystemSelector::Ldtr,
            Address::Direct(VReg::virt(1)),
            false,
        ),
        memory(
            X86SystemSelector::Ldtr,
            Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
                x86(X86Reg::Rax),
            ))))),
            false,
        ),
        memory(
            X86SystemSelector::Ldtr,
            Address::Direct(x86(X86Reg::R31)),
            false,
        ),
        memory(
            X86SystemSelector::Ldtr,
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rax)),
                index: x86(X86Reg::Rcx),
                scale: 3,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            false,
        ),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, malformed.clone());
        builder.set_terminator(Terminator::Return { values: vec![] });
        let function = builder.finish();
        assert!(!x86_system_selector_store_shape_valid(
            &function.blocks[0].ops[0]
        ));
        assert!(matches!(
            lower(malformed, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }
}

#[test]
fn lower_selector_store_wraps_its_system_helper_with_vector_state_when_requested() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        register(X86SystemSelector::Ldtr, 0, OpWidth::W32, false),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer.set_preserve_vector_system_helpers(true);
    lowerer
        .lower_function(&builder.finish())
        .expect("lower vector-preserving SLDT");
    let code = lowerer.finalize().expect("finalize vector-preserving SLDT");

    let store_zmm0 = [0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x40, 0x05];
    let load_zmm0 = [0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x41, 0x05];
    assert_eq!(
        code.windows(store_zmm0.len())
            .filter(|window| *window == store_zmm0)
            .count(),
        1
    );
    assert_eq!(
        code.windows(load_zmm0.len())
            .filter(|window| *window == load_zmm0)
            .count(),
        1
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct SelectorContext {
    ldtr: u64,
    tr: u64,
    selector_calls: u64,
    last_selector: u64,
    stores: u64,
    last_addr: u64,
    last_value: u64,
    last_size: u64,
    store_ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
unsafe extern "C" fn read_selector(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    selector: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut SelectorContext) };
    context.selector_calls += 1;
    context.last_selector = u64::from(selector);
    match selector {
        0 => context.ldtr,
        1 => context.tr,
        _ => u64::MAX,
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn store(context: *mut SelectorContext, addr: u64, value: u64, size: u64) -> u64 {
    let context = unsafe { &mut *context };
    context.stores += 1;
    context.last_addr = addr;
    context.last_value = value;
    context.last_size = size;
    context.store_ok
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_register(
    kind: OpKind,
    context: &mut SelectorContext,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(kind, false, true).expect("lower guarded selector register form");
    let exec = ExecMem::new(&code).expect("map guarded selector register form");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.cr0 = 1;
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.ac_flag = 1;
    regs.exit_pc = 0xDEAD_BEEF_DEAD_BEEF;
    regs.ctx = (context as *mut SelectorContext) as u64;
    regs.system_selector_fn = read_selector as usize as u64;
    configure(&mut regs);
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_selector_registers_cover_both_selectors_widths_stack_aliases_egprs_and_flags() {
    for (selector, selector_id, value) in [
        (X86SystemSelector::Ldtr, 0, 0x1357_u64),
        (X86SystemSelector::Tr, 1, 0xBEEF_u64),
    ] {
        for index in [0_u8, 4, 5, 8, 15, 16, 31] {
            for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
                let mut context = SelectorContext {
                    ldtr: 0x1357,
                    tr: 0xBEEF,
                    ..SelectorContext::default()
                };
                let requires_apx = index >= 16;
                let regs = execute_register(
                    register(selector, index, width, requires_apx),
                    &mut context,
                    |regs| regs.apx_enabled = u64::from(requires_apx),
                );
                let incoming = 0xA500_0000_0000_0000 | u64::from(index);
                let expected = match width {
                    OpWidth::W16 => (incoming & !0xFFFF) | value,
                    OpWidth::W32 | OpWidth::W64 => value,
                    _ => unreachable!(),
                };
                for (other, observed) in regs.gpr.iter().enumerate() {
                    let expected_gpr = if other == usize::from(index) {
                        expected
                    } else {
                        0xA500_0000_0000_0000 | other as u64
                    };
                    assert_eq!(*observed, expected_gpr, "{selector:?} {index} {width:?}");
                }
                assert_eq!(context.selector_calls, 1);
                assert_eq!(context.last_selector, selector_id);
                assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
                assert_eq!(regs.ac_flag, 1);
                assert_eq!(regs.exit_pc, 0xDEAD_BEEF_DEAD_BEEF);
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_selector_dynamic_guards_are_ordered_and_noncommitting() {
    for (name, apx, cr0, rflags, cr4, cpl) in [
        ("APX", 0, 0, 1 << 17, 1 << 11, 3),
        ("real mode", 1, 0, 0, 1 << 11, 3),
        ("VM86", 1, 1, 1 << 17, 1 << 11, 3),
        ("UMIP", 1, 1, 0, 1 << 11, 3),
    ] {
        let mut context = SelectorContext {
            ldtr: 0x1357,
            ..SelectorContext::default()
        };
        let regs = execute_register(
            register(X86SystemSelector::Ldtr, 31, OpWidth::W32, true),
            &mut context,
            |regs| {
                regs.apx_enabled = apx;
                regs.cr0 = cr0;
                regs.rflags = 0x2 | 0x08D5 | (1 << 10) | rflags;
                regs.cr4 = cr4;
                regs.cpl = cpl;
            },
        );
        assert_eq!(context.selector_calls, 0, "{name}");
        assert_eq!(regs.gpr[31], 0xA500_0000_0000_001F, "{name}");
        assert_eq!(regs.exit_pc, 0x1000, "{name}");
    }

    for (name, cr4, cpl) in [("UMIP clear", 0, 3), ("CPL0", 1 << 11, 0)] {
        let mut context = SelectorContext {
            tr: 0xBEEF,
            ..SelectorContext::default()
        };
        let regs = execute_register(
            register(X86SystemSelector::Tr, 3, OpWidth::W32, false),
            &mut context,
            |regs| {
                regs.cr4 = cr4;
                regs.cpl = cpl;
            },
        );
        assert_eq!(context.selector_calls, 1, "{name}");
        assert_eq!(regs.gpr[3], 0xBEEF, "{name}");
        assert_eq!(regs.exit_pc, 0xDEAD_BEEF_DEAD_BEEF, "{name}");
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_selector_memory_is_two_bytes_fault_precise_and_stack_state_backed() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const FLAGS: u64 = 0x2 | 0x08D5 | (1 << 10);
    const SENTINEL_PC: u64 = 0xDEAD_BEEF_DEAD_BEEF;
    let address = Address::BaseIndexScale {
        base: Some(x86(X86Reg::Rsp)),
        index: x86(X86Reg::R31),
        scale: 2,
        disp: -8,
        disp_size: DispSize::Disp8,
    };
    let (code, entry) = lower(memory(X86SystemSelector::Tr, address, true), true, true)
        .expect("lower helper-backed APX STR memory form");
    let exec = ExecMem::new(&code).expect("map helper-backed APX STR memory form");

    let mut initial_gprs = [0u64; 32];
    for (index, value) in initial_gprs.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    initial_gprs[4] = 0x2000;
    initial_gprs[31] = 0x24;
    let expected_addr = 0x2000 + 0x24 * 2 - 8;

    for (store_ok, expected_exit) in [(1, SENTINEL_PC), (0, 0x1000)] {
        let mut context = SelectorContext {
            tr: 0xBEEF,
            store_ok,
            ..SelectorContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial_gprs;
        regs.cr0 = 1;
        regs.apx_enabled = 1;
        regs.rflags = FLAGS;
        regs.ac_flag = 1;
        regs.exit_pc = SENTINEL_PC;
        regs.ctx = (&mut context as *mut SelectorContext) as u64;
        regs.store_fn = store as usize as u64;
        regs.system_selector_fn = read_selector as usize as u64;
        exec.run(entry, &mut regs);

        assert_eq!(context.selector_calls, 1);
        assert_eq!(context.last_selector, 1);
        assert_eq!(context.stores, 1);
        assert_eq!(context.last_addr, expected_addr);
        assert_eq!(context.last_value, 0xBEEF);
        assert_eq!(context.last_size, 2);
        assert_eq!(regs.gpr, initial_gprs);
        assert_eq!(
            regs.rflags & (0x08D5 | (1 << 10)),
            FLAGS & (0x08D5 | (1 << 10))
        );
        assert_eq!(regs.ac_flag, 1);
        assert_eq!(regs.exit_pc, expected_exit);
    }

    for (name, apx_enabled, cr0, rflags, umip) in [
        ("APX", 0, 0, 1 << 17, false),
        ("mode", 1, 0, 0, false),
        ("VM86", 1, 1, 1 << 17, false),
        ("UMIP", 1, 1, 0, true),
    ] {
        let mut context = SelectorContext {
            tr: 0xBEEF,
            store_ok: 1,
            ..SelectorContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial_gprs;
        regs.cr0 = cr0;
        regs.rflags = FLAGS | rflags;
        regs.cr4 = u64::from(umip) << 11;
        regs.cpl = 3;
        regs.apx_enabled = apx_enabled;
        regs.ctx = (&mut context as *mut SelectorContext) as u64;
        regs.store_fn = store as usize as u64;
        regs.system_selector_fn = read_selector as usize as u64;
        exec.run(entry, &mut regs);

        assert_eq!(context.selector_calls, 0, "{name}");
        assert_eq!(context.stores, 0, "{name}");
        assert_eq!(regs.gpr, initial_gprs, "{name}");
        assert_eq!(regs.exit_pc, 0x1000, "{name}");
    }
}
