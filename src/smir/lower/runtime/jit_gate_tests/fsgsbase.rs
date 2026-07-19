//! Fail-closed native admission for x86 FSGSBASE.

use super::*;

fn fsgsbase(operand: VReg, base: VReg, write: bool, width: OpWidth, requires_apx: bool) -> OpKind {
    OpKind::X86FsGsBase {
        operand,
        base,
        write,
        width,
        requires_apx,
    }
}

#[test]
fn x86_fsgsbase_gate_admits_every_exact_direction_width_and_gpr_class() {
    for index in [0_u8, 4, 5, 8, 15, 16, 31] {
        for base_reg in [X86Reg::FsBase, X86Reg::GsBase] {
            for write in [false, true] {
                for width in [OpWidth::W32, OpWidth::W64] {
                    let op = fsgsbase(
                        x86(X86Reg::gpr(index)),
                        x86(base_reg),
                        write,
                        width,
                        index >= 16,
                    );
                    assert!(op.is_jit_safe(), "class whitelist: {op:?}");
                    assert!(x86_gate(op), "exact shape rejected: index={index}");
                }
            }
        }
    }

    // REX2 may encode legacy GPRs and still carries the dynamic APX guard.
    assert!(x86_gate(fsgsbase(
        x86(X86Reg::Rax),
        x86(X86Reg::FsBase),
        false,
        OpWidth::W64,
        true,
    )));
}

#[test]
fn x86_fsgsbase_gate_rejects_malformed_ir_and_cross_host_execution() {
    for malformed in [
        fsgsbase(
            x86(X86Reg::FsBase),
            x86(X86Reg::GsBase),
            false,
            OpWidth::W64,
            false,
        ),
        fsgsbase(
            VReg::Virtual(VirtualId(0)),
            x86(X86Reg::FsBase),
            false,
            OpWidth::W64,
            false,
        ),
        fsgsbase(
            x86(X86Reg::Rax),
            x86(X86Reg::Rbx),
            false,
            OpWidth::W64,
            false,
        ),
        fsgsbase(
            x86(X86Reg::Rax),
            x86(X86Reg::FsBase),
            false,
            OpWidth::W16,
            false,
        ),
        fsgsbase(
            x86(X86Reg::R16),
            x86(X86Reg::FsBase),
            false,
            OpWidth::W64,
            false,
        ),
    ] {
        assert!(malformed.is_jit_safe(), "class whitelist is shape-agnostic");
        assert!(!x86_gate(malformed), "malformed FSGSBASE admitted");
    }

    let exact = fsgsbase(
        x86(X86Reg::Rax),
        x86(X86Reg::FsBase),
        false,
        OpWidth::W64,
        false,
    );
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact.clone());
    builder.set_terminator(Terminator::Return { values: vec![] });
    assert!(
        !is_x86_aarch64_native_clobber_safe_excluding(
            &builder.finish(),
            &std::collections::HashMap::new(),
        ),
        "FSGSBASE has no AArch64-host lowering"
    );
    assert!(!x86_aarch64_scalar_shape_valid(&exact));
}

#[test]
fn x86_fsgsbase_encoded_operand_survives_o2_and_remains_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: x86(X86Reg::R16),
            src: SrcOperand::Reg(x86(X86Reg::Rax)),
            width: OpWidth::W64,
        },
    );
    builder.push_op(
        0x1004,
        fsgsbase(
            x86(X86Reg::R16),
            x86(X86Reg::GsBase),
            true,
            OpWidth::W64,
            true,
        ),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .any(|op| matches!(
                op.kind,
                OpKind::X86FsGsBase {
                    operand: VReg::Arch(ArchReg::X86(X86Reg::R16)),
                    base: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
                    write: true,
                    width: OpWidth::W64,
                    requires_apx: true,
                }
            ))
    );
    assert!(is_native_clobber_safe(&function));
}
