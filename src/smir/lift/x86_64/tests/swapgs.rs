//! Strict lift and canonical interpreter coverage for SWAPGS.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn exact_swapgs(result: &LiftResult) -> &SmirOp {
    assert_eq!(result.ops.len(), 1);
    result
        .ops
        .first()
        .filter(|op| matches!(op.kind, OpKind::X86SwapGs { .. }))
        .expect("one exact SWAPGS semantic op")
}

fn swapgs_block(count: usize) -> SmirBlock {
    let lifted = lift_single(&[0x0F, 0x01, 0xF8]).expect("strict SWAPGS lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    for index in 0..count {
        let mut op = exact_swapgs(&lifted).clone();
        op.id = OpId(index as u16);
        op.guest_pc = 0x1000 + (index as u64) * 3;
        block.ops.push(op);
    }
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn execute_swapgs(
    count: usize,
    configure: impl FnOnce(&mut SmirContext),
) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    configure(&mut context);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &swapgs_block(count),
    );
    (result, context)
}

#[test]
fn swapgs_strictly_lifts_without_an_interpreter_frontier() {
    let bytes = [0x0F, 0x01, 0xF8];
    let result = lift_single(&bytes).expect("SWAPGS must strictly lift");

    assert_eq!(result.bytes_consumed, bytes.len());
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert!(matches!(
        exact_swapgs(&result).kind,
        OpKind::X86SwapGs {
            gs_base,
            kernel_gs_base,
        } if gs_base == x86(X86Reg::GsBase)
            && kernel_gs_base == x86(X86Reg::KernelGsBase)
    ));
}

#[test]
fn swapgs_ignores_non_lock_legacy_rex_and_repeat_prefixes() {
    for bytes in [
        &[0x66, 0x0F, 0x01, 0xF8][..],
        &[0x67, 0x0F, 0x01, 0xF8],
        &[0x64, 0x0F, 0x01, 0xF8],
        &[0x48, 0x0F, 0x01, 0xF8],
        &[0xF2, 0x0F, 0x01, 0xF8],
        &[0xF3, 0x0F, 0x01, 0xF8],
    ] {
        let result = lift_single(bytes).expect("architecturally ignored SWAPGS prefix");
        assert_eq!(result.bytes_consumed, bytes.len());
        exact_swapgs(&result);
    }

    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x01, 0xF8]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}

#[test]
fn swapgs_metadata_exposes_simultaneous_state_dataflow_and_faults() {
    let op = exact_swapgs(&lift_single(&[0x0F, 0x01, 0xF8]).unwrap()).clone();
    let state = vec![x86(X86Reg::GsBase), x86(X86Reg::KernelGsBase)];
    assert_eq!(op.kind.source_vregs(), state);
    assert_eq!(op.kind.dests(), state);
    assert!(op.kind.has_side_effects());
    assert!(!op.kind.reads_memory());
    assert!(!op.kind.writes_memory());
    assert!(op.is_jit_safe());
}

#[test]
fn swapgs_interpreter_swaps_atomically_is_an_involution_and_preserves_flags() {
    let flags = MaterializedFlags {
        cf: true,
        zf: false,
        sf: true,
        of: false,
        pf: true,
        af: false,
        df: true,
    };
    let old_gs = 0x0000_7FFF_1234_5000;
    let old_kernel = 0xFFFF_8000_ABCD_E000;
    let (result, context) = execute_swapgs(1, |context| {
        context.flags.materialized = flags;
        context.write_vreg(x86(X86Reg::GsBase), old_gs);
        context.write_vreg(x86(X86Reg::KernelGsBase), old_kernel);
        context.write_vreg(x86(X86Reg::Rax), 0x1122_3344_5566_7788);
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86(X86Reg::GsBase)), old_kernel);
    assert_eq!(context.read_vreg(x86(X86Reg::KernelGsBase)), old_gs);
    assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0x1122_3344_5566_7788);
    assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
    assert!(context.flags.lazy.is_none());

    let (result, context) = execute_swapgs(2, |context| {
        context.write_vreg(x86(X86Reg::GsBase), old_gs);
        context.write_vreg(x86(X86Reg::KernelGsBase), old_kernel);
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86(X86Reg::GsBase)), old_gs);
    assert_eq!(context.read_vreg(x86(X86Reg::KernelGsBase)), old_kernel);
}

#[test]
fn swapgs_interpreter_cpl_fault_precedes_both_state_writes() {
    let old_gs = 0x1234;
    let old_kernel = 0xFFFF_8000_0000_5678;
    let (result, context) = execute_swapgs(1, |context| {
        let ArchRegState::X86_64(x86_state) = &mut context.arch_regs else {
            unreachable!()
        };
        x86_state.cpl = 3;
        x86_state.gs_base = old_gs;
        x86_state.kernel_gs_base = old_kernel;
    });
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0
        })
    ));
    assert_eq!(context.read_vreg(x86(X86Reg::GsBase)), old_gs);
    assert_eq!(context.read_vreg(x86(X86Reg::KernelGsBase)), old_kernel);
}

#[test]
fn swapgs_survives_o2_with_exact_fixed_state_operands() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86SwapGs {
            gs_base: x86(X86Reg::GsBase),
            kernel_gs_base: x86(X86Reg::KernelGsBase),
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);

    assert!(function.entry_block().unwrap().ops.iter().any(|op| {
        matches!(
            op.kind,
            OpKind::X86SwapGs {
                gs_base: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
                kernel_gs_base: VReg::Arch(ArchReg::X86(X86Reg::KernelGsBase)),
            }
        )
    }));
}
