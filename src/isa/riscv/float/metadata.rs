//! Operand and destination metadata for scalar floating-point operations.

use crate::isa::riscv::Op as RvFpInsn;

/// Does this OP-FP / FMA op write its result to an **integer** (x) register?
/// (compares, fp->int conversions, `fclass`, `fcvtmod`, `fmv.x.*`). Everything
/// else writes an f-register.
pub fn fp_writes_int_dst(op: RvFpInsn) -> bool {
    use RvFpInsn::*;
    matches!(
        op,
        FeqS | FltS
            | FleS
            | FeqD
            | FltD
            | FleD
            | FeqH
            | FltH
            | FleH
            | FleqS
            | FltqS
            | FleqD
            | FltqD
            | FleqH
            | FltqH
            | FclassS
            | FclassD
            | FclassH
            | FcvtWS
            | FcvtWuS
            | FcvtLS
            | FcvtLuS
            | FcvtWD
            | FcvtWuD
            | FcvtLD
            | FcvtLuD
            | FcvtWH
            | FcvtWuH
            | FcvtLH
            | FcvtLuH
            | FcvtmodWD
            | FmvXW
            | FmvXD
            | FmvhXD
            | FmvXH
    )
}

/// Does this OP-FP op take its first source operand from an **integer** (x)
/// register? (int->fp conversions and `fmv.*.x`). Everything else reads an
/// f-register.
pub fn fp_uses_int_src1(op: RvFpInsn) -> bool {
    use RvFpInsn::*;
    matches!(
        op,
        FcvtSW
            | FcvtSWu
            | FcvtSL
            | FcvtSLu
            | FcvtDW
            | FcvtDWu
            | FcvtDL
            | FcvtDLu
            | FcvtHW
            | FcvtHWu
            | FcvtHL
            | FcvtHLu
            | FmvWX
            | FmvDX
            | FmvpDX
            | FmvHX
    )
}

/// Whether this scalar FP operation consumes or produces a 64-bit integer and
/// is therefore illegal when XLEN is 32.
pub fn fp_requires_rv64(op: RvFpInsn) -> bool {
    use RvFpInsn::*;
    matches!(
        op,
        FcvtLS
            | FcvtLuS
            | FcvtLD
            | FcvtLuD
            | FcvtLH
            | FcvtLuH
            | FcvtSL
            | FcvtSLu
            | FcvtDL
            | FcvtDLu
            | FcvtHL
            | FcvtHLu
    )
}
