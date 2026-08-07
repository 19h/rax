//! Legacy AH/CH/DH/BH register replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg, X86Reg};

/// Documented legacy Group 2 operation selected by a byte-validated
/// AH/CH/DH/BH register encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86LegacyHighByteGroup2Kind {
    Rol,
    Ror,
    Rcl,
    Rcr,
    Shl,
    Shr,
    Sar,
}

/// Replay metadata for one documented register-only legacy Group 2 operation.
/// `raw_count == None` denotes the CL-count form; otherwise it is the complete
/// encoded count before the architectural 5-bit mask is applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyHighByteGroup2Replay {
    pub(crate) kind: X86LegacyHighByteGroup2Kind,
    /// Architectural parent GPR index: RAX=0, RCX=1, RDX=2, RBX=3.
    pub(crate) parent: u8,
    pub(crate) raw_count: Option<u8>,
    /// Prefix-free encoding used for native replay. Every admitted legacy
    /// prefix is semantically inert for a register-only byte Group 2 form.
    pub(crate) canonical_instruction: X86InstructionBytes,
}

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

/// Validate the exact extract-plus-implicit-IMUL graph emitted for an
/// AH/CH/DH/BH source and return its virtual extract temporary. The caller
/// must additionally prove that the temporary has exactly this definition and
/// use across the complete block.
pub(crate) fn x86_legacy_high_byte_imul_shape_temporary(
    ops: &[SmirOp],
    parent: u8,
) -> Option<VReg> {
    let [extract, multiply] = ops else {
        return None;
    };
    let temporary = match &extract.kind {
        OpKind::Shr {
            dst: temporary @ VReg::Virtual(_),
            src: VReg::Arch(ArchReg::X86(source)),
            amount: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if extract.x86_hint.is_none() && source.gpr_index() == Some(parent) => *temporary,
        _ => return None,
    };
    (multiply.x86_hint.is_none()
        && matches!(
            &multiply.kind,
            OpKind::MulS {
                dst_lo: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                dst_hi: None,
                src1: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src2: SrcOperand::Reg(source),
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            } if *source == temporary
        ))
    .then_some(temporary)
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
    /// The admitted set contains MOV (including the B4-B7 immediate forms),
    /// binary ALU, TEST, XCHG, Group 1 immediate, NOT, NEG, INC, DEC, SETcc,
    /// CMPXCHG, XADD, signed implicit IMUL, and documented Group 2 rotate/shift
    /// register forms. LOCK, REX, memory, undocumented Group 2 `/6`, Group 3
    /// `/1`, unsigned multiply, and divide forms fail closed.
    /// Group 2 replay uses a deterministic status wrapper because RAX
    /// preserves architecturally undefined AF/OF while the host instruction
    /// may change them. At most one legacy prefix from each prefix group is
    /// accepted; none changes an 8-bit register operand.
    pub fn is_legacy_high_byte_register_replay(&self) -> bool {
        if self.legacy_high_byte_group2_replay().is_some()
            || self.legacy_high_byte_imul_parent_index().is_some()
        {
            return true;
        }
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
            [0xB4..=0xB7, _] => true,
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

    /// Return the AH/CH/DH/BH parent GPR selected by a byte-validated implicit
    /// signed multiply (`F6 /5`).
    pub(crate) fn legacy_high_byte_imul_parent_index(&self) -> Option<u8> {
        let bytes = self.as_slice();
        let start = legacy_prefix_len(bytes)?;
        match &bytes[start..] {
            [0xF6, modrm] if modrm >> 6 == 3 && (modrm >> 3) & 7 == 5 && modrm & 7 >= 4 => {
                Some((modrm & 7) - 4)
            }
            _ => None,
        }
    }

    /// Return the prefix-free native encoding for an admitted high-byte IMUL.
    /// Legacy size, repeat, segment, and address-size prefixes do not alter a
    /// register-only byte operation; removing them also avoids host translators
    /// that reject an otherwise architecturally equivalent redundant prefix.
    pub(crate) fn legacy_high_byte_imul_replay_instruction(&self) -> Option<Self> {
        self.legacy_high_byte_imul_parent_index()?;
        let bytes = self.as_slice();
        let start = legacy_prefix_len(bytes)?;
        X86InstructionBytes::new(&bytes[start..])
    }

    /// Decode one documented register-only legacy Group 2 operation whose
    /// destination is AH, CH, DH, or BH. The undocumented `/6` alias remains
    /// rejected because its deterministic undefined-AF contract differs from
    /// the architectural `/4` SHL/SAL encoding.
    pub(crate) fn legacy_high_byte_group2_replay(&self) -> Option<X86LegacyHighByteGroup2Replay> {
        let bytes = self.as_slice();
        let start = legacy_prefix_len(bytes)?;
        let (modrm, raw_count) = match &bytes[start..] {
            [0xC0, modrm, immediate] => (*modrm, Some(*immediate)),
            [0xD0, modrm] => (*modrm, Some(1)),
            [0xD2, modrm] => (*modrm, None),
            _ => return None,
        };
        if modrm >> 6 != 3 || modrm & 7 < 4 {
            return None;
        }
        let kind = match (modrm >> 3) & 7 {
            0 => X86LegacyHighByteGroup2Kind::Rol,
            1 => X86LegacyHighByteGroup2Kind::Ror,
            2 => X86LegacyHighByteGroup2Kind::Rcl,
            3 => X86LegacyHighByteGroup2Kind::Rcr,
            4 => X86LegacyHighByteGroup2Kind::Shl,
            5 => X86LegacyHighByteGroup2Kind::Shr,
            7 => X86LegacyHighByteGroup2Kind::Sar,
            _ => return None,
        };
        Some(X86LegacyHighByteGroup2Replay {
            kind,
            parent: (modrm & 7) - 4,
            raw_count,
            canonical_instruction: X86InstructionBytes::new(&bytes[start..])?,
        })
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

            for opcode in 0xB0u8..=0xB7 {
                for immediate in u8::MIN..=u8::MAX {
                    let mut bytes = prefix.to_vec();
                    bytes.extend([opcode, immediate]);
                    let expected = opcode >= 0xB4;
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

            for (opcode, valid_extensions, has_immediate) in [
                (0xC6, 0b0000_0001u8, true),
                (0xF6, 0b0000_0001u8, true),
                (0xF6, 0b0010_1100u8, false),
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

            for opcode in [0xC0, 0xD0, 0xD2] {
                for extension in 0u8..8 {
                    for rm in 0u8..8 {
                        let mut bytes = prefix.to_vec();
                        bytes.extend([opcode, 0xC0 | (extension << 3) | rm]);
                        if opcode == 0xC0 {
                            bytes.push(0xA5);
                        }
                        let expected = extension != 6 && rm >= 4;
                        let instruction = X86InstructionBytes::new(&bytes).unwrap();
                        assert_eq!(
                            instruction.is_legacy_high_byte_register_replay(),
                            expected,
                            "{bytes:02X?}"
                        );
                        assert_eq!(
                            instruction.legacy_high_byte_group2_replay().is_some(),
                            expected,
                            "{bytes:02X?}"
                        );
                        accepted += usize::from(expected);
                    }
                }
            }
        }
        assert_eq!(accepted, 13_728);
    }

    #[test]
    fn classifier_covers_immediate_unary_and_map0f_families() {
        for bytes in [
            &[0x80, 0xC4, 0x81][..],                   // add ah,0x81
            &[0xC6, 0xC7, 0x5A][..],                   // mov bh,0x5a
            &[0xB4, 0xA5][..],                         // mov ah,0xa5
            &[0x65, 0x66, 0x67, 0xF3, 0xB7, 0x5A][..], // mov bh,0x5a
            &[0xF6, 0xC5, 0xA5][..],                   // test ch,0xa5
            &[0xF6, 0xD6][..],                         // not dh
            &[0xF6, 0xDF][..],                         // neg bh
            &[0xF6, 0xEC][..],                         // imul ah
            &[0x0F, 0x96, 0xC4][..],                   // setbe ah
            &[0x0F, 0xB0, 0xF5][..],                   // cmpxchg ch,dh
            &[0x0F, 0xC0, 0xFC][..],                   // xadd ah,bh
            &[0xC0, 0xC4, 0x00][..],                   // rol ah,0
            &[0xD0, 0xD5][..],                         // rcl ch,1
            &[0xD2, 0xDF][..],                         // rcr bh,cl
            &[0xC0, 0xE6, 0x08][..],                   // shl dh,8
            &[0xD2, 0xEF][..],                         // shr bh,cl
            &[0xC0, 0xFC, 0x1F][..],                   // sar ah,31
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
    fn classifier_reports_exact_group2_operation_parent_and_count_source() {
        for (bytes, kind, parent, raw_count, canonical_instruction) in [
            (
                &[0xC0, 0xC4, 0xA5][..],
                X86LegacyHighByteGroup2Kind::Rol,
                0,
                Some(0xA5),
                &[0xC0, 0xC4, 0xA5][..],
            ),
            (
                &[0xD0, 0xCD][..],
                X86LegacyHighByteGroup2Kind::Ror,
                1,
                Some(1),
                &[0xD0, 0xCD][..],
            ),
            (
                &[0xD2, 0xD6][..],
                X86LegacyHighByteGroup2Kind::Rcl,
                2,
                None,
                &[0xD2, 0xD6][..],
            ),
            (
                &[0xD2, 0xDF][..],
                X86LegacyHighByteGroup2Kind::Rcr,
                3,
                None,
                &[0xD2, 0xDF][..],
            ),
            (
                &[0x65, 0x66, 0x67, 0xF3, 0xC0, 0xE4, 0x08][..],
                X86LegacyHighByteGroup2Kind::Shl,
                0,
                Some(8),
                &[0xC0, 0xE4, 0x08][..],
            ),
            (
                &[0xC0, 0xED, 0x09][..],
                X86LegacyHighByteGroup2Kind::Shr,
                1,
                Some(9),
                &[0xC0, 0xED, 0x09][..],
            ),
            (
                &[0xC0, 0xFE, 0x1F][..],
                X86LegacyHighByteGroup2Kind::Sar,
                2,
                Some(31),
                &[0xC0, 0xFE, 0x1F][..],
            ),
        ] {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .legacy_high_byte_group2_replay(),
                Some(X86LegacyHighByteGroup2Replay {
                    kind,
                    parent,
                    raw_count,
                    canonical_instruction: X86InstructionBytes::new(canonical_instruction).unwrap(),
                }),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn classifier_reports_exact_high_byte_imul_parent() {
        for (bytes, parent) in [
            (&[0xF6, 0xEC][..], 0),
            (&[0x66, 0xF6, 0xED][..], 1),
            (&[0xF2, 0xF6, 0xEE][..], 2),
            (&[0x65, 0x67, 0xF3, 0xF6, 0xEF][..], 3),
        ] {
            let instruction = X86InstructionBytes::new(bytes).unwrap();
            assert_eq!(
                instruction.legacy_high_byte_imul_parent_index(),
                Some(parent),
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction
                    .legacy_high_byte_imul_replay_instruction()
                    .unwrap()
                    .as_slice(),
                &[0xF6, 0xEC + parent],
                "{bytes:02X?}"
            );
            assert!(instruction.is_legacy_high_byte_register_replay());
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
            &[0xC0, 0xF4, 0x03][..],       // undocumented Group 2 /6 alias
            &[0xD0, 0xF5][..],             // undocumented Group 2 /6 alias
            &[0xD2, 0xF6][..],             // undocumented Group 2 /6 alias
            &[0xC0, 0x04, 0x03][..],       // Group 2 memory form
            &[0x40, 0xC0, 0xC4, 0x03][..], // REX selects SPL, not AH
            &[0xF6, 0xCC, 0x01][..],       // Group 3 /1 compatibility alias
            &[0xF6, 0xE4][..],             // unsigned MUL remains unadmitted
            &[0xF6, 0xF4][..],             // div ah can raise #DE
            &[0xF6, 0xFC][..],             // idiv ah can raise #DE
            &[0xC6, 0xCC, 0x01][..],       // MOV requires /0
            &[0xB0, 0x01][..],             // MOV AL,imm8 needs no replay
            &[0x40, 0xB4, 0x01][..],       // REX selects SPL, not AH
            &[0xF0, 0xB4, 0x01][..],       // LOCK is #UD
            &[0xB4, 0x01, 0x00][..],       // trailing byte
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
