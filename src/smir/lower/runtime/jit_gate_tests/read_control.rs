//! Fail-closed native admission for x86 MOV-from-control-register operations.

use super::*;
use crate::smir::ir::ops::X86ControlReg;
use crate::smir::lower::x86_64::x86_read_control_shape_valid;

fn read(dst: VReg, control: X86ControlReg) -> OpKind {
    OpKind::X86ReadControl { dst, control }
}

#[test]
fn x86_read_control_gate_admits_every_gpr_including_stack_aliases_and_egprs() {
    for destination in [
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
            let op = read(x86(destination), control);
            assert!(op.is_jit_safe(), "{destination:?} {control:?}");
            assert!(x86_read_control_shape_valid(&op));
            assert!(x86_gate(op), "{destination:?} {control:?}");
        }
    }
}

#[test]
fn x86_read_control_gate_rejects_non_lifter_shapes_and_cross_hosts() {
    for malformed in [
        read(VReg::virt(1), X86ControlReg::Cr0),
        read(VReg::Imm(0), X86ControlReg::Cr2),
        read(arm_x(0), X86ControlReg::Cr4),
    ] {
        assert!(!x86_read_control_shape_valid(&malformed));
        assert!(!x86_gate(malformed));
    }

    let exact = read(x86(X86Reg::Rax), X86ControlReg::Cr0);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact.clone());
    builder.set_terminator(Terminator::Return { values: vec![] });
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &builder.finish(),
        &std::collections::HashMap::new(),
    ));
    assert!(!x86_aarch64_scalar_shape_valid(&exact));
    assert!(!aarch64_gate(vec![exact], false));
}

#[test]
fn x86_read_control_survives_o2_and_remains_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, read(x86(X86Reg::Rbx), X86ControlReg::Cr3));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(function.entry_block().unwrap().ops.iter().any(|op| {
        matches!(
            op.kind,
            OpKind::X86ReadControl {
                control: X86ControlReg::Cr3,
                ..
            }
        )
    }));
    assert!(is_native_clobber_safe(&function));
}
