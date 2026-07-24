//! Register-only EVEX scalar lane-transfer replay classification.

use super::X86InstructionBytes;

#[derive(Clone, Copy)]
enum GprField {
    None,
    Reg,
    Rm,
}

impl X86InstructionBytes {
    /// Validate one register-only EVEX scalar lane transfer that is not already
    /// directly lowerable from its semantic SMIR operations.
    ///
    /// The admitted set is `VEXTRACTPS`, `VINSERTPS`, `VPEXTRB/D/Q/W`, and
    /// `VPINSRB/D/Q/W`. Dword/qword integer forms require AVX-512DQ and return
    /// `true`; the remaining forms require AVX-512F or AVX-512BW. Every form is
    /// fixed at EVEX.128 and forbids masking, zeroing, and EVEX.b. Memory forms,
    /// fabricated GPR bit 4, and GPR operands using RSP/RBP fail closed.
    pub fn evex_register_scalar_lane_transfer_requires_dq(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }

        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p1 & 0x04 == 0 || p2 & !0x08 != 0 || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x0f;
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        if pp != 1 {
            return None;
        }

        let (needs_dq, reserved_vvvv, gpr_field) = match (map, opcode, w) {
            // VPINSRW and VPEXTRW reg,xmm aliases. W is ignored.
            (1, 0xC4, _) => (false, false, GprField::Rm),
            (1, 0xC5, _) => (false, true, GprField::Reg),

            // VPEXTRB/W/D/Q and VEXTRACTPS. W is ignored for B/W/PS.
            (3, 0x14 | 0x15 | 0x17, _) => (false, true, GprField::Rm),
            (3, 0x16, _) => (true, true, GprField::Rm),

            // VPINSRB/D/Q and VINSERTPS. W is ignored for VPINSRB; VINSERTPS
            // requires W0, while both VPINSRD and VPINSRQ require AVX-512DQ.
            (3, 0x20, _) => (false, false, GprField::Rm),
            (3, 0x21, false) => (false, false, GprField::None),
            (3, 0x22, _) => (true, false, GprField::Rm),
            _ => return None,
        };

        if reserved_vvvv && (p1 & 0x78 != 0x78 || p2 & 0x08 == 0) {
            return None;
        }

        let (extension_valid, low_gpr_bank, gpr_low) = match gpr_field {
            GprField::None => (true, false, 0),
            // EVEX.R' cannot name a 17th GPR. EVEX.R selects GPR0-7/8-15.
            GprField::Reg => (p0 & 0x10 != 0, p0 & 0x80 != 0, (modrm >> 3) & 0x07),
            // EVEX.X' cannot name a 17th GPR. EVEX.B selects GPR0-7/8-15.
            GprField::Rm => (p0 & 0x40 != 0, p0 & 0x20 != 0, modrm & 0x07),
        };
        if !extension_valid || (low_gpr_bank && matches!(gpr_low, 4 | 5)) {
            return None;
        }

        Some(needs_dq)
    }
}
