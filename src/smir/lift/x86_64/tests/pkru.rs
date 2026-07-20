//! Strict lift and canonical interpreter coverage for RDPKRU/WRPKRU.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn exact_pkru(result: &LiftResult) -> &SmirOp {
    assert_eq!(result.ops.len(), 1);
    result
        .ops
        .first()
        .filter(|op| matches!(op.kind, OpKind::X86Pkru { .. }))
        .expect("one exact PKRU semantic op")
}

fn pkru_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict PKRU lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn execute_pkru(
    bytes: &[u8],
    configure: impl FnOnce(&mut SmirContext),
) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr4 = 1 << 22;
    configure(&mut context);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &pkru_block(bytes),
    );
    (result, context)
}

#[test]
fn pkru_instructions_strictly_lift_without_an_interpreter_frontier() {
    for (bytes, write) in [
        (&[0x0F, 0x01, 0xEE][..], false),
        (&[0x0F, 0x01, 0xEF][..], true),
    ] {
        let result = lift_single(bytes).expect("valid PKRU instruction must strictly lift");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            exact_pkru(&result).kind,
            OpKind::X86Pkru {
                eax,
                ecx,
                edx,
                pkru,
                write: got_write,
            } if eax == x86(X86Reg::Rax)
                && ecx == x86(X86Reg::Rcx)
                && edx == x86(X86Reg::Rdx)
                && pkru == x86(X86Reg::Pkru)
                && got_write == write
        ));
    }
}

#[test]
fn pkru_ignores_legacy_size_rex_and_nonmandatory_repeat_prefixes() {
    for bytes in [
        &[0x66, 0x0F, 0x01, 0xEE][..],
        &[0xF2, 0x0F, 0x01, 0xEE],
        &[0x48, 0x0F, 0x01, 0xEE],
        &[0x67, 0x0F, 0x01, 0xEF],
        &[0x64, 0x0F, 0x01, 0xEF],
    ] {
        let result = lift_single(bytes).expect("architecturally ignored PKRU prefix");
        assert_eq!(result.bytes_consumed, bytes.len());
        exact_pkru(&result);
    }
}

#[test]
fn pkru_distinguishes_f3_user_interrupt_aliases_and_rejects_lock() {
    for bytes in [&[0xF3, 0x0F, 0x01, 0xEE][..], &[0xF3, 0x0F, 0x01, 0xEF]] {
        let result = lift_single(bytes).expect("unsupported UI alias must lift to #UD");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.is_empty());
        assert!(matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
    for bytes in [&[0xF0, 0x0F, 0x01, 0xEE][..], &[0xF0, 0x0F, 0x01, 0xEF]] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn pkru_metadata_exposes_exact_conditional_dataflow_and_fault_side_effects() {
    let read = exact_pkru(&lift_single(&[0x0F, 0x01, 0xEE]).unwrap()).clone();
    assert_eq!(
        read.kind.source_vregs(),
        vec![x86(X86Reg::Rcx), x86(X86Reg::Pkru)]
    );
    assert_eq!(read.kind.dests(), vec![x86(X86Reg::Rax), x86(X86Reg::Rdx)]);
    assert!(read.kind.has_side_effects());
    assert!(read.is_jit_safe());

    let write = exact_pkru(&lift_single(&[0x0F, 0x01, 0xEF]).unwrap()).clone();
    assert_eq!(
        write.kind.source_vregs(),
        vec![x86(X86Reg::Rax), x86(X86Reg::Rcx), x86(X86Reg::Rdx)]
    );
    assert_eq!(write.kind.dests(), vec![x86(X86Reg::Pkru)]);
    assert!(write.kind.has_side_effects());
    assert!(write.is_jit_safe());
    assert!(!write.kind.reads_memory());
    assert!(!write.kind.writes_memory());
}

#[test]
fn rdpkru_interpreter_zero_extends_outputs_ignores_high_ecx_and_preserves_flags() {
    let flags = MaterializedFlags {
        cf: true,
        zf: true,
        sf: true,
        of: true,
        pf: true,
        af: true,
        df: true,
        ac: true,
    };
    let (result, context) = execute_pkru(&[0x0F, 0x01, 0xEE], |context| {
        context.flags.materialized = flags;
        context.write_vreg(x86(X86Reg::Rax), u64::MAX);
        context.write_vreg(x86(X86Reg::Rcx), 0xFFFF_FFFF_0000_0000);
        context.write_vreg(x86(X86Reg::Rdx), u64::MAX);
        context.write_vreg(x86(X86Reg::Pkru), 0x89AB_CDEF);
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0x89AB_CDEF);
    assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0);
    assert_eq!(context.read_vreg(x86(X86Reg::Rcx)), 0xFFFF_FFFF_0000_0000);
    assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
    assert!(context.flags.lazy.is_none());
}

#[test]
fn wrpkru_interpreter_uses_low_eax_ignores_high_selectors_and_preserves_inputs() {
    let eax = 0xFFFF_FFFF_89AB_CDEF;
    let ecx = 0x1357_9BDF_0000_0000;
    let edx = 0x2468_ACE0_0000_0000;
    let (result, context) = execute_pkru(&[0x0F, 0x01, 0xEF], |context| {
        context.write_vreg(x86(X86Reg::Rax), eax);
        context.write_vreg(x86(X86Reg::Rcx), ecx);
        context.write_vreg(x86(X86Reg::Rdx), edx);
        context.write_vreg(x86(X86Reg::Pkru), 0x1234_5678);
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86(X86Reg::Pkru)), 0x89AB_CDEF);
    assert_eq!(context.read_vreg(x86(X86Reg::Rax)), eax);
    assert_eq!(context.read_vreg(x86(X86Reg::Rcx)), ecx);
    assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), edx);
}

#[test]
fn pkru_interpreter_faults_before_any_destination_or_state_commit() {
    let (result, context) = execute_pkru(&[0x0F, 0x01, 0xEE], |context| {
        let ArchRegState::X86_64(x86_state) = &mut context.arch_regs else {
            unreachable!()
        };
        x86_state.cr4 = 0;
        context.write_vreg(x86(X86Reg::Rax), 0xA5A5);
        context.write_vreg(x86(X86Reg::Rdx), 0x5A5A);
        context.write_vreg(x86(X86Reg::Pkru), 0x1234_5678);
    });
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined {
            addr: 0x1000,
            opcode: 0
        })
    ));
    assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0xA5A5);
    assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0x5A5A);

    let (result, context) = execute_pkru(&[0x0F, 0x01, 0xEE], |context| {
        context.write_vreg(x86(X86Reg::Rax), 0xA5A5);
        context.write_vreg(x86(X86Reg::Rcx), 1);
        context.write_vreg(x86(X86Reg::Rdx), 0x5A5A);
        context.write_vreg(x86(X86Reg::Pkru), 0x1234_5678);
    });
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0
        })
    ));
    assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0xA5A5);
    assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0x5A5A);

    for (ecx, edx) in [(1, 0), (0, 1), (1, 1)] {
        let (result, context) = execute_pkru(&[0x0F, 0x01, 0xEF], |context| {
            context.write_vreg(x86(X86Reg::Rax), 0x89AB_CDEF);
            context.write_vreg(x86(X86Reg::Rcx), ecx);
            context.write_vreg(x86(X86Reg::Rdx), edx);
            context.write_vreg(x86(X86Reg::Pkru), 0x1234_5678);
        });
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::GeneralProtection {
                addr: 0x1000,
                error_code: 0
            })
        ));
        assert_eq!(context.read_vreg(x86(X86Reg::Pkru)), 0x1234_5678);
    }
}

#[test]
fn pkru_o2_retains_write_read_order_and_fixed_implicit_operands() {
    let mut block = pkru_block(&[0x0F, 0x01, 0xEF]);
    let read = exact_pkru(&lift_single(&[0x0F, 0x01, 0xEE]).unwrap()).clone();
    block.ops.push(SmirOp::new(OpId(1), 0x1003, read.kind));
    block.ops.push(SmirOp::new(
        OpId(2),
        0x1006,
        OpKind::Mov {
            dst: x86(X86Reg::Rax),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    let pkru_ops: Vec<_> = function
        .entry_block()
        .unwrap()
        .ops
        .iter()
        .filter_map(|op| match &op.kind {
            OpKind::X86Pkru {
                eax,
                ecx,
                edx,
                pkru,
                write,
            } => Some((*eax, *ecx, *edx, *pkru, *write)),
            _ => None,
        })
        .collect();
    assert_eq!(
        pkru_ops,
        vec![
            (
                x86(X86Reg::Rax),
                x86(X86Reg::Rcx),
                x86(X86Reg::Rdx),
                x86(X86Reg::Pkru),
                true,
            ),
            (
                x86(X86Reg::Rax),
                x86(X86Reg::Rcx),
                x86(X86Reg::Rdx),
                x86(X86Reg::Pkru),
                false,
            ),
        ]
    );
}
