//! Strict lift and canonical interpretation coverage for indirect far CALL.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::ops::X86FarCallOp;
use crate::smir::optimize::{OptLevel, optimize_function};

const MEMORY_BASE: u64 = 0x2000;
const POINTER: u64 = 0x2020;
const GDT: u64 = 0x2100;
const TSS: u64 = 0x2200;
const CURRENT_RSP: u64 = 0x2600;
const PRIVILEGED_RSP: u64 = 0x2700;

fn exact_far_call(result: &LiftResult) -> &X86FarCallOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86FarCall(call) => call,
        other => panic!("expected one exact X86FarCall op, got {other:?}"),
    }
}

fn far_call_block(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict far-CALL lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn code_descriptor(dpl: u8, conforming: bool, accessed: bool) -> [u8; 8] {
    let raw = 0xFFFF_u64
        | ((0xA_u64 | (u64::from(conforming) << 2) | u64::from(accessed)) << 40)
        | (1 << 44)
        | (u64::from(dpl & 3) << 45)
        | (1 << 47)
        | (1 << 53);
    raw.to_le_bytes()
}

fn call_gate(target_selector: u16, target_offset: u64, dpl: u8) -> [u8; 16] {
    let low = (target_offset & 0xFFFF)
        | (u64::from(target_selector) << 16)
        | (0xC << 40)
        | (u64::from(dpl & 3) << 45)
        | (1 << 47)
        | (((target_offset >> 16) & 0xFFFF) << 48);
    let high = (target_offset >> 32) & 0xFFFF_FFFF;
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&low.to_le_bytes());
    bytes[8..].copy_from_slice(&high.to_le_bytes());
    bytes
}

fn far_pointer(offset: u64, selector: u16, width: OpWidth) -> Vec<u8> {
    let mut bytes = match width {
        OpWidth::W16 => (offset as u16).to_le_bytes().to_vec(),
        OpWidth::W32 => (offset as u32).to_le_bytes().to_vec(),
        OpWidth::W64 => offset.to_le_bytes().to_vec(),
        _ => unreachable!(),
    };
    bytes.extend_from_slice(&selector.to_le_bytes());
    bytes
}

fn context_for_far_call(pointer: u64, cpl: u8) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.efer = 1 << 10;
    x86.cs_l = true;
    x86.cpl = cpl;
    x86.cs_selector = 0x8 | u16::from(cpl);
    x86.cs_cache.l = true;
    x86.cs_cache.present = true;
    x86.cs_cache.s = true;
    x86.ss_selector = 0x10 | u16::from(cpl);
    x86.ss_cache.present = true;
    x86.ss_cache.s = true;
    x86.ss_cache.dpl = cpl;
    x86.gdtr_base = GDT;
    x86.gdtr_limit = 0x7F;
    x86.tr_selector = 0x28;
    x86.tr_cache.base = TSS;
    x86.tr_cache.limit = 0x67;
    x86.tr_cache.type_ = 0xB;
    x86.tr_cache.present = true;
    x86.tr_cache.s = false;
    x86.tr_cache.unusable = false;
    x86.gpr[0] = pointer;
    x86.gpr[4] = CURRENT_RSP;
    context
}

fn run_far_call(
    bytes: &[u8],
    context: &mut SmirContext,
    memory: &mut dyn SmirMemory,
) -> BlockResult {
    SmirInterpreter::new().execute_block(context, memory, &far_call_block(bytes))
}

fn read_u64(memory: &mut dyn SmirMemory, address: u64) -> u64 {
    let mut bytes = [0_u8; 8];
    memory.read(address, &mut bytes).unwrap();
    u64::from_le_bytes(bytes)
}

#[test]
fn far_call_strictly_lifts_all_widths_addresses_segments_and_dynamic_target() {
    for (bytes, width) in [
        (&[0xFF, 0x18][..], OpWidth::W32),
        (&[0x66, 0xFF, 0x18], OpWidth::W16),
        (&[0x48, 0xFF, 0x18], OpWidth::W64),
    ] {
        let result = lift_single(bytes).expect("strict FF /3 lift");
        assert_eq!(result.bytes_consumed, bytes.len());
        let call = exact_far_call(&result);
        assert_eq!(call.addr, Address::Direct(x86_gpr(0)));
        assert_eq!(call.target, VReg::Arch(ArchReg::X86(X86Reg::Rip)));
        assert_eq!(call.offset_width, width);
        assert!(!call.requires_apx);
        assert!(!call.stack_segment);
        assert_eq!(call.next_pc, 0x1000 + bytes.len() as u64);
        assert!(matches!(
            result.control_flow,
            ControlFlow::IndirectBranch { target }
                if target == VReg::Arch(ArchReg::X86(X86Reg::Rip))
        ));
    }

    assert!(exact_far_call(&lift_single(&[0xFF, 0x1C, 0x24]).unwrap()).stack_segment);
    assert!(!exact_far_call(&lift_single(&[0x3E, 0xFF, 0x1C, 0x24]).unwrap()).stack_segment);
    assert!(exact_far_call(&lift_single(&[0x36, 0xFF, 0x18]).unwrap()).stack_segment);

    let apx = lift_single(&[0xD5, 0x18, 0xFF, 0x18]).unwrap();
    assert!(matches!(
        exact_far_call(&apx),
        X86FarCallOp {
            addr: Address::Direct(base),
            offset_width: OpWidth::W64,
            requires_apx: true,
            ..
        } if *base == x86_gpr(16)
    ));

    let invalid = lift_single(&[0xFF, 0xD8]).expect("register FF /3 is #UD");
    assert!(invalid.ops.is_empty());
    assert!(matches!(
        invalid.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));
}

#[test]
fn interpreter_frontiers_preserve_typed_far_call() {
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut context = LiftContext::new(SourceArch::X86_64);
    let function = lifter
        .lift_function(
            0x1800,
            &TestMemory::new(0x1800, vec![0x48, 0xFF, 0x18]),
            &mut context,
        )
        .expect("typed far CALL must remain in its native candidate block");
    assert_eq!(function.blocks.len(), 1);
    assert!(matches!(
        function.blocks[0].ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86FarCall(_),
            ..
        }]
    ));
    assert!(matches!(
        function.blocks[0].terminator,
        Terminator::IndirectBranch { target, .. }
            if target == VReg::Arch(ArchReg::X86(X86Reg::Rip))
    ));
}

#[test]
fn far_call_metadata_retains_address_rip_and_faulting_memory_effects() {
    let result = lift_single(&[0x48, 0xFF, 0x5C, 0x88, 0x08]).unwrap();
    let op = &result.ops[0];
    assert_eq!(op.kind.source_vregs(), vec![x86_gpr(1), x86_gpr(0)]);
    assert_eq!(op.kind.dests(), vec![VReg::Arch(ArchReg::X86(X86Reg::Rip))]);
    assert!(op.kind.flags_read().is_empty());
    assert!(op.kind.flags_written().is_empty());
    assert!(op.kind.has_side_effects());
    assert!(op.kind.reads_memory());
    assert!(op.kind.writes_memory());
    assert!(op.is_jit_safe());
}

#[test]
fn optimizer_preserves_far_call_faults_frame_and_terminal_ownership() {
    let lifted = lift_single(&[0x48, 0xFF, 0x18]).unwrap();
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::IndirectBranch {
        target: VReg::Arch(ArchReg::X86(X86Reg::Rip)),
        possible_targets: vec![],
    });
    let mut function = SmirFunction::new(FunctionId(0), BlockId(0), 0x1000);
    function.add_block(block);
    optimize_function(&mut function, OptLevel::O2);
    assert!(matches!(
        function.blocks[0].ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86FarCall(_),
            ..
        }]
    ));
    assert!(matches!(
        function.blocks[0].terminator,
        Terminator::IndirectBranch {
            target: VReg::Arch(ArchReg::X86(X86Reg::Rip)),
            ref possible_targets,
        } if possible_targets.is_empty()
    ));
}

#[test]
fn far_call_interpreter_direct_target_commits_width_selected_frame_and_state_last() {
    let selector = 0x18;
    let target = 0xFFFF_8000_1234_5678;
    let descriptor = code_descriptor(0, false, false);
    let mut context = context_for_far_call(POINTER, 0);
    let original_flags = context.flags.materialized.to_rflags();
    let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x1000);
    memory.load(
        (POINTER - MEMORY_BASE) as usize,
        &far_pointer(target, selector, OpWidth::W64),
    );
    memory.load((GDT + 0x18 - MEMORY_BASE) as usize, &descriptor);

    let result = run_far_call(&[0x48, 0xFF, 0x18], &mut context, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rip, target);
    assert_eq!(x86.cs_selector, selector);
    assert_eq!(x86.gpr[4], CURRENT_RSP - 16);
    assert_eq!(read_u64(&mut memory, CURRENT_RSP - 16), 0x1003);
    assert_eq!(read_u64(&mut memory, CURRENT_RSP - 8), 0x8);
    assert_eq!(context.flags.materialized.to_rflags(), original_flags);
    assert_eq!(read_u64(&mut memory, GDT + 0x18) & (1 << 40), 1 << 40);
}

#[test]
fn far_call_interpreter_same_privilege_gate_uses_fixed_64_bit_frame() {
    let gate_selector = 0x18;
    let target_selector = 0x30;
    let target = 0xFFFF_8000_2468_ACE0;
    let mut context = context_for_far_call(POINTER, 3);
    let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x1000);
    memory.load(
        (POINTER - MEMORY_BASE) as usize,
        &far_pointer(0xDEAD, gate_selector | 3, OpWidth::W16),
    );
    memory.load(
        (GDT + 0x18 - MEMORY_BASE) as usize,
        &call_gate(target_selector, target, 3),
    );
    memory.load(
        (GDT + 0x30 - MEMORY_BASE) as usize,
        &code_descriptor(3, false, false),
    );

    let result = run_far_call(&[0x66, 0xFF, 0x18], &mut context, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rip, target);
    assert_eq!(x86.cs_selector, target_selector | 3);
    assert_eq!(x86.gpr[4], CURRENT_RSP - 16);
    assert_eq!(read_u64(&mut memory, CURRENT_RSP - 16), 0x1003);
    assert_eq!(read_u64(&mut memory, CURRENT_RSP - 8), 0xB);
}

#[test]
fn far_call_interpreter_conforming_gate_retains_cpl_and_resolves_target_in_ldt() {
    const LDT: u64 = 0x2300;
    const TARGET_SELECTOR: u16 = 0x0C;
    let gate_selector = 0x18;
    let target = 0xFFFF_8000_55AA_1234;
    let mut context = context_for_far_call(POINTER, 3);
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.ldtr_selector = 0x20;
    x86.ldtr_cache.base = LDT;
    x86.ldtr_cache.limit = 0x1F;
    x86.ldtr_cache.unusable = false;
    let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x1000);
    memory.load(
        (POINTER - MEMORY_BASE) as usize,
        &far_pointer(0xDEAD_BEEF, gate_selector | 3, OpWidth::W32),
    );
    memory.load(
        (GDT + 0x18 - MEMORY_BASE) as usize,
        &call_gate(TARGET_SELECTOR, target, 3),
    );
    memory.load(
        (LDT + 8 - MEMORY_BASE) as usize,
        &code_descriptor(0, true, false),
    );

    let result = run_far_call(&[0xFF, 0x18], &mut context, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rip, target);
    assert_eq!(x86.cpl, 3);
    assert_eq!(x86.cs_selector, TARGET_SELECTOR | 3);
    assert_eq!(x86.gpr[4], CURRENT_RSP - 16);
    assert_eq!(read_u64(&mut memory, CURRENT_RSP - 16), 0x1002);
    assert_eq!(read_u64(&mut memory, CURRENT_RSP - 8), 0xB);
    assert_eq!(read_u64(&mut memory, LDT + 8) & (1 << 40), 1 << 40);
}

#[test]
fn far_call_interpreter_privilege_gate_uses_tss_stack_and_pushes_complete_frame() {
    let gate_selector = 0x18;
    let target_selector = 0x30;
    let target = 0xFFFF_8000_1357_9BDF;
    let mut context = context_for_far_call(POINTER, 3);
    let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x1000);
    memory.load(
        (POINTER - MEMORY_BASE) as usize,
        &far_pointer(u64::MAX, gate_selector | 3, OpWidth::W64),
    );
    memory.load(
        (GDT + 0x18 - MEMORY_BASE) as usize,
        &call_gate(target_selector, target, 3),
    );
    memory.load(
        (GDT + 0x30 - MEMORY_BASE) as usize,
        &code_descriptor(0, false, false),
    );
    memory.load(
        (TSS + 4 - MEMORY_BASE) as usize,
        &PRIVILEGED_RSP.to_le_bytes(),
    );

    let result = run_far_call(&[0x48, 0xFF, 0x18], &mut context, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rip, target);
    assert_eq!(x86.cs_selector, target_selector);
    assert_eq!(x86.cpl, 0);
    assert_eq!(x86.ss_selector, 0);
    assert_eq!(x86.ss_cache.dpl, 0);
    assert_eq!(x86.gpr[4], PRIVILEGED_RSP - 32);
    assert_eq!(read_u64(&mut memory, PRIVILEGED_RSP - 32), 0x1003);
    assert_eq!(read_u64(&mut memory, PRIVILEGED_RSP - 24), 0xB);
    assert_eq!(read_u64(&mut memory, PRIVILEGED_RSP - 16), CURRENT_RSP);
    assert_eq!(read_u64(&mut memory, PRIVILEGED_RSP - 8), 0x13);
}

#[test]
fn far_call_interpreter_invalid_tss_and_noncanonical_stack_do_not_commit_state() {
    let gate_selector = 0x18;
    let target_selector = 0x30;
    let target = 0x0000_8000_1357_9BDF;
    let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x1000);
    memory.load(
        (POINTER - MEMORY_BASE) as usize,
        &far_pointer(0, gate_selector | 3, OpWidth::W64),
    );
    memory.load(
        (GDT + 0x18 - MEMORY_BASE) as usize,
        &call_gate(target_selector, target, 3),
    );
    memory.load(
        (GDT + 0x30 - MEMORY_BASE) as usize,
        &code_descriptor(0, false, false),
    );

    let mut invalid_tss = context_for_far_call(POINTER, 3);
    let ArchRegState::X86_64(x86) = &mut invalid_tss.arch_regs else {
        unreachable!()
    };
    x86.tr_cache.limit = 3;
    let result = run_far_call(&[0x48, 0xFF, 0x18], &mut invalid_tss, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::InvalidTss {
            addr: 0x1000,
            error_code: 0x28,
        })
    ));
    let ArchRegState::X86_64(x86) = &invalid_tss.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.cs_selector, 0xB);
    assert_eq!(x86.gpr[4], CURRENT_RSP);

    let selector = 0x38;
    memory.load(
        (POINTER - MEMORY_BASE) as usize,
        &far_pointer(target, selector, OpWidth::W64),
    );
    memory.load(
        (GDT + 0x38 - MEMORY_BASE) as usize,
        &code_descriptor(0, false, false),
    );
    let mut bad_stack = context_for_far_call(POINTER, 0);
    let ArchRegState::X86_64(x86) = &mut bad_stack.arch_regs else {
        unreachable!()
    };
    x86.gpr[4] = 0x0000_8000_0000_0008;
    let result = run_far_call(&[0x48, 0xFF, 0x18], &mut bad_stack, &mut memory);
    assert!(
        matches!(
            &result,
            BlockResult::Exit(ExitReason::StackSegment {
                addr: 0x1000,
                error_code: 0,
            })
        ),
        "stack fault must precede target fault: {result:?}"
    );
    let ArchRegState::X86_64(x86) = &bad_stack.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.cs_selector, 0x8);
    assert_eq!(x86.gpr[4], 0x0000_8000_0000_0008);

    let mut bad_target = context_for_far_call(POINTER, 0);
    let result = run_far_call(&[0x48, 0xFF, 0x18], &mut bad_target, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0,
        })
    ));
    let ArchRegState::X86_64(x86) = &bad_target.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.cs_selector, 0x8);
    assert_eq!(x86.gpr[4], CURRENT_RSP);
    assert_eq!(read_u64(&mut memory, GDT + 0x38) & (1 << 40), 0);
}

#[test]
fn far_call_interpreter_probes_the_complete_frame_before_any_write() {
    let selector = 0x38;
    let target = 0xFFFF_8000_1234_5678;
    let descriptor = code_descriptor(0, false, false);
    let sentinel = 0xA5A5_5A5A_DEAD_BEEF_u64;
    let mut context = context_for_far_call(POINTER, 0);
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    // The first push targets the first valid qword; the second falls below the
    // flat-memory base. Both probes must finish before the first store.
    x86.gpr[4] = MEMORY_BASE + 8;
    let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x1000);
    memory.load(0, &sentinel.to_le_bytes());
    memory.load(
        (POINTER - MEMORY_BASE) as usize,
        &far_pointer(target, selector, OpWidth::W64),
    );
    memory.load((GDT + 0x38 - MEMORY_BASE) as usize, &descriptor);

    let result = run_far_call(&[0x48, 0xFF, 0x18], &mut context, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: 0x1FF8,
            write: true,
        })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.cs_selector, 0x8);
    assert_eq!(x86.gpr[4], MEMORY_BASE + 8);
    assert_eq!(read_u64(&mut memory, MEMORY_BASE), sentinel);
    assert_eq!(read_u64(&mut memory, GDT + 0x38) & (1 << 40), 0);
}
