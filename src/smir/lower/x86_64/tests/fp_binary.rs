//! Native rejection tests for exact x86 binary floating-point operations.

use super::*;
use crate::smir::ir::types::{FpRoundMode, VecElementType, X86FpBinaryOp};

#[test]
fn lowerer_rejects_x86_fp_binary_until_mxcsr_atomicity_is_native() {
    let xmm0 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let xmm1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    let xmm2 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)));
    for kind in [
        OpKind::X86FpBinary {
            dst: xmm0,
            src1: xmm1,
            src2: xmm2,
            mask: None,
            elem: VecElementType::F32,
            lanes: 1,
            op: X86FpBinaryOp::Div,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
        },
        OpKind::X86FpBinary {
            dst: xmm0,
            src1: xmm1,
            src2: xmm2,
            mask: None,
            elem: VecElementType::F64,
            lanes: 1,
            op: X86FpBinaryOp::Add,
            round: FpRoundMode::RoundDown,
            suppress_exceptions: true,
        },
    ] {
        assert!(matches!(
            lower_single_op_err(kind),
            LowerError::UnsupportedOp { .. }
        ));
    }
}
