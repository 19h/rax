//! ARM instruction decoder for AArch64, AArch32, and Thumb/Thumb-2.
//!
//! This module provides comprehensive instruction decoding for all ARM execution states.
//! The decoder parses raw instruction bytes into structured representations that can be
//! used for disassembly, emulation, or analysis.
//!
//! # Supported Instruction Sets
//!
//! - **AArch64 (A64)**: 64-bit ARM instructions, all 32 bits wide
//! - **AArch32 (A32)**: 32-bit ARM instructions
//! - **Thumb (T16)**: 16-bit compact instructions
//! - **Thumb-2 (T32)**: Mixed 16/32-bit instructions
//!
//! # Usage
//!
//! ```ignore
//! use rax::arm::decoder::{Decoder, DecodedInsn};
//!
//! let decoder = Decoder::new_aarch64();
//! let insn = decoder.decode(&[0x20, 0x00, 0x80, 0xd2]); // mov x0, #1
//! ```

pub mod aarch32;
pub mod aarch64;
pub mod operand;
pub mod thumb;

use crate::arm::ExecutionState;

pub use aarch32::Aarch32Decoder;
pub use aarch64::Aarch64Decoder;
pub use operand::*;
pub use thumb::ThumbDecoder;

/// Condition codes for conditional execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Condition {
    /// Equal (Z == 1)
    EQ = 0b0000,
    /// Not equal (Z == 0)
    NE = 0b0001,
    /// Carry set / unsigned higher or same (C == 1)
    CS = 0b0010,
    /// Carry clear / unsigned lower (C == 0)
    CC = 0b0011,
    /// Minus / negative (N == 1)
    MI = 0b0100,
    /// Plus / positive or zero (N == 0)
    PL = 0b0101,
    /// Overflow (V == 1)
    VS = 0b0110,
    /// No overflow (V == 0)
    VC = 0b0111,
    /// Unsigned higher (C == 1 && Z == 0)
    HI = 0b1000,
    /// Unsigned lower or same (C == 0 || Z == 1)
    LS = 0b1001,
    /// Signed greater than or equal (N == V)
    GE = 0b1010,
    /// Signed less than (N != V)
    LT = 0b1011,
    /// Signed greater than (Z == 0 && N == V)
    GT = 0b1100,
    /// Signed less than or equal (Z == 1 || N != V)
    LE = 0b1101,
    /// Always (unconditional)
    AL = 0b1110,
    /// Never / unconditional (ARMv5+: also means AL in some encodings)
    NV = 0b1111,
}

impl Condition {
    /// Decode condition from 4-bit value.
    pub fn from_bits(bits: u8) -> Self {
        match bits & 0xF {
            0b0000 => Condition::EQ,
            0b0001 => Condition::NE,
            0b0010 => Condition::CS,
            0b0011 => Condition::CC,
            0b0100 => Condition::MI,
            0b0101 => Condition::PL,
            0b0110 => Condition::VS,
            0b0111 => Condition::VC,
            0b1000 => Condition::HI,
            0b1001 => Condition::LS,
            0b1010 => Condition::GE,
            0b1011 => Condition::LT,
            0b1100 => Condition::GT,
            0b1101 => Condition::LE,
            0b1110 => Condition::AL,
            0b1111 => Condition::NV,
            _ => unreachable!(),
        }
    }

    /// Get the inverted condition.
    pub fn invert(self) -> Self {
        Self::from_bits(self as u8 ^ 1)
    }

    /// Get the mnemonic suffix for this condition.
    pub fn suffix(self) -> &'static str {
        match self {
            Condition::EQ => "eq",
            Condition::NE => "ne",
            Condition::CS => "cs",
            Condition::CC => "cc",
            Condition::MI => "mi",
            Condition::PL => "pl",
            Condition::VS => "vs",
            Condition::VC => "vc",
            Condition::HI => "hi",
            Condition::LS => "ls",
            Condition::GE => "ge",
            Condition::LT => "lt",
            Condition::GT => "gt",
            Condition::LE => "le",
            Condition::AL => "",
            Condition::NV => "",
        }
    }
}

/// Shift type for register operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShiftType {
    /// Logical shift left
    LSL,
    /// Logical shift right
    LSR,
    /// Arithmetic shift right
    ASR,
    /// Rotate right
    ROR,
    /// Rotate right with extend (33-bit rotate through carry)
    RRX,
}

impl ShiftType {
    /// Decode shift type from 2-bit encoding.
    pub fn from_bits(bits: u8) -> Self {
        match bits & 0x3 {
            0b00 => ShiftType::LSL,
            0b01 => ShiftType::LSR,
            0b10 => ShiftType::ASR,
            0b11 => ShiftType::ROR,
            _ => unreachable!(),
        }
    }

    /// Get the mnemonic for this shift type.
    pub fn mnemonic(self) -> &'static str {
        match self {
            ShiftType::LSL => "lsl",
            ShiftType::LSR => "lsr",
            ShiftType::ASR => "asr",
            ShiftType::ROR => "ror",
            ShiftType::RRX => "rrx",
        }
    }
}

/// Extend type for register operands (AArch64).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExtendType {
    /// Unsigned byte extend
    UXTB,
    /// Unsigned halfword extend
    UXTH,
    /// Unsigned word extend
    UXTW,
    /// Unsigned doubleword extend (also LSL for 64-bit)
    UXTX,
    /// Signed byte extend
    SXTB,
    /// Signed halfword extend
    SXTH,
    /// Signed word extend
    SXTW,
    /// Signed doubleword extend
    SXTX,
}

impl ExtendType {
    /// Decode extend type from 3-bit encoding.
    pub fn from_bits(bits: u8) -> Self {
        match bits & 0x7 {
            0b000 => ExtendType::UXTB,
            0b001 => ExtendType::UXTH,
            0b010 => ExtendType::UXTW,
            0b011 => ExtendType::UXTX,
            0b100 => ExtendType::SXTB,
            0b101 => ExtendType::SXTH,
            0b110 => ExtendType::SXTW,
            0b111 => ExtendType::SXTX,
            _ => unreachable!(),
        }
    }

    /// Get the mnemonic for this extend type.
    pub fn mnemonic(self) -> &'static str {
        match self {
            ExtendType::UXTB => "uxtb",
            ExtendType::UXTH => "uxth",
            ExtendType::UXTW => "uxtw",
            ExtendType::UXTX => "uxtx",
            ExtendType::SXTB => "sxtb",
            ExtendType::SXTH => "sxth",
            ExtendType::SXTW => "sxtw",
            ExtendType::SXTX => "sxtx",
        }
    }
}

/// Result of decoding an instruction.
#[derive(Clone, Debug)]
pub struct DecodedInsn {
    /// The instruction mnemonic/opcode.
    pub mnemonic: Mnemonic,
    /// Condition code (for conditional instructions).
    pub cond: Option<Condition>,
    /// Operands (destination first, then sources).
    pub operands: Vec<Operand>,
    /// Whether the instruction sets flags.
    pub sets_flags: bool,
    /// Original instruction bytes.
    pub raw: u32,
    /// Size of the instruction in bytes (2 for T16, 4 for others).
    pub size: u8,
    /// The execution state this instruction was decoded in.
    pub state: ExecutionState,
}

impl DecodedInsn {
    /// Create a new decoded instruction.
    pub fn new(mnemonic: Mnemonic, state: ExecutionState, raw: u32, size: u8) -> Self {
        DecodedInsn {
            mnemonic,
            cond: None,
            operands: Vec::new(),
            sets_flags: false,
            raw,
            size,
            state,
        }
    }

    /// Add a condition code.
    pub fn with_cond(mut self, cond: Condition) -> Self {
        self.cond = Some(cond);
        self
    }

    /// Mark as flag-setting.
    pub fn with_flags(mut self) -> Self {
        self.sets_flags = true;
        self
    }

    /// Add an operand.
    pub fn with_operand(mut self, op: Operand) -> Self {
        self.operands.push(op);
        self
    }

    /// Add multiple operands.
    pub fn with_operands(mut self, ops: impl IntoIterator<Item = Operand>) -> Self {
        self.operands.extend(ops);
        self
    }
}

/// Instruction mnemonic enumeration.
///
/// This covers the major instruction categories across all ARM ISAs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(non_camel_case_types)]
pub enum Mnemonic {
    // =========================================================================
    // Data Processing - Arithmetic
    // =========================================================================
    ADD,
    ADDS,
    ADC,
    ADCS,
    SUB,
    SUBS,
    SBC,
    SBCS,
    RSB,
    RSBS,
    RSC,
    RSCS,
    NEG,
    NEGS,
    MUL,
    MULS,
    MLA,
    MLS,
    UMULL,
    UMULLS,
    SMULL,
    SMULLS,
    UMLAL,
    SMLAL,
    UMADDL,
    SMADDL,
    UMSUBL,
    SMSUBL,
    UMULH,
    SMULH,
    UDIV,
    SDIV,
    MADD,
    MSUB,
    MNEG,

    // =========================================================================
    // Data Processing - Logical
    // =========================================================================
    AND,
    ANDS,
    ORR,
    ORRS,
    ORN,
    ORNS,
    EOR,
    EORS,
    EON,
    BIC,
    BICS,
    MVN,
    MVNS,
    TST,
    TEQ,

    // =========================================================================
    // Data Processing - Address Calculation
    // =========================================================================
    /// ADR - Form PC-relative address
    ADR,
    /// ADRP - Form PC-relative address to 4KB page
    ADRP,

    // =========================================================================
    // Data Processing - Move
    // =========================================================================
    MOV,
    MOVS,
    MOVZ,
    MOVN,
    MOVK,
    MVN_REG,

    // =========================================================================
    // Data Processing - Shift
    // =========================================================================
    LSL,
    LSLS,
    LSR,
    LSRS,
    ASR,
    ASRS,
    ROR,
    RORS,
    RRX,
    RRXS,

    // =========================================================================
    // Data Processing - Bit Field
    // =========================================================================
    BFI,
    BFXIL,
    BFC,
    SBFM,
    SBFX,
    SBFIZ,
    UBFM,
    UBFX,
    UBFIZ,
    EXTR,

    // =========================================================================
    // Data Processing - Compare
    // =========================================================================
    CMP,
    CMN,

    // =========================================================================
    // Data Processing - Conditional
    // =========================================================================
    CSEL,
    CSINC,
    CSINV,
    CSNEG,
    CSET,
    CSETM,
    CINC,
    CINV,
    CNEG,
    CCMP,
    CCMN,

    // =========================================================================
    // Data Processing - Extend
    // =========================================================================
    SXTB,
    SXTH,
    SXTW,
    UXTB,
    UXTH,

    // =========================================================================
    // Data Processing - Count/Reverse
    // =========================================================================
    CLZ,
    CLS,
    RBIT,
    REV,
    REV16,
    REV32,
    REV64,
    REVSH,

    // =========================================================================
    // Branch Instructions
    // =========================================================================
    B,
    BL,
    BLX,
    BX,
    BR,
    BLR,
    RET,
    ERET,
    DRPS,
    BCC, // Generic conditional branch (uses cond field)
    CBZ,
    CBNZ,
    TBZ,
    TBNZ,
    TBB, // Table Branch Byte (Thumb-2)
    TBH, // Table Branch Halfword (Thumb-2)

    // =========================================================================
    // Load/Store - Single Register
    // =========================================================================
    LDR,
    LDRB,
    LDRH,
    LDRSB,
    LDRSH,
    LDRSW,
    PRFM,
    STR,
    STRB,
    STRH,

    // =========================================================================
    // Load/Store - Pair
    // =========================================================================
    LDP,
    LDPSW,
    STP,
    LDNP,
    STNP,

    // =========================================================================
    // Load/Store - Exclusive
    // =========================================================================
    LDXR,
    LDXRB,
    LDXRH,
    LDXP,
    STXR,
    STXRB,
    STXRH,
    STXP,
    LDAXR,
    LDAXRB,
    LDAXRH,
    LDAXP,
    STLXR,
    STLXRB,
    STLXRH,
    STLXP,
    LDAR,
    LDARB,
    LDARH,
    STLR,
    STLRB,
    STLRH,

    // =========================================================================
    // Load/Store - Multiple (AArch32/Thumb)
    // =========================================================================
    LDM,
    LDMIA,
    LDMIB,
    LDMDA,
    LDMDB,
    STM,
    STMIA,
    STMIB,
    STMDA,
    STMDB,
    PUSH,
    POP,

    // =========================================================================
    // Load/Store - Atomic (ARMv8.1+)
    // =========================================================================
    LDADD,
    LDADDA,
    LDADDAL,
    LDADDL,
    LDCLR,
    LDEOR,
    LDSET,
    LDSMAX,
    LDSMIN,
    LDUMAX,
    LDUMIN,
    SWP,
    SWPA,
    SWPAL,
    SWPL,
    CAS,
    CASA,
    CASAL,
    CASL,
    CASP,

    // =========================================================================
    // System Instructions
    // =========================================================================
    SVC,
    HVC,
    SMC,
    BRK,
    HLT,
    UDF,
    NOP,
    IT,
    YIELD,
    WFE,
    WFI,
    SEV,
    SEVL,
    ISB,
    DMB,
    DSB,
    CLREX,
    MSR,
    MRS,
    SYS,
    SYSL,
    DC,
    IC,
    AT,
    TLBI,
    HINT,

    // =========================================================================
    // Exception Generation
    // =========================================================================
    SWI, // Legacy name for SVC
    BKPT,

    // =========================================================================
    // Coprocessor (AArch32)
    // =========================================================================
    MCR,
    MRC,
    MCR2,
    MRC2,
    MCRR,
    MRRC,
    CDP,
    CDP2,
    LDC,
    STC,

    // =========================================================================
    // SIMD/FP - Data Processing
    // =========================================================================
    FADD,
    FSUB,
    FMUL,
    FDIV,
    FNMUL,
    FMADD,
    FMSUB,
    FNMADD,
    FNMSUB,
    FMAX,
    FMIN,
    FMAXNM,
    FMINNM,
    FABS,
    FNEG,
    FSQRT,
    FRINT,
    FRINTN,
    FRINTP,
    FRINTM,
    FRINTZ,
    FRINTA,
    FRINTX,
    FRINTI,

    // =========================================================================
    // SIMD/FP - Compare
    // =========================================================================
    FCMP,
    FCMPE,
    FCCMP,
    FCCMPE,

    // =========================================================================
    // SIMD/FP - Conversion
    // =========================================================================
    FCVT,
    FCVTAS,
    FCVTAU,
    FCVTMS,
    FCVTMU,
    FCVTNS,
    FCVTNU,
    FCVTPS,
    FCVTPU,
    FCVTZS,
    FCVTZU,
    SCVTF,
    UCVTF,
    FMOV,

    // =========================================================================
    // SIMD/FP - Conditional
    // =========================================================================
    FCSEL,

    // =========================================================================
    // SIMD Vector
    // =========================================================================
    VADD,
    VSUB,
    VMUL,
    VMLA,
    VMLS,
    VAND,
    VORR,
    VEOR,
    VBIC,
    VMOV,
    VMVN,
    VDUP,
    VEXT,
    VZIP,
    VUZP,
    VTRN,
    VTBL,
    VTBX,
    VCMP,
    VCMPE,
    VABS,
    VNEG,
    VSQRT,
    VRECPE,
    VRSQRTE,
    VCVT_F32_S32,
    VCVT_F32_U32,
    VCVT_S32_F32,
    VCVT_U32_F32,
    VLD1,
    VLD2,
    VLD3,
    VLD4,
    VST1,
    VST2,
    VST3,
    VST4,
    VLDM,
    VSTM,
    VPUSH,
    VPOP,

    // =========================================================================
    // Pointer Authentication (ARMv8.3+)
    // =========================================================================
    PACIA,
    PACIB,
    PACDA,
    PACDB,
    PACGA,
    AUTIA,
    AUTIB,
    AUTDA,
    AUTDB,
    XPACI,
    XPACD,
    PACIZA,
    PACIZB,
    PACDZA,
    PACDZB,
    AUTIZA,
    AUTIZB,
    AUTDZA,
    AUTDZB,
    BLRAA,
    BLRAB,
    BRAA,
    BRAB,
    RETAA,
    RETAB,

    // =========================================================================
    // Memory Tagging Extension (ARMv8.5+)
    // =========================================================================
    IRG,
    GMI,
    ADDG,
    SUBG,
    STG,
    STZG,
    ST2G,
    STZ2G,
    LDG,
    STGP,

    // =========================================================================
    // Branch Target Identification (ARMv8.5+)
    // =========================================================================
    BTI,

    // =========================================================================
    // Saturation Instructions (ARMv6+)
    // =========================================================================
    SSAT, // Signed Saturate
    USAT, // Unsigned Saturate

    // =========================================================================
    // Miscellaneous
    // =========================================================================
    SETF8,
    SETF16,
    RMIF,
    CFINV,
    AXFLAG,
    XAFLAG,

    // =========================================================================
    // SVE Instructions (basic)
    // =========================================================================
    SVE_ADD,
    SVE_SUB,
    SVE_MUL,
    SVE_AND,
    SVE_ORR,
    SVE_EOR,
    SVE_LD1,
    SVE_ST1,
    SVE_PTRUE,
    SVE_PFALSE,
    SVE_WHILELT,
    SVE_WHILELE,

    // =========================================================================
    // Unknown/Invalid
    // =========================================================================
    UNKNOWN,
    UNDEFINED,
}

impl Mnemonic {
    /// Get the string representation of this mnemonic.
    pub fn as_str(&self) -> &'static str {
        match self {
            // Arithmetic
            Mnemonic::ADD => "add",
            Mnemonic::ADDS => "adds",
            Mnemonic::ADC => "adc",
            Mnemonic::ADCS => "adcs",
            Mnemonic::SUB => "sub",
            Mnemonic::SUBS => "subs",
            Mnemonic::SBC => "sbc",
            Mnemonic::SBCS => "sbcs",
            Mnemonic::RSB => "rsb",
            Mnemonic::RSBS => "rsbs",
            Mnemonic::RSC => "rsc",
            Mnemonic::RSCS => "rscs",
            Mnemonic::NEG => "neg",
            Mnemonic::NEGS => "negs",
            Mnemonic::MUL => "mul",
            Mnemonic::MULS => "muls",
            Mnemonic::MLA => "mla",
            Mnemonic::MLS => "mls",
            Mnemonic::UMULL => "umull",
            Mnemonic::UMULLS => "umulls",
            Mnemonic::SMULL => "smull",
            Mnemonic::SMULLS => "smulls",
            Mnemonic::UMLAL => "umlal",
            Mnemonic::SMLAL => "smlal",
            Mnemonic::UMADDL => "umaddl",
            Mnemonic::SMADDL => "smaddl",
            Mnemonic::UMSUBL => "umsubl",
            Mnemonic::SMSUBL => "smsubl",
            Mnemonic::UMULH => "umulh",
            Mnemonic::SMULH => "smulh",
            Mnemonic::UDIV => "udiv",
            Mnemonic::SDIV => "sdiv",
            Mnemonic::MADD => "madd",
            Mnemonic::MSUB => "msub",
            Mnemonic::MNEG => "mneg",

            // Logical
            Mnemonic::AND => "and",
            Mnemonic::ANDS => "ands",
            Mnemonic::ORR => "orr",
            Mnemonic::ORRS => "orrs",
            Mnemonic::ORN => "orn",
            Mnemonic::ORNS => "orns",
            Mnemonic::EOR => "eor",
            Mnemonic::EORS => "eors",
            Mnemonic::EON => "eon",
            Mnemonic::BIC => "bic",
            Mnemonic::BICS => "bics",
            Mnemonic::MVN => "mvn",
            Mnemonic::MVNS => "mvns",
            Mnemonic::TST => "tst",
            Mnemonic::TEQ => "teq",

            // Address calculation
            Mnemonic::ADR => "adr",
            Mnemonic::ADRP => "adrp",

            // Move
            Mnemonic::MOV => "mov",
            Mnemonic::MOVS => "movs",
            Mnemonic::MOVZ => "movz",
            Mnemonic::MOVN => "movn",
            Mnemonic::MOVK => "movk",
            Mnemonic::MVN_REG => "mvn",

            // Shift
            Mnemonic::LSL => "lsl",
            Mnemonic::LSLS => "lsls",
            Mnemonic::LSR => "lsr",
            Mnemonic::LSRS => "lsrs",
            Mnemonic::ASR => "asr",
            Mnemonic::ASRS => "asrs",
            Mnemonic::ROR => "ror",
            Mnemonic::RORS => "rors",
            Mnemonic::RRX => "rrx",
            Mnemonic::RRXS => "rrxs",

            // Bit field
            Mnemonic::BFI => "bfi",
            Mnemonic::BFXIL => "bfxil",
            Mnemonic::BFC => "bfc",
            Mnemonic::SBFM => "sbfm",
            Mnemonic::SBFX => "sbfx",
            Mnemonic::SBFIZ => "sbfiz",
            Mnemonic::UBFM => "ubfm",
            Mnemonic::UBFX => "ubfx",
            Mnemonic::UBFIZ => "ubfiz",
            Mnemonic::EXTR => "extr",

            // Compare
            Mnemonic::CMP => "cmp",
            Mnemonic::CMN => "cmn",

            // Conditional
            Mnemonic::CSEL => "csel",
            Mnemonic::CSINC => "csinc",
            Mnemonic::CSINV => "csinv",
            Mnemonic::CSNEG => "csneg",
            Mnemonic::CSET => "cset",
            Mnemonic::CSETM => "csetm",
            Mnemonic::CINC => "cinc",
            Mnemonic::CINV => "cinv",
            Mnemonic::CNEG => "cneg",
            Mnemonic::CCMP => "ccmp",
            Mnemonic::CCMN => "ccmn",

            // Extend
            Mnemonic::SXTB => "sxtb",
            Mnemonic::SXTH => "sxth",
            Mnemonic::SXTW => "sxtw",
            Mnemonic::UXTB => "uxtb",
            Mnemonic::UXTH => "uxth",

            // Count/Reverse
            Mnemonic::CLZ => "clz",
            Mnemonic::CLS => "cls",
            Mnemonic::RBIT => "rbit",
            Mnemonic::REV => "rev",
            Mnemonic::REV16 => "rev16",
            Mnemonic::REV32 => "rev32",
            Mnemonic::REV64 => "rev64",
            Mnemonic::REVSH => "revsh",

            // Branch
            Mnemonic::B => "b",
            Mnemonic::BL => "bl",
            Mnemonic::BLX => "blx",
            Mnemonic::BX => "bx",
            Mnemonic::BR => "br",
            Mnemonic::BLR => "blr",
            Mnemonic::RET => "ret",
            Mnemonic::ERET => "eret",
            Mnemonic::DRPS => "drps",
            Mnemonic::BCC => "b",
            Mnemonic::CBZ => "cbz",
            Mnemonic::CBNZ => "cbnz",
            Mnemonic::TBZ => "tbz",
            Mnemonic::TBNZ => "tbnz",
            Mnemonic::TBB => "tbb",
            Mnemonic::TBH => "tbh",

            // Load/Store single
            Mnemonic::LDR => "ldr",
            Mnemonic::LDRB => "ldrb",
            Mnemonic::LDRH => "ldrh",
            Mnemonic::LDRSB => "ldrsb",
            Mnemonic::LDRSH => "ldrsh",
            Mnemonic::LDRSW => "ldrsw",
            Mnemonic::PRFM => "prfm",
            Mnemonic::STR => "str",
            Mnemonic::STRB => "strb",
            Mnemonic::STRH => "strh",

            // Load/Store pair
            Mnemonic::LDP => "ldp",
            Mnemonic::LDPSW => "ldpsw",
            Mnemonic::STP => "stp",
            Mnemonic::LDNP => "ldnp",
            Mnemonic::STNP => "stnp",

            // Load/Store exclusive
            Mnemonic::LDXR => "ldxr",
            Mnemonic::LDXRB => "ldxrb",
            Mnemonic::LDXRH => "ldxrh",
            Mnemonic::LDXP => "ldxp",
            Mnemonic::STXR => "stxr",
            Mnemonic::STXRB => "stxrb",
            Mnemonic::STXRH => "stxrh",
            Mnemonic::STXP => "stxp",
            Mnemonic::LDAXR => "ldaxr",
            Mnemonic::LDAXRB => "ldaxrb",
            Mnemonic::LDAXRH => "ldaxrh",
            Mnemonic::LDAXP => "ldaxp",
            Mnemonic::STLXR => "stlxr",
            Mnemonic::STLXRB => "stlxrb",
            Mnemonic::STLXRH => "stlxrh",
            Mnemonic::STLXP => "stlxp",
            Mnemonic::LDAR => "ldar",
            Mnemonic::LDARB => "ldarb",
            Mnemonic::LDARH => "ldarh",
            Mnemonic::STLR => "stlr",
            Mnemonic::STLRB => "stlrb",
            Mnemonic::STLRH => "stlrh",

            // Load/Store multiple
            Mnemonic::LDM => "ldm",
            Mnemonic::LDMIA => "ldmia",
            Mnemonic::LDMIB => "ldmib",
            Mnemonic::LDMDA => "ldmda",
            Mnemonic::LDMDB => "ldmdb",
            Mnemonic::STM => "stm",
            Mnemonic::STMIA => "stmia",
            Mnemonic::STMIB => "stmib",
            Mnemonic::STMDA => "stmda",
            Mnemonic::STMDB => "stmdb",
            Mnemonic::PUSH => "push",
            Mnemonic::POP => "pop",

            // Atomic
            Mnemonic::LDADD => "ldadd",
            Mnemonic::LDADDA => "ldadda",
            Mnemonic::LDADDAL => "ldaddal",
            Mnemonic::LDADDL => "ldaddl",
            Mnemonic::LDCLR => "ldclr",
            Mnemonic::LDEOR => "ldeor",
            Mnemonic::LDSET => "ldset",
            Mnemonic::LDSMAX => "ldsmax",
            Mnemonic::LDSMIN => "ldsmin",
            Mnemonic::LDUMAX => "ldumax",
            Mnemonic::LDUMIN => "ldumin",
            Mnemonic::SWP => "swp",
            Mnemonic::SWPA => "swpa",
            Mnemonic::SWPAL => "swpal",
            Mnemonic::SWPL => "swpl",
            Mnemonic::CAS => "cas",
            Mnemonic::CASA => "casa",
            Mnemonic::CASAL => "casal",
            Mnemonic::CASL => "casl",
            Mnemonic::CASP => "casp",

            // System
            Mnemonic::SVC => "svc",
            Mnemonic::HVC => "hvc",
            Mnemonic::SMC => "smc",
            Mnemonic::BRK => "brk",
            Mnemonic::HLT => "hlt",
            Mnemonic::UDF => "udf",
            Mnemonic::NOP => "nop",
            Mnemonic::IT => "it",
            Mnemonic::YIELD => "yield",
            Mnemonic::WFE => "wfe",
            Mnemonic::WFI => "wfi",
            Mnemonic::SEV => "sev",
            Mnemonic::SEVL => "sevl",
            Mnemonic::ISB => "isb",
            Mnemonic::DMB => "dmb",
            Mnemonic::DSB => "dsb",
            Mnemonic::CLREX => "clrex",
            Mnemonic::MSR => "msr",
            Mnemonic::MRS => "mrs",
            Mnemonic::SYS => "sys",
            Mnemonic::SYSL => "sysl",
            Mnemonic::DC => "dc",
            Mnemonic::IC => "ic",
            Mnemonic::AT => "at",
            Mnemonic::TLBI => "tlbi",
            Mnemonic::HINT => "hint",

            // Exception
            Mnemonic::SWI => "swi",
            Mnemonic::BKPT => "bkpt",

            // Coprocessor
            Mnemonic::MCR => "mcr",
            Mnemonic::MRC => "mrc",
            Mnemonic::MCR2 => "mcr2",
            Mnemonic::MRC2 => "mrc2",
            Mnemonic::MCRR => "mcrr",
            Mnemonic::MRRC => "mrrc",
            Mnemonic::CDP => "cdp",
            Mnemonic::CDP2 => "cdp2",
            Mnemonic::LDC => "ldc",
            Mnemonic::STC => "stc",

            // SIMD/FP
            Mnemonic::FADD => "fadd",
            Mnemonic::FSUB => "fsub",
            Mnemonic::FMUL => "fmul",
            Mnemonic::FDIV => "fdiv",
            Mnemonic::FNMUL => "fnmul",
            Mnemonic::FMADD => "fmadd",
            Mnemonic::FMSUB => "fmsub",
            Mnemonic::FNMADD => "fnmadd",
            Mnemonic::FNMSUB => "fnmsub",
            Mnemonic::FMAX => "fmax",
            Mnemonic::FMIN => "fmin",
            Mnemonic::FMAXNM => "fmaxnm",
            Mnemonic::FMINNM => "fminnm",
            Mnemonic::FABS => "fabs",
            Mnemonic::FNEG => "fneg",
            Mnemonic::FSQRT => "fsqrt",
            Mnemonic::FRINT => "frint",
            Mnemonic::FRINTN => "frintn",
            Mnemonic::FRINTP => "frintp",
            Mnemonic::FRINTM => "frintm",
            Mnemonic::FRINTZ => "frintz",
            Mnemonic::FRINTA => "frinta",
            Mnemonic::FRINTX => "frintx",
            Mnemonic::FRINTI => "frinti",
            Mnemonic::FCMP => "fcmp",
            Mnemonic::FCMPE => "fcmpe",
            Mnemonic::FCCMP => "fccmp",
            Mnemonic::FCCMPE => "fccmpe",
            Mnemonic::FCVT => "fcvt",
            Mnemonic::FCVTAS => "fcvtas",
            Mnemonic::FCVTAU => "fcvtau",
            Mnemonic::FCVTMS => "fcvtms",
            Mnemonic::FCVTMU => "fcvtmu",
            Mnemonic::FCVTNS => "fcvtns",
            Mnemonic::FCVTNU => "fcvtnu",
            Mnemonic::FCVTPS => "fcvtps",
            Mnemonic::FCVTPU => "fcvtpu",
            Mnemonic::FCVTZS => "fcvtzs",
            Mnemonic::FCVTZU => "fcvtzu",
            Mnemonic::SCVTF => "scvtf",
            Mnemonic::UCVTF => "ucvtf",
            Mnemonic::FMOV => "fmov",
            Mnemonic::FCSEL => "fcsel",

            // SIMD Vector
            Mnemonic::VADD => "vadd",
            Mnemonic::VSUB => "vsub",
            Mnemonic::VMUL => "vmul",
            Mnemonic::VMLA => "vmla",
            Mnemonic::VMLS => "vmls",
            Mnemonic::VAND => "vand",
            Mnemonic::VORR => "vorr",
            Mnemonic::VEOR => "veor",
            Mnemonic::VBIC => "vbic",
            Mnemonic::VMOV => "vmov",
            Mnemonic::VMVN => "vmvn",
            Mnemonic::VDUP => "vdup",
            Mnemonic::VEXT => "vext",
            Mnemonic::VZIP => "vzip",
            Mnemonic::VUZP => "vuzp",
            Mnemonic::VTRN => "vtrn",
            Mnemonic::VTBL => "vtbl",
            Mnemonic::VTBX => "vtbx",
            Mnemonic::VCMP => "vcmp",
            Mnemonic::VCMPE => "vcmpe",
            Mnemonic::VABS => "vabs",
            Mnemonic::VNEG => "vneg",
            Mnemonic::VSQRT => "vsqrt",
            Mnemonic::VRECPE => "vrecpe",
            Mnemonic::VRSQRTE => "vrsqrte",
            Mnemonic::VCVT_F32_S32 => "vcvt.f32.s32",
            Mnemonic::VCVT_F32_U32 => "vcvt.f32.u32",
            Mnemonic::VCVT_S32_F32 => "vcvt.s32.f32",
            Mnemonic::VCVT_U32_F32 => "vcvt.u32.f32",
            Mnemonic::VLD1 => "vld1",
            Mnemonic::VLD2 => "vld2",
            Mnemonic::VLD3 => "vld3",
            Mnemonic::VLD4 => "vld4",
            Mnemonic::VST1 => "vst1",
            Mnemonic::VST2 => "vst2",
            Mnemonic::VST3 => "vst3",
            Mnemonic::VST4 => "vst4",
            Mnemonic::VLDM => "vldm",
            Mnemonic::VSTM => "vstm",
            Mnemonic::VPUSH => "vpush",
            Mnemonic::VPOP => "vpop",

            // PAC
            Mnemonic::PACIA => "pacia",
            Mnemonic::PACIB => "pacib",
            Mnemonic::PACDA => "pacda",
            Mnemonic::PACDB => "pacdb",
            Mnemonic::PACGA => "pacga",
            Mnemonic::AUTIA => "autia",
            Mnemonic::AUTIB => "autib",
            Mnemonic::AUTDA => "autda",
            Mnemonic::AUTDB => "autdb",
            Mnemonic::XPACI => "xpaci",
            Mnemonic::XPACD => "xpacd",
            Mnemonic::PACIZA => "paciza",
            Mnemonic::PACIZB => "pacizb",
            Mnemonic::PACDZA => "pacdza",
            Mnemonic::PACDZB => "pacdzb",
            Mnemonic::AUTIZA => "autiza",
            Mnemonic::AUTIZB => "autizb",
            Mnemonic::AUTDZA => "autdza",
            Mnemonic::AUTDZB => "autdzb",
            Mnemonic::BLRAA => "blraa",
            Mnemonic::BLRAB => "blrab",
            Mnemonic::BRAA => "braa",
            Mnemonic::BRAB => "brab",
            Mnemonic::RETAA => "retaa",
            Mnemonic::RETAB => "retab",

            // MTE
            Mnemonic::IRG => "irg",
            Mnemonic::GMI => "gmi",
            Mnemonic::ADDG => "addg",
            Mnemonic::SUBG => "subg",
            Mnemonic::STG => "stg",
            Mnemonic::STZG => "stzg",
            Mnemonic::ST2G => "st2g",
            Mnemonic::STZ2G => "stz2g",
            Mnemonic::LDG => "ldg",
            Mnemonic::STGP => "stgp",

            // BTI
            Mnemonic::BTI => "bti",

            // Saturation
            Mnemonic::SSAT => "ssat",
            Mnemonic::USAT => "usat",

            // Misc
            Mnemonic::SETF8 => "setf8",
            Mnemonic::SETF16 => "setf16",
            Mnemonic::RMIF => "rmif",
            Mnemonic::CFINV => "cfinv",
            Mnemonic::AXFLAG => "axflag",
            Mnemonic::XAFLAG => "xaflag",

            // SVE
            Mnemonic::SVE_ADD => "add",
            Mnemonic::SVE_SUB => "sub",
            Mnemonic::SVE_MUL => "mul",
            Mnemonic::SVE_AND => "and",
            Mnemonic::SVE_ORR => "orr",
            Mnemonic::SVE_EOR => "eor",
            Mnemonic::SVE_LD1 => "ld1",
            Mnemonic::SVE_ST1 => "st1",
            Mnemonic::SVE_PTRUE => "ptrue",
            Mnemonic::SVE_PFALSE => "pfalse",
            Mnemonic::SVE_WHILELT => "whilelt",
            Mnemonic::SVE_WHILELE => "whilele",

            // Unknown/Invalid
            Mnemonic::UNKNOWN => "unknown",
            Mnemonic::UNDEFINED => "udf",
        }
    }
}

impl std::fmt::Display for Mnemonic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Unified decoder that handles all ARM execution states.
pub struct Decoder {
    state: ExecutionState,
}

impl Decoder {
    /// Create a new decoder for the specified execution state.
    pub fn new(state: ExecutionState) -> Self {
        Decoder { state }
    }

    /// Create a decoder for AArch64.
    pub fn new_aarch64() -> Self {
        Self::new(ExecutionState::Aarch64)
    }

    /// Create a decoder for AArch32 (ARM mode).
    pub fn new_aarch32() -> Self {
        Self::new(ExecutionState::Aarch32)
    }

    /// Create a decoder for 32-bit ARM mode.
    pub fn new_arm() -> Self {
        Self::new(ExecutionState::Arm)
    }

    /// Create a decoder for Thumb mode.
    pub fn new_thumb() -> Self {
        Self::new(ExecutionState::Thumb)
    }

    /// Decode an instruction from bytes.
    ///
    /// Returns the decoded instruction and the number of bytes consumed.
    pub fn decode(&self, bytes: &[u8]) -> Result<DecodedInsn, DecodeError> {
        match self.state {
            ExecutionState::Aarch64 => {
                if bytes.len() < 4 {
                    return Err(DecodeError::InsufficientBytes);
                }
                let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                Aarch64Decoder::decode(raw)
            }
            ExecutionState::Arm | ExecutionState::Aarch32 => {
                if bytes.len() < 4 {
                    return Err(DecodeError::InsufficientBytes);
                }
                let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                Aarch32Decoder::decode(raw)
            }
            ExecutionState::Thumb | ExecutionState::Thumb2 => {
                if bytes.len() < 2 {
                    return Err(DecodeError::InsufficientBytes);
                }
                let hw1 = u16::from_le_bytes([bytes[0], bytes[1]]);

                // Check if this is a 32-bit Thumb instruction
                if ThumbDecoder::is_32bit_instruction(hw1) {
                    if bytes.len() < 4 {
                        return Err(DecodeError::InsufficientBytes);
                    }
                    let hw2 = u16::from_le_bytes([bytes[2], bytes[3]]);
                    let raw = ((hw1 as u32) << 16) | (hw2 as u32);
                    ThumbDecoder::decode_32bit(raw)
                } else {
                    ThumbDecoder::decode_16bit(hw1)
                }
            }
        }
    }

    /// Get the current execution state.
    pub fn state(&self) -> ExecutionState {
        self.state
    }

    /// Set the execution state.
    pub fn set_state(&mut self, state: ExecutionState) {
        self.state = state;
    }
}

/// Errors that can occur during decoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Not enough bytes provided for decoding.
    InsufficientBytes,
    /// The instruction encoding is invalid or undefined.
    InvalidEncoding,
    /// The instruction is undefined for this architecture version.
    Undefined,
    /// The instruction is unpredictable.
    Unpredictable,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::InsufficientBytes => write!(f, "insufficient bytes for decoding"),
            DecodeError::InvalidEncoding => write!(f, "invalid instruction encoding"),
            DecodeError::Undefined => write!(f, "undefined instruction"),
            DecodeError::Unpredictable => write!(f, "unpredictable instruction"),
        }
    }
}

impl std::error::Error for DecodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_condition_decode() {
        assert_eq!(Condition::from_bits(0b0000), Condition::EQ);
        assert_eq!(Condition::from_bits(0b0001), Condition::NE);
        assert_eq!(Condition::from_bits(0b1110), Condition::AL);
    }

    #[test]
    fn test_condition_invert() {
        assert_eq!(Condition::EQ.invert(), Condition::NE);
        assert_eq!(Condition::NE.invert(), Condition::EQ);
        assert_eq!(Condition::LT.invert(), Condition::GE);
    }

    #[test]
    fn test_shift_type_decode() {
        assert_eq!(ShiftType::from_bits(0b00), ShiftType::LSL);
        assert_eq!(ShiftType::from_bits(0b01), ShiftType::LSR);
        assert_eq!(ShiftType::from_bits(0b10), ShiftType::ASR);
        assert_eq!(ShiftType::from_bits(0b11), ShiftType::ROR);
    }

    #[test]
    fn test_extend_type_decode() {
        assert_eq!(ExtendType::from_bits(0b000), ExtendType::UXTB);
        assert_eq!(ExtendType::from_bits(0b011), ExtendType::UXTX);
        assert_eq!(ExtendType::from_bits(0b110), ExtendType::SXTW);
    }

    #[test]
    fn test_mnemonic_display() {
        assert_eq!(Mnemonic::ADD.as_str(), "add");
        assert_eq!(Mnemonic::LDR.as_str(), "ldr");
        assert_eq!(Mnemonic::BL.as_str(), "bl");
    }
}
