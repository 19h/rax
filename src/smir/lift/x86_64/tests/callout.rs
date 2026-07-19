//! x86-64 CALL lifting and interpreter-frontier tests.

use super::*;

#[test]
fn test_lift_call_ret() {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // CALL rel32
    let result = lifter
        .lift_insn(0x1000, &[0xE8, 0x00, 0x10, 0x00, 0x00], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 5);
    assert!(matches!(
        result.control_flow,
        ControlFlow::Call {
            target: CallTarget::GuestAddr(0x2005)
        }
    ));

    // RET
    let result = lifter.lift_insn(0x1000, &[0xC3], &mut ctx).unwrap();
    assert_eq!(result.bytes_consumed, 1);
    assert!(matches!(result.control_flow, ControlFlow::Return));
}

#[test]
fn addr32_memory_call_retains_exact_architectural_address_expression() {
    let direct = lift_single(&[0x67, 0xFF, 0x10]).unwrap();
    assert!(direct.ops.is_empty());
    assert!(matches!(
        &direct.control_flow,
        ControlFlow::Call {
            target: CallTarget::X86IndirectMemAddr32(Address::Direct(base)),
        } if *base == x86_gpr(0)
    ));

    // CALL qword ptr [r8d+r12d*4-8]. Address components remain architectural;
    // the target variant supplies the modulo-2^32 evaluation contract.
    let sib = lift_single(&[0x67, 0x4B, 0xFF, 0x54, 0xA0, 0xF8]).unwrap();
    assert!(sib.ops.is_empty());
    assert!(matches!(
        &sib.control_flow,
        ControlFlow::Call {
            target: CallTarget::X86IndirectMemAddr32(Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: -8,
                disp_size: DispSize::Disp8,
            }),
        } if *base == x86_gpr(8) && *index == x86_gpr(12)
    ));

    let segmented = lift_single(&[0x64, 0x67, 0xFF, 0x50, 0x20]).unwrap();
    assert!(segmented.ops.is_empty());
    assert!(matches!(
        &segmented.control_flow,
        ControlFlow::Call {
            target: CallTarget::X86IndirectMemAddr32(Address::SegmentRel {
                segment,
                base: Some(base),
                index: None,
                scale: 1,
                disp: 0x20,
            }),
        } if *segment == VReg::Arch(ArchReg::X86(X86Reg::FsBase))
            && *base == x86_gpr(0)
    ));

    // ModR/M mod=00,r/m=101 remains EIP-relative under 67h. At 1000h the
    // seven-byte instruction ends at 1007h; 1007h + FF9h = 2000h.
    let eip_relative = lift_single(&[0x67, 0xFF, 0x15, 0xF9, 0x0F, 0x00, 0x00]).unwrap();
    assert!(eip_relative.ops.is_empty());
    assert!(matches!(
        eip_relative.control_flow,
        ControlFlow::Call {
            target: CallTarget::X86IndirectMemAddr32(Address::Absolute(0x2000)),
        }
    ));

    // The SIB no-base/no-index form remains an absolute disp32 rather than
    // becoming EIP-relative.
    let absolute = lift_single(&[0x67, 0xFF, 0x14, 0x25, 0x80, 0, 0, 0]).unwrap();
    assert!(absolute.ops.is_empty());
    assert!(matches!(
        absolute.control_flow,
        ControlFlow::Call {
            target: CallTarget::X86IndirectMemAddr32(Address::Absolute(0x80)),
        }
    ));
}

#[test]
fn lift_through_calls_retains_state_backed_memory_targets_and_rejects_virtual_addresses() {
    // MOV EAX,imm32; CALL qword ptr [RAX]; HLT. The architectural base can be
    // reconstructed directly from GuestRegs, so the prefix and CALL remain in
    // one native-candidate block.
    let mem = TestMemory::new(0x1900, vec![0xB8, 0x78, 0x56, 0x34, 0x12, 0xFF, 0x10, 0xF4]);
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    lifter.set_lift_through_calls(512);
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let function = lifter.lift_function(0x1900, &mem, &mut ctx).unwrap();
    let entry = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1900)
        .unwrap();
    assert_eq!(entry.ops.len(), 1);
    assert!(matches!(
        &entry.terminator,
        Terminator::Call {
            target: CallTarget::IndirectMem(Address::Direct(reg)),
            continuation,
            ..
        } if *reg == x86_gpr(0)
            && function
                .blocks
                .iter()
                .any(|block| block.id == *continuation && block.guest_pc == 0x1907)
    ));

    // RIP-relative targets are likewise state-backed; the lowerer reconstructs
    // their base from exact instruction provenance.
    let mem = TestMemory::new(0x1A00, vec![0x83, 0xC0, 0x01, 0xFF, 0x15, 0, 0, 0, 0, 0xF4]);
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    lifter.set_lift_through_calls(512);
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let function = lifter.lift_function(0x1A00, &mem, &mut ctx).unwrap();
    let entry = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1A00)
        .unwrap();
    assert_eq!(entry.ops.len(), 1);
    assert!(matches!(
        &entry.terminator,
        Terminator::Call {
            target: CallTarget::IndirectMem(addr @ Address::PcRel { .. }),
            ..
        } if addr.is_x86_state_backed_shape()
    ));

    // addr32 CALL [EAX] retains its architectural base and explicit address-size
    // marker, so it no longer creates an interpreter frontier or virtual pre-op.
    let mem = TestMemory::new(
        0x1B00,
        vec![0xB8, 0x78, 0x56, 0x34, 0x12, 0x67, 0xFF, 0x10, 0xF4],
    );
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    lifter.set_lift_through_calls(512);
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let function = lifter.lift_function(0x1B00, &mem, &mut ctx).unwrap();
    let entry = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1B00)
        .unwrap();
    assert_eq!(entry.ops.len(), 1);
    assert!(matches!(
        &entry.terminator,
        Terminator::Call {
            target: CallTarget::X86IndirectMemAddr32(Address::Direct(base)),
            continuation,
            ..
        } if *base == x86_gpr(0)
            && function
                .blocks
                .iter()
                .any(|block| block.id == *continuation && block.guest_pc == 0x1B08)
    ));

    // This slice changes CALL only. addr32 JMP [EAX] remains an exact
    // interpreter frontier because its native exit path has no addr32 marker.
    let mem = TestMemory::new(0x1D00, vec![0xB8, 0x78, 0x56, 0x34, 0x12, 0x67, 0xFF, 0x20]);
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    lifter.set_lift_through_calls(512);
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let function = lifter.lift_function(0x1D00, &mem, &mut ctx).unwrap();
    let entry = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1D00)
        .unwrap();
    assert!(matches!(entry.terminator, Terminator::Branch { .. }));
    assert!(function.blocks.iter().any(|block| {
        block.guest_pc == 0x1D05
            && block.ops.is_empty()
            && matches!(block.terminator, Terminator::Return { .. })
    }));
}

#[test]
fn direct_calls_still_require_explicit_lift_through_call_mode() {
    let mem = TestMemory::new(
        0x1C00,
        vec![0xB8, 0x78, 0x56, 0x34, 0x12, 0xE8, 0, 0, 0, 0, 0xF4],
    );
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let function = lifter.lift_function(0x1C00, &mem, &mut ctx).unwrap();
    let entry = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1C00)
        .unwrap();

    assert!(matches!(entry.terminator, Terminator::Branch { .. }));
    assert!(function.blocks.iter().any(|block| {
        block.guest_pc == 0x1C05
            && block.ops.is_empty()
            && matches!(block.terminator, Terminator::Return { .. })
    }));
}

#[test]
fn lift_through_calls_block_cap_materializes_all_queued_edges_as_frontiers() {
    // TEST EAX,EAX; JZ +2. With a one-block cap, both the fallthrough at
    // 0x3004 and the taken target at 0x3006 are queued but cannot be lifted.
    // They must remain explicit frontier blocks rather than dangling IDs.
    let mem = TestMemory::new(0x3000, vec![0x85, 0xC0, 0x74, 0x02, 0x90, 0xC3, 0x90, 0xC3]);
    let mut lifter = X86_64Lifter::strict();
    lifter.set_lift_through_calls(1);
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let mut function = lifter.lift_function(0x3000, &mem, &mut ctx).unwrap();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    let entry = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x3000)
        .unwrap();
    let Terminator::CondBranch {
        true_target,
        false_target,
        ..
    } = &entry.terminator
    else {
        panic!("capped entry must retain its conditional branch");
    };

    assert_eq!(function.blocks.len(), 3);
    for (target, guest_pc) in [(*true_target, 0x3006), (*false_target, 0x3004)] {
        let frontier = function
            .blocks
            .iter()
            .find(|block| block.id == target)
            .unwrap_or_else(|| panic!("missing capped frontier at {guest_pc:#x}"));
        assert_eq!(frontier.guest_pc, guest_pc);
        assert!(frontier.ops.is_empty());
        assert!(matches!(frontier.terminator, Terminator::Return { .. }));
    }
}
