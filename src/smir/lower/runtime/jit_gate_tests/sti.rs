//! Fail-closed native admission and ABI-layout coverage for x86 STI.

use super::*;
use crate::smir::lower::runtime::GuestRegs;
use crate::smir::lower::x86_64::x86_sti_shape_valid;
use crate::smir::lower::{
    X86_GUEST_CLI_FN_OFFSET, X86_GUEST_INTERRUPT_INHIBIT_OFFSET, X86_GUEST_STI_FN_OFFSET,
};

fn sti(requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86Sti {
        requires_apx,
        next_pc,
    }
}

fn smir_op(pc: u64, kind: OpKind) -> crate::smir::ir::ops::SmirOp {
    crate::smir::ir::ops::SmirOp::new(crate::smir::ir::types::OpId(0), pc, kind)
}

#[test]
fn x86_sti_state_layout_is_exact_appended_and_zero_initialized() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, interrupt_inhibit),
        X86_GUEST_INTERRUPT_INHIBIT_OFFSET as usize
    );
    assert_eq!(
        X86_GUEST_INTERRUPT_INHIBIT_OFFSET,
        X86_GUEST_CLI_FN_OFFSET + 8
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, sti_fn),
        X86_GUEST_STI_FN_OFFSET as usize
    );
    assert_eq!(
        X86_GUEST_STI_FN_OFFSET,
        X86_GUEST_INTERRUPT_INHIBIT_OFFSET + 8
    );
    assert_eq!(GuestRegs::default().interrupt_inhibit, 0);
    assert_eq!(GuestRegs::default().sti_fn, 0);
}

#[test]
fn x86_gate_admits_every_exact_sti_length_and_apx_form() {
    for (requires_apx, lengths) in [(false, &[1_u64, 2, 15][..]), (true, &[3, 4, 15])] {
        for &length in lengths {
            let kind = sti(requires_apx, 0x1000 + length);
            let op = smir_op(0x1000, kind.clone());
            assert!(kind.is_jit_safe(), "APX={requires_apx} len={length}");
            assert!(x86_sti_shape_valid(&op), "APX={requires_apx} len={length}");
            assert!(x86_gate(kind), "APX={requires_apx} len={length}");
        }
    }
}

#[test]
fn x86_sti_gate_rejects_malformed_frontiers_hints_and_cross_hosts() {
    for malformed in [
        sti(false, 0x1000),
        sti(false, 0x1010),
        sti(false, 0x0FFF),
        sti(true, 0x1002),
        sti(true, 0x1010),
    ] {
        assert!(malformed.is_jit_safe(), "class whitelist is shape-agnostic");
        assert!(!x86_sti_shape_valid(&smir_op(0x1000, malformed.clone())));
        assert!(!x86_gate(malformed));
    }

    let exact = sti(false, 0x1001);
    assert!(!aarch64_gate(vec![exact.clone()], false));
    assert!(!x86_aarch64_gate(vec![exact.clone()]));
    assert!(!x86_aarch64_scalar_shape_valid(&exact));

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_sti_shape_valid(&hinted.blocks[0].ops[0]));
    assert!(!is_native_clobber_safe(&hinted));
}

#[test]
fn x86_sti_survives_o2_and_remains_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, sti(true, 0x1003));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(matches!(
        function.entry_block().unwrap().ops.as_slice(),
        [crate::smir::ir::ops::SmirOp {
            kind: OpKind::X86Sti {
                requires_apx: true,
                next_pc: 0x1003
            },
            ..
        }]
    ));
    assert!(is_native_clobber_safe(&function));
}
