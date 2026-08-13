//! Exact legacy SSE4.1 `DPPS`/`DPPD` replay classification.

use super::*;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{
    ArchReg, FunctionId, SignExtend, SourceArch, VecElementType, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0x0B0A_0900;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Dpps,
    Dppd,
}

impl Kind {
    const ALL: [Self; 2] = [Self::Dpps, Self::Dppd];

    fn fields(self) -> (u8, VecElementType, u8) {
        match self {
            Self::Dpps => (0x40, VecElementType::F32, 4),
            Self::Dppd => (0x41, VecElementType::F64, 2),
        }
    }
}

fn encoding(kind: Kind, rex: Option<u8>, modrm: u8, immediate: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, 0x3A, kind.fields().0, modrm, immediate]);
    bytes
}

fn expected(kind: Kind, rex: Option<u8>, modrm: u8, immediate: u8) -> X86LegacyDotProductReplay {
    let rex = rex.unwrap_or(0);
    let (_, elem, lanes) = kind.fields();
    X86LegacyDotProductReplay {
        destination: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
        source: (modrm & 7) | ((rex & 0x01) << 3),
        elem,
        lanes,
        immediate,
    }
}

#[test]
fn classifier_covers_all_557056_rex_register_immediate_encodings() {
    let mut classified = 0usize;
    for kind in Kind::ALL {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                for immediate in u8::MIN..=u8::MAX {
                    let bytes = encoding(kind, rex, modrm, immediate);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_register_dot_product_replay(),
                        Some(expected(kind, rex, modrm, immediate)),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, Kind::ALL.len() * 17 * 64 * 256);
}

#[test]
fn classifier_exhausts_opcode_modrm_and_canonical_frontiers() {
    for opcode in u8::MIN..=u8::MAX {
        for modrm in u8::MIN..=u8::MAX {
            let bytes = [0x66, 0x4F, 0x0F, 0x3A, opcode, modrm, 0xA5];
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_dot_product_replay()
                    .is_some(),
                matches!(opcode, 0x40 | 0x41) && modrm >> 6 == 3,
                "{bytes:02X?}"
            );
        }
    }

    // LLVM 23 independently decodes every REX image with R/B extending only
    // the two XMM operands and W/X ignored.
    for rex in 0x40..=0x4F {
        for kind in Kind::ALL {
            let bytes = encoding(kind, Some(rex), 0xCA, 0xA5);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_dot_product_replay(),
                Some(expected(kind, Some(rex), 0xCA, 0xA5)),
                "{bytes:02X?}"
            );
        }
    }

    let invalid: &[&[u8]] = &[
        &[0x0F, 0x3A, 0x40, 0xCA, 0xA5],             // missing mandatory 66
        &[0xF2, 0x0F, 0x3A, 0x40, 0xCA, 0xA5],       // wrong mandatory prefix
        &[0xF0, 0x66, 0x0F, 0x3A, 0x40, 0xCA, 0xA5], // LOCK
        &[0x67, 0x66, 0x0F, 0x3A, 0x40, 0xCA, 0xA5], // reserved address prefix
        &[0x64, 0x66, 0x0F, 0x3A, 0x40, 0xCA, 0xA5], // segment prefix excluded
        &[0x48, 0x66, 0x0F, 0x3A, 0x40, 0xCA, 0xA5], // REX not final
        &[0x66, 0x48, 0x49, 0x0F, 0x3A, 0x40, 0xCA, 0xA5], // duplicate REX
        &[0x66, 0xD5, 0x00, 0x0F, 0x3A, 0x40, 0xCA, 0xA5], // REX2
        &[0x66, 0x0F, 0x38, 0x40, 0xCA, 0xA5],       // wrong map
        &[0x66, 0x0F, 0x3A, 0x3F, 0xCA, 0xA5],       // adjacent opcode
        &[0x66, 0x0F, 0x3A, 0x42, 0xCA, 0xA5],       // adjacent opcode
        &[0x66, 0x0F, 0x3A, 0x40, 0x0A, 0xA5],       // memory source
        &[0x66, 0x0F, 0x3A, 0x40, 0xCA],             // missing immediate
        &[0x66, 0x0F, 0x3A, 0x40, 0xCA, 0xA5, 0],    // trailing byte
        &[0xC4, 0xE3, 0x79, 0x40, 0xCA, 0xA5],       // VEX
        &[0x62, 0xF3, 0x7D, 0x08, 0x40, 0xCA, 0xA5], // EVEX neighbor
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_dot_product_replay(),
            None,
            "{bytes:02X?}"
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
        X86InstructionBytes::new(bytes).expect("legacy dot-product provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_span(function: &SmirFunction, bytes: &[u8], expected_end: usize) {
    for spans in [
        x86_legacy_dot_product_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(span.end, expected_end, "{bytes:02X?}");
        assert_eq!(span.instruction.as_slice(), bytes, "{bytes:02X?}");
        assert!(!span.needs_avx512vl, "{bytes:02X?}");
        assert!(!span.needs_avx512dq, "{bytes:02X?}");
        assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        assert!(!span.preserve_mxcsr_de, "{bytes:02X?}");
    }
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert!(
        x86_legacy_dot_product_replay_spans(&function.blocks[0], &function.x86_instruction_bytes,)
            .is_empty(),
        "dedicated span admitted {label}"
    );
    assert!(
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes).is_empty(),
        "aggregate span admitted {label}"
    );
}

#[test]
fn exact_graph_validator_covers_all_immediates_rex_aliases_and_o0_o1_o2() {
    for kind in Kind::ALL {
        let expected_end = 1 + 2 * usize::from(kind.fields().2);
        for immediate in u8::MIN..=u8::MAX {
            let bytes = encoding(kind, Some(0x4F), 0xCA, immediate);
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                let function = function(&bytes, level);
                assert_eq!(
                    function.blocks[0].ops.len(),
                    expected_end,
                    "{level:?} {bytes:02X?}"
                );
                assert_span(&function, &bytes, expected_end);
            }
        }

        for (rex_index, rex) in [None]
            .into_iter()
            .chain((0x40..=0x4F).map(Some))
            .enumerate()
        {
            for (shape_index, modrm) in [0xC0, 0xC9, 0xCA, 0xFF].into_iter().enumerate() {
                let immediate = (rex_index * 37 + shape_index * 73) as u8;
                let bytes = encoding(kind, rex, modrm, immediate);
                for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                    assert_span(&function(&bytes, level), &bytes, expected_end);
                }
            }
        }
    }
}

fn alternate_elem(elem: VecElementType) -> VecElementType {
    if elem == VecElementType::F32 {
        VecElementType::F64
    } else {
        VecElementType::F32
    }
}

#[test]
fn graph_validator_rejects_every_operation_field_and_escaping_temporary() {
    for kind in Kind::ALL {
        let bytes = encoding(kind, Some(0x45), 0xCA, 0xA5);
        let baseline = function(&bytes, OptLevel::O0);
        let lanes = usize::from(kind.fields().2);

        for mutation in 0..7 {
            let mut malformed = baseline.clone();
            let operation = &mut malformed.blocks[0].ops[0];
            if mutation == 0 {
                operation.x86_hint = Some(X86OpHint::RexByteReg);
            } else {
                let OpKind::X86DotProduct {
                    dst,
                    src1,
                    src2,
                    elem,
                    width,
                    imm,
                } = &mut operation.kind
                else {
                    panic!("expected X86DotProduct")
                };
                match mutation {
                    1 => *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    2 => *src1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    3 => *src2 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    4 => *elem = alternate_elem(*elem),
                    5 => *width = VecWidth::V256,
                    6 => *imm ^= 1,
                    _ => unreachable!(),
                }
            }
            assert_rejected(&malformed, &format!("{kind:?} dot mutation {mutation}"));
        }

        for lane in 0..lanes {
            for mutation in 0..6 {
                let mut malformed = baseline.clone();
                let operation = &mut malformed.blocks[0].ops[1 + lane];
                if mutation == 0 {
                    operation.x86_hint = Some(X86OpHint::RexByteReg);
                } else {
                    let OpKind::VExtractLane {
                        dst,
                        vec,
                        lane: actual_lane,
                        elem,
                        sign,
                    } = &mut operation.kind
                    else {
                        panic!("expected VExtractLane")
                    };
                    match mutation {
                        1 => *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        2 => *vec = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        3 => *actual_lane = actual_lane.wrapping_add(1),
                        4 => *elem = alternate_elem(*elem),
                        5 => *sign = SignExtend::Sign,
                        _ => unreachable!(),
                    }
                }
                assert_rejected(
                    &malformed,
                    &format!("{kind:?} extract lane {lane} mutation {mutation}"),
                );
            }

            for mutation in 0..6 {
                let mut malformed = baseline.clone();
                let operation = &mut malformed.blocks[0].ops[1 + lanes + lane];
                if mutation == 0 {
                    operation.x86_hint = Some(X86OpHint::RexByteReg);
                } else {
                    let OpKind::VInsertLane {
                        dst,
                        vec,
                        scalar,
                        lane: actual_lane,
                        elem,
                    } = &mut operation.kind
                    else {
                        panic!("expected VInsertLane")
                    };
                    match mutation {
                        1 => *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        2 => *vec = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        3 => *scalar = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        4 => *actual_lane = actual_lane.wrapping_add(1),
                        5 => *elem = alternate_elem(*elem),
                        _ => unreachable!(),
                    }
                }
                assert_rejected(
                    &malformed,
                    &format!("{kind:?} insert lane {lane} mutation {mutation}"),
                );
            }
        }

        let raw = baseline.blocks[0].ops[0].kind.dests()[0];
        let mut escaped = baseline.clone();
        escaped.blocks[0].push_op(SmirOp::new(
            OpId((1 + 2 * lanes) as u16),
            PC + 1,
            OpKind::VMov {
                dst: VReg::Virtual(VirtualId(0xFFF0)),
                src: raw,
                width: VecWidth::V128,
            },
        ));
        assert_rejected(&escaped, &format!("{kind:?} escaping result"));

        let mut redefined = baseline.clone();
        redefined.blocks[0].push_op(SmirOp::new(
            OpId((1 + 2 * lanes) as u16),
            PC + 1,
            OpKind::VMov {
                dst: raw,
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                width: VecWidth::V128,
            },
        ));
        assert_rejected(&redefined, &format!("{kind:?} redefined result"));

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert_rejected(&missing, &format!("{kind:?} missing provenance"));

        let mut wrong_bytes = baseline.clone();
        wrong_bytes.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&encoding(kind, Some(0x45), 0xD3, 0xA5)).unwrap(),
        );
        assert_rejected(&wrong_bytes, &format!("{kind:?} wrong operands"));

        let mut extra = baseline.clone();
        extra.blocks[0].push_op(SmirOp::new(OpId((1 + 2 * lanes) as u16), PC, OpKind::Nop));
        assert_rejected(&extra, &format!("{kind:?} extra same-PC operation"));
    }
}
