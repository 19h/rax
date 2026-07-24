//! Optimizer preservation for scalar x86 integer-to-FP status side effects.

use super::*;

fn optimized_overwritten_conversion(
    elem: VecElementType,
    int_width: OpWidth,
    round: FpRoundMode,
    suppress_exceptions: bool,
) -> SmirFunction {
    let result = VReg::virt(0);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86IntToFp {
            dst: result,
            merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            elem,
            int_width,
            signed: true,
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
        .any(|op| matches!(op.kind, OpKind::X86IntToFp { .. }))
}

#[test]
fn o2_preserves_only_scalar_int_to_fp_status_or_invalid_side_effects() {
    let inexact = optimized_overwritten_conversion(
        VecElementType::F32,
        OpWidth::W64,
        FpRoundMode::Dynamic,
        false,
    );
    assert!(retains_conversion(&inexact));

    let sae = optimized_overwritten_conversion(
        VecElementType::F32,
        OpWidth::W64,
        FpRoundMode::RoundDown,
        true,
    );
    assert!(!retains_conversion(&sae));

    let exact_binary64_w0 = optimized_overwritten_conversion(
        VecElementType::F64,
        OpWidth::W32,
        FpRoundMode::Dynamic,
        false,
    );
    assert!(!retains_conversion(&exact_binary64_w0));

    let invalid = optimized_overwritten_conversion(
        VecElementType::F64,
        OpWidth::W32,
        FpRoundMode::RoundNearestTiesAway,
        true,
    );
    assert!(retains_conversion(&invalid), "invalid IR must fail closed");
}
