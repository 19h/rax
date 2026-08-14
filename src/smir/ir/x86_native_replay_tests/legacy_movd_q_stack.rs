//! Exact source-byte replay classification for legacy MMX/XMM MOVD/MOVQ
//! transfers whose GPR operand is guest RSP or RBP.

use super::*;
use crate::smir::ir::ops::{X86OpHint, X86SsePrefix};
use crate::smir::ir::types::OpWidth;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransferKind {
    GprToMmx,
    MmxToGpr,
    GprToXmm,
    XmmToGpr,
}

impl TransferKind {
    const ALL: [Self; 4] = [
        Self::GprToMmx,
        Self::MmxToGpr,
        Self::GprToXmm,
        Self::XmmToGpr,
    ];

    fn mmx(self) -> bool {
        matches!(self, Self::GprToMmx | Self::MmxToGpr)
    }

    fn vector_destination(self) -> bool {
        matches!(self, Self::GprToMmx | Self::GprToXmm)
    }

    fn opcode(self) -> u8 {
        if self.vector_destination() {
            0x6E
        } else {
            0x7E
        }
    }
}

fn encoding(kind: TransferKind, rex: Option<u8>, gpr: u8, vector: u8) -> Vec<u8> {
    assert!(matches!(gpr, 4 | 5));
    assert!(vector < 8);
    let mut bytes = Vec::with_capacity(5);
    if !kind.mmx() {
        bytes.push(0x66);
    }
    if let Some(rex) = rex {
        bytes.push(rex);
    }
    bytes.extend_from_slice(&[0x0F, kind.opcode(), 0xC0 | (vector << 3) | (gpr & 7)]);
    bytes
}

fn assert_classified(bytes: &[u8], kind: TransferKind, gpr: u8, vector: u8, width: OpWidth) {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    let replay = instruction
        .legacy_movd_q_stack_replay()
        .unwrap_or_else(|| panic!("not classified: {bytes:02X?}"));
    assert_eq!(replay.gpr, gpr, "{bytes:02X?}");
    assert_eq!(replay.vector, vector, "{bytes:02X?}");
    assert_eq!(replay.width, width, "{bytes:02X?}");
    assert_eq!(
        replay.vector_destination,
        kind.vector_destination(),
        "{bytes:02X?}"
    );
    assert_eq!(replay.touches_mmx(), kind.mmx(), "{bytes:02X?}");
    assert_eq!(replay.gpr_is_destination(), !kind.vector_destination());
    assert_eq!(
        replay.hint,
        X86OpHint::SseOp {
            prefix: if kind.mmx() {
                X86SsePrefix::None
            } else {
                X86SsePrefix::OpSize
            },
            opcode: kind.opcode(),
        },
        "{bytes:02X?}"
    );
}

#[test]
fn classifier_covers_all_576_canonical_stack_gpr_encodings() {
    let mut classified = 0usize;
    for kind in TransferKind::ALL {
        for gpr in [4, 5] {
            for vector in 0..8 {
                let bytes = encoding(kind, None, gpr, vector);
                assert_classified(&bytes, kind, gpr, vector, OpWidth::W32);
                classified += 1;
            }
        }
        for rex in [0x40, 0x42, 0x44, 0x46, 0x48, 0x4A, 0x4C, 0x4E] {
            for gpr in [4, 5] {
                for encoded_vector in 0..8 {
                    let bytes = encoding(kind, Some(rex), gpr, encoded_vector);
                    let vector = if kind.mmx() {
                        encoded_vector
                    } else {
                        ((rex >> 2) & 1) << 3 | encoded_vector
                    };
                    assert_classified(
                        &bytes,
                        kind,
                        gpr,
                        vector,
                        if rex & 8 == 0 {
                            OpWidth::W32
                        } else {
                            OpWidth::W64
                        },
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 576);
}

#[test]
fn classifier_exhausts_prefix_opcode_modrm_and_length_frontiers() {
    for kind in TransferKind::ALL {
        for lead in u8::MIN..=u8::MAX {
            let bytes = encoding(kind, Some(lead), 4, 1);
            let rex_with_low_gpr =
                matches!(lead, 0x40 | 0x42 | 0x44 | 0x46 | 0x48 | 0x4A | 0x4C | 0x4E);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_movd_q_stack_replay()
                    .is_some(),
                rex_with_low_gpr || (kind.mmx() && lead == 0x66),
                "{bytes:02X?}"
            );
        }

        let base = encoding(kind, None, 4, 1);
        let opcode_index = base.len() - 2;
        for opcode in u8::MIN..=u8::MAX {
            let mut bytes = base.clone();
            bytes[opcode_index] = opcode;
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_movd_q_stack_replay()
                    .is_some(),
                matches!(opcode, 0x6E | 0x7E),
                "{bytes:02X?}"
            );
        }
        for modrm in u8::MIN..=u8::MAX {
            let mut bytes = base.clone();
            *bytes.last_mut().unwrap() = modrm;
            let expected = modrm >> 6 == 3 && matches!(modrm & 7, 4 | 5);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_movd_q_stack_replay()
                    .is_some(),
                expected,
                "{bytes:02X?}"
            );
        }
    }

    for bytes in [
        &[0x0F, 0x6E][..],
        &[0x0F, 0x6E, 0xC4, 0x00][..],
        &[0x66, 0x66, 0x0F, 0x6E, 0xC4][..],
        &[0x48, 0x66, 0x0F, 0x6E, 0xC4][..],
        &[0xF0, 0x0F, 0x6E, 0xC4][..],
        &[0xF2, 0x0F, 0x6E, 0xC4][..],
        &[0xF3, 0x0F, 0x7E, 0xC4][..],
        &[0xD5, 0x00, 0x0F, 0x6E, 0xC4][..],
        &[0xC5, 0xF9, 0x6E, 0xC4][..],
        &[0x62, 0xF1, 0x7D, 0x08, 0x6E, 0xC4][..],
        &[0x41, 0x0F, 0x6E, 0xC4][..],
        &[0x66, 0x4D, 0x0F, 0x7E, 0xCD][..],
        &[0x0F, 0x6E, 0x04][..],
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_movd_q_stack_replay(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn gpr_rewrite_changes_only_modrm_rm_and_retains_rex_vector_and_direction() {
    for kind in TransferKind::ALL {
        for rex in [None, Some(0x40), Some(0x46), Some(0x48), Some(0x4E)] {
            for gpr in [4, 5] {
                let bytes = encoding(kind, rex, gpr, 7);
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                let rewritten = instruction.legacy_movd_q_stack_with_gpr_rax().unwrap();
                let mut expected = bytes;
                *expected.last_mut().unwrap() &= !0x07;
                assert_eq!(rewritten.as_slice(), expected, "{kind:?} {rex:?}");
            }
        }
    }
}

fn lifted_block(bytes: &[u8], pc: u64) -> SmirBlock {
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(pc, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    let mut block = SmirBlock::new(BlockId(59), pc);
    block.ops = result.ops;
    block
}

#[test]
fn llvm_samples_prefix_canonicalization_and_semantic_shape_are_fail_closed() {
    const PC: u64 = 0x1060;
    // LLVM 23.0.0 independently assembled the eight unprefixed samples.
    for (source, canonical) in [
        (&[0x0F, 0x6E, 0xC4][..], &[0x0F, 0x6E, 0xC4][..]),
        (&[0x0F, 0x7E, 0xFD][..], &[0x0F, 0x7E, 0xFD][..]),
        (&[0x48, 0x0F, 0x6E, 0xC4][..], &[0x48, 0x0F, 0x6E, 0xC4][..]),
        (&[0x48, 0x0F, 0x7E, 0xFD][..], &[0x48, 0x0F, 0x7E, 0xFD][..]),
        (&[0x66, 0x0F, 0x6E, 0xC4][..], &[0x66, 0x0F, 0x6E, 0xC4][..]),
        (
            &[0x66, 0x44, 0x0F, 0x7E, 0xFD][..],
            &[0x66, 0x44, 0x0F, 0x7E, 0xFD][..],
        ),
        (
            &[0x66, 0x48, 0x0F, 0x6E, 0xC4][..],
            &[0x66, 0x48, 0x0F, 0x6E, 0xC4][..],
        ),
        (
            &[0x66, 0x4C, 0x0F, 0x7E, 0xFD][..],
            &[0x66, 0x4C, 0x0F, 0x7E, 0xFD][..],
        ),
        (
            &[0x64, 0x46, 0x0F, 0x6E, 0xC4][..],
            &[0x46, 0x0F, 0x6E, 0xC4][..],
        ),
        (
            &[0x67, 0x65, 0x66, 0x4C, 0x0F, 0x7E, 0xFD][..],
            &[0x66, 0x4C, 0x0F, 0x7E, 0xFD][..],
        ),
    ] {
        let block = lifted_block(source, PC);
        let provenance =
            HashMap::from([((block.id, PC), X86InstructionBytes::new(source).unwrap())]);
        let expected_instruction = X86InstructionBytes::new(canonical).unwrap();
        let replay = expected_instruction.legacy_movd_q_stack_replay().unwrap();
        let start = usize::from(replay.touches_mmx());
        for spans in [
            x86_legacy_movd_q_stack_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            assert_eq!(
                spans.get(&start),
                Some(&X86NativeReplaySpan {
                    end: block.ops.len(),
                    instruction: expected_instruction,
                    needs_avx512vl: false,
                    needs_avx512dq: false,
                    needs_avx512fp16: false,
                    preserve_mxcsr_de: false,
                }),
                "{source:02X?}"
            );
        }

        let mut wrong_width = block.clone();
        let OpKind::X86MovdQ { width, .. } = &mut wrong_width.ops.last_mut().unwrap().kind else {
            unreachable!()
        };
        *width = if *width == OpWidth::W32 {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        assert!(x86_native_replay_spans(&wrong_width, &provenance).is_empty());

        let mut wrong_zero_upper = block.clone();
        let OpKind::X86MovdQ { zero_upper, .. } =
            &mut wrong_zero_upper.ops.last_mut().unwrap().kind
        else {
            unreachable!()
        };
        *zero_upper = true;
        assert!(x86_native_replay_spans(&wrong_zero_upper, &provenance).is_empty());

        let mut missing_hint = block.clone();
        missing_hint.ops.last_mut().unwrap().x86_hint = None;
        assert!(x86_native_replay_spans(&missing_hint, &provenance).is_empty());

        if replay.touches_mmx() {
            let mut wrong_marker = block.clone();
            wrong_marker.ops[0].kind = OpKind::Nop;
            assert!(x86_native_replay_spans(&wrong_marker, &provenance).is_empty());

            let mut hinted_marker = block.clone();
            hinted_marker.ops[0].x86_hint = Some(replay.hint);
            assert!(x86_native_replay_spans(&hinted_marker, &provenance).is_empty());

            let mut missing_marker = block.clone();
            missing_marker.ops.remove(0);
            assert!(x86_native_replay_spans(&missing_marker, &provenance).is_empty());
        }

        let mut extra = block.clone();
        extra.push_op(SmirOp::new(OpId(1), PC, OpKind::Nop));
        assert!(x86_native_replay_spans(&extra, &provenance).is_empty());
        assert!(x86_native_replay_spans(&block, &HashMap::new()).is_empty());
    }
}
