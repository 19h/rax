//! Register-destination AVX VEX scalar lane extracts.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-destination VEX `VEXTRACTPS` or `VPEXTRB/D/Q/W`.
    ///
    /// Every admitted form is fixed at VEX.128, mandatory 66H, reserves
    /// VEX.vvvv=`1111b`, and requires AVX. W selects `VPEXTRD`/`VPEXTRQ` for
    /// opcode 16H and is ignored for the remaining forms. Compact C5 and
    /// extended C4 encodings are both accepted for map-0F `VPEXTRW`.
    /// Guest RSP/RBP destinations are admitted through a lowerer rewrite that
    /// commits the result to state without clobbering the host stack or frame
    /// register. Memory destinations remain at the precise interpreter
    /// boundary.
    pub fn is_vex_register_scalar_extract(&self) -> bool {
        match self.as_slice() {
            [0xC5, p1, 0xC5, modrm, _imm] if p1 & 0x7F == 0x79 && modrm >> 6 == 3 => true,
            [0xC4, p0, p1, opcode, modrm, _imm] if p1 & 0x7F == 0x79 && modrm >> 6 == 3 => {
                matches!((p0 & 0x1F, opcode), (1, 0xC5) | (3, 0x14..=0x17))
            }
            _ => false,
        }
    }

    /// Return the architectural GPR destination after exact validation.
    pub(crate) fn vex_scalar_extract_destination_index(&self) -> Option<u8> {
        if !self.is_vex_register_scalar_extract() {
            return None;
        }
        match self.as_slice() {
            [0xC5, p1, 0xC5, modrm, _imm] => {
                Some((u8::from(p1 & 0x80 == 0) << 3) | ((modrm >> 3) & 7))
            }
            [0xC4, p0, _p1, 0xC5, modrm, _imm] if p0 & 0x1F == 1 => {
                Some((u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7))
            }
            [0xC4, p0, _p1, 0x14..=0x17, modrm, _imm] if p0 & 0x1F == 3 => {
                Some((u8::from(p0 & 0x20 == 0) << 3) | (modrm & 7))
            }
            _ => unreachable!("VEX scalar-extract shape was validated"),
        }
    }

    /// Rewrite an exact register-destination scalar extract to another GPR.
    ///
    /// This is used only by the x86-64 lowerer to redirect guest RSP/RBP
    /// destinations through a preserved scratch register before committing the
    /// result to state. Every non-destination bit, including ignored W/X and
    /// immediate bits, is retained exactly.
    pub(crate) fn vex_scalar_extract_with_destination(&self, destination: u8) -> Option<Self> {
        if destination >= 16 || !self.is_vex_register_scalar_extract() {
            return None;
        }

        let mut rewritten = *self;
        match self.as_slice() {
            [0xC5, _p1, 0xC5, _modrm, _imm] => {
                if destination < 8 {
                    rewritten.bytes[1] |= 0x80;
                } else {
                    rewritten.bytes[1] &= !0x80;
                }
                rewritten.bytes[3] = (rewritten.bytes[3] & !0x38) | ((destination & 7) << 3);
            }
            [0xC4, p0, _p1, 0xC5, _modrm, _imm] if p0 & 0x1F == 1 => {
                if destination < 8 {
                    rewritten.bytes[1] |= 0x80;
                } else {
                    rewritten.bytes[1] &= !0x80;
                }
                rewritten.bytes[4] = (rewritten.bytes[4] & !0x38) | ((destination & 7) << 3);
            }
            [0xC4, p0, _p1, 0x14..=0x17, _modrm, _imm] if p0 & 0x1F == 3 => {
                if destination < 8 {
                    rewritten.bytes[1] |= 0x20;
                } else {
                    rewritten.bytes[1] &= !0x20;
                }
                rewritten.bytes[4] = (rewritten.bytes[4] & !0x07) | (destination & 7);
            }
            _ => unreachable!("VEX scalar-extract shape was validated"),
        }
        debug_assert_eq!(
            rewritten.vex_scalar_extract_destination_index(),
            Some(destination)
        );
        Some(rewritten)
    }
}
