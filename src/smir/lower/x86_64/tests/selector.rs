//! Fault-precise helper-backed native lowering for selector stores and loads.

use super::*;
use crate::smir::ir::ops::{
    X86SystemSelector, X86SystemSelectorLoadOp, X86SystemSelectorSource, X86SystemSelectorStoreOp,
    X86SystemSelectorTarget,
};
use crate::smir::lower::{
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CPL_OFFSET, X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET,
    X86_GUEST_RFLAGS_OFFSET, X86_GUEST_SYSTEM_SELECTOR_FN_OFFSET,
    X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET,
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

fn load_register(index: u8, requires_apx: bool, next_pc: u64) -> OpKind {
    load_register_for(X86SystemSelector::Ldtr, index, requires_apx, next_pc)
}

fn load_register_for(
    selector: X86SystemSelector,
    index: u8,
    requires_apx: bool,
    next_pc: u64,
) -> OpKind {
    OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
        selector,
        source: X86SystemSelectorSource::Register {
            src: x86(X86Reg::gpr(index)),
        },
        requires_apx,
        next_pc,
    })
}

fn load_memory(addr: Address, requires_apx: bool, next_pc: u64) -> OpKind {
    load_memory_for(X86SystemSelector::Ldtr, addr, requires_apx, next_pc)
}

fn load_memory_for(
    selector: X86SystemSelector,
    addr: Address,
    requires_apx: bool,
    next_pc: u64,
) -> OpKind {
    load_memory_for_width(selector, addr, MemWidth::B2, false, requires_apx, next_pc)
}

fn load_memory_for_width(
    selector: X86SystemSelector,
    addr: Address,
    width: MemWidth,
    stack_segment: bool,
    requires_apx: bool,
    next_pc: u64,
) -> OpKind {
    OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
        selector,
        source: X86SystemSelectorSource::Memory {
            addr,
            width,
            stack_segment,
        },
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
fn lower_mov_rm_sreg_uses_selector_helper_without_sldt_str_mode_or_umip_guards() {
    for selector in [
        X86SystemSelector::Es,
        X86SystemSelector::Cs,
        X86SystemSelector::Ss,
        X86SystemSelector::Ds,
        X86SystemSelector::Fs,
        X86SystemSelector::Gs,
    ] {
        let (code, _) = lower(register(selector, 0, OpWidth::W32, false), false, true)
            .unwrap_or_else(|error| panic!("lower MOV EAX,{selector:?}: {error}"));
        assert!(
            code.windows(4).any(|window| {
                window == (X86_GUEST_SYSTEM_SELECTOR_FN_OFFSET as u32).to_le_bytes()
            }),
            "{selector:?}: missing selector helper"
        );
        for offset in [
            X86_GUEST_CR0_OFFSET,
            X86_GUEST_RFLAGS_OFFSET,
            X86_GUEST_CR4_OFFSET,
            X86_GUEST_CPL_OFFSET,
            X86_GUEST_APX_ENABLED_OFFSET,
        ] {
            assert!(
                !code
                    .windows(4)
                    .any(|window| window == (offset as u32).to_le_bytes()),
                "{selector:?}: MOV r/m,Sreg must not test state offset {offset}: {code:02X?}"
            );
        }
    }

    let (apx, _) = lower(
        register(X86SystemSelector::Cs, 31, OpWidth::W64, true),
        false,
        true,
    )
    .expect("lower REX2 MOV R31,CS");
    assert!(
        apx.windows(4)
            .any(|window| window == (X86_GUEST_APX_ENABLED_OFFSET as u32).to_le_bytes())
    );
    for offset in [
        X86_GUEST_CR0_OFFSET,
        X86_GUEST_RFLAGS_OFFSET,
        X86_GUEST_CR4_OFFSET,
        X86_GUEST_CPL_OFFSET,
    ] {
        assert!(
            !apx.windows(4)
                .any(|window| window == (offset as u32).to_le_bytes())
        );
    }
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
fn lower_selector_loads_require_guards_helpers_serialize_and_never_touch_host_state() {
    let register = load_register(0, false, 0x1003);
    assert!(matches!(
        lower(register.clone(), true, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower(register.clone(), false, true),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let (register_code, _) = lower(register, true, true).expect("guarded LLDT register lowering");
    let (apx_code, _) = lower(load_register(31, true, 0x1004), true, true)
        .expect("guarded APX LLDT register lowering");
    let (memory_code, _) = lower(
        load_memory_for(
            X86SystemSelector::Tr,
            Address::Direct(x86(X86Reg::Rax)),
            false,
            0x1003,
        ),
        true,
        true,
    )
    .expect("guarded LTR memory lowering");

    for code in [&register_code, &apx_code, &memory_code] {
        assert!(
            code.windows(4).any(|window| {
                window == (X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET as u32).to_le_bytes()
            }),
            "missing selector-load helper offset: {code:02X?}"
        );
        for offset in [
            X86_GUEST_CR0_OFFSET,
            X86_GUEST_RFLAGS_OFFSET,
            X86_GUEST_CPL_OFFSET,
        ] {
            assert!(
                code.windows(4)
                    .any(|window| window == (offset as u32).to_le_bytes()),
                "missing selector-load state offset {offset}: {code:02X?}"
            );
        }
        assert!(
            code.windows(2).any(|window| window == [0x0F, 0xA2]),
            "successful selector load must serialize: {code:02X?}"
        );
        assert!(
            !code
                .windows(3)
                .any(|window| window[..2] == [0x0F, 0x00] && matches!((window[2] >> 3) & 7, 2 | 3)),
            "guest selector load must not update host LDTR/TR: {code:02X?}"
        );
    }
    assert!(
        apx_code
            .windows(4)
            .any(|window| window == (X86_GUEST_APX_ENABLED_OFFSET as u32).to_le_bytes())
    );
}

#[test]
fn lower_selector_loads_reject_every_non_lifter_shape_and_frontier() {
    for malformed in [
        load_register_for(X86SystemSelector::Cs, 0, false, 0x1003),
        OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
            selector: X86SystemSelector::Tr,
            source: X86SystemSelectorSource::Register { src: VReg::virt(0) },
            requires_apx: false,
            next_pc: 0x1003,
        }),
        load_register(16, false, 0x1004),
        load_memory(Address::Direct(VReg::virt(1)), false, 0x1003),
        load_memory(Address::Direct(x86(X86Reg::R31)), false, 0x1004),
        load_memory(
            Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
                x86(X86Reg::Rax),
            ))))),
            false,
            0x1004,
        ),
        load_memory(
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rax)),
                index: x86(X86Reg::Rcx),
                scale: 3,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            false,
            0x1004,
        ),
        load_register(0, false, 0x1002),
        load_register(0, false, 0x1010),
        load_register(0, false, 0x0FFF),
        load_register_for(X86SystemSelector::Es, 0, false, 0x1001),
        load_memory_for_width(
            X86SystemSelector::Ldtr,
            Address::Direct(x86(X86Reg::Rax)),
            MemWidth::B8,
            false,
            false,
            0x1003,
        ),
        load_memory_for_width(
            X86SystemSelector::Ds,
            Address::Direct(x86(X86Reg::Rax)),
            MemWidth::B4,
            false,
            false,
            0x1002,
        ),
    ] {
        assert!(matches!(
            lower(malformed, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, load_register(0, false, 0x1003));
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
fn lower_mov_sreg_load_admits_all_ordinary_selectors_and_both_memory_widths() {
    for selector in [
        X86SystemSelector::Es,
        X86SystemSelector::Ss,
        X86SystemSelector::Ds,
        X86SystemSelector::Fs,
        X86SystemSelector::Gs,
    ] {
        for kind in [
            load_register_for(selector, 0, false, 0x1002),
            load_register_for(selector, 31, true, 0x1004),
            load_memory_for_width(
                selector,
                Address::Direct(x86(X86Reg::Rsp)),
                MemWidth::B2,
                true,
                false,
                0x1002,
            ),
            load_memory_for_width(
                selector,
                Address::Direct(x86(X86Reg::R31)),
                MemWidth::B8,
                false,
                true,
                0x1004,
            ),
        ] {
            let function = {
                let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
                builder.push_op(0x1000, kind.clone());
                builder.set_terminator(Terminator::Return { values: vec![] });
                builder.finish()
            };
            assert!(x86_system_selector_load_shape_valid(
                &function.blocks[0].ops[0]
            ));
            let (code, _) = lower(kind, true, true)
                .unwrap_or_else(|error| panic!("{selector:?} lowering failed: {error}"));
            assert!(
                !code.windows(2).any(|window| window == [0x0F, 0xA2]),
                "MOV Sreg is not serializing: {selector:?} {code:02X?}"
            );
        }
    }
}

#[test]
fn lower_selector_load_serialization_frontier_ends_the_native_block() {
    for selector in [X86SystemSelector::Ldtr, X86SystemSelector::Tr] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, load_register_for(selector, 0, false, 0x1003));
        builder.push_op(
            0x1003,
            load_register_for(
                if selector == X86SystemSelector::Ldtr {
                    X86SystemSelector::Tr
                } else {
                    X86SystemSelector::Ldtr
                },
                0,
                false,
                0x1006,
            ),
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer.set_jit_fault_deopt_guards(true);
        lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("{selector:?} frontier failed: {error}"));
        let code = lowerer.finalize().unwrap();
        assert_eq!(
            code.windows(4)
                .filter(|window| {
                    **window == (X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET as u32).to_le_bytes()
                })
                .count(),
            1,
            "{selector:?} must end lowering before a later same-block selector load"
        );
    }
}

#[test]
fn lower_selector_loads_wrap_implicit_memory_with_vector_state_when_requested() {
    for selector in [X86SystemSelector::Ldtr, X86SystemSelector::Tr] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, load_register_for(selector, 0, false, 0x1003));
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer.set_jit_fault_deopt_guards(true);
        lowerer.set_preserve_vector_mem_helpers(true);
        lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("lower vector-preserving {selector:?}: {error}"));
        let code = lowerer.finalize().unwrap();

        let store_zmm0 = [0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x40, 0x05];
        let load_zmm0 = [0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x41, 0x05];
        assert_eq!(
            code.windows(store_zmm0.len())
                .filter(|window| *window == store_zmm0)
                .count(),
            1,
            "{selector:?}"
        );
        assert_eq!(
            code.windows(load_zmm0.len())
                .filter(|window| *window == load_zmm0)
                .count(),
            2, // distinct success and fault-restoration paths
            "{selector:?}"
        );
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
    segments: [u64; 6],
    selector_calls: u64,
    last_selector: u64,
    stores: u64,
    last_addr: u64,
    last_value: u64,
    last_size: u64,
    store_ok: u64,
    load_calls: u64,
    last_operand: u64,
    last_encoding: u32,
    load_ok: u64,
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
        2..=7 => context.segments[(selector - 2) as usize],
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
unsafe extern "C" fn load_selector(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    operand: u64,
    encoding: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut SelectorContext) };
    context.load_calls += 1;
    context.last_operand = operand;
    context.last_encoding = encoding;
    context.load_ok
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_load(
    kind: OpKind,
    context: &mut SelectorContext,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(kind, true, true).expect("lower guarded selector load");
    let exec = ExecMem::new(&code).expect("map guarded selector load");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.cr0 = 1;
    regs.cpl = 0;
    regs.apx_enabled = 1;
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.ac_flag = 1;
    regs.exit_pc = 0xDEAD_BEEF_DEAD_BEEF;
    regs.ctx = (context as *mut SelectorContext) as u64;
    regs.system_selector_load_fn = load_selector as usize as u64;
    configure(&mut regs);
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_selector_load_register_sources_cover_both_selectors_aliases_and_egprs() {
    for selector in [X86SystemSelector::Ldtr, X86SystemSelector::Tr] {
        for index in [0_u8, 4, 5, 8, 15, 16, 31] {
            let requires_apx = index >= 16;
            let operand = 0xA500_0000_0000_0000 | u64::from(index);
            let mut context = SelectorContext {
                load_ok: 1,
                ..SelectorContext::default()
            };
            let regs = execute_load(
                load_register_for(selector, index, requires_apx, 0x1004),
                &mut context,
                |_| {},
            );

            assert_eq!(context.load_calls, 1, "{selector:?} source={index}");
            assert_eq!(context.last_operand, operand, "{selector:?} source={index}");
            let selector_bit = u32::from(selector == X86SystemSelector::Tr) << 2;
            assert_eq!(
                context.last_encoding,
                (u32::from(requires_apx) << 1) | selector_bit
            );
            for (other, value) in regs.gpr.iter().enumerate() {
                assert_eq!(
                    *value,
                    0xA500_0000_0000_0000 | other as u64,
                    "{selector:?} source={index}, GPR={other}"
                );
            }
            assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
            assert_eq!(regs.ac_flag, 1);
            assert_eq!(regs.exit_pc, 0x1004);
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_mov_sreg_load_encodes_selectors_widths_and_skips_system_privilege_guards() {
    for (selector, selector_id) in [
        (X86SystemSelector::Es, 2_u32),
        (X86SystemSelector::Ss, 4),
        (X86SystemSelector::Ds, 5),
        (X86SystemSelector::Fs, 6),
        (X86SystemSelector::Gs, 7),
    ] {
        let mut register_context = SelectorContext {
            load_ok: 1,
            ..SelectorContext::default()
        };
        let register = execute_load(
            load_register_for(selector, 0, false, 0x1002),
            &mut register_context,
            |regs| {
                // MOV Sreg has no LLDT/LTR PE/VM/CPL guards in the lowerer;
                // the owning runtime helper handles the current execution mode.
                regs.cr0 = 0;
                regs.rflags |= 1 << 17;
                regs.cpl = 3;
            },
        );
        assert_eq!(register_context.load_calls, 1, "{selector:?}");
        assert_eq!(register_context.last_encoding, selector_id << 2);
        assert_eq!(register.exit_pc, 0x1002);

        for (width, width_bit) in [(MemWidth::B2, 0_u32), (MemWidth::B8, 1 << 5)] {
            let mut memory_context = SelectorContext {
                load_ok: 1,
                ..SelectorContext::default()
            };
            let memory = execute_load(
                load_memory_for_width(
                    selector,
                    Address::Direct(x86(X86Reg::Rax)),
                    width,
                    false,
                    false,
                    0x1002,
                ),
                &mut memory_context,
                |regs| regs.gpr[0] = 0x3456,
            );
            assert_eq!(memory_context.load_calls, 1, "{selector:?} {width:?}");
            assert_eq!(memory_context.last_operand, 0x3456);
            assert_eq!(
                memory_context.last_encoding,
                1 | (selector_id << 2) | width_bit
            );
            assert_eq!(memory.exit_pc, 0x1002);
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_ltr_memory_address_and_dynamic_failures_are_precise_and_noncommitting() {
    let address = Address::BaseIndexScale {
        base: Some(x86(X86Reg::Rsp)),
        index: x86(X86Reg::R31),
        scale: 2,
        disp: -8,
        disp_size: DispSize::Disp8,
    };
    let mut success = SelectorContext {
        load_ok: 1,
        ..SelectorContext::default()
    };
    let regs = execute_load(
        load_memory_for(X86SystemSelector::Tr, address.clone(), true, 0x1005),
        &mut success,
        |regs| {
            regs.gpr[4] = 0x2000;
            regs.gpr[31] = 0x24;
        },
    );
    assert_eq!(success.load_calls, 1);
    assert_eq!(success.last_operand, 0x2000 + 0x24 * 2 - 8);
    assert_eq!(success.last_encoding, 0x7);
    assert_eq!(regs.gpr[4], 0x2000);
    assert_eq!(regs.gpr[31], 0x24);
    assert_eq!(regs.exit_pc, 0x1005);

    for (name, apx, cr0, rflags, cpl, helper_ok, expected_calls) in [
        ("APX", 0, 1, 0, 0, 1, 0),
        ("real mode", 1, 0, 0, 0, 1, 0),
        ("VM86", 1, 1, 1 << 17, 0, 1, 0),
        ("CPL", 1, 1, 0, 3, 1, 0),
        ("helper", 1, 1, 0, 0, 0, 1),
    ] {
        let mut context = SelectorContext {
            load_ok: helper_ok,
            ..SelectorContext::default()
        };
        let regs = execute_load(
            load_memory_for(X86SystemSelector::Tr, address.clone(), true, 0x1005),
            &mut context,
            |regs| {
                regs.gpr[4] = 0x2000;
                regs.gpr[31] = 0x24;
                regs.apx_enabled = apx;
                regs.cr0 = cr0;
                regs.rflags |= rflags;
                regs.cpl = cpl;
            },
        );
        assert_eq!(context.load_calls, expected_calls, "{name}");
        assert_eq!(regs.gpr[4], 0x2000, "{name}");
        assert_eq!(regs.gpr[31], 0x24, "{name}");
        assert_eq!(
            regs.rflags & (0x08D5 | (1 << 10)),
            0x08D5 | (1 << 10),
            "{name}"
        );
        assert_eq!(regs.ac_flag, 1, "{name}");
        assert_eq!(regs.exit_pc, 0x1000, "{name}");
    }
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
fn native_mov_rm_sreg_covers_all_selectors_widths_egprs_and_ignores_system_guards() {
    let selectors = [
        (X86SystemSelector::Es, 2_u64, 0x0101_u64),
        (X86SystemSelector::Cs, 3, 0x0202),
        (X86SystemSelector::Ss, 4, 0x0303),
        (X86SystemSelector::Ds, 5, 0x0404),
        (X86SystemSelector::Fs, 6, 0x0505),
        (X86SystemSelector::Gs, 7, 0x0606),
    ];
    for (selector, selector_id, value) in selectors {
        for index in [0_u8, 4, 5, 15, 16, 31] {
            for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
                let mut context = SelectorContext {
                    segments: [0x0101, 0x0202, 0x0303, 0x0404, 0x0505, 0x0606],
                    ..SelectorContext::default()
                };
                let requires_apx = index >= 16;
                let regs = execute_register(
                    register(selector, index, width, requires_apx),
                    &mut context,
                    |regs| {
                        regs.apx_enabled = u64::from(requires_apx);
                        // These are SLDT/STR guards, not MOV r/m,Sreg guards.
                        regs.cr0 = 0;
                        regs.rflags |= 1 << 17;
                        regs.cr4 = 1 << 11;
                        regs.cpl = 3;
                    },
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
                assert_eq!(
                    regs.rflags & (0x08D5 | (1 << 10)),
                    0x08D5 | (1 << 10),
                    "{selector:?} {index} {width:?}"
                );
                assert_eq!(regs.ac_flag, 1);
                assert_eq!(regs.exit_pc, 0xDEAD_BEEF_DEAD_BEEF);
            }
        }
    }

    let mut disabled = SelectorContext {
        segments: [0x0101, 0x0202, 0x0303, 0x0404, 0x0505, 0x0606],
        ..SelectorContext::default()
    };
    let regs = execute_register(
        register(X86SystemSelector::Cs, 31, OpWidth::W64, true),
        &mut disabled,
        |regs| regs.apx_enabled = 0,
    );
    assert_eq!(disabled.selector_calls, 0);
    assert_eq!(regs.gpr[31], 0xA500_0000_0000_001F);
    assert_eq!(regs.exit_pc, 0x1000);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_mov_rm_sreg_memory_bypasses_system_guards_and_stores_exactly_two_bytes() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(
        memory(
            X86SystemSelector::Fs,
            Address::Direct(x86(X86Reg::Rsp)),
            false,
        ),
        true,
        true,
    )
    .expect("lower MOV [RSP],FS");
    let exec = ExecMem::new(&code).expect("map MOV [RSP],FS");
    let mut context = SelectorContext {
        segments: [0x0101, 0x0202, 0x0303, 0x0404, 0xBEEF, 0x0606],
        store_ok: 1,
        ..SelectorContext::default()
    };
    let mut regs = GuestRegs::default();
    regs.gpr[4] = 0x2345;
    regs.cr0 = 0;
    regs.rflags = 0x2 | (1 << 17);
    regs.cr4 = 1 << 11;
    regs.cpl = 3;
    regs.ctx = (&mut context as *mut SelectorContext) as u64;
    regs.store_fn = store as usize as u64;
    regs.system_selector_fn = read_selector as usize as u64;
    exec.run(entry, &mut regs);

    assert_eq!(context.selector_calls, 1);
    assert_eq!(context.last_selector, 6);
    assert_eq!(context.stores, 1);
    assert_eq!(context.last_addr, 0x2345);
    assert_eq!(context.last_value, 0xBEEF);
    assert_eq!(context.last_size, 2);
    assert_eq!(regs.gpr[4], 0x2345);
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
