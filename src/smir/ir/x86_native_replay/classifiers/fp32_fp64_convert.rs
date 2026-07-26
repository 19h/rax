//! Register-only EVEX binary32/binary64 precision-conversion replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX `VCVTPS2PD` or `VCVTPD2PS`
    /// instruction and return whether native execution requires AVX-512VL.
    ///
    /// With `EVEX.b=0`, `L'L=00/01/10` select 128-/256-/512-bit instruction
    /// widths and `L'L=11` is reserved. With a register source and
    /// `EVEX.b=1`, both instructions imply a 512-bit width:
    /// `VCVTPD2PS` consumes `L'L` as embedded rounding control, whereas the
    /// exact widening `VCVTPS2PD` ignores `L'L` and uses SAE. Memory forms and
    /// malformed reserved fields fail closed.
    pub fn evex_register_fp32_fp64_convert_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p0 & 0x0F != 1
            || p1 & 0x7C != 0x7C
            || p2 & 0x08 == 0
            || opcode != 0x5A
            || modrm >> 6 != 3
            || (p2 & 0x80 != 0 && p2 & 0x07 == 0)
        {
            return None;
        }

        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        if !matches!((pp, w), (0, false) | (1, true)) {
            return None;
        }

        let ll = (p2 >> 5) & 0x03;
        if p2 & 0x10 != 0 {
            return Some(false);
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }
}
