//! Deterministic native replay for legacy register instructions carrying an
//! address-size or segment-override prefix.
//!
//! Intel SDM Order No. 325383-092US (June 2026), Vol. 2A, Section 2.1.1,
//! classifies those prefixes without a memory operand as reserved and
//! potentially unpredictable. RAX defines decoder-accepted register images by
//! their canonical unprefixed SMIR semantics, so the x86 host must emit the
//! canonical instruction rather than execute the reserved source byte image.

use super::*;
use crate::smir::ir::types::{FunctionId, SourceArch};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0x6764_6500;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const SCANNER_PREFIXES: [&[u8]; 3] = [&[0x64], &[0x65], &[0x67]];
const COMPLETE_PREFIXES: [&[u8]; 19] = [
    &[0x26],
    &[0x2E],
    &[0x36],
    &[0x3E],
    &[0x64],
    &[0x65],
    &[0x67],
    &[0x26, 0x67],
    &[0x67, 0x26],
    &[0x2E, 0x67],
    &[0x67, 0x2E],
    &[0x36, 0x67],
    &[0x67, 0x36],
    &[0x3E, 0x67],
    &[0x67, 0x3E],
    &[0x64, 0x67],
    &[0x67, 0x64],
    &[0x65, 0x67],
    &[0x67, 0x65],
];

#[derive(Clone, Copy, Debug)]
struct ScannerFamily {
    opcode: u8,
    immediate: bool,
}

// These 22 no-mandatory-prefix families account for 4,224 of the 4,320 exact
// residual scanner cells closed by this change:
//
// 22 families x 3 prefix images x 64 register ModR/M cells = 4,224 cells.
const SCANNER_FAMILIES: [ScannerFamily; 22] = [
    ScannerFamily {
        opcode: 0x58,
        immediate: false,
    }, // ADDPS
    ScannerFamily {
        opcode: 0x59,
        immediate: false,
    }, // MULPS
    ScannerFamily {
        opcode: 0x5C,
        immediate: false,
    }, // SUBPS
    ScannerFamily {
        opcode: 0x5D,
        immediate: false,
    }, // MINPS
    ScannerFamily {
        opcode: 0x5E,
        immediate: false,
    }, // DIVPS
    ScannerFamily {
        opcode: 0x5F,
        immediate: false,
    }, // MAXPS
    ScannerFamily {
        opcode: 0xC2,
        immediate: true,
    }, // CMPPS
    ScannerFamily {
        opcode: 0x2E,
        immediate: false,
    }, // UCOMISS
    ScannerFamily {
        opcode: 0x2F,
        immediate: false,
    }, // COMISS
    ScannerFamily {
        opcode: 0x2A,
        immediate: false,
    }, // CVTPI2PS
    ScannerFamily {
        opcode: 0x2C,
        immediate: false,
    }, // CVTTPS2PI
    ScannerFamily {
        opcode: 0x2D,
        immediate: false,
    }, // CVTPS2PI
    ScannerFamily {
        opcode: 0x5A,
        immediate: false,
    }, // CVTPS2PD
    ScannerFamily {
        opcode: 0x12,
        immediate: false,
    }, // MOVHLPS
    ScannerFamily {
        opcode: 0x16,
        immediate: false,
    }, // MOVLHPS
    ScannerFamily {
        opcode: 0xF4,
        immediate: false,
    }, // PMULUDQ
    ScannerFamily {
        opcode: 0x52,
        immediate: false,
    }, // RSQRTPS
    ScannerFamily {
        opcode: 0x53,
        immediate: false,
    }, // RCPPS
    ScannerFamily {
        opcode: 0xC6,
        immediate: true,
    }, // SHUFPS
    ScannerFamily {
        opcode: 0x51,
        immediate: false,
    }, // SQRTPS
    ScannerFamily {
        opcode: 0x14,
        immediate: false,
    }, // UNPCKLPS
    ScannerFamily {
        opcode: 0x15,
        immediate: false,
    }, // UNPCKHPS
];

// One representative for each independent legacy replay classifier path.
// Existing per-family tests exhaust canonical opcodes, registers, immediates,
// shape validation, and feature requirements; this matrix composes every path
// with all legal single/pair segment and address prefix arrangements.
const CLASSIFIER_ANCHORS: [&[u8]; 31] = [
    &[0x66, 0x0F, 0x38, 0xDC, 0xCA],       // AESENC
    &[0x66, 0x0F, 0x3A, 0x0C, 0xCA, 0xA5], // BLENDPS
    &[0x0F, 0x2A, 0xCA],                   // CVTPI2PS
    &[0xF3, 0x0F, 0x2A, 0xCA],             // CVTSI2SS
    &[0x0F, 0xC5, 0xE0, 0xA5],             // PEXTRW MMX -> RSP
    &[0x0F, 0xC4, 0xC4, 0xA5],             // PINSRW RSP -> MMX
    &[0xF2, 0x0F, 0x12, 0xCA],             // MOVDDUP
    &[0x66, 0x0F, 0x3A, 0x0F, 0xCA, 0xA5], // PALIGNR
    &[0x66, 0x0F, 0x38, 0xCF, 0xCA],       // GF2P8MULB
    &[0x66, 0x0F, 0x3A, 0x08, 0xCA, 0xA5], // ROUNDPS
    &[0x66, 0x0F, 0x3A, 0x40, 0xCA, 0xA5], // DPPS
    &[0x66, 0x0F, 0x3A, 0x21, 0xCA, 0xA5], // INSERTPS
    &[0x66, 0x0F, 0x3A, 0x44, 0xCA, 0xA5], // PCLMULQDQ
    &[0x66, 0x0F, 0x38, 0x17, 0xCA],       // PTEST
    &[0x66, 0x0F, 0x38, 0x20, 0xCA],       // PMOVSXBW
    &[0x66, 0x0F, 0x71, 0xF2, 0x01],       // PSLLW imm8
    &[0x66, 0x0F, 0x38, 0x28, 0xCA],       // PMULDQ
    &[0x66, 0x0F, 0x2F, 0xCA],             // COMISD
    &[0x0F, 0x38, 0xC9, 0xCA],             // SHA1MSG1
    &[0x88, 0xE4],                         // MOV AH, AH
    &[0xC0, 0xE4, 0x01],                   // SHL AH, 1
    &[0xF6, 0xE4],                         // MUL AH
    &[0x0F, 0x94, 0xE4],                   // SETE AH
    &[0xF2, 0x0F, 0x38, 0xF0, 0xC4],       // CRC32 EAX, AH
    &[0xF2, 0x0F, 0x58, 0xCA],             // ADDSD
    &[0xF3, 0x0F, 0x53, 0xCA],             // RCPSS
    &[0x66, 0x0F, 0xC2, 0xCA, 0x07],       // CMPPD
    &[0x66, 0x0F, 0xC6, 0xCA, 0xA5],       // SHUFPD
    &[0xF2, 0x0F, 0x7C, 0xCA],             // HADDPS
    &[0x0F, 0x12, 0xCA],                   // MOVHLPS
    &[0xF3, 0x0F, 0x10, 0xCA],             // MOVSS
];

fn prefixed(prefix: &[u8], canonical: &[u8]) -> Vec<u8> {
    prefix.iter().chain(canonical).copied().collect()
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
        X86InstructionBytes::new(bytes).expect("complete x86 instruction"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn semantic_graph(function: &SmirFunction) -> Vec<String> {
    function.blocks[0]
        .ops
        .iter()
        .map(|op| format!("{:?}|{:?}", op.x86_hint, op.kind))
        .collect()
}

fn assert_canonical_replay(source: &[u8], canonical: &[u8], level: OptLevel) -> Option<bool> {
    let source_function = function(source, level);
    let canonical_function = function(canonical, level);
    assert_eq!(
        semantic_graph(&source_function),
        semantic_graph(&canonical_function),
        "{level:?} source={source:02X?} canonical={canonical:02X?}"
    );

    let spans = crate::smir::ir::x86_native_replay_spans(
        &source_function.blocks[0],
        &source_function.x86_instruction_bytes,
    );
    let span = spans
        .values()
        .next()
        .unwrap_or_else(|| panic!("{level:?} source={source:02X?}"));
    assert_eq!(
        span.instruction.as_slice(),
        canonical,
        "{level:?} source={source:02X?}"
    );
    assert!(
        is_native_clobber_safe(&source_function),
        "{level:?} source={source:02X?}"
    );
    let requirements =
        x86_native_replay_feature_requirements(&source_function, &std::collections::HashMap::new());

    let mut lowerer = X86_64Lowerer::new();
    lowerer
        .lower_function(&source_function)
        .unwrap_or_else(|error| panic!("{level:?} source={source:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} source={source:02X?}: {error:?}"));
    assert!(
        !code.windows(source.len()).any(|window| window == source),
        "reserved source bytes were emitted: {level:?} source={source:02X?}"
    );
    requirements
        .any
        .then_some(requirements.all_spans_support_avx_ymm16)
}

#[test]
fn all_8_640_o0_o2_ci_visible_gap_cases_admit_and_lower_canonical_replay() {
    let mut cases = 0usize;
    for family in SCANNER_FAMILIES {
        for prefix in SCANNER_PREFIXES {
            for modrm in 0xC0..=0xFF {
                let mut canonical = vec![0x0F, family.opcode, modrm];
                if family.immediate {
                    canonical.push(0);
                }
                let source = prefixed(prefix, &canonical);
                for level in [OptLevel::O0, OptLevel::O2] {
                    let expected_boundary = match family.opcode {
                        0xF4 => None,        // MMX-only state bridge
                        0xC2 => Some(false), // full AVX-512 vector-state bridge
                        _ => Some(true),     // AVX YMM0-YMM15 vector-state bridge
                    };
                    assert_eq!(
                        assert_canonical_replay(&source, &canonical, level),
                        expected_boundary,
                        "{level:?} source={source:02X?}"
                    );
                    cases += 1;
                }
            }
        }
    }

    // The remaining 96 scanner cells are PEXTRW destinations or PINSRW
    // sources mapped through guest RSP/RBP state slots:
    // 2 instructions x 3 prefixes x 16 stack-register cells = 96 cells.
    for (opcode, stack_in_reg_field) in [(0xC5, true), (0xC4, false)] {
        for prefix in SCANNER_PREFIXES {
            for first in 0u8..8 {
                for stack in [4u8, 5] {
                    let fields = if stack_in_reg_field {
                        (stack << 3) | first
                    } else {
                        (first << 3) | stack
                    };
                    let canonical = [0x0F, opcode, 0xC0 | fields, 0];
                    let source = prefixed(prefix, &canonical);
                    for level in [OptLevel::O0, OptLevel::O2] {
                        assert_eq!(
                            assert_canonical_replay(&source, &canonical, level),
                            None,
                            "{level:?} source={source:02X?}"
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 2 * (22 * 3 * 64 + 2 * 3 * 16));
}

#[test]
fn all_1_767_classifier_prefix_optimizer_compositions_are_canonical() {
    let mut cases = 0usize;
    for canonical in CLASSIFIER_ANCHORS {
        for prefix in COMPLETE_PREFIXES {
            let source = prefixed(prefix, canonical);
            for level in LEVELS {
                let _ = assert_canonical_replay(&source, canonical, level);
                cases += 1;
            }
        }
    }
    assert_eq!(
        cases,
        CLASSIFIER_ANCHORS.len() * COMPLETE_PREFIXES.len() * LEVELS.len()
    );
}

#[test]
fn memory_vector_lead_duplicate_and_nonfinal_prefix_frontiers_fail_closed() {
    for bytes in [
        &[0x67, 0x0F, 0x58, 0x00][..],
        &[0x64, 0x66, 0x0F, 0x58, 0x00][..],
        &[0x64, 0xC5, 0xF8, 0x58, 0xC0][..],
        &[0x67, 0x62, 0xF1, 0x7C, 0x08, 0x58, 0xC0][..],
        &[0x64, 0x65, 0x0F, 0x58, 0xC0][..],
        &[0x67, 0x67, 0x0F, 0x58, 0xC0][..],
        &[0x40, 0x67, 0x0F, 0x58, 0xC0][..],
    ] {
        let function = function(bytes, OptLevel::O2);
        assert!(
            crate::smir::ir::x86_native_replay_spans(
                &function.blocks[0],
                &function.x86_instruction_bytes,
            )
            .is_empty(),
            "{bytes:02X?}"
        );
        assert!(!is_native_clobber_safe(&function), "{bytes:02X?}");
    }
}
