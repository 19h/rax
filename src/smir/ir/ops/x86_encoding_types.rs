//! x86 encoding provenance retained by otherwise architecture-neutral SMIR operations.

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::types::{ArchReg, OpWidth, VReg, VecWidth, X86Reg};

/// One architectural register operand of register-form XADD.
///
/// `high_byte` selects AH/CH/DH/BH within the RAX/RCX/RDX/RBX parent. Keeping
/// the byte lane explicit is necessary because those aliases cannot be
/// represented by a standalone [`X86Reg`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct X86GprOperand {
    pub reg: X86Reg,
    pub high_byte: bool,
}

impl X86GprOperand {
    pub const fn low(reg: X86Reg) -> Self {
        Self {
            reg,
            high_byte: false,
        }
    }

    pub const fn high(reg: X86Reg) -> Self {
        Self {
            reg,
            high_byte: true,
        }
    }

    pub fn vreg(self) -> VReg {
        VReg::Arch(ArchReg::X86(self.reg))
    }

    pub fn gpr_index(self) -> Option<u8> {
        self.reg.gpr_index()
    }

    pub fn is_valid_for(self, width: OpWidth) -> bool {
        let Some(index) = self.gpr_index() else {
            return false;
        };
        !self.high_byte || (width == OpWidth::W8 && index < 4)
    }
}

/// Exact register-only `XADD r/m, r` semantics.
///
/// Both operands are read before either destination is committed. `src`
/// receives the old `dst`; `dst` receives their wrapping sum. Equal operands
/// therefore end with twice the original value. `flags` is `All` in freshly
/// lifted code and may become `None` after dead-flag elimination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct X86XaddOp {
    pub dst: X86GprOperand,
    pub src: X86GprOperand,
    pub width: OpWidth,
    pub flags: FlagUpdate,
}

impl X86XaddOp {
    pub fn is_valid(self) -> bool {
        let operands_encodable = if self.dst.high_byte || self.src.high_byte {
            self.dst.gpr_index().is_some_and(|index| index < 4)
                && self.src.gpr_index().is_some_and(|index| index < 4)
        } else {
            true
        };
        matches!(
            self.width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        ) && matches!(self.flags, FlagUpdate::None | FlagUpdate::All)
            && self.dst.is_valid_for(self.width)
            && self.src.is_valid_for(self.width)
            && operands_encodable
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86AluEncoding {
    /// r/m, reg encoding
    RmReg,
    /// reg, r/m encoding
    RegRm,
    /// Accumulator immediate encoding
    AccImm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86SsePrefix {
    None,
    OpSize,
    Rep,
    Repne,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86VecMap {
    Map0F,
    Map0F38,
    Map0F3A,
    Map5,
    Map6,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86VecAlign {
    Aligned,
    Unaligned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86OpHint {
    /// ALU encoding preference
    AluEncoding(X86AluEncoding),
    /// Use ModR/M immediate encoding for MOV
    MovImmModRm,
    /// Push with 8-bit immediate
    PushImm8,
    /// Push with 16-bit immediate
    PushImm16,
    /// Push with 32-bit immediate
    PushImm32,
    /// IMUL with 8-bit immediate
    ImulImm8,
    /// IMUL with 32-bit immediate
    ImulImm32,
    /// BMI2 MULX, which has non-destructive RAX/RDX semantics.
    Mulx,
    /// Legacy Group-2 `/6` SAL alias. The architectural result matches SHL,
    /// but the interpreter's deterministic undefined-AF policy differs from
    /// `/4`; native tiers must fail closed unless they model that distinction.
    ShiftGroup6,
    /// Byte-register source was encoded with a REX/REX2 prefix, so ModR/M
    /// codes 4..7 name SPL/BPL/SIL/DIL (or extended low-byte regs), not
    /// legacy AH/CH/DH/BH.
    RexByteReg,
    /// Byte-register source was encoded without REX and ModR/M codes 4..7
    /// select AH/CH/DH/BH. The associated extension operation names the full
    /// parent GPR (RAX/RCX/RDX/RBX); lowering must read bits 15:8.
    LegacyHighByteReg,
    /// SSE mov with explicit prefix/opcode
    SseMov { prefix: X86SsePrefix, opcode: u8 },
    /// SSE opcode with explicit prefix/opcode
    SseOp { prefix: X86SsePrefix, opcode: u8 },
    /// VEX-encoded opcode (map/pp/opcode/width/W)
    VexOp {
        map: X86VecMap,
        pp: X86SsePrefix,
        opcode: u8,
        width: VecWidth,
        w: bool,
    },
    /// EVEX-encoded opcode (map/pp/opcode/width/W)
    EvexOp {
        map: X86VecMap,
        pp: X86SsePrefix,
        opcode: u8,
        width: VecWidth,
        w: bool,
    },
    /// AMD XOP VPCOM semantic provenance. The generic `VCmp` operation is
    /// cross-architecture and not independently safe for state-backed x86
    /// lowering; this tag identifies the exact strict-lifter contract.
    XopVpcom,
    /// Alignment hint for default vector moves
    VecAlign(X86VecAlign),
}
