//! Shared Intel APX architectural encoding rules.

/// Return the number of opcode-stream bytes that establish a REX2 reservation.
///
/// `map1` is the decoded REX2.M0 bit, `opcode` is the effective opcode in that
/// map, and `following` is the byte immediately after it when available. Most
/// reservations are known from the opcode alone. The XSAVE*/XRSTOR* exceptions
/// additionally require a memory-form ModR/M group selector.
#[inline]
pub(crate) fn rex2_reserved_opcode_len(
    map1: bool,
    opcode: u8,
    following: Option<u8>,
) -> Option<usize> {
    let reserved_opcode = if map1 {
        matches!(opcode & 0xF0, 0x30 | 0x80)
    } else {
        matches!(opcode & 0xF0, 0x40 | 0x70 | 0xE0)
            || opcode & 0xF0 == 0xA0 && opcode != 0xA1
            || matches!(
                opcode,
                0x0F | 0x26 | 0x2E | 0x36 | 0x3E | 0x62 | 0x64
                    ..=0x67 | 0xC4 | 0xC5 | 0xD5 | 0xF0 | 0xF2 | 0xF3
            )
    };
    if reserved_opcode {
        return Some(1);
    }

    if map1 && matches!(opcode, 0xAE | 0xC7) {
        let modrm = following?;
        let memory = modrm >> 6 != 3;
        let group = (modrm >> 3) & 7;
        let xsave_family = memory
            && match opcode {
                0xAE => matches!(group, 4 | 5 | 6),
                0xC7 => matches!(group, 3 | 4 | 5),
                _ => false,
            };
        if xsave_family {
            return Some(2);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rex2_opcode_reservations_cover_exact_map_rows_and_prefix_bytes() {
        for opcode in 0_u8..=u8::MAX {
            let map0_row = matches!(opcode & 0xF0, 0x40 | 0x70 | 0xE0)
                || opcode & 0xF0 == 0xA0 && opcode != 0xA1;
            let prefix_byte = matches!(
                opcode,
                0x0F | 0x26 | 0x2E | 0x36 | 0x3E | 0x62 | 0x64
                    ..=0x67 | 0xC4 | 0xC5 | 0xD5 | 0xF0 | 0xF2 | 0xF3
            );
            assert_eq!(
                rex2_reserved_opcode_len(false, opcode, Some(0xC0)),
                (map0_row || prefix_byte).then_some(1),
                "map 0 opcode {opcode:#04x}"
            );

            let map1_row = matches!(opcode & 0xF0, 0x30 | 0x80);
            assert_eq!(
                rex2_reserved_opcode_len(true, opcode, Some(0xC0)),
                map1_row.then_some(1),
                "map 1 opcode {opcode:#04x}"
            );
        }
    }

    #[test]
    fn rex2_xsave_reservations_require_exact_memory_groups() {
        for opcode in [0xAE, 0xC7] {
            for mod_bits in 0_u8..=3 {
                for group in 0_u8..=7 {
                    let modrm = mod_bits << 6 | group << 3;
                    let expected = mod_bits != 3
                        && match opcode {
                            0xAE => matches!(group, 4 | 5 | 6),
                            0xC7 => matches!(group, 3 | 4 | 5),
                            _ => unreachable!(),
                        };
                    assert_eq!(
                        rex2_reserved_opcode_len(true, opcode, Some(modrm)),
                        expected.then_some(2),
                        "opcode={opcode:#04x}, ModR/M={modrm:#04x}"
                    );
                }
            }
            assert_eq!(rex2_reserved_opcode_len(true, opcode, None), None);
        }
    }
}
