//! Fail-closed native admission for x86 MOV-to-debug-register operations.

use super::*;
use crate::smir::ir::ops::X86DebugReg;
use crate::smir::lower::x86_64::x86_write_debug_shape_valid;

fn write(src: VReg, debug: X86DebugReg) -> OpKind {
    OpKind::X86WriteDebug { src, debug }
}

#[test]
fn x86_write_debug_gate_admits_exact_legacy_gprs_including_rsp_rbp() {
    for source in [X86Reg::Rax, X86Reg::Rsp, X86Reg::Rbp, X86Reg::R15] {
        for debug in [
            X86DebugReg::Dr0,
            X86DebugReg::Dr1,
            X86DebugReg::Dr2,
            X86DebugReg::Dr3,
            X86DebugReg::Dr4,
            X86DebugReg::Dr5,
            X86DebugReg::Dr6,
            X86DebugReg::Dr7,
        ] {
            let op = write(x86(source), debug);
            assert!(op.is_jit_safe(), "{source:?} {debug:?}");
            assert!(x86_write_debug_shape_valid(&op));
            assert!(x86_gate(op), "{source:?} {debug:?}");
        }
    }
}

#[test]
fn x86_write_debug_gate_rejects_non_lifter_shapes_and_cross_hosts() {
    for malformed in [
        write(VReg::virt(1), X86DebugReg::Dr0),
        write(VReg::Imm(0), X86DebugReg::Dr2),
        write(x86(X86Reg::gpr(16)), X86DebugReg::Dr6),
        write(arm_x(0), X86DebugReg::Dr7),
    ] {
        assert!(!x86_write_debug_shape_valid(&malformed));
        assert!(!x86_gate(malformed));
    }

    let exact = write(x86(X86Reg::Rax), X86DebugReg::Dr0);
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
fn x86_write_debug_survives_o2_and_remains_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, write(x86(X86Reg::Rbx), X86DebugReg::Dr3));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(function.entry_block().unwrap().ops.iter().any(|op| {
        matches!(
            op.kind,
            OpKind::X86WriteDebug {
                debug: X86DebugReg::Dr3,
                ..
            }
        )
    }));
    assert!(is_native_clobber_safe(&function));
}
