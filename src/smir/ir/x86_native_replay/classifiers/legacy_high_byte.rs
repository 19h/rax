//! Legacy AH/CH/DH/BH register replay classification.

use super::X86InstructionBytes;

fn legacy_prefix_len(bytes: &[u8]) -> Option<usize> {
    let mut prefix_groups = 0u8;
    let mut start = 0usize;
    while let Some(byte) = bytes.get(start) {
        let group = match byte {
            0xF2 | 0xF3 => 1,
            0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 => 2,
            0x66 => 4,
            0x67 => 8,
            _ => break,
        };
        if prefix_groups & group != 0 {
            return None;
        }
        prefix_groups |= group;
        start += 1;
    }
    Some(start)
}

impl X86InstructionBytes {
    /// Validate one baseline scalar instruction whose register-only byte
    /// encoding names AH, CH, DH, or BH.
    ///
    /// Native replay is required because the semantic lifter represents a
    /// high-byte operand as an extract/merge graph with virtual registers. The
    /// x86 identity-map JIT has no unoccupied GPR in which to materialize that
    /// graph, while replaying the exact source instruction preserves aliasing
    /// between each high byte and its full-width parent.
    ///
    /// The admitted set contains MOV, binary ALU, TEST, XCHG, Group 1
    /// immediate, NOT, NEG, INC, DEC, SETcc, CMPXCHG, and XADD register forms.
    /// LOCK, REX, memory, Group 2 shifts/rotates, Group 3 `/1`, multiply, and
    /// divide forms fail closed. Group 2 needs a separate deterministic flag
    /// merge because RAX preserves architecturally undefined AF/OF while the
    /// host instruction may change them. At most one legacy prefix from each
    /// prefix group is accepted; none changes an 8-bit register operand.
    pub fn is_legacy_high_byte_register_replay(&self) -> bool {
        let bytes = self.as_slice();
        let Some(start) = legacy_prefix_len(bytes) else {
            return false;
        };

        let register_fields =
            |modrm: u8| (modrm >> 6 == 3).then_some(((modrm >> 3) & 7, modrm & 7));
        let is_high = |register: u8| register >= 4;

        match &bytes[start..] {
            [opcode, modrm]
                if matches!(
                    opcode,
                    0x00 | 0x02
                        | 0x08
                        | 0x0A
                        | 0x10
                        | 0x12
                        | 0x18
                        | 0x1A
                        | 0x20
                        | 0x22
                        | 0x28
                        | 0x2A
                        | 0x30
                        | 0x32
                        | 0x38
                        | 0x3A
                        | 0x84
                        | 0x86
                        | 0x88
                        | 0x8A
                ) =>
            {
                register_fields(*modrm).is_some_and(|(reg, rm)| is_high(reg) || is_high(rm))
            }
            [0xFE, modrm] => {
                register_fields(*modrm).is_some_and(|(extension, rm)| extension <= 1 && is_high(rm))
            }
            [0x80, modrm, _] => register_fields(*modrm).is_some_and(|(_, rm)| is_high(rm)),
            [0xC6, modrm, _] => {
                register_fields(*modrm).is_some_and(|(extension, rm)| extension == 0 && is_high(rm))
            }
            [0xF6, modrm, _] => {
                register_fields(*modrm).is_some_and(|(extension, rm)| extension == 0 && is_high(rm))
            }
            [0xF6, modrm] => register_fields(*modrm)
                .is_some_and(|(extension, rm)| matches!(extension, 2 | 3) && is_high(rm)),
            [0x0F, opcode @ (0xB0 | 0xC0), modrm] => {
                let _ = opcode;
                register_fields(*modrm).is_some_and(|(reg, rm)| is_high(reg) || is_high(rm))
            }
            [0x0F, opcode @ 0x90..=0x9F, modrm] => {
                let _ = opcode;
                register_fields(*modrm).is_some_and(|(extension, rm)| extension == 0 && is_high(rm))
            }
            _ => false,
        }
    }

    /// Return the ModR/M destination index for an admitted high-byte
    /// `CMPXCHG r8, r8`. The accumulator comparison uses AL regardless of
    /// whether the destination or source is the high-byte operand.
    pub fn legacy_high_byte_cmpxchg_destination_index(&self) -> Option<u8> {
        if !self.is_legacy_high_byte_register_replay() {
            return None;
        }
        let bytes = self.as_slice();
        let start = legacy_prefix_len(bytes)?;
        match &bytes[start..] {
            [0x0F, 0xB0, modrm] => Some(modrm & 7),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifier_exhaustively_accepts_documented_register_cells() {
        let mut accepted = 0usize;
        for prefix in [
            &[][..],
            &[0x66][..],
            &[0x67][..],
            &[0x64][..],
            &[0xF2][..],
            &[0x65, 0x66, 0x67, 0xF3][..],
        ] {
            for opcode in [
                0x00, 0x02, 0x08, 0x0A, 0x10, 0x12, 0x18, 0x1A, 0x20, 0x22, 0x28, 0x2A, 0x30, 0x32,
                0x38, 0x3A, 0x84, 0x86, 0x88, 0x8A,
            ] {
                for fields in 0u8..=0x3F {
                    let mut bytes = prefix.to_vec();
                    bytes.extend([opcode, 0xC0 | fields]);
                    let expected = fields & 7 >= 4 || (fields >> 3) & 7 >= 4;
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .is_legacy_high_byte_register_replay(),
                        expected,
                        "{bytes:02X?}"
                    );
                    accepted += usize::from(expected);
                }
            }

            for (opcode, valid_extensions, has_immediate) in
                [(0xFE, 0b0000_0011u8, false), (0x80, 0b1111_1111, true)]
            {
                for extension in 0u8..8 {
                    for rm in 0u8..8 {
                        let mut bytes = prefix.to_vec();
                        bytes.extend([opcode, 0xC0 | (extension << 3) | rm]);
                        if has_immediate {
                            bytes.push(0xA5);
                        }
                        let expected = valid_extensions & (1 << extension) != 0 && rm >= 4;
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .is_legacy_high_byte_register_replay(),
                            expected,
                            "{bytes:02X?}"
                        );
                        accepted += usize::from(expected);
                    }
                }
            }

            for (opcode, valid_extensions, has_immediate) in [
                (0xC6, 0b0000_0001u8, true),
                (0xF6, 0b0000_0001u8, true),
                (0xF6, 0b0000_1100u8, false),
            ] {
                for extension in 0u8..8 {
                    for rm in 0u8..8 {
                        let mut bytes = prefix.to_vec();
                        bytes.extend([opcode, 0xC0 | (extension << 3) | rm]);
                        if has_immediate {
                            bytes.push(0xA5);
                        }
                        let expected = valid_extensions & (1 << extension) != 0 && rm >= 4;
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .is_legacy_high_byte_register_replay(),
                            expected,
                            "{bytes:02X?}"
                        );
                        accepted += usize::from(expected);
                    }
                }
            }

            for opcode in 0x90u8..=0x9F {
                for extension in 0u8..8 {
                    for rm in 0u8..8 {
                        let mut bytes = prefix.to_vec();
                        bytes.extend([0x0F, opcode, 0xC0 | (extension << 3) | rm]);
                        let expected = extension == 0 && rm >= 4;
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .is_legacy_high_byte_register_replay(),
                            expected,
                            "{bytes:02X?}"
                        );
                        accepted += usize::from(expected);
                    }
                }
            }

            for opcode in [0xB0, 0xC0] {
                for fields in 0u8..=0x3F {
                    let mut bytes = prefix.to_vec();
                    bytes.extend([0x0F, opcode, 0xC0 | fields]);
                    let expected = fields & 7 >= 4 || (fields >> 3) & 7 >= 4;
                    let instruction = X86InstructionBytes::new(&bytes).unwrap();
                    assert_eq!(
                        instruction.is_legacy_high_byte_register_replay(),
                        expected,
                        "{bytes:02X?}"
                    );
                    assert_eq!(
                        instruction.legacy_high_byte_cmpxchg_destination_index(),
                        (opcode == 0xB0 && expected).then_some(fields & 7),
                        "{bytes:02X?}"
                    );
                    accepted += usize::from(expected);
                }
            }
        }
        assert_eq!(accepted, 7_056);
    }

    #[test]
    fn classifier_covers_immediate_unary_and_map0f_families() {
        for bytes in [
            &[0x80, 0xC4, 0x81][..],                   // add ah,0x81
            &[0xC6, 0xC7, 0x5A][..],                   // mov bh,0x5a
            &[0xF6, 0xC5, 0xA5][..],                   // test ch,0xa5
            &[0xF6, 0xD6][..],                         // not dh
            &[0xF6, 0xDF][..],                         // neg bh
            &[0x0F, 0x96, 0xC4][..],                   // setbe ah
            &[0x0F, 0xB0, 0xF5][..],                   // cmpxchg ch,dh
            &[0x0F, 0xC0, 0xFC][..],                   // xadd ah,bh
            &[0x65, 0x66, 0x67, 0xF3, 0x00, 0xEC][..], // add ah,ch
        ] {
            assert!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .is_legacy_high_byte_register_replay(),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn classifier_rejects_every_unsafe_or_undocumented_frontier() {
        for bytes in [
            &[0x00, 0xC3][..],             // add bl,al: no high byte
            &[0x00, 0x04][..],             // memory destination
            &[0x40, 0x00, 0xC4][..],       // REX selects SPL, not AH
            &[0xF0, 0x00, 0xC4][..],       // LOCK register form is #UD
            &[0xF2, 0xF3, 0x00, 0xC4][..], // duplicate prefix group
            &[0x66, 0x66, 0x00, 0xC4][..], // duplicate prefix group
            &[0xD0, 0xC4][..],             // Group 2 needs exact flag merging
            &[0xD2, 0xED][..],             // dynamic Group 2 count
            &[0xC0, 0xFF, 0x03][..],       // immediate Group 2 count
            &[0xF6, 0xCC, 0x01][..],       // Group 3 /1 compatibility alias
            &[0xF6, 0xE4][..],             // mul ah: undefined flags
            &[0xF6, 0xEC][..],             // imul ah: undefined flags
            &[0xF6, 0xF4][..],             // div ah can raise #DE
            &[0xF6, 0xFC][..],             // idiv ah can raise #DE
            &[0xC6, 0xCC, 0x01][..],       // MOV requires /0
            &[0x0F, 0x96, 0xCC][..],       // SETcc requires /0
            &[0x0F, 0xB0, 0x35][..],       // CMPXCHG memory form
            &[0x0F, 0xC0, 0xFC, 0x00][..], // trailing byte
        ] {
            assert!(
                !X86InstructionBytes::new(bytes)
                    .unwrap()
                    .is_legacy_high_byte_register_replay(),
                "{bytes:02X?}"
            );
        }
    }
}
