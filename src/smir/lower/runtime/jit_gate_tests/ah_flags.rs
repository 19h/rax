//! Exact x86 native admission for LAHF and SAHF lift graphs.

use super::*;
use crate::smir::SourceArch;
use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::x86_jit_ah_flags_sequence_len;
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{SmirLowerer, runtime::is_x86_aarch64_native_clobber_safe_excluding};
use crate::smir::optimize::{OptLevel, optimize_function};

const PC: u64 = 0x1000;

const PREFIXES: &[&[u8]] = &[
    &[],
    &[0x66],
    &[0xF2],
    &[0xF3],
    &[0x67],
    &[0x64],
    &[0x65],
    &[0x48],
    &[0x44],
    &[0x41],
    &[0x4D],
    &[0x66, 0x48],
    &[0xF2, 0x48],
    &[0xF3, 0x48],
];

fn counts(
    block: &SmirBlock,
) -> (
    std::collections::HashMap<VReg, usize>,
    std::collections::HashMap<VReg, usize>,
) {
    let mut definitions = std::collections::HashMap::new();
    let mut uses = std::collections::HashMap::new();
    for op in &block.ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *definitions.entry(reg).or_insert(0usize) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *uses.entry(reg).or_insert(0usize) += 1;
            }
        }
    }
    (definitions, uses)
}

fn lift_function(bytes: &[u8], level: OptLevel) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("lift {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(
        matches!(
            result.control_flow,
            ControlFlow::Fallthrough | ControlFlow::NextInsn
        ),
        "{bytes:02X?}: {:?}",
        result.control_flow
    );

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: vec![] });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("valid instruction bytes"),
    );
    optimize_function(&mut function, level);
    function
}

fn sequence_index(function: &SmirFunction) -> usize {
    usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ))
}

fn matched_len(function: &SmirFunction) -> Option<usize> {
    let block = &function.blocks[0];
    let (definitions, uses) = counts(block);
    x86_jit_ah_flags_sequence_len(block, sequence_index(function), &definitions, &uses)
}

fn assert_admits_and_lowers(bytes: &[u8], level: OptLevel) {
    let function = lift_function(bytes, level);
    assert_eq!(matched_len(&function), Some(6), "{bytes:02X?}, {level:?}");
    assert!(
        is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), false,),
        "x86 gate rejected {bytes:02X?}, {level:?}: {:?}",
        function.blocks[0].ops
    );
    assert!(
        !is_x86_aarch64_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(),),
        "x86 LAHF/SAHF graph must remain outside the AArch64-host gate"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("lower {bytes:02X?}, {level:?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("finalize {bytes:02X?}, {level:?}: {error:?}"));
    assert!(!code.is_empty(), "{bytes:02X?}, {level:?}");
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert_eq!(matched_len(function), None, "matcher accepted {label}");
    assert!(
        !is_native_clobber_safe_excluding(function, &std::collections::HashMap::new(), false,),
        "gate accepted {label}"
    );
}

#[test]
fn every_scanned_lahf_sahf_prefix_admits_and_lowers_at_all_levels() {
    for opcode in [0x9E, 0x9F] {
        for prefix in PREFIXES {
            let mut bytes = prefix.to_vec();
            bytes.push(opcode);
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                assert_admits_and_lowers(&bytes, level);
            }
        }
    }
}

#[test]
fn every_map_zero_rex2_payload_is_guarded_admitted_and_lowered() {
    for opcode in [0x9E, 0x9F] {
        for payload in 0x00..=0x7F {
            let bytes = [0xD5, payload, opcode];
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                let function = lift_function(&bytes, level);
                assert!(
                    matches!(function.blocks[0].ops[0].kind, OpKind::X86RequireApx),
                    "missing APX guard for {bytes:02X?}, {level:?}"
                );
                assert_admits_and_lowers(&bytes, level);
            }
        }
    }
}

#[test]
fn lock_and_illegal_rex2_prefix_orders_fail_before_admission() {
    for opcode in [0x9E, 0x9F] {
        for bytes in [
            vec![0xF0, opcode],
            vec![0xF0, 0x48, opcode],
            vec![0xF0, 0xD5, 0x00, opcode],
            vec![0x48, 0xD5, 0x00, opcode],
        ] {
            let mut lifter = X86_64Lifter::strict();
            let mut context = LiftContext::new(SourceArch::X86_64);
            assert!(
                matches!(
                    lifter.lift_insn(PC, &bytes, &mut context),
                    Err(crate::smir::lift::LiftError::InvalidEncoding { .. })
                ),
                "accepted invalid LAHF/SAHF encoding {bytes:02X?}"
            );
        }
    }
}

#[test]
fn ah_flag_matcher_rejects_metadata_frontier_ssa_and_semantic_mutations() {
    for opcode in [0x9E, 0x9F] {
        let original = lift_function(&[opcode], OptLevel::O2);

        for index in 0..6 {
            let mut hinted = original.clone();
            hinted.blocks[0].ops[index].x86_hint = Some(X86OpHint::Mulx);
            assert_rejected(&hinted, &format!("opcode={opcode:#04X} hint at {index}"));

            let mut split_pc = original.clone();
            split_pc.blocks[0].ops[index].guest_pc = PC + 1;
            assert_rejected(
                &split_pc,
                &format!("opcode={opcode:#04X} guest PC at {index}"),
            );
        }

        let mut architectural_read = original.clone();
        let OpKind::ReadFlags { dst } = &mut architectural_read.blocks[0].ops[0].kind else {
            unreachable!();
        };
        *dst = x86(X86Reg::Rbx);
        assert_rejected(&architectural_read, "architectural ReadFlags destination");

        let mut escaped = original.clone();
        let OpKind::ReadFlags { dst: read_result } = &escaped.blocks[0].ops[0].kind else {
            unreachable!();
        };
        let read_result = *read_result;
        escaped.blocks[0]
            .ops
            .push(crate::smir::ir::ops::SmirOp::new(
                crate::smir::ir::types::OpId(6),
                PC + 1,
                OpKind::Mov {
                    dst: x86(X86Reg::Rbx),
                    src: SrcOperand::Reg(read_result),
                    width: OpWidth::W64,
                },
            ));
        assert_rejected(&escaped, "escaped ReadFlags temporary");

        let mut same_pc_tail = original.clone();
        same_pc_tail.blocks[0]
            .ops
            .push(crate::smir::ir::ops::SmirOp::new(
                crate::smir::ir::types::OpId(6),
                PC,
                OpKind::Mov {
                    dst: x86(X86Reg::Rbx),
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                },
            ));
        assert_rejected(&same_pc_tail, "same-PC operation after exact graph");

        if opcode == 0x9E {
            let mut wrong_shift = original.clone();
            let OpKind::Shr { amount, .. } = &mut wrong_shift.blocks[0].ops[1].kind else {
                unreachable!();
            };
            *amount = SrcOperand::Imm(7);
            assert_rejected(&wrong_shift, "SAHF AH extraction shift");

            let mut wrong_status = original.clone();
            let OpKind::And { src2, .. } = &mut wrong_status.blocks[0].ops[2].kind else {
                unreachable!();
            };
            *src2 = SrcOperand::Imm(0xD4);
            assert_rejected(&wrong_status, "SAHF status mask");

            let mut wrong_preserve = original.clone();
            let OpKind::And { src2, .. } = &mut wrong_preserve.blocks[0].ops[3].kind else {
                unreachable!();
            };
            *src2 = SrcOperand::Imm(!0xD4);
            assert_rejected(&wrong_preserve, "SAHF preservation mask");

            let mut wrong_write = original.clone();
            let OpKind::WriteFlags { src } = &mut wrong_write.blocks[0].ops[5].kind else {
                unreachable!();
            };
            *src = x86(X86Reg::Rax);
            assert_rejected(&wrong_write, "SAHF final flag source");
        } else {
            let mut wrong_status = original.clone();
            let OpKind::And { src2, .. } = &mut wrong_status.blocks[0].ops[1].kind else {
                unreachable!();
            };
            *src2 = SrcOperand::Imm(0xD4);
            assert_rejected(&wrong_status, "LAHF status mask");

            let mut wrong_reserved = original.clone();
            let OpKind::Or { src2, .. } = &mut wrong_reserved.blocks[0].ops[2].kind else {
                unreachable!();
            };
            *src2 = SrcOperand::Imm(0);
            assert_rejected(&wrong_reserved, "LAHF fixed reserved bit");

            let mut wrong_shift = original.clone();
            let OpKind::Shl { amount, .. } = &mut wrong_shift.blocks[0].ops[3].kind else {
                unreachable!();
            };
            *amount = SrcOperand::Imm(7);
            assert_rejected(&wrong_shift, "LAHF AH insertion shift");

            let mut wrong_rax_mask = original.clone();
            let OpKind::And { src2, .. } = &mut wrong_rax_mask.blocks[0].ops[4].kind else {
                unreachable!();
            };
            *src2 = SrcOperand::Imm(!0xFE00);
            assert_rejected(&wrong_rax_mask, "LAHF partial-register mask");

            let mut wrong_destination = original.clone();
            let OpKind::Or { dst, .. } = &mut wrong_destination.blocks[0].ops[5].kind else {
                unreachable!();
            };
            *dst = x86(X86Reg::Rbx);
            assert_rejected(&wrong_destination, "LAHF architectural destination");
        }
    }
}
