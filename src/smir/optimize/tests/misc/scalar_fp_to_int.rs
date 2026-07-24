//! Optimizer preservation for scalar x86 FP-to-integer status side effects.

use super::*;

fn optimized_overwritten_conversion(suppress_exceptions: bool) -> SmirFunction {
    let result = VReg::virt(0);
    let xmm1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86FpToInt {
            dst: result,
            src: xmm1,
            elem: VecElementType::F32,
            int_width: OpWidth::W32,
            signed: true,
            truncate: false,
            round: FpRoundMode::Dynamic,
            suppress_exceptions,
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

#[test]
fn o2_preserves_non_sae_fp_to_int_status_and_removes_dead_sae_result() {
    let non_sae = optimized_overwritten_conversion(false);
    assert!(non_sae.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86FpToInt {
            suppress_exceptions: false,
            ..
        }
    )));

    let sae = optimized_overwritten_conversion(true);
    assert!(
        !sae.blocks[0]
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86FpToInt { .. }))
    );
}
