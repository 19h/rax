//! Strict lift, interpretation, optimizer, and native-gate coverage for legacy SHA-NI.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, VecValue};
use crate::smir::ir::memory::FlatMemory;

const SRC1_LO: u64 = 0x0123_4567_89AB_CDEF;
const SRC1_HI: u64 = 0xFEDC_BA98_7654_3210;
const SRC2_LO: u64 = 0x0F1E_2D3C_4B5A_6978;
const SRC2_HI: u64 = 0x8877_6655_4433_2211;
const WK_LO: u64 = 0x1020_3040_5060_7080;

fn exact_sha(result: &LiftResult) -> &SmirOp {
    result
        .ops
        .iter()
        .find(|op| matches!(op.kind, OpKind::X86Sha32 { .. }))
        .expect("one exact SHA-NI semantic op")
}

fn block_for(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).unwrap();
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn set_xmm(context: &mut SmirContext, index: usize, value: VecValue) {
    match &mut context.arch_regs {
        ArchRegState::X86_64(state) => state.xmm[index] = value,
        _ => unreachable!(),
    }
}

fn get_xmm(context: &SmirContext, index: usize) -> VecValue {
    match &context.arch_regs {
        ArchRegState::X86_64(state) => state.xmm[index],
        _ => unreachable!(),
    }
}

fn seeded_context() -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    let mut destination = [0u64; 16];
    destination[0] = SRC1_LO;
    destination[1] = SRC1_HI;
    for (index, lane) in destination.iter_mut().enumerate().skip(2) {
        *lane = 0xA5A5_0000_0000_0000 | index as u64;
    }
    let mut source = [0u64; 16];
    source[0] = SRC2_LO;
    source[1] = SRC2_HI;
    let mut wk = [0u64; 16];
    wk[0] = WK_LO;
    wk[1] = 0xDEAD_BEEF_CAFE_BABE;
    set_xmm(&mut context, 2, destination);
    set_xmm(&mut context, 1, source);
    set_xmm(&mut context, 0, wk);
    context
}

fn interpret_sha(bytes: &[u8]) -> VecValue {
    let mut context = seeded_context();
    let mut memory = FlatMemory::new(1);
    assert!(matches!(
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &block_for(bytes)),
        BlockResult::Exit(ExitReason::Halt)
    ));
    get_xmm(&context, 2)
}

#[test]
fn legacy_sha_ni_strictly_lifts_every_exact_opcode_and_dependency() {
    for (bytes, expected_op, expected_wk, imm) in [
        (
            &[0x0F, 0x38, 0xC8, 0xD1][..],
            X86Sha32Op::Sha1Nexte,
            None,
            0,
        ),
        (&[0x0F, 0x38, 0xC9, 0xD1][..], X86Sha32Op::Sha1Msg1, None, 0),
        (&[0x0F, 0x38, 0xCA, 0xD1][..], X86Sha32Op::Sha1Msg2, None, 0),
        (
            &[0x0F, 0x38, 0xCB, 0xD1][..],
            X86Sha32Op::Sha256Rounds2,
            Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)))),
            0,
        ),
        (
            &[0x0F, 0x38, 0xCC, 0xD1][..],
            X86Sha32Op::Sha256Msg1,
            None,
            0,
        ),
        (
            &[0x0F, 0x38, 0xCD, 0xD1][..],
            X86Sha32Op::Sha256Msg2,
            None,
            0,
        ),
        (
            &[0x0F, 0x3A, 0xCC, 0xD1, 0xFD][..],
            X86Sha32Op::Sha1Rounds4,
            None,
            0xFD,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        let sha = exact_sha(&result);
        match &sha.kind {
            OpKind::X86Sha32 {
                dst,
                src1,
                src2,
                wk,
                op,
                imm: actual_imm,
            } => {
                assert!(matches!(dst, VReg::Virtual(_)), "{bytes:02X?}");
                assert_eq!(*src1, VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))));
                assert_eq!(*src2, VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))));
                assert_eq!(*wk, expected_wk);
                assert_eq!(*op, expected_op);
                assert_eq!(*actual_imm, imm);
                let mut sources = vec![*src1, *src2];
                sources.extend(*wk);
                assert_eq!(sha.kind.source_vregs(), sources);
                assert_eq!(sha.kind.dests(), vec![*dst]);
            }
            _ => unreachable!(),
        }
        assert!(sha.kind.flags_written().is_empty());
        assert!(!sha.kind.is_jit_safe());
        assert!(!sha.is_jit_safe());
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                ..
            }
        )));
    }

    let high = lift_single(&[0x45, 0x0F, 0x38, 0xC8, 0xC1]).unwrap();
    assert!(matches!(
        exact_sha(&high).kind,
        OpKind::X86Sha32 {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(8))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
            ..
        }
    ));
}

#[test]
fn legacy_sha_ni_tracks_memory_addressing_alignment_and_full_immediate_length() {
    let rip = lift_single(&[0x0F, 0x3A, 0xCC, 0x05, 0x20, 0x00, 0x00, 0x00, 0x03]).unwrap();
    assert_eq!(rip.bytes_consumed, 9);
    let alignment = rip
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .unwrap();
    let load = rip
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .unwrap();
    let sha = rip
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86Sha32 { .. }))
        .unwrap();
    assert!(alignment < load && load < sha);
    assert!(matches!(
        &rip.ops[load].kind,
        OpKind::VLoad {
            addr: Address::PcRel {
                offset: 0x20,
                base: Some(0x1009),
                ..
            },
            ..
        }
    ));

    let addr32 = lift_single(&[0x64, 0x67, 0x0F, 0x38, 0xC9, 0x04, 0x88]).unwrap();
    assert_eq!(addr32.bytes_consumed, 7);
    assert!(addr32.ops.iter().any(|op| matches!(
        &op.kind,
        OpKind::VLoad {
            addr: Address::X86Addr32(inner),
            width: VecWidth::V128,
            ..
        } if matches!(
            inner.as_ref(),
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
                index: Some(VReg::Arch(ArchReg::X86(X86Reg::Rcx))),
                scale: 4,
                disp: 0,
            }
        )
    )));
}

#[test]
fn legacy_sha_ni_accepts_redundant_prefixes_and_rejects_faulting_encodings() {
    for opcode in 0xC8..=0xCD {
        for prefix in [0x66, 0xF2, 0xF3] {
            let bytes = [prefix, 0x0F, 0x38, opcode, 0xC1];
            assert_eq!(lift_single(&bytes).unwrap().bytes_consumed, bytes.len());
        }
    }
    for prefix in [0x66, 0xF2, 0xF3] {
        let bytes = [prefix, 0x0F, 0x3A, 0xCC, 0xC1, 0x03];
        assert_eq!(lift_single(&bytes).unwrap().bytes_consumed, bytes.len());
    }

    for bytes in [
        &[0xF0, 0x0F, 0x38, 0xC8, 0xC1][..],
        &[0xF0, 0x0F, 0x3A, 0xCC, 0xC1, 0x03][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "invalid SHA-NI encoding accepted: {bytes:02X?}",
        );
    }
    for bytes in [
        &[0xD5, 0x00, 0x0F, 0x38, 0xC8, 0xC1][..],
        &[0xD5, 0x00, 0x0F, 0x3A, 0xCC, 0xC1, 0x03][..],
    ] {
        let result = lift_single(bytes).expect("REX2 followed by 0F is an explicit #UD");
        assert_invalid_opcode_trap(&result, 3);
    }
    for bytes in [
        &[0x0F, 0x38, 0xC8][..],
        &[0x0F, 0x38, 0xC8, 0x84, 0x88, 0x00, 0x00][..],
        &[0x0F, 0x3A, 0xCC, 0xC1][..],
        &[0x0F, 0x3A, 0xCC, 0x84, 0x88, 0x00, 0x00, 0x00][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::Incomplete { .. })),
            "truncated SHA-NI encoding did not report Incomplete: {bytes:02X?}",
        );
    }
}

#[test]
fn legacy_sha_ni_family_stays_in_one_strict_block_before_terminal_frontier() {
    let memory = TestMemory::new(
        0x2000,
        vec![
            0x0F, 0x38, 0xC8, 0xD1, // SHA1NEXTE xmm2,xmm1
            0x0F, 0x38, 0xC9, 0xD1, // SHA1MSG1 xmm2,xmm1
            0x0F, 0x38, 0xCA, 0xD1, // SHA1MSG2 xmm2,xmm1
            0x0F, 0x38, 0xCB, 0xD1, // SHA256RNDS2 xmm2,xmm1,<xmm0>
            0x0F, 0x38, 0xCC, 0xD1, // SHA256MSG1 xmm2,xmm1
            0x0F, 0x38, 0xCD, 0xD1, // SHA256MSG2 xmm2,xmm1
            0x0F, 0x3A, 0xCC, 0xD1, 0x03, // SHA1RNDS4 xmm2,xmm1,3
            0xF4,
        ],
    );
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut context = LiftContext::new(SourceArch::X86_64);
    let function = lifter.lift_function(0x2000, &memory, &mut context).unwrap();
    let block = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x2000)
        .expect("SHA-NI entry block");
    let frontier = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x201D)
        .expect("exact HLT interpreter frontier");
    assert_eq!(
        block
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86Sha32 { .. }))
            .count(),
        7
    );
    assert!(matches!(
        block.terminator,
        Terminator::Branch { target } if target == frontier.id
    ));
    assert!(frontier.ops.is_empty());
    assert!(matches!(frontier.terminator, Terminator::Return { .. }));
}

#[test]
fn legacy_sha_ni_interpreter_matches_sdm_vectors_and_preserves_upper_state() {
    for (bytes, expected_lo, expected_hi) in [
        (
            &[0x0F, 0x38, 0xC8, 0xD1][..],
            0x0F1E_2D3C_4B5A_6978,
            0xC82E_94FB_4433_2211,
        ),
        (
            &[0x0F, 0x38, 0xC9, 0xD1][..],
            0x8954_2332_CD98_EFFE,
            0xFFFF_FFFF_FFFF_FFFF,
        ),
        (
            &[0x0F, 0x38, 0xCA, 0xD1][..],
            0x94F2_583E_F8E9_F9F9,
            0x75DF_3113_F294_3E58,
        ),
        (
            &[0x0F, 0x38, 0xCB, 0xD1][..],
            0x6CF6_8933_3446_DF70,
            0x05D7_0AD6_1792_9B80,
        ),
        (
            &[0x0F, 0x38, 0xCC, 0xD1][..],
            0x23C5_791A_A92B_BC5D,
            0x6280_A5C3_76D4_43A1,
        ),
        (
            &[0x0F, 0x38, 0xCD, 0xD1][..],
            0x60E5_AE53_7F07_5446,
            0x61D8_9F9D_B708_63C6,
        ),
        (
            &[0x0F, 0x3A, 0xCC, 0xD1, 0x00][..],
            0x536E_35C2_120F_403F,
            0xE7FC_996B_3F93_DA09,
        ),
        (
            &[0x0F, 0x3A, 0xCC, 0xD1, 0x01][..],
            0x95F3_F135_7471_57EB,
            0x9341_F2E5_6CFB_769D,
        ),
        (
            &[0x0F, 0x3A, 0xCC, 0xD1, 0xFE][..],
            0x8111_CC17_FD13_6EED,
            0xF02F_9E72_65D8_69E5,
        ),
        (
            &[0x0F, 0x3A, 0xCC, 0xD1, 0xFF][..],
            0x891C_D865_8B53_8D78,
            0xD379_D9C9_787C_7EFD,
        ),
    ] {
        let result = interpret_sha(bytes);
        assert_eq!(result[0], expected_lo, "{bytes:02X?}");
        assert_eq!(result[1], expected_hi, "{bytes:02X?}");
        for (index, lane) in result.iter().enumerate().skip(2) {
            assert_eq!(*lane, 0xA5A5_0000_0000_0000 | index as u64);
        }
    }

    // Destination/source aliasing must snapshot the full source before the
    // legacy lane-by-lane architectural writeback begins.
    let alias = interpret_sha(&[0x0F, 0x38, 0xC9, 0xD2]);
    assert_eq!(alias[0], u64::MAX);
    assert_eq!(alias[1], u64::MAX);
    for (index, lane) in alias.iter().enumerate().skip(2) {
        assert_eq!(*lane, 0xA5A5_0000_0000_0000 | index as u64);
    }
}

#[test]
fn legacy_sha_ni_copy_propagation_rewrites_every_pure_source() {
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    for (dst, src) in [
        (VReg::virt(3), VReg::virt(0)),
        (VReg::virt(4), VReg::virt(1)),
        (VReg::virt(5), VReg::virt(2)),
    ] {
        block.push_op(SmirOp::new(
            OpId(block.ops.len() as u16),
            0x1000,
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(src),
                width: OpWidth::W64,
            },
        ));
    }
    block.push_op(SmirOp::new(
        OpId(3),
        0x1000,
        OpKind::X86Sha32 {
            dst: VReg::virt(6),
            src1: VReg::virt(3),
            src2: VReg::virt(4),
            wk: Some(VReg::virt(5)),
            op: X86Sha32Op::Sha256Rounds2,
            imm: 0,
        },
    ));

    assert_eq!(crate::smir::optimize::copy_propagation(&mut block), 3);
    assert!(matches!(
        block.ops[3].kind,
        OpKind::X86Sha32 {
            src1: VReg::Virtual(VirtualId(0)),
            src2: VReg::Virtual(VirtualId(1)),
            wk: Some(VReg::Virtual(VirtualId(2))),
            ..
        }
    ));
}

#[test]
fn malformed_sha256_rounds_without_xmm0_dependency_exits_undefined() {
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(SmirOp::new(
        OpId(0),
        0x1000,
        OpKind::X86Sha32 {
            dst: VReg::virt(0),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            wk: None,
            op: X86Sha32Op::Sha256Rounds2,
            imm: 0,
        },
    ));
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut context = seeded_context();
    let mut memory = FlatMemory::new(1);
    assert!(matches!(
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &block),
        BlockResult::Exit(ExitReason::Undefined {
            addr: 0x1000,
            opcode: 0,
        })
    ));
}

#[test]
fn legacy_sha_ni_alignment_fault_precedes_memory_access() {
    let block = block_for(&[0x0F, 0x38, 0xC9, 0x00]);
    let mut memory = FlatMemory::with_base(0x3000, 0x100);

    let mut misaligned = SmirContext::new_x86_64();
    misaligned.write_vreg(x86_gpr(0), 0xDEAD_0001);
    assert!(matches!(
        SmirInterpreter::new().execute_block(&mut misaligned, &mut memory, &block),
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0,
        })
    ));

    let mut aligned = SmirContext::new_x86_64();
    aligned.write_vreg(x86_gpr(0), 0xDEAD_0010);
    assert!(matches!(
        SmirInterpreter::new().execute_block(&mut aligned, &mut memory, &block),
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: 0xDEAD_0010,
            write: false,
        })
    ));
}

#[test]
fn legacy_sha_ni_o2_preserves_semantics_and_native_admission_stays_closed() {
    let original = block_for(&[0x0F, 0x38, 0xCB, 0xD1]);
    let mut function = SmirFunction::new(FunctionId(0), original.id, 0x1000);
    function.add_block(original.clone());
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
    let optimized = function.entry_block().unwrap();
    let optimized_sha = optimized
        .ops
        .iter()
        .find(|op| matches!(op.kind, OpKind::X86Sha32 { .. }))
        .expect("O2 must retain the SHA result feeding architectural XMM2");

    let mut original_context = seeded_context();
    let mut optimized_context = seeded_context();
    let mut original_memory = FlatMemory::new(1);
    let mut optimized_memory = FlatMemory::new(1);
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut original_context,
            &mut original_memory,
            &original,
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut optimized_context,
            &mut optimized_memory,
            optimized,
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(
        get_xmm(&optimized_context, 2),
        get_xmm(&original_context, 2)
    );
    assert!(!optimized_sha.kind.is_jit_safe());
    #[cfg(feature = "smir-jit")]
    assert!(!crate::smir::lower::runtime::is_x86_native_vector_op(
        &optimized_sha.kind
    ));
}
