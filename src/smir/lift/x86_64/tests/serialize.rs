//! Strict-lift coverage for SERIALIZE.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_serialize(result: &LiftResult) -> &SmirOp {
    assert_eq!(result.ops.len(), 1);
    result
        .ops
        .first()
        .filter(|op| {
            matches!(
                op.kind,
                OpKind::Fence {
                    kind: FenceKind::InstructionSerialize
                }
            )
        })
        .expect("one exact instruction-serialization fence")
}

#[test]
fn serialize_strictly_lifts_without_an_interpreter_frontier() {
    let bytes = [0x0F, 0x01, 0xE8];
    let result = lift_single(&bytes).expect("SERIALIZE must strictly lift");

    assert_eq!(result.bytes_consumed, bytes.len());
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    exact_serialize(&result);
}

#[test]
fn serialize_ignores_legacy_size_segment_address_and_rex_prefixes() {
    for bytes in [
        &[0x66, 0x0F, 0x01, 0xE8][..],
        &[0x67, 0x0F, 0x01, 0xE8],
        &[0x64, 0x0F, 0x01, 0xE8],
        &[0x48, 0x0F, 0x01, 0xE8],
    ] {
        let result = lift_single(bytes).expect("architecturally ignored SERIALIZE prefix");
        assert_eq!(result.bytes_consumed, bytes.len());
        exact_serialize(&result);
    }
}

#[test]
fn serialize_distinguishes_fixed_prefix_aliases_and_rejects_lock() {
    for bytes in [&[0xF2, 0x0F, 0x01, 0xE8][..], &[0xF3, 0x0F, 0x01, 0xE8]] {
        let result = lift_single(bytes).expect("unsupported fixed alias must lift to #UD");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.is_empty());
        assert!(matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x01, 0xE8]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}

#[test]
fn serialize_metadata_is_side_effecting_flag_neutral_and_jit_safe() {
    let op = exact_serialize(&lift_single(&[0x0F, 0x01, 0xE8]).unwrap()).clone();
    assert!(op.kind.source_vregs().is_empty());
    assert!(op.kind.dests().is_empty());
    assert!(op.kind.has_side_effects());
    assert!(!op.kind.reads_memory());
    assert!(!op.kind.writes_memory());
    assert!(op.is_jit_safe());
}

#[test]
fn serialize_interpreter_preserves_registers_and_flags() {
    let result = lift_single(&[0x0F, 0x01, 0xE8]).unwrap();
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });

    let flags = MaterializedFlags {
        cf: true,
        zf: false,
        sf: true,
        of: false,
        pf: true,
        af: false,
        df: true,
    };
    let mut context = SmirContext::new_x86_64();
    context.flags.materialized = flags;
    context.flags.lazy = None;
    context.write_vreg(x86_gpr(0), 0x1122_3344_5566_7788);
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.rflags = 0x0004_0402;

    let execution =
        SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &block);
    assert!(matches!(execution, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86_gpr(0)), 0x1122_3344_5566_7788);
    assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rflags, 0x0004_0402);
}

#[test]
fn serialize_survives_o2_without_data_destinations() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Fence {
            kind: FenceKind::InstructionSerialize,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);

    assert!(function.entry_block().unwrap().ops.iter().any(|op| {
        matches!(
            op.kind,
            OpKind::Fence {
                kind: FenceKind::InstructionSerialize
            }
        )
    }));
}
