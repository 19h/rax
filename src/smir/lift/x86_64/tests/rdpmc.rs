//! Strict lift, metadata, optimizer, and canonical interpreter coverage for
//! RDPMC under the deterministic legacy-PMU profile.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::X86ReadPmcOp;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn exact_pmc(result: &LiftResult) -> &X86ReadPmcOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86ReadPmc(read) => read,
        other => panic!("expected one exact performance-counter read, got {other:?}"),
    }
}

fn execute_pmc(configure: impl FnOnce(&mut SmirContext)) -> (BlockResult, SmirContext) {
    let lifted = lift_single(&[0x0F, 0x33]).expect("strict RDPMC lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut context = SmirContext::new_x86_64();
    configure(&mut context);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &block);
    (result, context)
}

#[test]
fn rdpmc_strictly_lifts_with_exact_implicit_registers() {
    let result = lift_single(&[0x0F, 0x33]).expect("RDPMC must strictly lift");
    assert_eq!(result.bytes_consumed, 2);
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert!(matches!(
        exact_pmc(&result),
        X86ReadPmcOp {
            dst_lo: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            dst_hi: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
            selector: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
        }
    ));
}

#[test]
fn rdpmc_ignores_legacy_and_rex_prefixes_but_rejects_lock_and_rex2() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
        0x66, 0x67, // operand/address size
        0x40, 0x48, // ordinary REX and REX.W
        0xF2, 0xF3, // repeat prefixes
    ] {
        let bytes = [prefix, 0x0F, 0x33];
        let result = lift_single(&bytes).expect("architecturally ignored RDPMC prefix");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        exact_pmc(&result);
    }

    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x33]),
        Err(LiftError::InvalidEncoding { .. })
    ));

    let reserved_row =
        lift_single(&[0xD5, 0x80, 0x33]).expect("REX2 compressed map 1 row 3 is #UD");
    assert_invalid_opcode_trap(&reserved_row, 3);
}

#[test]
fn rdpmc_metadata_tracks_selector_destinations_and_volatile_read() {
    let result = lift_single(&[0x0F, 0x33]).unwrap();
    let op = &result.ops[0];
    assert_eq!(op.kind.source_vregs(), vec![x86(X86Reg::Rcx)]);
    assert_eq!(op.kind.dests(), vec![x86(X86Reg::Rax), x86(X86Reg::Rdx)]);
    assert!(op.kind.has_side_effects());
    assert!(!op.kind.reads_memory());
    assert!(!op.kind.writes_memory());
    assert!(op.is_jit_safe());
}

#[test]
fn rdpmc_interpreter_masks_to_40_bits_zero_extends_and_preserves_state() {
    let flags = MaterializedFlags {
        cf: true,
        zf: false,
        sf: true,
        of: true,
        pf: false,
        af: true,
        df: true,
        ac: true,
    };
    let (result, context) = execute_pmc(|context| {
        context.cycle_count = 0xABCD_EF12_3456_7890;
        context.flags.materialized = flags;
        context.write_vreg(x86(X86Reg::Rax), u64::MAX);
        context.write_vreg(x86(X86Reg::Rdx), u64::MAX);
        context.write_vreg(x86(X86Reg::Rcx), 0xFFFF_FFFF_0000_0007);
        context.write_vreg(x86(X86Reg::Rbx), 0xA5A5_5A5A_F0F0_0F0F);
    });

    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0x3456_7890);
    assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0x12);
    assert_eq!(context.read_vreg(x86(X86Reg::Rcx)), 0xFFFF_FFFF_0000_0007);
    assert_eq!(context.read_vreg(x86(X86Reg::Rbx)), 0xA5A5_5A5A_F0F0_0F0F);
    assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
    assert!(context.flags.lazy.is_none());
}

#[test]
fn rdpmc_interpreter_fast_mode_returns_only_low_32_bits() {
    let (result, context) = execute_pmc(|context| {
        context.cycle_count = 0xABCD_EF12_3456_7890;
        context.write_vreg(x86(X86Reg::Rcx), 0x8000_0000);
        context.write_vreg(x86(X86Reg::Rdx), u64::MAX);
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0x3456_7890);
    assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0);
}

#[test]
fn rdpmc_interpreter_faults_are_precise_and_noncommitting() {
    for (selector, cr0, cr4, cpl) in [
        (8, 1, 1 << 8, 3),      // invalid model-specific selector
        (0, 1, 0, 3),           // protected CPL3 with CR4.PCE clear
        (0x4000_0000, 1, 0, 0), // architectural fixed type is invalid in v0
    ] {
        let (result, context) = execute_pmc(|context| {
            let ArchRegState::X86_64(x86_state) = &mut context.arch_regs else {
                unreachable!()
            };
            x86_state.cr0 = cr0;
            x86_state.cr4 = cr4;
            x86_state.cpl = cpl;
            context.cycle_count = u64::MAX;
            context.write_vreg(x86(X86Reg::Rax), 0x1111);
            context.write_vreg(x86(X86Reg::Rdx), 0x2222);
            context.write_vreg(x86(X86Reg::Rcx), selector);
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
        assert_eq!(context.read_vreg(x86(X86Reg::Rcx)), selector);
    }
}

#[test]
fn rdpmc_interpreter_allows_each_architectural_privilege_bypass() {
    for (cr0, cr4, cpl) in [(0, 0, 3), (1, 1 << 8, 3), (1, 0, 0)] {
        let (result, context) = execute_pmc(|context| {
            let ArchRegState::X86_64(x86_state) = &mut context.arch_regs else {
                unreachable!()
            };
            x86_state.cr0 = cr0;
            x86_state.cr4 = cr4;
            x86_state.cpl = cpl;
            context.cycle_count = 0x0123_4567_89AB_CDEF;
            context.write_vreg(x86(X86Reg::Rcx), 0);
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0x89AB_CDEF);
        assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0x67);
    }
}

#[test]
fn rdpmc_reads_survive_o2_in_program_order() {
    let kind = lift_single(&[0x0F, 0x33]).unwrap().ops.remove(0).kind;
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind.clone());
    builder.push_op(0x1002, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86ReadPmc(..)))
            .count(),
        2
    );
}
