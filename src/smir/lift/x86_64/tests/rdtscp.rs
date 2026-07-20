//! Strict lift, metadata, optimizer, and canonical interpreter coverage for
//! RDTSC/RDTSCP.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn exact_timestamp(result: &LiftResult) -> &X86ReadTscOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86ReadTsc(read) => read,
        other => panic!("expected one exact timestamp-read op, got {other:?}"),
    }
}

fn timestamp_block(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict timestamp lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn execute_timestamp(
    bytes: &[u8],
    configure: impl FnOnce(&mut SmirContext),
) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    configure(&mut context);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &timestamp_block(bytes),
    );
    (result, context)
}

#[test]
fn rdtscp_strictly_lifts_without_an_interpreter_frontier() {
    let bytes = [0x0F, 0x01, 0xF9];
    let result = lift_single(&bytes).expect("RDTSCP must strictly lift");
    assert_eq!(result.bytes_consumed, bytes.len());
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert!(matches!(
        exact_timestamp(&result),
        X86ReadTscOp {
            dst_lo: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            dst_hi: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
            dst_aux: Some(VReg::Arch(ArchReg::X86(X86Reg::Rcx))),
        }
    ));
}

#[test]
fn timestamp_reads_ignore_non_lock_legacy_and_rex_prefixes() {
    for bytes in [
        &[0x66, 0x0F, 0x01, 0xF9][..],
        &[0x67, 0x0F, 0x01, 0xF9],
        &[0x48, 0x0F, 0x01, 0xF9],
        &[0xF2, 0x0F, 0x01, 0xF9],
        &[0xF3, 0x0F, 0x01, 0xF9],
        &[0x66, 0x0F, 0x31],
        &[0x67, 0x0F, 0x31],
        &[0x48, 0x0F, 0x31],
        &[0xF2, 0x0F, 0x31],
        &[0xF3, 0x0F, 0x31],
    ] {
        let result = lift_single(bytes).expect("architecturally ignored timestamp prefix");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        exact_timestamp(&result);
    }

    for bytes in [&[0xF0, 0x0F, 0x01, 0xF9][..], &[0xF0, 0x0F, 0x31]] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn timestamp_metadata_tracks_exact_destinations_and_volatile_read() {
    let rdtsc = lift_single(&[0x0F, 0x31]).unwrap();
    let rdtscp = lift_single(&[0x0F, 0x01, 0xF9]).unwrap();
    for (result, expected_dests) in [
        (&rdtsc, vec![x86(X86Reg::Rax), x86(X86Reg::Rdx)]),
        (
            &rdtscp,
            vec![x86(X86Reg::Rax), x86(X86Reg::Rdx), x86(X86Reg::Rcx)],
        ),
    ] {
        let op = &result.ops[0];
        assert!(op.kind.source_vregs().is_empty());
        assert_eq!(op.kind.dests(), expected_dests);
        assert!(op.kind.has_side_effects());
        assert!(!op.kind.reads_memory());
        assert!(!op.kind.writes_memory());
        assert!(op.is_jit_safe());
    }
}

#[test]
fn rdtscp_interpreter_reads_cycle_and_guest_aux_with_zero_extending_writes() {
    let flags = MaterializedFlags {
        cf: true,
        zf: false,
        sf: true,
        of: true,
        pf: false,
        af: true,
        df: true,
    };
    let (result, context) = execute_timestamp(&[0x0F, 0x01, 0xF9], |context| {
        context.cycle_count = 0x1234_5678_9ABC_DEF0;
        context.flags.materialized = flags;
        context.write_vreg(x86(X86Reg::Rax), u64::MAX);
        context.write_vreg(x86(X86Reg::Rdx), u64::MAX);
        context.write_vreg(x86(X86Reg::Rcx), u64::MAX);
        context.write_vreg(x86(X86Reg::Rbx), 0xA5A5_5A5A_F0F0_0F0F);
        let ArchRegState::X86_64(x86_state) = &mut context.arch_regs else {
            unreachable!()
        };
        x86_state.tsc_aux = 0x89AB_CDEF;
    });

    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0x9ABC_DEF0);
    assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0x1234_5678);
    assert_eq!(context.read_vreg(x86(X86Reg::Rcx)), 0x89AB_CDEF);
    assert_eq!(context.read_vreg(x86(X86Reg::Rbx)), 0xA5A5_5A5A_F0F0_0F0F);
    assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
    assert!(context.flags.lazy.is_none());
}

#[test]
fn timestamp_interpreter_tsd_fault_is_precise_and_noncommitting() {
    for bytes in [&[0x0F, 0x31][..], &[0x0F, 0x01, 0xF9]] {
        let (result, context) = execute_timestamp(bytes, |context| {
            let ArchRegState::X86_64(x86_state) = &mut context.arch_regs else {
                unreachable!()
            };
            x86_state.cr0 = 1;
            x86_state.cr4 = 1 << 2;
            x86_state.cpl = 3;
            x86_state.tsc_aux = 0x89AB_CDEF;
            context.cycle_count = 0x1234_5678_9ABC_DEF0;
            context.write_vreg(x86(X86Reg::Rax), 0x1111);
            context.write_vreg(x86(X86Reg::Rdx), 0x2222);
            context.write_vreg(x86(X86Reg::Rcx), 0x3333);
        });
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::GeneralProtection {
                addr: 0x1000,
                error_code: 0
            })
        ));
        assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0x1111);
        assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0x2222);
        assert_eq!(context.read_vreg(x86(X86Reg::Rcx)), 0x3333);
    }
}

#[test]
fn timestamp_interpreter_allows_each_architectural_tsd_bypass() {
    for (cr0, cr4, cpl) in [(0, 1 << 2, 3), (1, 0, 3), (1, 1 << 2, 0)] {
        let (result, context) = execute_timestamp(&[0x0F, 0x01, 0xF9], |context| {
            let ArchRegState::X86_64(x86_state) = &mut context.arch_regs else {
                unreachable!()
            };
            x86_state.cr0 = cr0;
            x86_state.cr4 = cr4;
            x86_state.cpl = cpl;
            x86_state.tsc_aux = 0xCAFE_BABE;
            context.cycle_count = 0x0123_4567_89AB_CDEF;
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0x89AB_CDEF);
        assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0x0123_4567);
        assert_eq!(context.read_vreg(x86(X86Reg::Rcx)), 0xCAFE_BABE);
    }
}

#[test]
fn timestamp_reads_survive_o2_in_program_order() {
    let first = lift_single(&[0x0F, 0x31]).unwrap().ops.remove(0);
    let second = lift_single(&[0x0F, 0x01, 0xF9]).unwrap().ops.remove(0);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, first.kind);
    builder.push_op(0x1002, second.kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    let reads: Vec<_> = function
        .entry_block()
        .unwrap()
        .ops
        .iter()
        .filter_map(|op| match &op.kind {
            OpKind::X86ReadTsc(read) => Some(read.dst_aux),
            _ => None,
        })
        .collect();
    assert_eq!(reads, vec![None, Some(x86(X86Reg::Rcx))]);
}
