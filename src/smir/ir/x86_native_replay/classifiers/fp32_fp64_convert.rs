//! Register-only VEX/EVEX binary32/binary64 precision-conversion replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only AVX VEX `VCVTPS2PD` or `VCVTPD2PS`
    /// instruction.
    ///
    /// Both forms use map 0F opcode 5A, reserve `VEX.vvvv=1111b`, and specify
    /// WIG. `pp=00` selects binary32-to-binary64 widening and `pp=01` selects
    /// binary64-to-binary32 narrowing. `VEX.L` selects the 128- or 256-bit
    /// source/destination relationship. Memory and malformed byte shapes fail
    /// closed.
    pub fn is_vex_register_fp32_fp64_convert(&self) -> bool {
        let bytes = self.as_slice();
        let (p1, opcode, modrm) = match bytes {
            [0xC5, p1, opcode, modrm] => (*p1, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm),
            _ => return false,
        };

        opcode == 0x5A && p1 & 0x78 == 0x78 && matches!(p1 & 0x03, 0 | 1) && modrm >> 6 == 3
    }

    /// Return the architectural VEX conversion destination register after
    /// exact validation. The native AVX-only state bridge uses this to clear
    /// the destination's state-backed ZMM[511:256] after the replayed VEX
    /// instruction zeros its architectural upper state.
    pub(crate) fn vex_fp32_fp64_convert_destination_index(&self) -> Option<u8> {
        if !self.is_vex_register_fp32_fp64_convert() {
            return None;
        }
        let bytes = self.as_slice();
        let (reg_extension, modrm) = match bytes {
            [0xC5, p1, _, modrm] => (p1 & 0x80 == 0, *modrm),
            [0xC4, p0, _, _, modrm] => (p0 & 0x80 == 0, *modrm),
            _ => unreachable!("VEX FP32/FP64 conversion shape was validated"),
        };
        Some(((modrm >> 3) & 7) + if reg_extension { 8 } else { 0 })
    }

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
