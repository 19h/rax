//! Fail-closed native admission for x86 MOV-to-control-register operations.

use super::*;
use crate::smir::ir::ops::X86ControlReg;
use crate::smir::lower::x86_64::x86_write_control_shape_valid;

fn write(src: VReg, control: X86ControlReg, next_pc: u64) -> OpKind {
    OpKind::X86WriteControl {
        src,
        control,
        next_pc,
    }
}

fn smir_op(pc: u64, kind: OpKind) -> crate::smir::ir::ops::SmirOp {
    crate::smir::ir::ops::SmirOp::new(crate::smir::ir::types::OpId(0), pc, kind)
}

#[test]
fn x86_write_control_layout_and_gate_admit_exact_lifter_shapes() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, efer),
        X86_GUEST_EFER_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cs_l),
        X86_GUEST_CS_L_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, tr_type),
        X86_GUEST_TR_TYPE_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, control_write_fn),
        X86_GUEST_CONTROL_WRITE_FN_OFFSET as usize
    );

    for source in [
        X86Reg::Rax,
        X86Reg::Rsp,
        X86Reg::Rbp,
        X86Reg::R15,
        X86Reg::R16,
        X86Reg::R31,
    ] {
        for control in [
            X86ControlReg::Cr0,
            X86ControlReg::Cr2,
            X86ControlReg::Cr3,
            X86ControlReg::Cr4,
            X86ControlReg::Cr8,
        ] {
            let lengths: &[u64] = if source.gpr_index().unwrap() >= 16 {
                &[4, 15]
            } else {
                &[3, 4, 15]
            };
            for &length in lengths {
                let kind = write(x86(source), control, 0x1000 + length);
                let op = smir_op(0x1000, kind.clone());
                assert!(kind.is_jit_safe(), "{source:?} {control:?} len={length}");
                assert!(
                    x86_write_control_shape_valid(&op),
                    "{source:?} {control:?} len={length}"
                );
                assert!(x86_gate(kind), "{source:?} {control:?} len={length}");
            }
        }
    }
}

#[test]
fn x86_write_control_gate_rejects_non_lifter_shapes_and_cross_hosts() {
    for malformed in [
        write(VReg::virt(1), X86ControlReg::Cr0, 0x1003),
        write(VReg::Imm(0), X86ControlReg::Cr2, 0x1003),
        write(x86(X86Reg::R16), X86ControlReg::Cr3, 0x1003),
        write(arm_x(0), X86ControlReg::Cr4, 0x1003),
        write(x86(X86Reg::Rax), X86ControlReg::Cr8, 0x1002),
        write(x86(X86Reg::Rax), X86ControlReg::Cr8, 0x1010),
        write(x86(X86Reg::Rax), X86ControlReg::Cr8, 0x0FFF),
    ] {
        assert!(!x86_write_control_shape_valid(&smir_op(
            0x1000,
            malformed.clone()
        )));
        assert!(!x86_gate(malformed));
    }

    let exact = write(x86(X86Reg::Rax), X86ControlReg::Cr0, 0x1003);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact.clone());
    builder.set_terminator(Terminator::Return { values: vec![] });
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &builder.finish(),
        &std::collections::HashMap::new(),
    ));
    assert!(!x86_aarch64_scalar_shape_valid(&exact));
    assert!(!aarch64_gate(vec![exact.clone()], false));

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_write_control_shape_valid(&hinted.blocks[0].ops[0]));
    assert!(!is_native_clobber_safe(&hinted));
}

#[test]
fn x86_write_control_survives_o2_and_remains_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, write(x86(X86Reg::Rbx), X86ControlReg::Cr3, 0x1003));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(function.entry_block().unwrap().ops.iter().any(|op| {
        matches!(
            op.kind,
            OpKind::X86WriteControl {
                control: X86ControlReg::Cr3,
                next_pc: 0x1003,
                ..
            }
        )
    }));
    assert!(is_native_clobber_safe(&function));
}
