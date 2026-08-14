//! Exact source-byte replay classification for guest-stack-destination legacy
//! MOVMSKPS and MOVMSKPD.

use super::*;
use crate::smir::ir::ops::{X86OpHint, X86SsePrefix};
use crate::smir::ir::types::{OpWidth, VecElementType, VecWidth};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaskKind {
    Movmskps,
    Movmskpd,
}

impl MaskKind {
    const ALL: [Self; 2] = [Self::Movmskps, Self::Movmskpd];

    fn fields(self) -> (VecElementType, u8, X86SsePrefix) {
        match self {
            Self::Movmskps => (VecElementType::F32, 4, X86SsePrefix::None),
            Self::Movmskpd => (VecElementType::F64, 2, X86SsePrefix::OpSize),
        }
    }
}

fn encoding(kind: MaskKind, rex: Option<u8>, destination: u8, rm: u8) -> Vec<u8> {
    assert!(matches!(destination, 4 | 5));
    assert!(rm < 8);
    let mut bytes = Vec::with_capacity(5);
    if kind == MaskKind::Movmskpd {
        bytes.push(0x66);
    }
    if let Some(rex) = rex {
        bytes.push(rex);
    }
    bytes.extend_from_slice(&[0x0F, 0x50, 0xC0 | (destination << 3) | rm]);
    bytes
}

fn assert_classified(
    bytes: &[u8],
    kind: MaskKind,
    destination: u8,
    source: u8,
    dst_width: OpWidth,
) {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    let replay = instruction
        .legacy_mov_mask_stack_destination_replay()
        .unwrap_or_else(|| panic!("not classified: {bytes:02X?}"));
    let (elem, lanes, prefix) = kind.fields();
    assert_eq!(replay.destination, destination, "{bytes:02X?}");
    assert_eq!(replay.source, source, "{bytes:02X?}");
    assert_eq!(replay.elem, elem, "{bytes:02X?}");
    assert_eq!(replay.lanes, lanes, "{bytes:02X?}");
    assert_eq!(replay.dst_width, dst_width, "{bytes:02X?}");
    assert_eq!(replay.vector_width, VecWidth::V128, "{bytes:02X?}");
    assert_eq!(
        replay.hint,
        X86OpHint::SseOp {
            prefix,
            opcode: 0x50,
        },
        "{bytes:02X?}"
    );
    assert!(!replay.needs_avx2, "{bytes:02X?}");
    assert_eq!(
        instruction.legacy_mov_mask_stack_destination_index(),
        Some(destination),
        "{bytes:02X?}"
    );
}

#[test]
fn classifier_covers_all_288_canonical_legacy_stack_destination_encodings() {
    let mut classified = 0usize;
    for kind in MaskKind::ALL {
        for destination in [4, 5] {
            for rm in 0..8 {
                let bytes = encoding(kind, None, destination, rm);
                assert_classified(&bytes, kind, destination, rm, OpWidth::W32);
                classified += 1;
            }
        }
        for rex in [0x40, 0x41, 0x42, 0x43, 0x48, 0x49, 0x4A, 0x4B] {
            for destination in [4, 5] {
                for rm in 0..8 {
                    let bytes = encoding(kind, Some(rex), destination, rm);
                    assert_classified(
                        &bytes,
                        kind,
                        destination,
                        ((rex & 1) << 3) | rm,
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
    assert_eq!(classified, 288);
}

#[test]
fn classifier_exhausts_rex_opcode_modrm_and_exact_shape_frontiers() {
    for kind in MaskKind::ALL {
        for lead in u8::MIN..=u8::MAX {
            let bytes = encoding(kind, Some(lead), 4, 1);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_mov_mask_stack_destination_replay()
                    .is_some(),
                matches!(lead, 0x40..=0x43 | 0x48..=0x4B)
                    || (kind == MaskKind::Movmskps && lead == 0x66),
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
                    .legacy_mov_mask_stack_destination_replay()
                    .is_some(),
                opcode == 0x50,
                "{bytes:02X?}"
            );
        }
        for modrm in u8::MIN..=u8::MAX {
            let mut bytes = base.clone();
            *bytes.last_mut().unwrap() = modrm;
            let expected = modrm >> 6 == 3 && matches!((modrm >> 3) & 7, 4 | 5);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_mov_mask_stack_destination_replay()
                    .is_some(),
                expected,
                "{bytes:02X?}"
            );
        }
    }

    for bytes in [
        &[0x0F, 0x50][..],
        &[0x0F, 0x50, 0xE1, 0x00][..],
        &[0x66, 0x66, 0x0F, 0x50, 0xE1][..],
        &[0x48, 0x66, 0x0F, 0x50, 0xE1][..],
        &[0xF0, 0x0F, 0x50, 0xE1][..],
        &[0xF2, 0x0F, 0x50, 0xE1][..],
        &[0xF3, 0x0F, 0x50, 0xE1][..],
        &[0xD5, 0x00, 0x0F, 0x50, 0xE1][..],
        &[0xC5, 0xF8, 0x50, 0xE1][..],
        &[0x62, 0xF1, 0x7C, 0x08, 0x50, 0xE1][..],
        &[0x44, 0x0F, 0x50, 0xE1][..],
        &[0x66, 0x4C, 0x0F, 0x50, 0xE1][..],
        &[0x0F, 0x50, 0x21][..],
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_mov_mask_stack_destination_replay(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn destination_rewrite_changes_only_modrm_reg_and_retains_rex_width_and_source() {
    for kind in MaskKind::ALL {
        for rex in [None, Some(0x40), Some(0x43), Some(0x48), Some(0x4B)] {
            for destination in [4, 5] {
                let bytes = encoding(kind, rex, destination, 7);
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                let rewritten = instruction
                    .legacy_mov_mask_stack_destination_with_destination_rax()
                    .unwrap();
                let mut expected = bytes;
                *expected.last_mut().unwrap() &= !0x38;
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
    let mut block = SmirBlock::new(BlockId(58), pc);
    block.ops = result.ops;
    block
}

#[test]
fn llvm_samples_prefix_canonicalization_and_semantic_shape_are_fail_closed() {
    const PC: u64 = 0x1050;
    // LLVM 23.0.0 independently assembled the four unprefixed samples.
    for (source, canonical) in [
        (&[0x41, 0x0F, 0x50, 0xE7][..], &[0x41, 0x0F, 0x50, 0xE7][..]),
        (
            &[0x66, 0x41, 0x0F, 0x50, 0xE9][..],
            &[0x66, 0x41, 0x0F, 0x50, 0xE9][..],
        ),
        (&[0x0F, 0x50, 0xE2][..], &[0x0F, 0x50, 0xE2][..]),
        (&[0x66, 0x0F, 0x50, 0xEB][..], &[0x66, 0x0F, 0x50, 0xEB][..]),
        (&[0x64, 0x0F, 0x50, 0xE2][..], &[0x0F, 0x50, 0xE2][..]),
        (
            &[0x67, 0x65, 0x66, 0x49, 0x0F, 0x50, 0xEC][..],
            &[0x66, 0x49, 0x0F, 0x50, 0xEC][..],
        ),
    ] {
        let block = lifted_block(source, PC);
        let provenance =
            HashMap::from([((block.id, PC), X86InstructionBytes::new(source).unwrap())]);
        let expected_instruction = X86InstructionBytes::new(canonical).unwrap();
        for spans in [
            x86_legacy_mov_mask_stack_destination_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            assert_eq!(
                spans.get(&0),
                Some(&X86NativeReplaySpan {
                    end: 1,
                    instruction: expected_instruction,
                    needs_avx512vl: false,
                    needs_avx512dq: false,
                    needs_avx512fp16: false,
                    preserve_mxcsr_de: false,
                }),
                "{source:02X?}"
            );
        }

        let mut malformed = block.clone();
        let OpKind::X86MovMask { lanes, .. } = &mut malformed.ops[0].kind else {
            unreachable!()
        };
        *lanes = lanes.saturating_sub(1);
        assert!(x86_native_replay_spans(&malformed, &provenance).is_empty());

        let mut missing_hint = block.clone();
        missing_hint.ops[0].x86_hint = None;
        assert!(x86_native_replay_spans(&missing_hint, &provenance).is_empty());

        let mut extra = block.clone();
        extra.push_op(SmirOp::new(OpId(1), PC, OpKind::Nop));
        assert!(x86_native_replay_spans(&extra, &provenance).is_empty());
        assert!(x86_native_replay_spans(&block, &HashMap::new()).is_empty());
    }
}
