//! Fault-precise native lowering for long-mode `LSS/LFS/LGS`.

use super::*;
use crate::smir::ir::ops::{X86SystemSelector, X86SystemSelectorLoadOp, X86SystemSelectorSource};
use crate::smir::lower::{
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CS_L_OFFSET, X86_GUEST_EFER_OFFSET,
    X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET,
};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn far_load(
    selector: X86SystemSelector,
    addr: Address,
    dst: VReg,
    width: OpWidth,
    requires_apx: bool,
    stack_segment: bool,
    next_pc: u64,
) -> OpKind {
    OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
        selector,
        source: X86SystemSelectorSource::FarPointer {
            addr,
            dst,
            offset_width: width,
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
    continuation: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    if continuation {
        builder.push_op(
            0x1004,
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
fn lower_far_pointer_load_requires_precise_guards_mmu_and_exact_lifter_shape() {
    let valid = far_load(
        X86SystemSelector::Fs,
        Address::Direct(x86(X86Reg::R31)),
        x86(X86Reg::R30),
        OpWidth::W64,
        true,
        false,
        0x1004,
    );
    assert!(shape_valid(valid.clone()));
    assert!(matches!(
        lower(valid.clone(), true, false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower(valid.clone(), false, true, false),
        Err(LowerError::UnsupportedOp { .. })
    ));

    let (code, _) = lower(valid, true, true, false).expect("guarded LFS lowering");
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
    assert!(
        code.windows(4)
            .any(|window| window == 0x1000_u32.to_le_bytes()),
        "helper failure must restart at the faulting guest PC: {code:02X?}"
    );

    for malformed in [
        far_load(
            X86SystemSelector::Ds,
            Address::Direct(x86(X86Reg::Rax)),
            x86(X86Reg::Rcx),
            OpWidth::W32,
            false,
            false,
            0x1003,
        ),
        far_load(
            X86SystemSelector::Fs,
            Address::Direct(VReg::virt(0)),
            x86(X86Reg::Rcx),
            OpWidth::W32,
            false,
            false,
            0x1003,
        ),
        far_load(
            X86SystemSelector::Fs,
            Address::Direct(x86(X86Reg::R31)),
            x86(X86Reg::Rcx),
            OpWidth::W32,
            false,
            false,
            0x1003,
        ),
        far_load(
            X86SystemSelector::Fs,
            Address::Direct(x86(X86Reg::Rax)),
            x86(X86Reg::R31),
            OpWidth::W32,
            false,
            false,
            0x1003,
        ),
        far_load(
            X86SystemSelector::Fs,
            Address::Direct(x86(X86Reg::Rax)),
            VReg::virt(0),
            OpWidth::W32,
            false,
            false,
            0x1003,
        ),
        far_load(
            X86SystemSelector::Fs,
            Address::Direct(x86(X86Reg::Rax)),
            x86(X86Reg::Rcx),
            OpWidth::W8,
            false,
            false,
            0x1003,
        ),
        far_load(
            X86SystemSelector::Fs,
            Address::Direct(x86(X86Reg::Rax)),
            x86(X86Reg::Rcx),
            OpWidth::W64,
            false,
            false,
            0x1003,
        ),
        far_load(
            X86SystemSelector::Fs,
            Address::Direct(x86(X86Reg::Rax)),
            x86(X86Reg::Rcx),
            OpWidth::W32,
            false,
            false,
            0x1002,
        ),
        far_load(
            X86SystemSelector::Fs,
            Address::Direct(x86(X86Reg::Rax)),
            x86(X86Reg::Rcx),
            OpWidth::W32,
            false,
            false,
            0x1010,
        ),
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
struct FarLoadContext {
    calls: u64,
    operand: u64,
    encoding: u32,
    offset: u64,
    ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
unsafe extern "C" fn load_selector(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    operand: u64,
    encoding: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut FarLoadContext) };
    context.calls += 1;
    context.operand = operand;
    context.encoding = encoding;
    if context.ok == 0 {
        return 0;
    }
    let dst = ((encoding >> 8) & 0x1F) as usize;
    state.gpr[dst] = match (encoding >> 13) & 3 {
        0 => (state.gpr[dst] & !0xFFFF) | (context.offset & 0xFFFF),
        1 => context.offset & 0xFFFF_FFFF,
        2 => context.offset,
        _ => return 0,
    };
    1
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute(
    kind: OpKind,
    context: &mut FarLoadContext,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> ([u64; 32], crate::smir::lower::runtime::GuestRegs) {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(kind, true, true, true).expect("lower executable LSS/LFS/LGS");
    let exec = ExecMem::new(&code).expect("map executable LSS/LFS/LGS");
    let mut state = GuestRegs::default();
    for (index, value) in state.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    state.cr0 = 1;
    state.efer = 1 << 10;
    state.cs_l = 1;
    state.apx_enabled = 1;
    state.rflags = 0x2 | 0x08D5 | (1 << 10);
    state.ac_flag = 1;
    state.exit_pc = 0xDEAD_BEEF_DEAD_BEEF;
    state.ctx = (context as *mut FarLoadContext) as u64;
    state.system_selector_load_fn = load_selector as usize as u64;
    configure(&mut state);
    let initial = state.gpr;
    exec.run(entry, &mut state);
    (initial, state)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_far_pointer_load_encodes_all_targets_widths_aliases_and_hands_off_exactly() {
    for (selector, selector_id, addr, dst, width, apx, next_pc, offset) in [
        (
            X86SystemSelector::Fs,
            6_u32,
            Address::Direct(x86(X86Reg::Rax)),
            X86Reg::Rcx,
            OpWidth::W16,
            false,
            0x1004,
            0x1234_BEEF,
        ),
        (
            X86SystemSelector::Gs,
            7,
            Address::Direct(x86(X86Reg::Rax)),
            X86Reg::Rax,
            OpWidth::W32,
            false,
            0x1003,
            0x1234_89AB_CDEF,
        ),
        (
            X86SystemSelector::Ss,
            4,
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rsp)),
                index: x86(X86Reg::R30),
                scale: 4,
                disp: -8,
                disp_size: DispSize::Disp8,
            },
            X86Reg::R31,
            OpWidth::W64,
            true,
            0x1006,
            0x0123_4567_89AB_CDEF,
        ),
    ] {
        let mut context = FarLoadContext {
            offset,
            ok: 1,
            ..FarLoadContext::default()
        };
        let (initial, state) = execute(
            far_load(
                selector,
                addr,
                x86(dst),
                width,
                apx,
                selector == X86SystemSelector::Ss,
                next_pc,
            ),
            &mut context,
            |state| {
                state.gpr[0] = 0x3000;
                state.gpr[4] = 0x4000;
                state.gpr[30] = 0x24;
            },
        );
        let dst_index = dst.gpr_index().unwrap() as usize;
        let expected_operand = match selector {
            X86SystemSelector::Ss => 0x4000 + 0x24 * 4 - 8,
            _ => initial[0],
        };
        let expected_dst = match width {
            OpWidth::W16 => (initial[dst_index] & !0xFFFF) | (offset & 0xFFFF),
            OpWidth::W32 => offset & 0xFFFF_FFFF,
            OpWidth::W64 => offset,
            _ => unreachable!(),
        };
        let width_code = match width {
            OpWidth::W16 => 0,
            OpWidth::W32 => 1,
            OpWidth::W64 => 2,
            _ => unreachable!(),
        };
        assert_eq!(context.calls, 1, "{selector:?} {width:?}");
        assert_eq!(context.operand, expected_operand, "{selector:?} {width:?}");
        assert_eq!(
            context.encoding,
            1 | (u32::from(apx) << 1)
                | (selector_id << 2)
                | (1 << 7)
                | ((dst_index as u32) << 8)
                | (width_code << 13),
            "{selector:?} {width:?}"
        );
        for (index, observed) in state.gpr.iter().enumerate() {
            assert_eq!(
                *observed,
                if index == dst_index {
                    expected_dst
                } else {
                    initial[index]
                },
                "{selector:?} {width:?} GPR{index}"
            );
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
fn native_far_pointer_load_guards_and_helper_failure_are_noncommitting() {
    let op = far_load(
        X86SystemSelector::Fs,
        Address::Direct(x86(X86Reg::R31)),
        x86(X86Reg::R30),
        OpWidth::W64,
        true,
        false,
        0x1004,
    );
    for (name, ok, apx, efer, cs_l, expected_calls) in [
        ("helper", 0_u64, 1_u64, 1_u64 << 10, 1_u64, 1_u64),
        ("APX", 1, 0, 1 << 10, 1, 0),
        ("EFER.LMA", 1, 1, 0, 1, 0),
        ("CS.L", 1, 1, 1 << 10, 0, 0),
    ] {
        let mut context = FarLoadContext {
            offset: 0x0123_4567_89AB_CDEF,
            ok,
            ..FarLoadContext::default()
        };
        let (initial, state) = execute(op.clone(), &mut context, |state| {
            state.gpr[31] = 0x3000;
            state.apx_enabled = apx;
            state.efer = efer;
            state.cs_l = cs_l;
        });
        assert_eq!(context.calls, expected_calls, "{name}");
        assert_eq!(state.gpr, initial, "{name}");
        assert_eq!(state.exit_pc, 0x1000, "{name}");
        assert_eq!(state.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
        assert_eq!(state.ac_flag, 1, "{name}");
    }
}
