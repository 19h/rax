//! Register-only floating-point shuffle/interleave replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only legacy SSE or AVX VEX `UNPCKL*`, `UNPCKH*`,
    /// or `SHUF*` instruction and report whether it requires AVX.
    ///
    /// Canonical legacy encodings accept no mandatory prefix for packed
    /// binary32 and `66` for packed binary64, followed by an optional final REX
    /// prefix. VEX map 0F accepts C5 and C4 forms, both defined vector lengths,
    /// and WIG encodings. `SHUF*` carries an imm8 while `UNPCKL*`/`UNPCKH*`
    /// does not. Memory operands and every non-canonical byte shape fail closed.
    pub fn legacy_vex_register_fp_shuffle_needs_avx(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let legacy_modrm = match bytes {
            [0x0F, 0x14 | 0x15, modrm]
            | [0x40..=0x4F, 0x0F, 0x14 | 0x15, modrm]
            | [0x66, 0x0F, 0x14 | 0x15, modrm]
            | [0x66, 0x40..=0x4F, 0x0F, 0x14 | 0x15, modrm]
            | [0x0F, 0xC6, modrm, _]
            | [0x40..=0x4F, 0x0F, 0xC6, modrm, _]
            | [0x66, 0x0F, 0xC6, modrm, _]
            | [0x66, 0x40..=0x4F, 0x0F, 0xC6, modrm, _] => Some(*modrm),
            _ => None,
        };
        if let Some(modrm) = legacy_modrm {
            return (modrm >> 6 == 3).then_some(false);
        }

        let (p1, modrm) = match bytes {
            [0xC5, p1, 0x14 | 0x15, modrm] | [0xC5, p1, 0xC6, modrm, _] => (*p1, *modrm),
            [0xC4, p0, p1, 0x14 | 0x15, modrm] | [0xC4, p0, p1, 0xC6, modrm, _]
                if p0 & 0x1F == 1 =>
            {
                (*p1, *modrm)
            }
            _ => return None,
        };
        (matches!(p1 & 0x03, 0 | 1) && modrm >> 6 == 3).then_some(true)
    }

    /// Validate register-only EVEX binary32/binary64 shuffle and unpack
    /// operations and return whether the vector length requires AVX-512VL.
    /// VSHUF* carries an imm8 while VUNPCKL*/VUNPCKH* does not. Memory,
    /// broadcast, EVEX.b, reserved vector lengths, and malformed masks fail
    /// closed.
    pub fn evex_register_fp_shuffle_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if !matches!(bytes.len(), 6 | 7) || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 1 || p1 & 0x04 == 0 || modrm >> 6 != 3 {
            return None;
        }
        match opcode {
            0x14 | 0x15 if bytes.len() == 6 => {}
            0xC6 if bytes.len() == 7 => {}
            _ => return None,
        }

        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        if !matches!(pp, 0 | 1) || w != (pp == 1) {
            return None;
        }
        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }
}
