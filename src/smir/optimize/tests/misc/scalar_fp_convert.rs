//! Optimizer preservation for scalar x86 FP precision-conversion effects.

use super::*;

fn optimized_overwritten_conversion(
    from: VecElementType,
    to: VecElementType,
    round: FpRoundMode,
    suppress_exceptions: bool,
) -> SmirFunction {
    let result = VReg::virt(0);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86FpConvert {
            dst: result,
            merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
            mask: None,
            from,
            to,
            mask_zeroing: false,
            round,
            suppress_exceptions,
            zero_upper: true,
        },
    );
    builder.push_op(
        0x1004,
        OpKind::Mov {
            dst: result,
            src: SrcOperand::Imm(7),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);
    function
}

fn retains_conversion(function: &SmirFunction) -> bool {
    function.blocks[0]
        .ops
        .iter()
        .any(|op| matches!(op.kind, OpKind::X86FpConvert { .. }))
}

#[test]
fn o2_preserves_only_scalar_fp_convert_status_or_invalid_effects() {
    for (from, to) in [
        (VecElementType::F16, VecElementType::F32),
        (VecElementType::F16, VecElementType::F64),
        (VecElementType::F32, VecElementType::F16),
        (VecElementType::F32, VecElementType::F64),
        (VecElementType::F64, VecElementType::F16),
        (VecElementType::F64, VecElementType::F32),
    ] {
        assert!(retains_conversion(&optimized_overwritten_conversion(
            from,
            to,
            FpRoundMode::Dynamic,
            false,
        )));
        assert!(!retains_conversion(&optimized_overwritten_conversion(
            from,
            to,
            FpRoundMode::RoundDown,
            true,
        )));
    }

    for (from, to, round) in [
        (
            VecElementType::F32,
            VecElementType::F32,
            FpRoundMode::Dynamic,
        ),
        (
            VecElementType::I32,
            VecElementType::F64,
            FpRoundMode::Dynamic,
        ),
        (
            VecElementType::F64,
            VecElementType::F32,
            FpRoundMode::RoundNearestTiesAway,
        ),
    ] {
        assert!(
            retains_conversion(&optimized_overwritten_conversion(from, to, round, true)),
            "invalid IR must fail closed: {from:?}->{to:?} {round:?}"
        );
    }
}
