//! Fault-precise native lowering for long-mode `PUSH FS/GS`.

use super::*;
use crate::smir::ir::ops::{X86SystemSelector, X86SystemSelectorStoreOp, X86SystemSelectorTarget};
use crate::smir::lower::{
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CS_L_OFFSET, X86_GUEST_EFER_OFFSET,
    X86_GUEST_STORE_FN_OFFSET, X86_GUEST_SYSTEM_SELECTOR_FN_OFFSET,
};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn stack(selector: X86SystemSelector, width: MemWidth, requires_apx: bool) -> OpKind {
    OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
        selector,
        target: X86SystemSelectorTarget::Stack {
            stack_pointer: x86(X86Reg::Rsp),
            width,
        },
        requires_apx,
    })
}

fn lower(
    kind: OpKind,
    mem_helpers: bool,
    fault_guards: bool,
    continuation: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    if continuation {
        builder.push_op(
            0x1002,
            OpKind::Mov {
                dst: x86(X86Reg::R11),
                src: SrcOperand::Reg(x86(X86Reg::Rsp)),
                width: OpWidth::W64,
            },
        );
        builder.push_op(
            0x1003,
            OpKind::Mov {
                dst: x86(X86Reg::R10),
                src: SrcOperand::Imm64(0x1234_5678_9ABC_DEF0),
                width: OpWidth::W64,
            },
        );
    }
    builder.set_terminator(Terminator::Return { values: vec![] });

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

fn shape_valid(kind: OpKind) -> bool {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let function = builder.finish();
    x86_system_selector_store_shape_valid(&function.blocks[0].ops[0])
}

#[test]
fn lower_push_segment_requires_precise_guards_mmu_and_exact_lifter_shape() {
    let valid = stack(X86SystemSelector::Fs, MemWidth::B8, true);
    assert!(matches!(
        lower(valid.clone(), true, false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower(valid.clone(), false, true, false),
        Err(LowerError::UnsupportedOp { .. })
    ));

    let (code, _) = lower(valid, true, true, false).expect("lower guarded PUSH FS");
    for offset in [
        X86_GUEST_APX_ENABLED_OFFSET,
        X86_GUEST_EFER_OFFSET,
        X86_GUEST_CS_L_OFFSET,
        X86_GUEST_SYSTEM_SELECTOR_FN_OFFSET,
        X86_GUEST_STORE_FN_OFFSET,
    ] {
        assert!(
            code.windows(4)
                .any(|window| window == (offset as u32).to_le_bytes()),
            "missing state/helper offset {offset}: {code:02X?}"
        );
    }

    for malformed in [
        OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Cs,
            target: X86SystemSelectorTarget::Stack {
                stack_pointer: x86(X86Reg::Rsp),
                width: MemWidth::B8,
            },
            requires_apx: false,
        }),
        OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Gs,
            target: X86SystemSelectorTarget::Stack {
                stack_pointer: x86(X86Reg::Rax),
                width: MemWidth::B8,
            },
            requires_apx: false,
        }),
        stack(X86SystemSelector::Fs, MemWidth::B1, false),
        stack(X86SystemSelector::Gs, MemWidth::B4, false),
    ] {
        assert!(!shape_valid(malformed.clone()), "{malformed:?}");
        let error = lower(malformed.clone(), true, true, false)
            .expect_err("malformed PUSH-segment shape must fail closed");
        assert!(
            matches!(
                error,
                LowerError::InvalidOperand { .. } | LowerError::InvalidRegister(_)
            ),
            "{malformed:?}: {error:?}"
        );
    }
}

#[test]
fn lower_push_segment_preserves_vectors_around_selector_and_mmu_helpers() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, stack(X86SystemSelector::Fs, MemWidth::B8, false));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer.set_preserve_vector_system_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer
        .lower_function(&builder.finish())
        .expect("lower vector-preserving PUSH FS");
    let code = lowerer
        .finalize()
        .expect("finalize vector-preserving PUSH FS");

    let store_zmm0 = [0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x40, 0x05];
    let load_zmm0 = [0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x41, 0x05];
    assert_eq!(
        code.windows(store_zmm0.len())
            .filter(|window| *window == store_zmm0)
            .count(),
        2,
        "selector and MMU helper boundaries each need one vector spill"
    );
    assert_eq!(
        code.windows(load_zmm0.len())
            .filter(|window| *window == load_zmm0)
            .count(),
        3,
        "selector success plus MMU success/fault paths each restore vectors"
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct PushSegmentContext {
    fs: u64,
    gs: u64,
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
    let context = unsafe { &mut *(state.ctx as *mut PushSegmentContext) };
    context.selector_calls += 1;
    context.last_selector = u64::from(selector);
    match selector {
        6 => context.fs,
        7 => context.gs,
        _ => u64::MAX,
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn store(context: *mut PushSegmentContext, addr: u64, value: u64, size: u64) -> u64 {
    let context = unsafe { &mut *context };
    context.stores += 1;
    context.last_addr = addr;
    context.last_value = value;
    context.last_size = size;
    context.store_ok
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute(
    kind: OpKind,
    context: &mut PushSegmentContext,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> ([u64; 32], crate::smir::lower::runtime::GuestRegs) {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(kind, true, true, true).expect("lower executable PUSH FS/GS");
    let exec = ExecMem::new(&code).expect("map executable PUSH FS/GS");
    let mut state = GuestRegs::default();
    for (index, value) in state.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    state.gpr[4] = 0x4000;
    state.efer = 1 << 10;
    state.cs_l = 1;
    state.apx_enabled = 1;
    state.rflags = 0x2 | 0x08D5 | (1 << 10);
    state.ac_flag = 1;
    state.exit_pc = 0xDEAD_BEEF_DEAD_BEEF;
    state.ctx = (context as *mut PushSegmentContext) as u64;
    state.store_fn = store as usize as u64;
    state.system_selector_fn = read_selector as usize as u64;
    configure(&mut state);
    let initial_gprs = state.gpr;
    exec.run(entry, &mut state);
    (initial_gprs, state)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_push_segment_stores_exact_width_then_commits_rsp_and_continues() {
    const CONTINUATION: u64 = 0x1234_5678_9ABC_DEF0;
    for (selector, selector_id, selector_value, width, requires_apx) in [
        (
            X86SystemSelector::Fs,
            6_u64,
            0x1357_u64,
            MemWidth::B8,
            false,
        ),
        (X86SystemSelector::Gs, 7, 0xBEEF, MemWidth::B2, false),
        (X86SystemSelector::Gs, 7, 0xBEEF, MemWidth::B8, true),
    ] {
        let mut context = PushSegmentContext {
            fs: 0x1357,
            gs: 0xBEEF,
            store_ok: 1,
            ..PushSegmentContext::default()
        };
        let (initial_gprs, state) =
            execute(stack(selector, width, requires_apx), &mut context, |_| {});

        assert_eq!(context.selector_calls, 1, "{selector:?} {width:?}");
        assert_eq!(context.last_selector, selector_id, "{selector:?} {width:?}");
        assert_eq!(context.stores, 1, "{selector:?} {width:?}");
        assert_eq!(
            context.last_addr,
            initial_gprs[4] - u64::from(width.bytes()),
            "{selector:?} {width:?}"
        );
        assert_eq!(context.last_value, selector_value, "{selector:?} {width:?}");
        assert_eq!(context.last_size, u64::from(width.bytes()), "{selector:?}");
        for (index, observed) in state.gpr.iter().enumerate() {
            let expected = match index {
                4 => initial_gprs[4] - u64::from(width.bytes()),
                10 => CONTINUATION,
                11 => initial_gprs[4] - u64::from(width.bytes()),
                _ => initial_gprs[index],
            };
            assert_eq!(*observed, expected, "{selector:?} {width:?} GPR{index}");
        }
        assert_eq!(state.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
        assert_eq!(state.ac_flag, 1);
        assert_eq!(state.exit_pc, 0xDEAD_BEEF_DEAD_BEEF);
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_push_segment_guards_and_store_failure_are_precise_and_noncommitting() {
    let mut failed_store = PushSegmentContext {
        fs: 0x1357,
        store_ok: 0,
        ..PushSegmentContext::default()
    };
    let (initial_gprs, failed) = execute(
        stack(X86SystemSelector::Fs, MemWidth::B8, false),
        &mut failed_store,
        |_| {},
    );
    assert_eq!(failed_store.selector_calls, 1);
    assert_eq!(failed_store.stores, 1);
    assert_eq!(failed.gpr, initial_gprs);
    assert_eq!(failed.exit_pc, 0x1000);
    assert_eq!(failed.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    assert_eq!(failed.ac_flag, 1);

    for (name, apx_enabled, efer, cs_l) in [
        ("APX", 0_u64, 1 << 10, 1_u64),
        ("EFER.LMA", 1, 0, 1),
        ("CS.L", 1, 1 << 10, 0),
    ] {
        let mut context = PushSegmentContext {
            gs: 0xBEEF,
            store_ok: 1,
            ..PushSegmentContext::default()
        };
        let (initial_gprs, state) = execute(
            stack(X86SystemSelector::Gs, MemWidth::B8, true),
            &mut context,
            |state| {
                state.apx_enabled = apx_enabled;
                state.efer = efer;
                state.cs_l = cs_l;
            },
        );
        assert_eq!(context.selector_calls, 0, "{name}");
        assert_eq!(context.stores, 0, "{name}");
        assert_eq!(state.gpr, initial_gprs, "{name}");
        assert_eq!(state.exit_pc, 0x1000, "{name}");
        assert_eq!(state.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
        assert_eq!(state.ac_flag, 1, "{name}");
    }

    for (name, width, initial_rsp) in [
        (
            "B8 lower canonical boundary",
            MemWidth::B8,
            0x0000_8000_0000_0004_u64,
        ),
        (
            "B8 upper canonical boundary",
            MemWidth::B8,
            0xFFFF_8000_0000_0004,
        ),
        ("B8 64-bit wrap", MemWidth::B8, 4),
        (
            "B2 lower canonical boundary",
            MemWidth::B2,
            0x0000_8000_0000_0001,
        ),
        (
            "B2 upper canonical boundary",
            MemWidth::B2,
            0xFFFF_8000_0000_0001,
        ),
        ("B2 64-bit wrap", MemWidth::B2, 1),
    ] {
        let mut context = PushSegmentContext {
            fs: 0x1357,
            store_ok: 1,
            ..PushSegmentContext::default()
        };
        let (initial_gprs, state) = execute(
            stack(X86SystemSelector::Fs, width, false),
            &mut context,
            |state| state.gpr[4] = initial_rsp,
        );
        assert_eq!(context.selector_calls, 0, "{name}");
        assert_eq!(context.stores, 0, "{name}");
        assert_eq!(state.gpr, initial_gprs, "{name}");
        assert_eq!(state.exit_pc, 0x1000, "{name}");
        assert_eq!(state.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
        assert_eq!(state.ac_flag, 1, "{name}");
    }
}
