//! State-backed native lowering tests for x87 no-wait controls.

use super::*;
use crate::smir::lower::{
    X86_GUEST_CR0_OFFSET, X86_GUEST_X87_CONTROL_WORD_OFFSET, X86_GUEST_X87_STATUS_WORD_OFFSET,
    X86_GUEST_X87_TAG_WORD_OFFSET,
};

fn control(kind: X86X87ControlKind) -> OpKind {
    OpKind::X86X87Control { kind, addr: None }
}

fn lower(
    kind: OpKind,
    fault_guards: bool,
    hint: Option<X86OpHint>,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = hint;
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&function)?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_x87_no_wait_controls_require_dynamic_fault_guards_and_exact_state_slots() {
    for kind in [
        X86X87ControlKind::Init,
        X86X87ControlKind::ClearExceptions,
        X86X87ControlKind::StoreStatusAx,
    ] {
        assert!(matches!(
            lower(control(kind), false, None),
            Err(LowerError::UnsupportedOp { .. })
        ));
        let (code, _) = lower(control(kind), true, None).expect("guarded x87 control");
        assert!(
            code.windows(4)
                .any(|window| window == (X86_GUEST_CR0_OFFSET as u32).to_le_bytes()),
            "{kind:?}: missing dynamic CR0 guard: {code:02X?}"
        );
        let state_offset = match kind {
            X86X87ControlKind::Init => X86_GUEST_X87_CONTROL_WORD_OFFSET,
            X86X87ControlKind::ClearExceptions | X86X87ControlKind::StoreStatusAx => {
                X86_GUEST_X87_STATUS_WORD_OFFSET
            }
            _ => unreachable!(),
        };
        assert!(
            code.windows(4)
                .any(|window| window == (state_offset as u32).to_le_bytes()),
            "{kind:?}: missing x87 state slot: {code:02X?}"
        );
    }

    let (init, _) = lower(control(X86X87ControlKind::Init), true, None).unwrap();
    assert!(
        init.windows(4)
            .any(|window| window == (X86_GUEST_X87_TAG_WORD_OFFSET as u32).to_le_bytes())
    );
}

#[test]
fn lower_x87_controls_reject_non_lifter_shapes_fail_closed() {
    let malformed_address = OpKind::X86X87Control {
        kind: X86X87ControlKind::Init,
        addr: Some(Address::PcRel {
            offset: 0,
            disp_size: DispSize::Disp32,
            base: Some(0x1002),
        }),
    };
    assert!(matches!(
        lower(malformed_address, true, None),
        Err(LowerError::InvalidOperand { .. })
    ));
    assert!(matches!(
        lower(
            control(X86X87ControlKind::ClearExceptions),
            true,
            Some(X86OpHint::RexByteReg),
        ),
        Err(LowerError::InvalidOperand { .. })
    ));

    for kind in [
        X86X87ControlKind::LoadControlWord,
        X86X87ControlKind::StoreControlWord,
        X86X87ControlKind::StoreStatusWord,
    ] {
        assert!(matches!(
            lower(control(kind), true, None),
            Err(LowerError::UnsupportedOp { .. })
        ));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native(
    kind: X86X87ControlKind,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(control(kind), true, None).expect("lower native x87 control");
    let exec = ExecMem::new(&code).expect("map native x87 control");
    let mut regs = GuestRegs::default();
    regs.gpr = std::array::from_fn(|index| 0xA500_0000_0000_0000 | index as u64);
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.exit_pc = 0xDEAD_BEEF;
    regs.cr0 = 0x21;
    regs.x87_control_word = 0x027F;
    regs.x87_status_word = 0xFFFF;
    regs.x87_tag_word = 0x6996;
    regs.x87_data_ptr = 0x1122_3344_5566_7788;
    regs.x87_instr_ptr = 0x8877_6655_4433_2211;
    regs.x87_last_opcode = 0x05A5;
    configure(&mut regs);
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_x87_controls_commit_only_the_architecturally_selected_state() {
    let init = execute_native(X86X87ControlKind::Init, |_| {});
    assert_eq!(init.x87_control_word, 0x037F);
    assert_eq!(init.x87_status_word, 0);
    assert_eq!(init.x87_tag_word, 0xFFFF);
    assert_eq!(init.x87_data_ptr, 0);
    assert_eq!(init.x87_instr_ptr, 0);
    assert_eq!(init.x87_last_opcode, 0);
    assert_eq!(init.gpr[0], 0xA500_0000_0000_0000);
    assert_eq!(init.rflags, 0x2 | 0x08D5 | (1 << 10));

    let clear = execute_native(X86X87ControlKind::ClearExceptions, |_| {});
    assert_eq!(clear.x87_control_word, 0x027F);
    assert_eq!(clear.x87_status_word, 0x7F00);
    assert_eq!(clear.x87_tag_word, 0x6996);
    assert_eq!(clear.x87_data_ptr, 0x1122_3344_5566_7788);
    assert_eq!(clear.x87_instr_ptr, 0x8877_6655_4433_2211);
    assert_eq!(clear.x87_last_opcode, 0x05A5);
    assert_eq!(clear.gpr[0], 0xA500_0000_0000_0000);
    assert_eq!(clear.rflags, 0x2 | 0x08D5 | (1 << 10));

    let status = execute_native(X86X87ControlKind::StoreStatusAx, |regs| {
        regs.x87_status_word = 0xC5A3;
        regs.gpr[0] = 0x1122_3344_5566_7788;
    });
    assert_eq!(status.gpr[0], 0x1122_3344_5566_C5A3);
    assert_eq!(status.x87_status_word, 0xC5A3);
    assert_eq!(status.rflags, 0x2 | 0x08D5 | (1 << 10));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_x87_cr0_guard_exits_at_the_instruction_without_committing() {
    for fault_bits in [1 << 2, 1 << 3, (1 << 2) | (1 << 3)] {
        for kind in [
            X86X87ControlKind::Init,
            X86X87ControlKind::ClearExceptions,
            X86X87ControlKind::StoreStatusAx,
        ] {
            let guarded = execute_native(kind, |regs| regs.cr0 |= fault_bits);
            assert_eq!(guarded.exit_pc, 0x1000, "{kind:?}, CR0={fault_bits:#x}");
            assert_eq!(guarded.x87_control_word, 0x027F, "{kind:?}");
            assert_eq!(guarded.x87_status_word, 0xFFFF, "{kind:?}");
            assert_eq!(guarded.x87_tag_word, 0x6996, "{kind:?}");
            assert_eq!(guarded.gpr[0], 0xA500_0000_0000_0000, "{kind:?}");
            assert_eq!(guarded.rflags, 0x2 | 0x08D5 | (1 << 10), "{kind:?}");
        }
    }
}
