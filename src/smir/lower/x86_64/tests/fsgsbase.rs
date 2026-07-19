//! Fault-precise state-backed native lowering for FSGSBASE.

use super::*;

fn x86_gpr(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

fn base(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn fsgsbase_kind(
    operand: u8,
    base_reg: X86Reg,
    write: bool,
    width: OpWidth,
    requires_apx: bool,
) -> OpKind {
    OpKind::X86FsGsBase {
        operand: x86_gpr(operand),
        base: base(base_reg),
        write,
        width,
        requires_apx,
    }
}

fn lower_fsgsbase(kind: OpKind, fault_guards: bool) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_fsgsbase_requires_precise_jit_fault_guards() {
    assert!(matches!(
        lower_fsgsbase(
            fsgsbase_kind(0, X86Reg::FsBase, false, OpWidth::W64, false),
            false,
        ),
        Err(LowerError::UnsupportedOp { .. })
    ));
    lower_fsgsbase(
        fsgsbase_kind(0, X86Reg::FsBase, false, OpWidth::W64, false),
        true,
    )
    .expect("guarded FSGSBASE lowering");
}

#[test]
fn lower_fsgsbase_rejects_every_malformed_ir_shape() {
    for malformed in [
        OpKind::X86FsGsBase {
            operand: base(X86Reg::FsBase),
            base: base(X86Reg::GsBase),
            write: false,
            width: OpWidth::W64,
            requires_apx: false,
        },
        OpKind::X86FsGsBase {
            operand: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
            base: base(X86Reg::FsBase),
            write: false,
            width: OpWidth::W64,
            requires_apx: false,
        },
        OpKind::X86FsGsBase {
            operand: x86_gpr(0),
            base: x86_gpr(1),
            write: false,
            width: OpWidth::W64,
            requires_apx: false,
        },
        fsgsbase_kind(0, X86Reg::FsBase, false, OpWidth::W16, false),
        fsgsbase_kind(0, X86Reg::FsBase, false, OpWidth::W128, false),
        fsgsbase_kind(16, X86Reg::FsBase, false, OpWidth::W64, false),
    ] {
        assert!(!x86_fsgsbase_shape_valid(&malformed));
        assert!(matches!(
            lower_fsgsbase(malformed, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native(
    kind: OpKind,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower_fsgsbase(kind, true).expect("lower guarded FSGSBASE");
    let exec = ExecMem::new(&code).expect("map guarded FSGSBASE");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.cr4 = 1 << 16;
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.exit_pc = 0xDEAD_BEEF;
    configure(&mut regs);
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_fsgsbase_reads_every_identity_stack_and_egpr_operand_class() {
    for operand in [0_u8, 4, 5, 8, 15, 16, 31] {
        let requires_apx = operand >= 16;
        for (base_reg, expected) in [
            (X86Reg::FsBase, 0xFFFF_8000_89AB_CDEF),
            (X86Reg::GsBase, 0x0000_7FFF_7654_3210),
        ] {
            for (width, width_expected) in [
                (OpWidth::W32, expected & u32::MAX as u64),
                (OpWidth::W64, expected),
            ] {
                let regs = execute_native(
                    fsgsbase_kind(operand, base_reg, false, width, requires_apx),
                    |regs| {
                        regs.fs_base = 0xFFFF_8000_89AB_CDEF;
                        regs.gs_base = 0x0000_7FFF_7654_3210;
                        regs.apx_enabled = u64::from(requires_apx);
                    },
                );
                assert_eq!(
                    regs.gpr[usize::from(operand)],
                    width_expected,
                    "{base_reg:?} {operand} {width:?}"
                );
                for index in 0..32 {
                    if index != usize::from(operand) {
                        assert_eq!(
                            regs.gpr[index],
                            0xA500_0000_0000_0000 | index as u64,
                            "unexpected GPR mutation for {base_reg:?} operand {operand} width {width:?}"
                        );
                    }
                }
                assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
                assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_fsgsbase_writes_w32_and_canonical_w64_from_every_operand_class() {
    for operand in [0_u8, 4, 5, 8, 15, 16, 31] {
        let requires_apx = operand >= 16;
        for (width, value, expected) in [
            (OpWidth::W32, 0xFFFF_FFFF_89AB_CDEF, 0x89AB_CDEF),
            (OpWidth::W64, 0xFFFF_8000_89AB_CDEF, 0xFFFF_8000_89AB_CDEF),
        ] {
            let regs = execute_native(
                fsgsbase_kind(operand, X86Reg::GsBase, true, width, requires_apx),
                |regs| {
                    regs.gpr[usize::from(operand)] = value;
                    regs.fs_base = 0x1357;
                    regs.gs_base = 0x2468;
                    regs.apx_enabled = u64::from(requires_apx);
                },
            );
            assert_eq!(regs.gs_base, expected, "operand {operand} width {width:?}");
            assert_eq!(regs.fs_base, 0x1357);
            assert_eq!(regs.gpr[usize::from(operand)], value);
            assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
            assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
        }
    }

    for value in [0, 0x0000_7FFF_FFFF_FFFF, 0xFFFF_8000_0000_0000, u64::MAX] {
        let regs = execute_native(
            fsgsbase_kind(3, X86Reg::FsBase, true, OpWidth::W64, false),
            |regs| {
                regs.gpr[3] = value;
                regs.fs_base = 0x1357;
            },
        );
        assert_eq!(regs.fs_base, value);
        assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_fsgsbase_dynamic_fault_guards_are_precise_and_noncommitting() {
    let regs = execute_native(
        fsgsbase_kind(0, X86Reg::FsBase, false, OpWidth::W64, false),
        |regs| {
            regs.cr4 = 0;
            regs.gpr[0] = 0xA5A5;
            regs.fs_base = 0x1234;
        },
    );
    assert_eq!(regs.exit_pc, 0x1000);
    assert_eq!(regs.gpr[0], 0xA5A5);
    assert_eq!(regs.fs_base, 0x1234);

    let regs = execute_native(
        fsgsbase_kind(16, X86Reg::FsBase, false, OpWidth::W64, true),
        |regs| {
            regs.apx_enabled = 0;
            regs.gpr[16] = 0xA5A5;
            regs.fs_base = 0x1234;
        },
    );
    assert_eq!(regs.exit_pc, 0x1000);
    assert_eq!(regs.gpr[16], 0xA5A5);
    assert_eq!(regs.fs_base, 0x1234);

    for value in [0x0000_8000_0000_0000, 0xFFFF_7FFF_FFFF_FFFF] {
        let regs = execute_native(
            fsgsbase_kind(5, X86Reg::GsBase, true, OpWidth::W64, false),
            |regs| {
                regs.gpr[5] = value;
                regs.gs_base = 0x2468;
            },
        );
        assert_eq!(regs.exit_pc, 0x1000);
        assert_eq!(regs.gpr[5], value);
        assert_eq!(regs.gs_base, 0x2468);
        assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    }
}
