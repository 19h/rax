//! Destination classification for VEX replay on the AVX YMM0-YMM15 bridge.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Return the architectural vector destination whose state-backed ZMM
    /// upper half must be cleared after one validated VEX register replay.
    ///
    /// These families were historically admitted only through the full
    /// AVX-512 state bridge. The AVX YMM0-YMM15 bridge stores bits 255:0 but
    /// deliberately leaves bits 511:256 in `GuestRegs`; VEX destination
    /// semantics therefore require an explicit clear of that state-backed
    /// upper half. Opcode 11H scalar moves encode their destination in
    /// ModR/M.r/m; every other family below uses ModR/M.reg.
    ///
    /// The constituent classifiers validate the complete instruction,
    /// register-only ModR/M form, map, prefix fields, vector length, immediate,
    /// and reserved bits before this method decodes either destination field.
    /// Complexity is O(1) time and O(1) space.
    pub(crate) fn vex_avx_ymm16_upper_clear_destination_index(&self) -> Option<u8> {
        let scalar_move = self.legacy_vex_register_scalar_move_needs_avx() == Some(true);
        let classified = self.is_vex_register_fma3()
            || self.is_vex_register_fp_logic()
            || self.legacy_vex_register_fp_horizontal_addsub_needs_avx() == Some(true)
            || self
                .vex_register_widening_dword_multiply_needs_avx2()
                .is_some()
            || self.legacy_vex_register_fp_arithmetic_needs_avx() == Some(true)
            || self.legacy_vex_register_fp_shuffle_needs_avx() == Some(true)
            || self.legacy_vex_register_high_low_move_needs_avx() == Some(true)
            || scalar_move
            || self.legacy_vex_register_fp_sqrt_needs_avx() == Some(true);
        if !classified {
            return None;
        }

        let (r_extension, b_extension, opcode, modrm) = match self.as_slice() {
            [0xC5, p1, opcode, modrm, ..] => (u8::from(p1 & 0x80 == 0) << 3, 0, *opcode, *modrm),
            [0xC4, p0, _, opcode, modrm, ..] => (
                u8::from(p0 & 0x80 == 0) << 3,
                u8::from(p0 & 0x20 == 0) << 3,
                *opcode,
                *modrm,
            ),
            _ => return None,
        };
        if scalar_move && opcode == 0x11 {
            Some(b_extension | (modrm & 7))
        } else {
            Some(r_extension | ((modrm >> 3) & 7))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_c5_c4_reg_and_scalar_store_rm_destinations() {
        for (bytes, destination) in [
            (&[0xC5, 0xE8, 0x54, 0xCB][..], 1),
            (&[0xC4, 0x41, 0x2C, 0x57, 0xCB][..], 9),
            (&[0xC5, 0xEA, 0x11, 0xCB][..], 3),
            (&[0xC4, 0xC1, 0x6A, 0x11, 0xCB][..], 11),
        ] {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .vex_avx_ymm16_upper_clear_destination_index(),
                Some(destination),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn excludes_legacy_memory_and_already_owned_vex_families() {
        for bytes in [
            &[0x0F, 0x54, 0xCB][..],
            &[0xC5, 0xE8, 0x54, 0x0B][..],
            &[0xC5, 0xF8, 0x53, 0xC0][..],
        ] {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .vex_avx_ymm16_upper_clear_destination_index(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
