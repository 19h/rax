//! Exact legacy `PTEST` replay classification and semantic-graph tests.

use super::*;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{
    ArchReg, Condition, FunctionId, OpWidth, SignExtend, SourceArch, SrcOperand, VecElementType,
    VirtualId, X86Reg,
};
use crate::smir::ir::{SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0x7E57_0041;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

fn encoding(rex: Option<u8>, modrm: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, 0x38, 0x17, modrm]);
    bytes
}

fn expected(rex: Option<u8>, modrm: u8) -> X86LegacyPtestReplay {
    let rex = rex.unwrap_or(0);
    X86LegacyPtestReplay {
        first_source: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
        second_source: (modrm & 7) | ((rex & 0x01) << 3),
    }
}

#[test]
fn classifier_covers_all_1088_rex_register_encodings() {
    let mut classified = 0usize;
    for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
        for modrm in 0xC0..=0xFF {
            let bytes = encoding(rex, modrm);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_ptest_replay(),
                Some(expected(rex, modrm)),
                "{bytes:02X?}"
            );
            classified += 1;
        }
    }
    assert_eq!(classified, 17 * 64);
}

#[test]
fn classifier_exhausts_opcode_modrm_and_canonical_frontiers() {
    for opcode in u8::MIN..=u8::MAX {
        for modrm in u8::MIN..=u8::MAX {
            let bytes = [0x66, 0x4F, 0x0F, 0x38, opcode, modrm];
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_ptest_replay()
                    .is_some(),
                opcode == 0x17 && modrm >> 6 == 3,
                "{bytes:02X?}"
            );
        }
    }

    // LLVM 23 independently decodes every REX image with R/B extending only
    // the two XMM operands and W/X ignored.
    for rex in 0x40..=0x4F {
        let bytes = encoding(Some(rex), 0xCA);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_register_ptest_replay(),
            Some(expected(Some(rex), 0xCA)),
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0x0F, 0x38, 0x17, 0xCA],                   // missing mandatory 66
        &[0xF2, 0x0F, 0x38, 0x17, 0xCA],             // wrong mandatory prefix
        &[0xF0, 0x66, 0x0F, 0x38, 0x17, 0xCA],       // LOCK
        &[0x67, 0x66, 0x0F, 0x38, 0x17, 0xCA],       // address prefix excluded
        &[0x64, 0x66, 0x0F, 0x38, 0x17, 0xCA],       // segment prefix excluded
        &[0x48, 0x66, 0x0F, 0x38, 0x17, 0xCA],       // REX not final
        &[0x66, 0x48, 0x49, 0x0F, 0x38, 0x17, 0xCA], // duplicate REX
        &[0x66, 0xD5, 0x00, 0x0F, 0x38, 0x17, 0xCA], // REX2
        &[0x66, 0x0F, 0x3A, 0x17, 0xCA],             // wrong map
        &[0x66, 0x0F, 0x38, 0x16, 0xCA],             // adjacent opcode
        &[0x66, 0x0F, 0x38, 0x18, 0xCA],             // adjacent opcode
        &[0x66, 0x0F, 0x38, 0x17, 0x0A],             // memory source
        &[0x66, 0x0F, 0x38, 0x17],                   // missing ModR/M
        &[0x66, 0x0F, 0x38, 0x17, 0xCA, 0],          // trailing byte
        &[0xC4, 0xE2, 0x79, 0x17, 0xCA],             // VEX
        &[0x62, 0xF2, 0x7D, 0x08, 0x17, 0xCA],       // EVEX
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_ptest_replay(),
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
        X86InstructionBytes::new(bytes).expect("legacy PTEST provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_span(function: &SmirFunction, bytes: &[u8]) {
    assert_eq!(function.blocks[0].ops.len(), 24, "{bytes:02X?}");
    for spans in [
        x86_legacy_ptest_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(span.end, 24, "{bytes:02X?}");
        assert_eq!(span.instruction.as_slice(), bytes, "{bytes:02X?}");
        assert!(!span.needs_avx512vl, "{bytes:02X?}");
        assert!(!span.needs_avx512dq, "{bytes:02X?}");
        assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        assert!(!span.preserve_mxcsr_de, "{bytes:02X?}");
    }
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert!(
        x86_legacy_ptest_replay_spans(&function.blocks[0], &function.x86_instruction_bytes)
            .is_empty(),
        "dedicated span admitted {label}"
    );
    assert!(
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes).is_empty(),
        "aggregate span admitted {label}"
    );
}

#[test]
fn exact_graph_validator_covers_all_rex_register_aliases_and_o0_o1_o2() {
    let mut validated = 0usize;
    for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
        for modrm in 0xC0..=0xFF {
            let bytes = encoding(rex, modrm);
            for level in LEVELS {
                assert_span(&function(&bytes, level), &bytes);
                validated += 1;
            }
        }
    }
    assert_eq!(validated, 17 * 64 * LEVELS.len());
}

fn mutation_count(kind: &OpKind) -> usize {
    match kind {
        OpKind::Mov { .. } | OpKind::Cmp { .. } | OpKind::SetCC { .. } => 3,
        OpKind::VExtractLane { .. } | OpKind::Shl { .. } => 5,
        OpKind::And { .. } | OpKind::Or { .. } | OpKind::AndNot { .. } => 5,
        OpKind::ReadFlags { .. } | OpKind::WriteFlags { .. } => 1,
        _ => panic!("unexpected PTEST graph operation: {kind:?}"),
    }
}

fn mutate(kind: &mut OpKind, mutation: usize) {
    let x0 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    match kind {
        OpKind::Mov { dst, src, width } => match mutation {
            0 => *dst = x0,
            1 => *src = SrcOperand::Imm(1),
            2 => *width = OpWidth::W32,
            _ => unreachable!(),
        },
        OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem,
            sign,
        } => match mutation {
            0 => *dst = x0,
            1 => *vec = x0,
            2 => *lane = lane.wrapping_add(1),
            3 => *elem = VecElementType::I32,
            4 => *sign = SignExtend::Sign,
            _ => unreachable!(),
        },
        OpKind::And {
            dst,
            src1,
            src2,
            width,
            flags,
        }
        | OpKind::Or {
            dst,
            src1,
            src2,
            width,
            flags,
        }
        | OpKind::AndNot {
            dst,
            src1,
            src2,
            width,
            flags,
        } => match mutation {
            0 => *dst = x0,
            1 => *src1 = x0,
            2 => *src2 = SrcOperand::Imm(1),
            3 => *width = OpWidth::W32,
            4 => *flags = FlagUpdate::All,
            _ => unreachable!(),
        },
        OpKind::Cmp { src1, src2, width } => match mutation {
            0 => *src1 = x0,
            1 => *src2 = SrcOperand::Imm(1),
            2 => *width = OpWidth::W32,
            _ => unreachable!(),
        },
        OpKind::SetCC { dst, cond, width } => match mutation {
            0 => *dst = x0,
            1 => *cond = Condition::Ne,
            2 => *width = OpWidth::W32,
            _ => unreachable!(),
        },
        OpKind::Shl {
            dst,
            src,
            amount,
            width,
            flags,
        } => match mutation {
            0 => *dst = x0,
            1 => *src = x0,
            2 => *amount = SrcOperand::Imm(5),
            3 => *width = OpWidth::W32,
            4 => *flags = FlagUpdate::All,
            _ => unreachable!(),
        },
        OpKind::ReadFlags { dst } => match mutation {
            0 => *dst = x0,
            _ => unreachable!(),
        },
        OpKind::WriteFlags { src } => match mutation {
            0 => *src = x0,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}

#[test]
fn graph_validator_rejects_every_operation_field_hint_and_escaping_temporary() {
    for level in LEVELS {
        let bytes = encoding(Some(0x45), 0xCA);
        let baseline = function(&bytes, level);
        for operation_index in 0..baseline.blocks[0].ops.len() {
            let mut hinted = baseline.clone();
            hinted.blocks[0].ops[operation_index].x86_hint = Some(X86OpHint::RexByteReg);
            assert_rejected(&hinted, &format!("{level:?} hint op {operation_index}"));

            let count = mutation_count(&baseline.blocks[0].ops[operation_index].kind);
            for mutation in 0..count {
                let mut malformed = baseline.clone();
                mutate(&mut malformed.blocks[0].ops[operation_index].kind, mutation);
                assert_rejected(
                    &malformed,
                    &format!("{level:?} op {operation_index} mutation {mutation}"),
                );
            }
        }
    }

    let bytes = encoding(Some(0x45), 0xCA);
    let baseline = function(&bytes, OptLevel::O0);
    let virtuals: Vec<_> = baseline.blocks[0]
        .ops
        .iter()
        .flat_map(|operation| operation.kind.dests())
        .filter(|register| matches!(register, VReg::Virtual(_)))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(virtuals.len(), 17);
    for (ordinal, temporary) in virtuals.into_iter().enumerate() {
        let mut escaped = baseline.clone();
        escaped.blocks[0].push_op(SmirOp::new(
            OpId(0xF000u16.wrapping_add(ordinal as u16)),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xF000u32.wrapping_add(ordinal as u32))),
                src: SrcOperand::Reg(temporary),
                width: OpWidth::W64,
            },
        ));
        assert_rejected(&escaped, &format!("escaping {temporary:?}"));

        let mut redefined = baseline.clone();
        redefined.blocks[0].push_op(SmirOp::new(
            OpId(0xE000u16.wrapping_add(ordinal as u16)),
            PC + 1,
            OpKind::Mov {
                dst: temporary,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        assert_rejected(&redefined, &format!("redefined {temporary:?}"));
    }

    let first_lane = match baseline.blocks[0].ops[2].kind {
        OpKind::VExtractLane { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut terminator_escape = baseline.clone();
    terminator_escape.blocks[0].set_terminator(Terminator::Return {
        values: vec![first_lane],
    });
    assert_rejected(&terminator_escape, "terminator escape");

    let mut missing = baseline.clone();
    missing.x86_instruction_bytes.clear();
    assert_rejected(&missing, "missing provenance");

    let mut wrong_bytes = baseline.clone();
    wrong_bytes.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&encoding(Some(0x45), 0xD3)).unwrap(),
    );
    assert_rejected(&wrong_bytes, "wrong operands");

    let mut memory_provenance = baseline.clone();
    memory_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&encoding(Some(0x45), 0x13)).unwrap(),
    );
    assert_rejected(&memory_provenance, "memory provenance");

    let mut extra = baseline;
    extra.blocks[0].push_op(SmirOp::new(OpId(0xD000), PC, OpKind::Nop));
    assert_rejected(&extra, "extra same-PC operation");
}
