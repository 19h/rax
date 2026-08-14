//! Long-mode x86 `LEAVE` width, state, and fault-commit interpretation.

use super::*;
use crate::isa::x86_64::flags;
use crate::smir::interpret::BlockResult;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::ops::{X86LeaveOp, X86LeaveWidth};

const PC: u64 = 0x1000;
const CR0_AM: u64 = 1 << 18;

fn configure(context: &mut SmirContext, rsp: u64, rbp: u64, rflags: u64) {
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    for (index, reg) in x86.gpr.iter_mut().enumerate() {
        *reg = 0x1100_0000_0000_0000 | index as u64;
    }
    x86.gpr[4] = rsp;
    x86.gpr[5] = rbp;
    x86.rflags = rflags;
    x86.efer = 1 << 10;
    x86.cs_l = true;
    context.flags.materialized = MaterializedFlags::from_rflags(rflags);
    context.flags.lazy = None;
}

fn x86(context: &SmirContext) -> &crate::smir::ir::context::X86RegState {
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    x86
}

fn execute_exact_op(
    op: SmirOp,
    context: &mut SmirContext,
    memory: &mut dyn SmirMemory,
) -> BlockResult {
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops.push(op);
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    SmirInterpreter::new().execute_block(context, memory, &block)
}

#[test]
fn leave64_commits_frame_and_preserves_unrelated_state_and_flags() {
    let frame = 0x1800;
    let saved_rbp: u64 = 0xCAFE_BABE_0123_4567;
    let rflags = 0x2 | flags::bits::CF | flags::bits::PF | flags::bits::OF;
    let mut context = SmirContext::new_x86_64();
    configure(&mut context, 0x2200, frame, rflags);
    let before = x86(&context).gpr;
    let mut memory = FlatMemory::new(0x3000);
    memory.write(frame, &saved_rbp.to_le_bytes()).unwrap();

    let result = execute_lifted_x86(&[0xC9], &mut context, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(x86(&context).gpr[4], frame + 8);
    assert_eq!(x86(&context).gpr[5], saved_rbp);
    for index in (0..32).filter(|index| !matches!(index, 4 | 5)) {
        assert_eq!(x86(&context).gpr[index], before[index], "GPR {index}");
    }
    context.flags.materialize_all();
    assert_eq!(context.flags.materialized.to_rflags(), rflags);
    assert_eq!(x86(&context).rflags, rflags);
}

#[test]
fn leave16_uses_full_rbp_as_address_and_merges_only_bp() {
    let frame = 0xFFFF_8000_0000_1800;
    let mut context = SmirContext::new_x86_64();
    configure(&mut context, 0x2200, frame, 0x2);
    let mut memory = FlatMemory::with_base(frame, 0x100);
    memory.write(frame, &0xBEEF_u16.to_le_bytes()).unwrap();

    let result = execute_lifted_x86(&[0x66, 0xC9], &mut context, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(x86(&context).gpr[4], frame + 2);
    assert_eq!(x86(&context).gpr[5], 0xFFFF_8000_0000_BEEF);
}

#[test]
fn leave_pop_fault_and_pretranslation_faults_do_not_commit_registers() {
    let frame = 0x1800;
    let mut context = SmirContext::new_x86_64();
    configure(&mut context, 0x2200, frame, 0x2);
    let mut unmapped = FlatMemory::with_base(frame + 8, 8);
    let result = execute_lifted_x86(&[0xC9], &mut context, &mut unmapped);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    assert_eq!(x86(&context).gpr[4], 0x2200);
    assert_eq!(x86(&context).gpr[5], frame);

    let noncanonical = 0x0000_8000_0000_0000;
    configure(&mut context, 0x2200, noncanonical, 0x2);
    let mut memory = FlatMemory::new(0x100);
    let result = execute_lifted_x86(&[0xC9], &mut context, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::StackSegment {
            addr: PC,
            error_code: 0
        })
    ));
    assert_eq!(x86(&context).gpr[4], 0x2200);
    assert_eq!(x86(&context).gpr[5], noncanonical);

    let crossing = 0x0000_7FFF_FFFF_FFFC;
    configure(&mut context, 0x2200, crossing, 0x2);
    let result = execute_lifted_x86(&[0xC9], &mut context, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::StackSegment {
            addr: PC,
            error_code: 0
        })
    ));
    assert_eq!(x86(&context).gpr[4], 0x2200);
    assert_eq!(x86(&context).gpr[5], crossing);

    configure(&mut context, 0x2200, frame + 1, 0x2 | flags::bits::AC);
    let ArchRegState::X86_64(state) = &mut context.arch_regs else {
        unreachable!()
    };
    state.cr0 = CR0_AM;
    state.cpl = 3;
    let mut memory = FlatMemory::new(0x3000);
    let result = execute_lifted_x86(&[0xC9], &mut context, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::AlignmentCheck { addr: PC })
    ));
    assert_eq!(x86(&context).gpr[4], 0x2200);
    assert_eq!(x86(&context).gpr[5], frame + 1);
}

#[test]
fn invalid_shape_mode_and_disabled_apx_do_not_commit() {
    let mut context = SmirContext::new_x86_64();
    configure(&mut context, 0x2200, 0x1800, 0x2);
    let mut memory = FlatMemory::new(0x3000);
    let invalid = SmirOp::new(
        OpId(0),
        PC,
        OpKind::X86Leave(X86LeaveOp {
            width: X86LeaveWidth::W64,
            requires_apx: false,
            next_pc: PC,
        }),
    );
    assert!(matches!(
        execute_exact_op(invalid, &mut context, &mut memory),
        BlockResult::Exit(ExitReason::Undefined { addr: PC, .. })
    ));
    assert_eq!(x86(&context).gpr[4], 0x2200);
    assert_eq!(x86(&context).gpr[5], 0x1800);

    for (name, invalidate) in [
        (
            "EFER.LMA clear",
            (|x86: &mut crate::smir::ir::context::X86RegState| x86.efer = 0)
                as fn(&mut crate::smir::ir::context::X86RegState),
        ),
        ("CS.L clear", |x86| x86.cs_l = false),
    ] {
        configure(&mut context, 0x2200, 0x1800, 0x2);
        let ArchRegState::X86_64(state) = &mut context.arch_regs else {
            unreachable!()
        };
        invalidate(state);
        assert!(
            matches!(
                execute_lifted_x86(&[0xC9], &mut context, &mut memory),
                BlockResult::Exit(ExitReason::Undefined { addr: PC, .. })
            ),
            "{name}"
        );
        assert_eq!(x86(&context).gpr[4], 0x2200, "{name}");
        assert_eq!(x86(&context).gpr[5], 0x1800, "{name}");
    }

    configure(&mut context, 0x2200, 0x1800, 0x2);
    let result = execute_lifted_x86(&[0xD5, 0x00, 0xC9], &mut context, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { addr: PC, .. })
    ));
    assert_eq!(x86(&context).gpr[4], 0x2200);
    assert_eq!(x86(&context).gpr[5], 0x1800);
}
