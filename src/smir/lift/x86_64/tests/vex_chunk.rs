//! Strict-lift, interpretation, and fault-frontier coverage for the VEX
//! 128-bit chunk insert/extract family.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::optimize::{OptLevel, optimize_function};

const INITIAL_FLAGS: u64 = 0x2 | 0x08D5;

fn lift_nonstrict(bytes: &[u8]) -> Result<LiftResult, LiftError> {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    lifter.lift_insn(0x1000, bytes, &mut ctx)
}

fn assert_incomplete(bytes: &[u8], need: usize) {
    let result = lift_single(bytes);
    let debug = format!("{result:?}");
    assert!(
        matches!(
            result,
            Err(LiftError::Incomplete {
                addr: 0x1000,
                have,
                need: actual_need,
            }) if have == bytes.len() && actual_need == need
        ),
        "{bytes:02X?}: {debug}"
    );
}

fn execute_vex_chunk(
    bytes: &[u8],
    level: OptLevel,
    ctx: &mut SmirContext,
    memory: &mut dyn SmirMemory,
) -> BlockResult {
    let lifted = lift_single(bytes).expect("strict VEX chunk lift");
    assert_eq!(lifted.bytes_consumed, bytes.len());
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = builder.finish();
    function.blocks[0].ops = lifted.ops;
    optimize_function(&mut function, level);
    SmirInterpreter::new().execute_block(ctx, memory, &function.blocks[0])
}

fn vector(words: &[u64]) -> [u64; 16] {
    let mut value = [0_u64; 16];
    value[..words.len()].copy_from_slice(words);
    value
}

fn context_with_flags() -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    context.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
    context.flags.lazy = None;
    context
}

#[test]
fn vex_chunk_strictly_lifts_all_four_register_memory_and_extension_forms() {
    for bytes in [
        &[0xC4, 0xE3, 0x75, 0x18, 0xC2, 0x81][..],
        &[0xC4, 0xE3, 0x7D, 0x19, 0xDA, 0xFE][..],
        &[0xC4, 0xE3, 0x55, 0x38, 0xE6, 0xFF][..],
        &[0xC4, 0x63, 0x7D, 0x39, 0xC7, 0xFE][..],
        &[0xC4, 0x43, 0x15, 0x38, 0xE6, 0x01][..],
        &[0xC4, 0x43, 0x7D, 0x39, 0xEE, 0x01][..],
    ] {
        let result =
            lift_single(bytes).unwrap_or_else(|error| panic!("VEX chunk {bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );
        assert!(
            result.ops.iter().any(|op| !op.is_jit_safe()),
            "SIMD state must remain outside native admission until its full state bridge is safe"
        );
    }

    let insert =
        lift_single(&[0xC4, 0xE3, 0x75, 0x18, 0xC2, 0x81]).expect("VINSERTF128 register form");
    assert!(insert.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VAnd {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
            width: VecWidth::V256,
            ..
        }
    )));
    for lane in [2, 3] {
        assert!(insert.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                lane: actual,
                elem: VecElementType::I64,
                ..
            } if actual == lane
        )));
    }
    assert!(insert.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VMov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
            width: VecWidth::V256,
            ..
        }
    )));

    // LLVM MC encodes both inverted VEX.R and VEX.B as zero here:
    // VINSERTI128 ymm12, ymm13, xmm14, 1.
    let extended_insert = lift_single(&[0xC4, 0x43, 0x15, 0x38, 0xE6, 0x01])
        .expect("extended VINSERTI128 register form");
    assert!(extended_insert.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VAnd {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(13))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(13))),
            width: VecWidth::V256,
            ..
        }
    )));
    assert!(extended_insert.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(14))),
            ..
        }
    )));
    assert!(extended_insert.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VMov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(12))),
            width: VecWidth::V256,
            ..
        }
    )));

    let extract = lift_single(&[0xC4, 0x63, 0x7D, 0x39, 0xC7, 0xFE])
        .expect("extended VEXTRACTI128 register form");
    for lane in [0, 1] {
        assert!(extract.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Ymm(8))),
                lane: actual,
                elem: VecElementType::I64,
                ..
            } if actual == lane
        )));
    }
    assert!(extract.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VMov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(7))),
            width: VecWidth::V128,
            ..
        }
    )));

    // VEXTRACTI128 xmm14, ymm13, 1 independently covers the corresponding
    // extended ModR/M.reg source and ModR/M.rm destination.
    let extended_extract = lift_single(&[0xC4, 0x43, 0x7D, 0x39, 0xEE, 0x01])
        .expect("doubly extended VEXTRACTI128 register form");
    assert!(extended_extract.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Ymm(13))),
            ..
        }
    )));
    assert!(extended_extract.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VMov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(14))),
            width: VecWidth::V128,
            ..
        }
    )));

    // Encodings independently assembled by LLVM MC. Both 32-bit addressing
    // and FS/GS segment bases remain part of the lifted address calculation.
    let insert_memory = lift_single(&[0x64, 0x67, 0xC4, 0xE3, 0x65, 0x18, 0x4C, 0x70, 0x01, 0x81])
        .expect("FS addr32 VINSERTF128 memory form");
    assert!(insert_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V128,
            ..
        }
    )));
    assert!(
        !insert_memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    let extract_memory = lift_single(&[0x65, 0x67, 0xC4, 0xE3, 0x7D, 0x39, 0x4C, 0x70, 0x01, 0xFE])
        .expect("GS addr32 VEXTRACTI128 memory form");
    assert!(extract_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VStore {
            width: VecWidth::V128,
            ..
        }
    )));
    assert!(
        !extract_memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );
}

#[test]
fn vex_chunk_reserved_fields_are_terminal_ud_at_the_opcode_frontier() {
    for opcode in [0x18, 0x38] {
        for third in [0xF5, 0x71, 0x74, 0x76, 0x77] {
            let bytes = [0xC4, 0xE3, third, opcode];
            let strict = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("strict insert {bytes:02X?}: {error:?}"));
            assert_invalid_opcode_trap(&strict, bytes.len());
            let nonstrict = lift_nonstrict(&bytes)
                .unwrap_or_else(|error| panic!("nonstrict insert {bytes:02X?}: {error:?}"));
            assert_invalid_opcode_trap(&nonstrict, bytes.len());
        }
    }

    for opcode in [0x19, 0x39] {
        // W=1, L=0, non-reserved logical vvvv, and all non-66 pp values.
        for third in [0xFD, 0x79, 0x75, 0x7C, 0x7E, 0x7F] {
            let bytes = [0xC4, 0xE3, third, opcode];
            let strict = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("strict extract {bytes:02X?}: {error:?}"));
            assert_invalid_opcode_trap(&strict, bytes.len());
            let nonstrict = lift_nonstrict(&bytes)
                .unwrap_or_else(|error| panic!("nonstrict extract {bytes:02X?}: {error:?}"));
            assert_invalid_opcode_trap(&nonstrict, bytes.len());
        }
    }

    let prefixed = [0x64, 0x67, 0xC4, 0xE3, 0xF5, 0x18];
    assert_invalid_opcode_trap(&lift_single(&prefixed).unwrap(), prefixed.len());
    assert_invalid_opcode_trap(&lift_nonstrict(&prefixed).unwrap(), prefixed.len());
}

#[test]
fn vex_chunk_fetch_frontiers_require_prefix_opcode_modrm_address_and_immediate() {
    assert_incomplete(&[0xC4], 3);
    assert_incomplete(&[0xC4, 0xE3], 3);
    assert_incomplete(&[0xC4, 0xE3, 0x75], 4);
    assert_incomplete(&[0xC4, 0xE3, 0x75, 0x18], 5);
    assert_incomplete(&[0xC4, 0xE3, 0x75, 0x18, 0xC2], 6);
    assert_incomplete(&[0xC4, 0xE3, 0x75, 0x18, 0x84], 6);
    assert_incomplete(&[0xC4, 0xE3, 0x75, 0x18, 0x84, 0x88], 10);
    assert_incomplete(&[0xC4, 0xE3, 0x75, 0x18, 0x84, 0x88, 0, 0, 0, 0], 11);
    assert_incomplete(&[0xC4, 0xE3, 0x7D, 0x39, 0xC0], 6);
}

#[test]
fn vex_chunk_interpretation_preserves_bits_aliases_flags_and_upper_zeroing() {
    let first = vector(&[
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0x8000_0000_7FFF_FFFF,
        0x7FC0_1234_FF80_0001,
    ]);
    let second = vector(&[
        0xAAAA_AAAA_5555_5555,
        0x0000_0000_FFFF_FFFF,
        0x1357_9BDF_2468_ACE0,
        0xFFFF_FFFE_0000_0001,
    ]);

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut context = context_with_flags();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!();
        };
        x86.xmm[0] = [u64::MAX; 16];
        x86.xmm[1] = first;
        x86.xmm[2] = second;
        let exit = execute_vex_chunk(
            &[0xC4, 0xE3, 0x75, 0x18, 0xC2, 0xFF],
            level,
            &mut context,
            &mut FlatMemory::new(1),
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        let mut expected = first;
        expected[2..4].copy_from_slice(&second[..2]);
        expected[4..].fill(0);
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[0], expected, "{level:?}: VINSERTF128");
        assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);

        // Destination aliases the 128-bit source. The old source must be read
        // before the architectural YMM write clears its upper state.
        let mut context = context_with_flags();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!();
        };
        x86.xmm[0] = second;
        x86.xmm[1] = first;
        let exit = execute_vex_chunk(
            &[0xC4, 0xE3, 0x75, 0x38, 0xC0, 0x00],
            level,
            &mut context,
            &mut FlatMemory::new(1),
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        let mut expected = first;
        expected[..2].copy_from_slice(&second[..2]);
        expected[4..].fill(0);
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[0], expected, "{level:?}: aliased VINSERTI128");
        assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);

        // Source and destination alias. All bits above the extracted 128-bit
        // result are zero, including ZMM[511:256].
        let mut context = context_with_flags();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!();
        };
        x86.xmm[0] = first;
        let exit = execute_vex_chunk(
            &[0xC4, 0xE3, 0x7D, 0x39, 0xC0, 0xFF],
            level,
            &mut context,
            &mut FlatMemory::new(1),
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!();
        };
        assert_eq!(
            x86.xmm[0],
            vector(&first[2..4]),
            "{level:?}: aliased VEXTRACTI128"
        );
        assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);

        let mut context = context_with_flags();
        context.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x81);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!();
        };
        x86.xmm[1] = first;
        let mut memory = FlatMemory::new(0x200);
        let source_bytes = second[..2]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        memory.write(0x81, &source_bytes).unwrap();
        let exit = execute_vex_chunk(
            &[0xC4, 0xE3, 0x75, 0x38, 0x00, 0x00],
            level,
            &mut context,
            &mut memory,
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        let mut expected = first;
        expected[..2].copy_from_slice(&second[..2]);
        expected[4..].fill(0);
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[0], expected, "{level:?}: memory VINSERTI128");
        assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);

        let mut context = context_with_flags();
        context.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x101);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!();
        };
        x86.xmm[0] = first;
        let mut memory = FlatMemory::new(0x200);
        let exit = execute_vex_chunk(
            &[0xC4, 0xE3, 0x7D, 0x19, 0x00, 0xFE],
            level,
            &mut context,
            &mut memory,
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        let mut stored = [0_u8; 16];
        memory.read(0x101, &mut stored).unwrap();
        let expected = first[..2]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        assert_eq!(stored.as_slice(), expected.as_slice());
        assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);
    }
}

#[test]
fn vex_chunk_memory_faults_precede_architectural_or_partial_store_commit() {
    let first = vector(&[
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0x8000_0000_7FFF_FFFF,
        0x7FC0_1234_FF80_0001,
    ]);
    let sentinel = [0xCCCC_CCCC_CCCC_CCCC_u64; 16];

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut context = context_with_flags();
        context.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0xF8);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!();
        };
        x86.xmm[0] = sentinel;
        x86.xmm[1] = first;
        let fault = execute_vex_chunk(
            &[0xC4, 0xE3, 0x75, 0x18, 0x00, 0x00],
            level,
            &mut context,
            &mut FlatMemory::new(0x100),
        );
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!();
        };
        assert_eq!(x86.xmm[0], sentinel, "{level:?}: insert fault commit");
        assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);

        let mut context = context_with_flags();
        context.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0xF8);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!();
        };
        x86.xmm[0] = first;
        let mut memory = FlatMemory::new(0x100);
        memory.load(0xF8, &[0xA5; 8]);
        let fault = execute_vex_chunk(
            &[0xC4, 0xE3, 0x7D, 0x39, 0x00, 0x01],
            level,
            &mut context,
            &mut memory,
        );
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        let mut after = [0_u8; 8];
        memory.read(0xF8, &mut after).unwrap();
        assert_eq!(after, [0xA5; 8], "{level:?}: partial extract store");
        assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);
    }
}
