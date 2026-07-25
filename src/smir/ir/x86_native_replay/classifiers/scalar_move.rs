//! Register-only scalar floating-point move replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only legacy SSE or AVX VEX `MOVSS`, `MOVSD`,
    /// `VMOVSS`, or `VMOVSD` instruction and report whether it requires AVX.
    ///
    /// Both opcode directions (`10` and `11`) are admitted because register
    /// forms only merge architectural XMM state and cannot fault. Canonical
    /// legacy mandatory-prefix placement and an optional final REX prefix are
    /// accepted. VEX map 0F accepts C5 and C4 encodings and treats W as
    /// ignored. Intel defines `VMOVSD` as LIG, but documents `VMOVSS` with
    /// `VEX.L=1` as generation-dependent unpredictable behavior; only the
    /// latter encoding is excluded. Memory operands and every non-canonical
    /// byte shape fail closed.
    pub fn legacy_vex_register_scalar_move_needs_avx(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let legacy = match bytes {
            [0xF2 | 0xF3, 0x0F, opcode, modrm] => Some((*opcode, *modrm)),
            [0xF2 | 0xF3, 0x40..=0x4F, 0x0F, opcode, modrm] => Some((*opcode, *modrm)),
            _ => None,
        };
        if let Some((opcode, modrm)) = legacy {
            return (matches!(opcode, 0x10 | 0x11) && modrm >> 6 == 3).then_some(false);
        }

        let (p1, opcode, modrm) = match bytes {
            [0xC5, p1, opcode, modrm] => (*p1, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm),
            _ => return None,
        };
        let pp = p1 & 0x03;
        let vmovss_l1 = pp == 2 && p1 & 0x04 != 0;
        (matches!(pp, 2 | 3) && matches!(opcode, 0x10 | 0x11) && modrm >> 6 == 3 && !vmovss_l1)
            .then_some(true)
    }

    /// Validate one register-only EVEX `VMOVSH`, `VMOVSS`, or `VMOVSD`
    /// instruction in either opcode direction.
    ///
    /// Returns whether AVX-512-FP16 is required. All three scalar families are
    /// LLIG, accept the three defined EVEX vector-length encodings, and require
    /// neither AVX-512VL nor AVX-512DQ. `VMOVSS` and `VMOVSD` require AVX-512F;
    /// `VMOVSH` requires AVX-512-FP16. Register forms consume EVEX.vvvv/V' as
    /// the upper-XMM merge source. EVEX.b, malformed zeroing with k0, memory
    /// forms, and every non-family opcode field fail closed.
    pub fn evex_register_scalar_move_requires_fp16(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p1 & 0x04 == 0 || !matches!(opcode, 0x10 | 0x11) || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let needs_fp16 = match (map, pp, w) {
            // VMOVSS and VMOVSD.
            (1, 2, false) | (1, 3, true) => false,
            // VMOVSH.
            (5, 2, false) => true,
            _ => return None,
        };

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || ll == 3 || (zeroing && mask == 0) {
            return None;
        }

        Some(needs_fp16)
    }
}
