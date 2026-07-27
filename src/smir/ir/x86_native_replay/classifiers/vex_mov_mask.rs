//! Guest-stack-destination AVX VEX vector sign-mask extracts.

use super::X86InstructionBytes;

fn opcode_matches_prefix(p1: u8, opcode: u8) -> bool {
    matches!((opcode, p1 & 0x03), (0x50, 0 | 1) | (0xD7, 1))
}

impl X86InstructionBytes {
    /// Validate a register-only VEX `VMOVMSKPS`, `VMOVMSKPD`, or `VPMOVMSKB`
    /// whose architectural r32 destination is guest RSP or RBP.
    ///
    /// The exact replay path is intentionally limited to these two
    /// destinations: every other GPR is already handled by canonical
    /// `X86MovMask` lowering. Both VEX.128 and VEX.256 are valid, VEX.W is
    /// ignored, VEX.vvvv must be encoded as `1111b`, and the source must be
    /// XMM0-XMM15 or YMM0-YMM15. `VPMOVMSKB` requires AVX2 only at 256 bits;
    /// every other admitted form requires AVX.
    pub fn vex_mov_mask_stack_destination_needs_avx2(&self) -> Option<bool> {
        let (p1, opcode, modrm, destination) = match self.as_slice() {
            [0xC5, p1, opcode, modrm] => (
                *p1,
                *opcode,
                *modrm,
                (u8::from(p1 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
            ),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (
                *p1,
                *opcode,
                *modrm,
                (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
            ),
            _ => return None,
        };
        if p1 & 0x78 != 0x78
            || modrm >> 6 != 3
            || !matches!(destination, 4 | 5)
            || !opcode_matches_prefix(p1, opcode)
        {
            return None;
        }
        Some(opcode == 0xD7 && p1 & 0x04 != 0)
    }

    /// Return the validated guest RSP/RBP destination index.
    pub(crate) fn vex_mov_mask_stack_destination_index(&self) -> Option<u8> {
        self.vex_mov_mask_stack_destination_needs_avx2()?;
        let destination = match self.as_slice() {
            [0xC5, p1, _opcode, modrm] => (u8::from(p1 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
            [0xC4, p0, _p1, _opcode, modrm] => (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
            _ => unreachable!("VEX MOVMSK stack-destination shape was validated"),
        };
        Some(destination)
    }

    /// Rewrite a validated guest RSP/RBP destination to another GPR while
    /// retaining every non-destination bit, including ignored W/X bits.
    pub(crate) fn vex_mov_mask_stack_destination_with_destination(
        &self,
        destination: u8,
    ) -> Option<Self> {
        if destination >= 16 || self.vex_mov_mask_stack_destination_needs_avx2().is_none() {
            return None;
        }

        let mut rewritten = *self;
        match self.as_slice() {
            [0xC5, _p1, _opcode, _modrm] => {
                if destination < 8 {
                    rewritten.bytes[1] |= 0x80;
                } else {
                    rewritten.bytes[1] &= !0x80;
                }
                rewritten.bytes[3] = (rewritten.bytes[3] & !0x38) | ((destination & 7) << 3);
            }
            [0xC4, _p0, _p1, _opcode, _modrm] => {
                if destination < 8 {
                    rewritten.bytes[1] |= 0x80;
                } else {
                    rewritten.bytes[1] &= !0x80;
                }
                rewritten.bytes[4] = (rewritten.bytes[4] & !0x38) | ((destination & 7) << 3);
            }
            _ => unreachable!("VEX MOVMSK stack-destination shape was validated"),
        }
        Some(rewritten)
    }
}
