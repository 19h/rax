//! x86 legacy floating-point and 3DNow! operation descriptors.

use crate::smir::ir::types::Condition;

/// 3DNow! operations selected by the trailing `imm8` of `0F 0F /r imm8`
/// that do not map exactly onto a generic packed-integer SMIR operation.
/// Operands and results are always one 64-bit MMX register containing either
/// two packed binary32 values or four packed signed 16-bit integers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86ThreeDNowKind {
    Pf2Iw,
    PfNAcc,
    PfPNAcc,
    Pi2Fw,
    Pf2Id,
    PfAcc,
    PfAdd,
    PfCmpEq,
    PfCmpGe,
    PfCmpGt,
    PfMax,
    PfMin,
    PfMul,
    PfRcp,
    PfRcpIt1,
    PfRcpIt2,
    PfRsqIt1,
    PfRsqrt,
    PfSub,
    PfSubR,
    Pi2Fd,
    PmulHrw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86X87EnvWidth {
    W16,
    W32,
}

/// x87 data-stack operations, explicit format conversions, and arithmetic.
/// Each arithmetic family has a distinct variant because binary80 rounding and
/// exception precedence differ from transfer/conversion responses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86X87DataKind {
    /// `FLD ST(i)`.
    LoadRegister,
    /// `FLD m80fp`.
    LoadExtended,
    /// `FLD m32fp`.
    LoadSingle,
    /// `FLD m64fp`.
    LoadDouble,
    /// `FILD m16int`.
    LoadInt16,
    /// `FILD m32int`.
    LoadInt32,
    /// `FILD m64int`.
    LoadInt64,
    /// `FBLD m80bcd`.
    LoadBcd,
    /// `FST ST(i)`.
    StoreRegister,
    /// `FSTP ST(i)`.
    StorePopRegister,
    /// `FSTP m80fp`.
    StorePopExtended,
    /// `FXCH ST(i)`.
    Exchange,
    /// `FFREE ST(i)`.
    Free,
    /// `FCHS`.
    ChangeSign,
    /// `FABS`.
    Absolute,
    /// `FDECSTP`.
    DecrementTop,
    /// `FINCSTP`.
    IncrementTop,
    /// One of the seven `FLD*` architectural constants.
    LoadConstant(X86X87Constant),
    /// `FCMOVcc ST(0), ST(i)` using the integer condition flags.
    ConditionalMove(Condition),
    /// `FXAM` raw binary80/tag classification.
    Examine,
    /// `FTST` ordered comparison of ST(0) with +0.0.
    TestZero,
    /// `FRNDINT` integral rounding in binary80 format using FCW.RC.
    RoundInteger,
    /// `FXTRACT` exponent/significand decomposition with a stack push.
    Extract,
    /// `FSCALE` multiplication of ST(0) by 2^trunc(ST(1)).
    Scale,
    /// `FSQRT` square root rounded according to FCW.PC and FCW.RC.
    SquareRoot,
    /// One of the x87 exponential, logarithmic, or trigonometric operations.
    /// The operation kind captures its exact stack shape and status-word
    /// contract; the interpreter retains binary80 inputs and results.
    Transcendental(X86X87TranscendentalKind),
    /// `FMUL`, `FMULP`, or `FIMUL` exact binary80 multiplication.
    Multiply {
        source: X86X87ArithmeticSource,
        destination: X86X87ArithmeticDestination,
        pop: bool,
    },
    /// `FADD`/`FADDP`/`FIADD`, `FSUB`/`FSUBP`/`FISUB`, or the reverse-
    /// subtract forms. `subtract` selects subtraction and `reverse` selects
    /// source-minus-destination rather than destination-minus-source.
    AddSubtract {
        source: X86X87ArithmeticSource,
        destination: X86X87ArithmeticDestination,
        pop: bool,
        subtract: bool,
        reverse: bool,
    },
    /// `FDIV`/`FDIVP`/`FIDIV` or the reverse-divide forms. `reverse` selects
    /// source-over-destination rather than destination-over-source.
    Divide {
        source: X86X87ArithmeticSource,
        destination: X86X87ArithmeticDestination,
        pop: bool,
        reverse: bool,
    },
    /// `FPREM` or IEEE quotient-rounding `FPREM1` on ST(0)/ST(1).
    Remainder { nearest: bool },
    /// x87 floating-point compare family. `unordered` selects FUCOM policy,
    /// `pop` is the architectural pop count, and `eflags` selects the
    /// FCOMI/FUCOMI destination instead of C0/C2/C3.
    Compare {
        source: X86X87CompareSource,
        unordered: bool,
        pop: u8,
        eflags: bool,
    },
    /// `FIST`, `FISTP`, or `FISTTP` integer store.
    StoreInteger {
        width: X86X87IntWidth,
        pop: bool,
        truncate: bool,
    },
    /// `FST` or `FSTP` narrowing store to IEEE binary32/binary64.
    StoreFloat { width: X86X87FloatWidth, pop: bool },
    /// `FBSTP m80bcd`.
    StoreBcd,
}

/// x87 operations whose architecturally specified result is a transcendental
/// approximation. Intel guarantees bounded error for reduced arguments while
/// defining instruction-specific stack, range, and exception behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86X87TranscendentalKind {
    /// `F2XM1`: replace ST(0) with `2^ST(0) - 1`.
    Exp2MinusOne,
    /// `FYL2X`: replace ST(1) with `ST(1) * log2(ST(0))`, then pop.
    YLog2X,
    /// `FPTAN`: replace ST(0) with its tangent, then push 1.0.
    Tangent,
    /// `FPATAN`: replace ST(1) with `atan2(ST(1), ST(0))`, then pop.
    Arctangent,
    /// `FYL2XP1`: replace ST(1) with `ST(1) * log2(ST(0) + 1)`, then pop.
    YLog2Xp1,
    /// `FSINCOS`: replace ST(0) with its sine, then push its cosine.
    SineCosine,
    /// `FSIN`: replace ST(0) with its sine.
    Sine,
    /// `FCOS`: replace ST(0) with its cosine.
    Cosine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86X87CompareSource {
    Register,
    Single,
    Double,
    Int16,
    Int32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86X87ArithmeticSource {
    Register,
    Single,
    Double,
    Int16,
    Int32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86X87ArithmeticDestination {
    St0,
    StI,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86X87IntWidth {
    I16,
    I32,
    I64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86X87FloatWidth {
    F32,
    F64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86X87Constant {
    One,
    Log2Ten,
    Log2E,
    Pi,
    Log10Two,
    LnTwo,
    Zero,
}
