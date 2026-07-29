//! Register-only VEX/EVEX packed sign/zero-extension replay.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Validate one register-only AVX/AVX2 VEX packed sign/zero-extension
    /// instruction and return whether its 256-bit destination requires AVX2.
    ///
    /// This covers VPMOVSXBW/BD/BQ/WD/WQ/DQ and
    /// VPMOVZXBW/BD/BQ/WD/WQ/DQ. Every form uses the three-byte VEX prefix,
    /// map 0F38, mandatory 66, reserved `VEX.vvvv=1111b`, and WIG. `VEX.L=0`
    /// forms require AVX; `VEX.L=1` forms require AVX2. Memory and malformed
    /// byte shapes fail closed.
    pub fn vex_register_packed_extend_needs_avx2(&self) -> Option<bool> {
        let &[0xC4, p0, p1, opcode, modrm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 2
            || p1 & 0x78 != 0x78
            || p1 & 0x03 != 1
            || !matches!(opcode, 0x20..=0x25 | 0x30..=0x35)
            || modrm >> 6 != 3
        {
            return None;
        }
        Some(p1 & 0x04 != 0)
    }

    /// Return the architectural VEX packed-extension destination after exact
    /// validation. The AVX-only state bridge uses this to clear the
    /// destination's state-backed ZMM[511:256] after architectural VEX
    /// upper-zeroing.
    pub(crate) fn vex_packed_extend_destination_index(&self) -> Option<u8> {
        self.vex_register_packed_extend_needs_avx2()?;
        let &[_, p0, _, _, modrm] = self.as_slice() else {
            unreachable!("VEX packed-extension shape was validated")
        };
        Some(((modrm >> 3) & 7) + if p0 & 0x80 == 0 { 8 } else { 0 })
    }

    /// Validate one complete AVX/AVX2 VEX packed sign/zero-extension
    /// instruction whose sole source is memory and return
    /// `(destination, source element, destination element, vector width,
    /// signed, opcode, W)`.
    ///
    /// All twelve forms use map 0F38, mandatory prefix 66H, reserve VEX.vvvv
    /// as encoded `1111b`, and define VEX.W as ignored. The shared parser
    /// validates the complete ModR/M/SIB/displacement shape and permits only
    /// segment/address-size legacy prefixes.
    pub(crate) fn vex_memory_packed_extend_fields(
        &self,
    ) -> Option<(u8, VecElementType, VecElementType, VecWidth, bool, u8, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.source1 != 0 || fields.map != 2 || fields.pp != 1 {
            return None;
        }
        let signed = fields.opcode < 0x30;
        let (source_element, destination_element) = match fields.opcode & 0x0F {
            0x00 => (VecElementType::I8, VecElementType::I16),
            0x01 => (VecElementType::I8, VecElementType::I32),
            0x02 => (VecElementType::I8, VecElementType::I64),
            0x03 => (VecElementType::I16, VecElementType::I32),
            0x04 => (VecElementType::I16, VecElementType::I64),
            0x05 => (VecElementType::I32, VecElementType::I64),
            _ => return None,
        };
        if !matches!(fields.opcode, 0x20..=0x25 | 0x30..=0x35) {
            return None;
        }
        Some((
            fields.destination,
            source_element,
            destination_element,
            if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            signed,
            fields.opcode,
            fields.w,
        ))
    }

    /// Validate register-only EVEX packed sign/zero-extension moves and return
    /// whether the destination vector length requires AVX-512VL. This covers
    /// VPMOVSXBW/BD/BQ/WD/WQ/DQ and VPMOVZXBW/BD/BQ/WD/WQ/DQ. W is ignored
    /// for every form except the fixed-W0 DQ forms. Reserved EVEX.vvvv/V',
    /// EVEX.b, vector length, masking, and memory forms fail closed.
    pub fn evex_register_packed_extend_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // Every admitted form uses map 0F38, mandatory 66, reserved
        // EVEX.vvvv=1111b and EVEX.V'=1, and a register ModR/M source.
        if p0 & 0x0F != 2
            || p1 & 0x07 != 0x05
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || modrm >> 6 != 3
            || !matches!(opcode, 0x20..=0x25 | 0x30..=0x35)
        {
            return None;
        }
        if matches!(opcode, 0x25 | 0x35) && p1 & 0x80 != 0 {
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
