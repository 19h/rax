//! x86-on-AArch64 dynamic APX feature-guard regressions.

use super::*;

fn guarded_function(hinted: bool) -> SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x2345, OpKind::X86RequireApx);
    builder.push_op(
        0x2345,
        OpKind::Mov {
            dst: x86(X86Reg::Rbx),
            src: SrcOperand::Imm(0x1357_9bdf_2468_ace0_u64 as i64),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    if hinted {
        function.blocks[0].ops[0].x86_hint = Some(crate::smir::ir::ops::X86OpHint::RexByteReg);
    }
    function
}

fn lower_guard(x86_guest_state_guards: bool, hinted: bool) -> Result<(Vec<u8>, usize), LowerError> {
    let mut lowerer = Aarch64Lowerer::new();
    lowerer.set_x86_guest_state_guards(x86_guest_state_guards);
    let lowered = lowerer.lower_function(&guarded_function(hinted))?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn apx_guard_requires_exact_x86_bridge_mode_and_encodes_live_state_load() {
    assert!(matches!(
        lower_guard(false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower_guard(true, true),
        Err(LowerError::InvalidOperand { .. })
    ));

    let (code, _) = lower_guard(true, false).expect("lower exact APX guard");
    let words = code_words(&code);
    let apx_load = (3 << 30)
        | (0b111 << 27)
        | (0b01 << 24)
        | (0b01 << 22)
        | ((A64_GUEST_X86_APX_ENABLED_OFFSET / 8) << 10)
        | (u32::from(A64_STATE_REG) << 5)
        | 9;
    let fault_pc_store = (3 << 30)
        | (0b111 << 27)
        | (0b01 << 24)
        | ((A64_GUEST_PC_OFFSET / 8) << 10)
        | (u32::from(A64_STATE_REG) << 5)
        | 9;
    assert!(
        words.contains(&apx_load),
        "missing APX state load: {words:08x?}"
    );
    assert!(
        words.contains(&fault_pc_store),
        "missing exact-PC state store: {words:08x?}"
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
#[test]
fn native_apx_guard_is_dynamic_precise_noncommitting_and_nzcv_neutral() {
    use crate::smir::lower::runtime::{Aarch64GuestRegs, ExecMem};

    let (code, entry_offset) = lower_guard(true, false).expect("lower guarded sequence");
    let exec = ExecMem::new(&code).expect("map guarded sequence");

    for enabled in [false, true] {
        let mut state = Aarch64GuestRegs {
            pc: 0xdead_beef_cafe_babe,
            nzcv: 0xb000_0000,
            exit_flags: 0xa5a5_5a5a_a5a5_5a5a,
            x86_apx_enabled: u64::from(enabled),
            ..Default::default()
        };
        for (index, value) in state.x.iter_mut().enumerate() {
            *value = 0xa500_0000_0000_0000 | index as u64;
        }

        exec.run_aarch64_identity(entry_offset, &mut state);

        assert_eq!(
            state.pc,
            if enabled {
                0xdead_beef_cafe_babe
            } else {
                0x2345
            },
            "APX={enabled}: precise exit PC"
        );
        for (index, actual) in state.x.iter().enumerate() {
            let expected = if enabled && index == 3 {
                0x1357_9bdf_2468_ace0
            } else {
                0xa500_0000_0000_0000 | index as u64
            };
            assert_eq!(*actual, expected, "APX={enabled}: X{index}");
        }
        assert_eq!(state.nzcv, 0xb000_0000, "APX={enabled}: NZCV");
        assert_eq!(
            state.exit_flags, 0xa5a5_5a5a_a5a5_5a5a,
            "APX={enabled}: unrelated exit metadata"
        );
    }
}
