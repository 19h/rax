//! Fault-precise native lowering for long-mode `POP FS/GS`.

use super::*;
use crate::smir::ir::ops::{X86SystemSelector, X86SystemSelectorLoadOp, X86SystemSelectorSource};
use crate::smir::lower::{
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CS_L_OFFSET, X86_GUEST_EFER_OFFSET,
    X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET,
};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn stack_load(
    selector: X86SystemSelector,
    width: MemWidth,
    requires_apx: bool,
    next_pc: u64,
) -> OpKind {
    OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
        selector,
        source: X86SystemSelectorSource::Stack {
            stack_pointer: x86(X86Reg::Rsp),
            width,
        },
        requires_apx,
        next_pc,
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
    x86_system_selector_load_shape_valid(&function.blocks[0].ops[0])
}

#[test]
fn lower_pop_segment_requires_precise_guards_mmu_and_exact_lifter_shape() {
    let valid = stack_load(X86SystemSelector::Fs, MemWidth::B8, true, 0x1004);
    assert!(shape_valid(valid.clone()));
    let missing_guards = lower(valid.clone(), true, false, false).unwrap_err();
    assert!(
        matches!(missing_guards, LowerError::UnsupportedOp { .. }),
        "{missing_guards:?}"
    );
    let missing_helpers = lower(valid.clone(), false, true, false).unwrap_err();
    assert!(
        matches!(missing_helpers, LowerError::UnsupportedOp { .. }),
        "{missing_helpers:?}"
    );

    let (code, _) = lower(valid, true, true, false).expect("lower guarded POP FS");
    for offset in [
        X86_GUEST_APX_ENABLED_OFFSET,
        X86_GUEST_EFER_OFFSET,
        X86_GUEST_CS_L_OFFSET,
        X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET,
    ] {
        assert!(
            code.windows(4)
                .any(|window| window == (offset as u32).to_le_bytes()),
            "missing state/helper offset {offset}: {code:02X?}"
        );
    }

    for malformed in [
        stack_load(X86SystemSelector::Ds, MemWidth::B8, false, 0x1002),
        OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
            selector: X86SystemSelector::Gs,
            source: X86SystemSelectorSource::Stack {
                stack_pointer: x86(X86Reg::Rax),
                width: MemWidth::B8,
            },
            requires_apx: false,
            next_pc: 0x1002,
        }),
        stack_load(X86SystemSelector::Fs, MemWidth::B1, false, 0x1002),
        stack_load(X86SystemSelector::Gs, MemWidth::B4, false, 0x1002),
        stack_load(X86SystemSelector::Gs, MemWidth::B2, false, 0x1002),
        stack_load(X86SystemSelector::Gs, MemWidth::B2, true, 0x1003),
        stack_load(X86SystemSelector::Fs, MemWidth::B8, true, 0x1002),
        stack_load(X86SystemSelector::Fs, MemWidth::B8, false, 0x1001),
    ] {
        assert!(!shape_valid(malformed.clone()), "{malformed:?}");
        assert!(matches!(
            lower(malformed, true, true, false),
            Err(LowerError::InvalidOperand { .. } | LowerError::InvalidRegister(_))
        ));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct PopSegmentContext {
    calls: u64,
    operand: u64,
    encoding: u32,
    ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
unsafe extern "C" fn load_selector(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    operand: u64,
    encoding: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut PopSegmentContext) };
    context.calls += 1;
    context.operand = operand;
    context.encoding = encoding;
    context.ok
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute(
    kind: OpKind,
    context: &mut PopSegmentContext,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> ([u64; 32], crate::smir::lower::runtime::GuestRegs) {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(kind, true, true, true).expect("lower executable POP FS/GS");
    let exec = ExecMem::new(&code).expect("map executable POP FS/GS");
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
    state.ctx = (context as *mut PopSegmentContext) as u64;
    state.system_selector_load_fn = load_selector as usize as u64;
    configure(&mut state);
    let initial_gprs = state.gpr;
    exec.run(entry, &mut state);
    (initial_gprs, state)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_pop_segment_encodes_stack_source_commits_rsp_and_hands_off_exactly() {
    for (selector, selector_id, width, requires_apx, next_pc) in [
        (X86SystemSelector::Fs, 6_u32, MemWidth::B8, false, 0x1002),
        (X86SystemSelector::Gs, 7, MemWidth::B2, false, 0x1003),
        (X86SystemSelector::Gs, 7, MemWidth::B8, true, 0x1004),
    ] {
        let mut context = PopSegmentContext {
            ok: 1,
            ..PopSegmentContext::default()
        };
        let (initial, state) = execute(
            stack_load(selector, width, requires_apx, next_pc),
            &mut context,
            |_| {},
        );
        assert_eq!(context.calls, 1, "{selector:?} {width:?}");
        assert_eq!(context.operand, initial[4], "{selector:?} {width:?}");
        assert_eq!(
            context.encoding,
            1 | (selector_id << 2)
                | (u32::from(width == MemWidth::B8) << 5)
                | (1 << 6)
                | (u32::from(requires_apx) << 1),
            "{selector:?} {width:?}"
        );
        for (index, observed) in state.gpr.iter().enumerate() {
            let expected = if index == 4 {
                initial[4] + u64::from(width.bytes())
            } else {
                initial[index]
            };
            assert_eq!(*observed, expected, "{selector:?} {width:?} GPR{index}");
        }
        assert_eq!(state.exit_pc, next_pc);
        assert_eq!(state.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
        assert_eq!(state.ac_flag, 1);
        assert_eq!(
            state.gpr[10], initial[10],
            "ops after the terminal handoff must be unreachable"
        );
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_pop_segment_guards_and_helper_failure_are_precise_and_noncommitting() {
    let mut failed = PopSegmentContext::default();
    let (initial, state) = execute(
        stack_load(X86SystemSelector::Fs, MemWidth::B8, false, 0x1002),
        &mut failed,
        |_| {},
    );
    assert_eq!(failed.calls, 1);
    assert_eq!(state.gpr, initial);
    assert_eq!(state.exit_pc, 0x1000);
    assert_eq!(state.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    assert_eq!(state.ac_flag, 1);

    for (name, apx, efer, cs_l) in [
        ("APX", 0_u64, 1 << 10, 1_u64),
        ("EFER.LMA", 1, 0, 1),
        ("CS.L", 1, 1 << 10, 0),
    ] {
        let mut context = PopSegmentContext {
            ok: 1,
            ..PopSegmentContext::default()
        };
        let (initial, state) = execute(
            stack_load(X86SystemSelector::Gs, MemWidth::B8, true, 0x1004),
            &mut context,
            |state| {
                state.apx_enabled = apx;
                state.efer = efer;
                state.cs_l = cs_l;
            },
        );
        assert_eq!(context.calls, 0, "{name}");
        assert_eq!(state.gpr, initial, "{name}");
        assert_eq!(state.exit_pc, 0x1000, "{name}");
        assert_eq!(state.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
        assert_eq!(state.ac_flag, 1, "{name}");
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_pop_segment_canonical_range_guards_cover_both_regions_and_wrap() {
    for (name, width, rsp) in [
        (
            "B8 crosses lower boundary",
            MemWidth::B8,
            0x0000_7FFF_FFFF_FFFC_u64,
        ),
        (
            "B8 noncanonical lower gap",
            MemWidth::B8,
            0x0000_8000_0000_0000,
        ),
        (
            "B8 noncanonical upper gap",
            MemWidth::B8,
            0xFFFF_7FFF_FFFF_FFFF,
        ),
        ("B8 wrap", MemWidth::B8, u64::MAX - 3),
        (
            "B2 crosses lower boundary",
            MemWidth::B2,
            0x0000_7FFF_FFFF_FFFF,
        ),
        (
            "B2 noncanonical upper gap",
            MemWidth::B2,
            0xFFFF_7FFF_FFFF_FFFF,
        ),
        ("B2 wrap", MemWidth::B2, u64::MAX),
    ] {
        let mut context = PopSegmentContext {
            ok: 1,
            ..PopSegmentContext::default()
        };
        let (initial, state) = execute(
            stack_load(
                X86SystemSelector::Fs,
                width,
                false,
                if width == MemWidth::B2 {
                    0x1003
                } else {
                    0x1002
                },
            ),
            &mut context,
            |state| state.gpr[4] = rsp,
        );
        assert_eq!(context.calls, 0, "{name}");
        assert_eq!(state.gpr, initial, "{name}");
        assert_eq!(state.exit_pc, 0x1000, "{name}");
        assert_eq!(state.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
        assert_eq!(state.ac_flag, 1, "{name}");
    }

    for (name, width, rsp) in [
        (
            "B8 lower region endpoint",
            MemWidth::B8,
            0x0000_7FFF_FFFF_FFF8_u64,
        ),
        ("B8 upper region start", MemWidth::B8, 0xFFFF_8000_0000_0000),
        ("B2 upper region start", MemWidth::B2, 0xFFFF_8000_0000_0000),
        (
            "B2 lower region endpoint",
            MemWidth::B2,
            0x0000_7FFF_FFFF_FFFE,
        ),
        (
            "B8 upper region endpoint",
            MemWidth::B8,
            0xFFFF_FFFF_FFFF_FFF8,
        ),
        ("B2 upper region endpoint", MemWidth::B2, u64::MAX - 1),
    ] {
        let mut context = PopSegmentContext {
            ok: 1,
            ..PopSegmentContext::default()
        };
        let (_, state) = execute(
            stack_load(
                X86SystemSelector::Gs,
                width,
                false,
                if width == MemWidth::B2 {
                    0x1003
                } else {
                    0x1002
                },
            ),
            &mut context,
            |state| state.gpr[4] = rsp,
        );
        assert_eq!(context.calls, 1, "{name}");
        assert_eq!(context.operand, rsp, "{name}");
        assert_eq!(
            state.gpr[4],
            rsp.wrapping_add(u64::from(width.bytes())),
            "{name}"
        );
        assert_eq!(
            state.exit_pc,
            if width == MemWidth::B2 {
                0x1003
            } else {
                0x1002
            },
            "{name}"
        );
        assert_eq!(state.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
        assert_eq!(state.ac_flag, 1, "{name}");
    }
}
