//! Fault-precise native lowering coverage for the operand-free APX guard.

use super::*;
use crate::isa::x86_64::flags;
use crate::smir::lower::X86_GUEST_APX_ENABLED_OFFSET;

fn lower_guard(fault_guards: bool, hinted: bool) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x2345, OpKind::X86RequireApx);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    if hinted {
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    }
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&function)?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_apx_guard_requires_deoptimization_and_encodes_state_and_fault_pc() {
    assert!(matches!(
        lower_guard(false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower_guard(true, true),
        Err(LowerError::InvalidOperand { .. })
    ));

    let (code, _) = lower_guard(true, false).expect("guarded APX lowering");
    assert!(
        code.windows(4)
            .any(|window| window == (X86_GUEST_APX_ENABLED_OFFSET as u32).to_le_bytes()),
        "missing APX enable-state displacement: {code:02X?}"
    );
    assert!(
        code.windows(4)
            .any(|window| window == 0x2345_u32.to_le_bytes()),
        "missing exact deoptimization PC: {code:02X?}"
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native(enabled: bool) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x2345, OpKind::X86RequireApx);
    builder.push_op(
        0x2345,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
            src: SrcOperand::Imm(0x1357_9BDF_2468_ACE0_u64 as i64),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower APX-guarded sequence");
    let code = lowerer.finalize().expect("finalize APX-guarded sequence");
    let exec = ExecMem::new(&code).expect("map APX-guarded sequence");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    regs.ac_flag = 1;
    regs.apx_enabled = u64::from(enabled);
    regs.exit_pc = 0xDEAD_BEEF_CAFE_BABE;
    exec.run(lowered.entry_offset, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_apx_guard_is_dynamic_precise_noncommitting_and_flag_neutral() {
    for enabled in [false, true] {
        let regs = execute_native(enabled);
        assert_eq!(
            regs.exit_pc,
            if enabled {
                0xDEAD_BEEF_CAFE_BABE
            } else {
                0x2345
            }
        );
        for (index, actual) in regs.gpr.iter().enumerate() {
            let expected = if enabled && index == 3 {
                0x1357_9BDF_2468_ACE0
            } else {
                0xA500_0000_0000_0000 | index as u64
            };
            assert_eq!(*actual, expected, "APX={enabled}, GPR{index}");
        }
        assert_eq!(
            regs.rflags & (0x08D5 | flags::bits::DF),
            0x08D5 | flags::bits::DF,
            "APX={enabled}"
        );
        assert_eq!(regs.ac_flag, 1, "APX={enabled}");
    }
}
