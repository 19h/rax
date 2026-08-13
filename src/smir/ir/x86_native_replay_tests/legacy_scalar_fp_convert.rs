//! Exact legacy SSE/SSE2 scalar floating-point conversion replay classifiers.

use super::*;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{FunctionId, OpWidth, SourceArch, VecElementType};
use crate::smir::ir::{SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xD8D0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    CvtSi2Ss,
    CvtSi2Sd,
    CvtSs2Si,
    CvtSd2Si,
    CvttSs2Si,
    CvttSd2Si,
    CvtSs2Sd,
    CvtSd2Ss,
}

impl Family {
    const ALL: [Self; 8] = [
        Self::CvtSi2Ss,
        Self::CvtSi2Sd,
        Self::CvtSs2Si,
        Self::CvtSd2Si,
        Self::CvttSs2Si,
        Self::CvttSd2Si,
        Self::CvtSs2Sd,
        Self::CvtSd2Ss,
    ];

    fn element(self) -> VecElementType {
        match self {
            Self::CvtSi2Ss | Self::CvtSs2Si | Self::CvttSs2Si | Self::CvtSs2Sd => {
                VecElementType::F32
            }
            Self::CvtSi2Sd | Self::CvtSd2Si | Self::CvttSd2Si | Self::CvtSd2Ss => {
                VecElementType::F64
            }
        }
    }

    fn prefix(self) -> u8 {
        if self.element() == VecElementType::F32 {
            0xF3
        } else {
            0xF2
        }
    }

    fn opcode(self) -> u8 {
        match self {
            Self::CvtSi2Ss | Self::CvtSi2Sd => 0x2A,
            Self::CvttSs2Si | Self::CvttSd2Si => 0x2C,
            Self::CvtSs2Si | Self::CvtSd2Si => 0x2D,
            Self::CvtSs2Sd | Self::CvtSd2Ss => 0x5A,
        }
    }

    fn expected_kind(self, rex: u8) -> X86LegacyScalarFpConvertKind {
        let int_width = if rex & 0x08 != 0 {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        match self {
            Self::CvtSi2Ss | Self::CvtSi2Sd => X86LegacyScalarFpConvertKind::IntToFp {
                elem: self.element(),
                int_width,
            },
            Self::CvtSs2Si | Self::CvtSd2Si => X86LegacyScalarFpConvertKind::FpToInt {
                elem: self.element(),
                int_width,
                truncate: false,
            },
            Self::CvttSs2Si | Self::CvttSd2Si => X86LegacyScalarFpConvertKind::FpToInt {
                elem: self.element(),
                int_width,
                truncate: true,
            },
            Self::CvtSs2Sd | Self::CvtSd2Ss => X86LegacyScalarFpConvertKind::FpConvert {
                from: self.element(),
                to: if self.element() == VecElementType::F32 {
                    VecElementType::F64
                } else {
                    VecElementType::F32
                },
            },
        }
    }
}

fn encoding(family: Family, rex: Option<u8>, modrm: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![family.prefix()];
    bytes.extend(rex);
    bytes.extend([0x0F, family.opcode(), modrm]);
    bytes
}

fn expected(family: Family, rex: Option<u8>, modrm: u8) -> X86LegacyScalarFpConvertReplay {
    let rex = rex.unwrap_or(0);
    X86LegacyScalarFpConvertReplay {
        kind: family.expected_kind(rex),
        destination: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
        source: (modrm & 7) | ((rex & 0x01) << 3),
    }
}

#[test]
fn classifier_covers_all_8704_canonical_rex_register_encodings() {
    let mut classified = 0usize;
    for family in Family::ALL {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(family, rex, modrm);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_register_scalar_fp_convert_replay(),
                    Some(expected(family, rex, modrm)),
                    "{bytes:02X?}"
                );
                classified += 1;
            }
        }
    }
    assert_eq!(classified, Family::ALL.len() * 17 * 64);
}

#[test]
fn classifier_exhausts_prefix_opcode_modrm_and_canonical_shape_frontiers() {
    for prefix in [0xF2, 0xF3] {
        for opcode in u8::MIN..=u8::MAX {
            for modrm in u8::MIN..=u8::MAX {
                let bytes = [prefix, 0x4F, 0x0F, opcode, modrm];
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_register_scalar_fp_convert_replay()
                        .is_some(),
                    matches!(opcode, 0x2A | 0x2C | 0x2D | 0x5A) && modrm >> 6 == 3,
                    "{bytes:02X?}"
                );
            }
        }
    }

    // Independently decoded with LLVM 23.0.0: REX.R/B extend the two
    // ModR/M operands, W selects the integer width for 2A/2C/2D, and X plus
    // opcode-5A W are ignored.
    for rex in 0x40..=0x4F {
        for family in Family::ALL {
            let bytes = encoding(family, Some(rex), 0xCA);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_scalar_fp_convert_replay(),
                Some(expected(family, Some(rex), 0xCA)),
                "{bytes:02X?}"
            );
        }
    }

    let invalid: &[&[u8]] = &[
        &[0x0F, 0x2A, 0xCA],                   // missing mandatory prefix
        &[0x66, 0x0F, 0x2A, 0xCA],             // packed neighbor
        &[0xF0, 0xF3, 0x0F, 0x2A, 0xCA],       // lock prefix
        &[0x67, 0xF2, 0x0F, 0x2D, 0xCA],       // address-size prefix
        &[0x64, 0xF3, 0x0F, 0x5A, 0xCA],       // segment prefix
        &[0x48, 0xF2, 0x0F, 0x2A, 0xCA],       // REX before mandatory prefix
        &[0xF2, 0x48, 0x49, 0x0F, 0x2A, 0xCA], // duplicate REX
        &[0xF3, 0x48, 0x67, 0x0F, 0x2C, 0xCA], // REX not final
        &[0xF2, 0xD5, 0x00, 0x0F, 0x2D, 0xCA], // REX2
        &[0xF3, 0x0F, 0x2A, 0x0A],             // memory source
        &[0xF2, 0x0F, 0x5A, 0x0A],             // memory source
        &[0xF3, 0x0F, 0x2A],                   // missing ModR/M
        &[0xF2, 0x0F, 0x5A, 0xCA, 0x00],       // trailing byte
        &[0xC5, 0xEA, 0x5A, 0xCA],             // VEX
        &[0x62, 0xF1, 0x6F, 0x08, 0x5A, 0xCA], // EVEX
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_scalar_fp_convert_replay(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn state_backed_rewrites_preserve_width_prefix_source_and_destination_fields() {
    for family in [
        Family::CvtSs2Si,
        Family::CvtSd2Si,
        Family::CvttSs2Si,
        Family::CvttSd2Si,
    ] {
        for rex in 0x40..=0x4F {
            let bytes = encoding(family, Some(rex), 0xEF);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let original = instruction
                .legacy_register_scalar_fp_convert_replay()
                .unwrap();
            let rewritten = instruction
                .legacy_scalar_fp_to_int_with_destination_rax()
                .unwrap();
            let actual = rewritten
                .legacy_register_scalar_fp_convert_replay()
                .unwrap();
            assert_eq!(actual.kind, original.kind, "{bytes:02X?}");
            assert_eq!(actual.source, original.source, "{bytes:02X?}");
            assert_eq!(actual.gpr_destination(), Some(0), "{bytes:02X?}");
        }
    }
    for family in [Family::CvtSi2Ss, Family::CvtSi2Sd] {
        for rex in 0x40..=0x4F {
            let bytes = encoding(family, Some(rex), 0xEF);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let original = instruction
                .legacy_register_scalar_fp_convert_replay()
                .unwrap();
            let rewritten = instruction
                .legacy_scalar_int_to_fp_with_source_rax()
                .unwrap();
            let actual = rewritten
                .legacy_register_scalar_fp_convert_replay()
                .unwrap();
            assert_eq!(actual.kind, original.kind, "{bytes:02X?}");
            assert_eq!(actual.destination, original.destination, "{bytes:02X?}");
            assert_eq!(actual.gpr_source(), Some(0), "{bytes:02X?}");
        }
    }

    for family in [Family::CvtSs2Sd, Family::CvtSd2Ss] {
        let instruction = X86InstructionBytes::new(&encoding(family, Some(0x4F), 0xEF)).unwrap();
        assert!(
            instruction
                .legacy_scalar_fp_to_int_with_destination_rax()
                .is_none()
        );
        assert!(
            instruction
                .legacy_scalar_int_to_fp_with_source_rax()
                .is_none()
        );
    }
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
        X86InstructionBytes::new(bytes).expect("legacy scalar-conversion provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert!(
        x86_legacy_scalar_fp_convert_replay_spans(
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
fn exact_graph_validator_survives_o0_o1_o2_and_fails_closed_on_mutation() {
    for family in Family::ALL {
        let bytes = encoding(family, Some(0x4F), 0xCA);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let function = function(&bytes, level);
            assert_eq!(function.blocks[0].ops.len(), 1, "{level:?} {bytes:02X?}");
            for spans in [
                x86_legacy_scalar_fp_convert_replay_spans(
                    &function.blocks[0],
                    &function.x86_instruction_bytes,
                ),
                x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
            ] {
                let span = spans
                    .get(&0)
                    .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}"));
                assert_eq!(span.end, 1, "{level:?} {bytes:02X?}");
                assert_eq!(span.instruction.as_slice(), bytes, "{level:?} {bytes:02X?}");
                assert!(!span.needs_avx512vl);
                assert!(!span.needs_avx512dq);
                assert!(!span.needs_avx512fp16);
                assert!(!span.preserve_mxcsr_de);
            }
        }

        let baseline = function(&bytes, OptLevel::O0);
        let mut wrong_op = baseline.clone();
        wrong_op.blocks[0].ops[0].kind = OpKind::Nop;
        assert_rejected(&wrong_op, &format!("{family:?} operation"));

        let mut wrong_hint = baseline.clone();
        wrong_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
        assert_rejected(&wrong_hint, &format!("{family:?} hint"));

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert_rejected(&missing, &format!("{family:?} missing provenance"));

        let mut wrong_operands = baseline.clone();
        wrong_operands.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&encoding(family, Some(0x4F), 0xD3)).unwrap(),
        );
        assert_rejected(&wrong_operands, &format!("{family:?} operands"));

        let mut extra = baseline.clone();
        extra.blocks[0].push_op(SmirOp::new(OpId(1), PC, OpKind::Nop));
        assert_rejected(&extra, &format!("{family:?} extra operation"));
    }
}
