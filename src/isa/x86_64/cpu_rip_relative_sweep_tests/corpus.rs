//! Every opcode of every x86-64 encoding family with a RIP-relative ModR/M
//! operand (`mod=00 rm=101`), for each ModR/M.reg value.
//!
//! Fields that do not change the operand's length or address are held at
//! their simplest legal value: all register-extension bits clear, VEX/EVEX
//! `vvvv` unused (1111b), EVEX `aaa=0`, `z=0`, `b=0`, no segment override.

/// One encoding family; the report groups cases by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Family {
    /// Legacy one-byte, 0F, 0F38, and 0F3A maps, with optional 66/F2/F3 and
    /// REX.W.
    Legacy,
    /// The legacy maps with a 0x67 prefix (32-bit, EIP-relative addressing).
    Addr32,
    /// APX REX2 (D5) over maps 0 and 1.
    Rex2,
    /// Three-byte VEX (C4), maps 1-3.
    Vex,
    /// EVEX (62), maps 1, 2, 3, 5, and 6.
    Evex,
    /// AMD XOP (8F), maps 8, 9, and A.
    Xop,
    /// APX EVEX map 4.
    Map4,
}

pub(super) const FAMILIES: [Family; 7] = [
    Family::Legacy,
    Family::Addr32,
    Family::Rex2,
    Family::Vex,
    Family::Evex,
    Family::Xop,
    Family::Map4,
];

/// An encoding through its ModR/M byte; the disp32 and immediate follow.
#[derive(Clone, Debug)]
pub(super) struct Case {
    pub family: Family,
    pub head: Vec<u8>,
}

impl Case {
    /// The instruction bytes with `disp`, followed by zero bytes for any
    /// immediate. Zero is a legal value for every reserved immediate field.
    pub(super) fn bytes(&self, disp: u32) -> Vec<u8> {
        let mut bytes = self.head.clone();
        bytes.extend_from_slice(&disp.to_le_bytes());
        bytes.extend_from_slice(&[0; 8]);
        bytes
    }

    /// The length the instruction would have without an immediate.
    pub(super) fn disp_end(&self) -> usize {
        self.head.len() + 4
    }

    /// The opcode map (0 = one-byte, 1 = 0F, 2 = 0F38, 3 = 0F3A), opcode,
    /// and operand-size controls of a legacy-map or REX2 case.
    pub(super) fn legacy_form(&self) -> Option<LegacyForm> {
        let head = &self.head;
        let reg = (head.last()? >> 3) & 7;
        match self.family {
            Family::Legacy | Family::Addr32 => {
                let n = head
                    .iter()
                    .take_while(|b| matches!(b, 0x66 | 0xF2 | 0xF3 | 0x67 | 0x48))
                    .count();
                let (map, opcode) = match &head[n..head.len() - 1] {
                    [op] => (0, *op),
                    [0x0F, op] => (1, *op),
                    [0x0F, 0x38, op] => (2, *op),
                    [0x0F, 0x3A, op] => (3, *op),
                    _ => return None,
                };
                Some(LegacyForm {
                    map,
                    opcode,
                    reg,
                    rex_w: head[..n].contains(&0x48),
                    opsize16: head[..n].contains(&0x66),
                })
            }
            Family::Rex2 => Some(LegacyForm {
                map: head[1] >> 7,
                opcode: head[2],
                reg,
                rex_w: head[1] & 0x08 != 0,
                opsize16: false,
            }),
            _ => None,
        }
    }
}

pub(super) struct LegacyForm {
    pub map: u8,
    pub opcode: u8,
    pub reg: u8,
    pub rex_w: bool,
    pub opsize16: bool,
}

fn modrm(reg: u8) -> u8 {
    (reg << 3) | 0b101
}

/// One-byte opcodes that are prefixes or escapes in 64-bit mode, and the
/// `MOV` moffs forms (A0-A3), whose absolute address replaces the ModR/M.
fn one_byte_is_skipped(op: u8) -> bool {
    matches!(
        op,
        0x0F | 0x26
            | 0x2E
            | 0x36
            | 0x3E
            | 0x40..=0x4F
            | 0x62
            | 0x64..=0x67
            | 0xA0..=0xA3
            | 0xC4
            | 0xC5
            | 0xD5
            | 0xF0
            | 0xF2
            | 0xF3
    )
}

/// The legacy opcode maps: escape bytes and the opcodes to skip.
const LEGACY_MAPS: [&[u8]; 4] = [&[], &[0x0F], &[0x0F, 0x38], &[0x0F, 0x3A]];

fn legacy_opcode_is_escape(map: &[u8], op: u8) -> bool {
    match map {
        [] => one_byte_is_skipped(op),
        [0x0F] => matches!(op, 0x38 | 0x3A),
        _ => false,
    }
}

fn legacy(cases: &mut Vec<Case>, family: Family, prefixes: &[&[u8]], rexes: &[Option<u8>]) {
    for prefix in prefixes {
        for rex in rexes {
            for map in LEGACY_MAPS {
                for op in 0..=0xFFu8 {
                    if legacy_opcode_is_escape(map, op) {
                        continue;
                    }
                    for reg in 0..8 {
                        let mut head = prefix.to_vec();
                        head.extend(rex);
                        head.extend_from_slice(map);
                        head.extend_from_slice(&[op, modrm(reg)]);
                        cases.push(Case { family, head });
                    }
                }
            }
        }
    }
}

pub(super) fn corpus() -> Vec<Case> {
    let mut cases = Vec::new();
    legacy(
        &mut cases,
        Family::Legacy,
        &[&[], &[0x66], &[0xF2], &[0xF3]],
        &[None, Some(0x48)],
    );
    legacy(
        &mut cases,
        Family::Addr32,
        &[&[0x67], &[0x67, 0x66]],
        &[None],
    );

    // REX2: D5 [M R4 X4 B4 W R3 X3 B3]; M selects map 0 or 1.
    for m in [0u8, 1] {
        for w in [0u8, 1] {
            for op in 0..=0xFFu8 {
                if m == 0 && one_byte_is_skipped(op) {
                    continue;
                }
                for reg in 0..8 {
                    cases.push(Case {
                        family: Family::Rex2,
                        head: vec![0xD5, (m << 7) | (w << 3), op, modrm(reg)],
                    });
                }
            }
        }
    }

    // VEX3: C4 [R X B mmmmm] [W vvvv L pp], extension bits inverted.
    for map in 1..=3u8 {
        for w in [0u8, 1] {
            for l in [0u8, 1] {
                for pp in 0..4u8 {
                    for op in 0..=0xFFu8 {
                        for reg in 0..8 {
                            let p1 = (w << 7) | 0x78 | (l << 2) | pp;
                            cases.push(Case {
                                family: Family::Vex,
                                head: vec![0xC4, 0xE0 | map, p1, op, modrm(reg)],
                            });
                        }
                    }
                }
            }
        }
    }

    // EVEX: 62 [R X B R' 0 mmm] [W vvvv 1 pp] [z L'L b V' aaa].
    for map in [1u8, 2, 3, 5, 6] {
        for w in [0u8, 1] {
            for ll in 0..3u8 {
                for pp in 0..4u8 {
                    for op in 0..=0xFFu8 {
                        for reg in 0..8 {
                            let p1 = (w << 7) | 0x7C | pp;
                            let p2 = (ll << 5) | 0x08;
                            cases.push(Case {
                                family: Family::Evex,
                                head: vec![0x62, 0xF0 | map, p1, p2, op, modrm(reg)],
                            });
                        }
                    }
                }
            }
        }
    }

    // XOP: 8F [R X B mmmmm] [W vvvv L pp=00].
    for map in [8u8, 9, 0xA] {
        for w in [0u8, 1] {
            for l in [0u8, 1] {
                for op in 0..=0xFFu8 {
                    for reg in 0..8 {
                        let p1 = (w << 7) | 0x78 | (l << 2);
                        cases.push(Case {
                            family: Family::Xop,
                            head: vec![0x8F, 0xE0 | map, p1, op, modrm(reg)],
                        });
                    }
                }
            }
        }
    }

    // APX map 4: 62 [R3 X3 B3 R4 B4 100] [W vvvv X4 pp] [0 00 ND V4 NF 00].
    for w in [0u8, 1] {
        for pp in [0u8, 1] {
            for nd in [0u8, 1] {
                for nf in [0u8, 1] {
                    for op in 0..=0xFFu8 {
                        for reg in 0..8 {
                            let p1 = (w << 7) | 0x7C | pp;
                            let p2 = (nd << 4) | 0x08 | (nf << 2);
                            cases.push(Case {
                                family: Family::Map4,
                                head: vec![0x62, 0xF4, p1, p2, op, modrm(reg)],
                            });
                        }
                    }
                }
            }
        }
    }
    cases
}
