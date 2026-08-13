//! Exact legacy MMX/SSE scalar-extract replay classification and graph validation.

use super::*;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{FunctionId, SourceArch};
use crate::smir::ir::{SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xE3E0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    ExtractPs,
    PextrB,
    PextrD,
    PextrQ,
    PextrWMap1Mmx,
    PextrWMap1Xmm,
    PextrWMap3,
}

impl Family {
    const ALL: [Self; 7] = [
        Self::ExtractPs,
        Self::PextrB,
        Self::PextrD,
        Self::PextrQ,
        Self::PextrWMap1Mmx,
        Self::PextrWMap1Xmm,
        Self::PextrWMap3,
    ];

    fn kind(self) -> X86LegacyScalarExtractKind {
        match self {
            Self::ExtractPs => X86LegacyScalarExtractKind::ExtractPs,
            Self::PextrB => X86LegacyScalarExtractKind::PextrB,
            Self::PextrD => X86LegacyScalarExtractKind::PextrD,
            Self::PextrQ => X86LegacyScalarExtractKind::PextrQ,
            Self::PextrWMap1Mmx => X86LegacyScalarExtractKind::PextrWMap1Mmx,
            Self::PextrWMap1Xmm => X86LegacyScalarExtractKind::PextrWMap1Xmm,
            Self::PextrWMap3 => X86LegacyScalarExtractKind::PextrWMap3,
        }
    }

    fn map1(self) -> bool {
        matches!(self, Self::PextrWMap1Mmx | Self::PextrWMap1Xmm)
    }

    fn mmx(self) -> bool {
        self == Self::PextrWMap1Mmx
    }

    fn lane_mask(self) -> u8 {
        match self {
            Self::PextrB => 0x0F,
            Self::PextrWMap1Mmx => 0x03,
            Self::PextrWMap1Xmm | Self::PextrWMap3 => 0x07,
            Self::ExtractPs | Self::PextrD => 0x03,
            Self::PextrQ => 0x01,
        }
    }

    fn rex_images(self) -> Vec<Option<u8>> {
        match self {
            Self::PextrD => [None].into_iter().chain((0x40..=0x47).map(Some)).collect(),
            Self::PextrQ => (0x48..=0x4F).map(Some).collect(),
            _ => [None].into_iter().chain((0x40..=0x4F).map(Some)).collect(),
        }
    }
}

fn encoding(family: Family, rex: Option<u8>, modrm: u8, immediate: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = Vec::new();
    if !family.mmx() {
        bytes.push(0x66);
    }
    bytes.extend(rex);
    if family.map1() {
        bytes.extend([0x0F, 0xC5, modrm, immediate]);
    } else {
        let opcode = match family {
            Family::PextrB => 0x14,
            Family::PextrWMap3 => 0x15,
            Family::PextrD | Family::PextrQ => 0x16,
            Family::ExtractPs => 0x17,
            Family::PextrWMap1Mmx | Family::PextrWMap1Xmm => unreachable!(),
        };
        bytes.extend([0x0F, 0x3A, opcode, modrm, immediate]);
    }
    bytes
}

fn expected(
    family: Family,
    rex: Option<u8>,
    modrm: u8,
    immediate: u8,
) -> X86LegacyScalarExtractReplay {
    let rex = rex.unwrap_or(0);
    let reg = (modrm >> 3) & 7;
    let rm = modrm & 7;
    let rex_r = (rex & 0x04) << 1;
    let rex_b = (rex & 0x01) << 3;
    let (destination, source) = if family.map1() {
        (reg | rex_r, if family.mmx() { rm } else { rm | rex_b })
    } else {
        (rm | rex_b, reg | rex_r)
    };
    X86LegacyScalarExtractReplay {
        kind: family.kind(),
        destination,
        source,
        lane: immediate & family.lane_mask(),
    }
}

#[test]
fn classifier_covers_all_1671168_canonical_rex_register_and_immediate_images() {
    let mut classified = 0usize;
    for family in Family::ALL {
        for rex in family.rex_images() {
            for modrm in 0xC0..=0xFF {
                for immediate in u8::MIN..=u8::MAX {
                    let bytes = encoding(family, rex, modrm, immediate);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_register_scalar_extract_replay(),
                        Some(expected(family, rex, modrm, immediate)),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    let rex_images = 5 * 17 + 9 + 8;
    assert_eq!(classified, rex_images * 64 * 256);
}

#[test]
fn classifier_partitions_rex_w_and_rejects_every_noncanonical_frontier() {
    for rex in 0x40..=0x4F {
        let family = if rex & 0x08 == 0 {
            Family::PextrD
        } else {
            Family::PextrQ
        };
        let bytes = encoding(family, Some(rex), 0xCA, 0xA5);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_register_scalar_extract_replay(),
            Some(expected(family, Some(rex), 0xCA, 0xA5)),
            "{bytes:02X?}"
        );
    }

    // Intel SDM 325383-092, Vol. 2A, classifies address-size and segment
    // prefixes on register-only MMX forms as reserved/unpredictable. This raw
    // family classifier therefore rejects them. The replay grouper may strip
    // one such prefix only after independently proving an exact non-memory
    // semantic group, and then emits the canonical unprefixed instruction.
    let invalid: &[&[u8]] = &[
        &[0x67, 0x0F, 0xC5, 0xCA, 0x03],
        &[0x64, 0x0F, 0xC5, 0xCA, 0x03],
        &[0x65, 0x0F, 0xC5, 0xCA, 0x03],
        &[0x67, 0x66, 0x0F, 0xC5, 0xCA, 0x07],
        &[0x64, 0x66, 0x0F, 0x3A, 0x17, 0xCA, 0x03],
        &[0xF0, 0x66, 0x0F, 0x3A, 0x14, 0xCA, 0x0F],
        &[0xF2, 0x66, 0x0F, 0x3A, 0x15, 0xCA, 0x07],
        &[0x48, 0x66, 0x0F, 0x3A, 0x16, 0xCA, 0x01],
        &[0x66, 0x48, 0x49, 0x0F, 0x3A, 0x16, 0xCA, 0x01],
        &[0x66, 0x48, 0x67, 0x0F, 0x3A, 0x17, 0xCA, 0x03],
        &[0x66, 0xD5, 0x00, 0x0F, 0x3A, 0x14, 0xCA, 0x0F],
        &[0x0F, 0x3A, 0x14, 0xCA, 0x0F],
        &[0x66, 0x0F, 0x3A, 0x13, 0xCA, 0x00],
        &[0x66, 0x0F, 0x3A, 0x18, 0xCA, 0x00],
        &[0x66, 0x0F, 0xC5, 0x0A, 0x07],
        &[0x0F, 0xC5, 0x0A, 0x03],
        &[0x66, 0x0F, 0x3A, 0x14, 0x0A, 0x0F],
        &[0x66, 0x0F, 0xC5, 0xCA],
        &[0x66, 0x0F, 0x3A, 0x17, 0xCA],
        &[0x66, 0x0F, 0xC5, 0xCA, 0x07, 0x00],
        &[0x66, 0x0F, 0x3A, 0x17, 0xCA, 0x03, 0x00],
        &[0xC5, 0xF9, 0xC5, 0xCA, 0x07],
        &[0x62, 0xF3, 0x7D, 0x08, 0x17, 0xCA, 0x03],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_scalar_extract_replay(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn state_backed_rewrite_preserves_kind_source_lane_and_rex_w_for_every_field_image() {
    let mut rewritten = 0usize;
    for family in Family::ALL {
        for rex in family.rex_images() {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(family, rex, modrm, 0xA5);
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                let original = instruction.legacy_register_scalar_extract_replay().unwrap();
                let actual = instruction
                    .legacy_scalar_extract_with_destination_rax()
                    .unwrap()
                    .legacy_register_scalar_extract_replay()
                    .unwrap();
                assert_eq!(actual.kind, original.kind, "{bytes:02X?}");
                assert_eq!(actual.destination, 0, "{bytes:02X?}");
                assert_eq!(actual.source, original.source, "{bytes:02X?}");
                assert_eq!(actual.lane, original.lane, "{bytes:02X?}");
                rewritten += 1;
            }
        }
    }
    assert_eq!(rewritten, (5 * 17 + 9 + 8) * 64);
}

fn function(bytes: &[u8], level: OptLevel) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("legacy scalar-extract provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_span(function: &SmirFunction, bytes: &[u8], family: Family) {
    assert_eq!(function.blocks[0].ops.len(), 2, "{bytes:02X?}");
    let start = usize::from(family.mmx());
    for spans in [
        x86_legacy_scalar_extract_replay_spans(
            &function.blocks[0],
            &function.x86_instruction_bytes,
        ),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans.get(&start).unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(span.end, 2, "{bytes:02X?}");
        assert_eq!(span.instruction.as_slice(), bytes, "{bytes:02X?}");
        assert!(!span.needs_avx512vl, "{bytes:02X?}");
        assert!(!span.needs_avx512dq, "{bytes:02X?}");
        assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        assert!(!span.preserve_mxcsr_de, "{bytes:02X?}");
    }
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert!(
        x86_legacy_scalar_extract_replay_spans(
            &function.blocks[0],
            &function.x86_instruction_bytes,
        )
        .is_empty(),
        "dedicated span admitted {label}"
    );
    assert!(
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes).is_empty(),
        "aggregate span admitted {label}"
    );
}

#[test]
fn exact_graph_validator_covers_all_19584_rex_register_and_level_images() {
    let mut validated = 0usize;
    for family in Family::ALL {
        for rex in family.rex_images() {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(family, rex, modrm, 0xA5);
                for level in LEVELS {
                    assert_span(&function(&bytes, level), &bytes, family);
                    validated += 1;
                }
            }
        }
    }
    assert_eq!(validated, (5 * 17 + 9 + 8) * 64 * LEVELS.len());
}

#[test]
fn exact_graph_validator_covers_every_masked_immediate_at_o0_o1_o2() {
    let mut validated = 0usize;
    for family in Family::ALL {
        for immediate in u8::MIN..=u8::MAX {
            let rex = if family == Family::PextrQ {
                Some(0x48)
            } else {
                Some(0x40)
            };
            let bytes = encoding(family, rex, 0xCA, immediate);
            for level in LEVELS {
                assert_span(&function(&bytes, level), &bytes, family);
                validated += 1;
            }
        }
    }
    assert_eq!(validated, Family::ALL.len() * 256 * LEVELS.len());
}

#[test]
fn graph_validator_rejects_every_operation_hint_kind_and_virtual_escape() {
    for family in Family::ALL {
        let rex = if family == Family::PextrQ {
            Some(0x4D)
        } else {
            Some(0x45)
        };
        let bytes = encoding(family, rex, 0xCA, 0xA5);
        for level in LEVELS {
            let baseline = function(&bytes, level);
            for operation_index in 0..baseline.blocks[0].ops.len() {
                let mut wrong_kind = baseline.clone();
                wrong_kind.blocks[0].ops[operation_index].kind = OpKind::Nop;
                assert_rejected(
                    &wrong_kind,
                    &format!("{family:?} {level:?} op {operation_index}"),
                );

                let mut wrong_hint = baseline.clone();
                wrong_hint.blocks[0].ops[operation_index].x86_hint = Some(X86OpHint::RexByteReg);
                assert_rejected(
                    &wrong_hint,
                    &format!("{family:?} {level:?} hint {operation_index}"),
                );
            }

            if !family.mmx() {
                let temporary = baseline.blocks[0]
                    .ops
                    .iter()
                    .flat_map(|op| op.kind.dests())
                    .find(|register| matches!(register, VReg::Virtual(_)))
                    .expect("legacy scalar-extract temporary");
                let mut escaped = baseline.clone();
                escaped.blocks[0].set_terminator(Terminator::Return {
                    values: vec![temporary],
                });
                assert_rejected(&escaped, &format!("{family:?} {level:?} escaped"));
            }
        }
    }
}

#[test]
fn graph_validator_rejects_mismatched_memory_and_canonicalizes_non_memory_prefixes() {
    for (index, family) in Family::ALL.into_iter().enumerate() {
        let rex = if family == Family::PextrQ {
            Some(0x48)
        } else {
            Some(0x40)
        };
        let bytes = encoding(family, rex, 0xCA, 0xA5);
        let baseline = function(&bytes, OptLevel::O0);

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert_rejected(&missing, &format!("{family:?} missing"));

        let mismatch_family = Family::ALL[(index + 1) % Family::ALL.len()];
        let mismatch_rex = if mismatch_family == Family::PextrQ {
            Some(0x48)
        } else {
            Some(0x40)
        };
        for (label, metadata) in [
            (
                "family",
                encoding(mismatch_family, mismatch_rex, 0xCA, 0xA5),
            ),
            ("destination/source", encoding(family, rex, 0xD3, 0xA5)),
            ("lane", encoding(family, rex, 0xCA, 0xA6)),
            ("memory", encoding(family, rex, 0x0A, 0xA5)),
            ("duplicate non-memory prefix", {
                let mut duplicate = vec![0x67, 0x67];
                duplicate.extend(&bytes);
                duplicate
            }),
        ] {
            let mut malformed = baseline.clone();
            malformed.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                X86InstructionBytes::new(&metadata).unwrap(),
            );
            assert_rejected(&malformed, &format!("{family:?} {label}"));
        }

        let mut prefixed = vec![0x67];
        prefixed.extend(&bytes);
        assert_span(&function(&prefixed, OptLevel::O0), &bytes, family);
    }
}
