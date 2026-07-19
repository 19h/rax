//! x86-64 lift-through-call helper lowering tests.

use super::*;
use crate::smir::ir::X86InstructionBytes;
use crate::smir::lower::{
    X86_GUEST_CALL_FN_OFFSET, X86_GUEST_EXIT_PC_OFFSET, X86_GUEST_LOAD_FN_OFFSET,
};

fn indirect_helper_call(offset: i32) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0x90];
    bytes.extend_from_slice(&(offset as u32).to_le_bytes());
    bytes
}

#[test]
fn vector_call_helper_emits_save_and_both_resume_reloads() {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_call_helpers(true);
    lowerer.set_preserve_vector_call_helpers(true);
    let continuation = BlockId(7);
    lowerer.block_guest_pcs.insert(continuation, 0x2000);
    lowerer
        .emit_jit_call_op(&CallTarget::GuestAddr(0x1800), continuation, 0x17fb)
        .expect("lower vector-preserving call helper");

    let bytes = lowerer.code.data();
    let store_zmm0 = &[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x40, 0x05];
    let load_zmm0 = &[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x41, 0x05];
    assert_eq!(
        bytes
            .windows(store_zmm0.len())
            .filter(|window| *window == store_zmm0)
            .count(),
        1,
        "call helper must save vector state once"
    );
    assert_eq!(
        bytes
            .windows(load_zmm0.len())
            .filter(|window| *window == load_zmm0)
            .count(),
        2,
        "call helper must reload vector state on success and bailout"
    );
}

#[test]
fn memory_indirect_call_loads_target_before_callout_with_precise_fault_pc() {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    let continuation = BlockId(7);
    let return_pc = 0x2000;
    let call_pc = 0x1ffa;
    lowerer.block_guest_pcs.insert(continuation, return_pc);
    let address = Address::PcRel {
        offset: 0x3a,
        disp_size: DispSize::Disp32,
        base: Some(return_pc),
    };

    lowerer
        .emit_jit_call_op(&CallTarget::IndirectMem(address), continuation, call_pc)
        .expect("lower memory-indirect CALL");

    let bytes = lowerer.code.data();
    let load_call = indirect_helper_call(X86_GUEST_LOAD_FN_OFFSET);
    let callout = indirect_helper_call(X86_GUEST_CALL_FN_OFFSET);
    let load_pos = bytes
        .windows(load_call.len())
        .position(|window| window == load_call)
        .expect("target-load helper call");
    let callout_pos = bytes
        .windows(callout.len())
        .position(|window| window == callout)
        .expect("interpreter callout helper call");
    assert!(
        load_pos < callout_pos,
        "target operand must be read before the architectural stack push"
    );
    assert!(
        bytes
            .windows(5)
            .any(|window| window == [0x48, 0x8B, 0x74, 0x24, 0x10]),
        "the loaded target must survive in the fixed caller stack slot"
    );

    let mut return_arg = vec![0x48, 0xBA];
    return_arg.extend_from_slice(&return_pc.to_le_bytes());
    let mut call_pc_arg = vec![0x48, 0xB9];
    call_pc_arg.extend_from_slice(&call_pc.to_le_bytes());
    assert!(
        bytes
            .windows(return_arg.len())
            .any(|window| window == return_arg),
        "missing exact return-PC argument"
    );
    assert!(
        bytes
            .windows(call_pc_arg.len())
            .any(|window| window == call_pc_arg),
        "missing exact call-PC argument"
    );

    let cleanup_32 = [0x48, 0x8D, 0x64, 0x24, 0x20];
    assert_eq!(
        bytes
            .windows(cleanup_32.len())
            .filter(|window| *window == cleanup_32)
            .count(),
        2,
        "success and callout-bail paths must release both 16-byte frames"
    );
    let cleanup_16 = [0x48, 0x8D, 0x64, 0x24, 0x10];
    assert!(
        bytes
            .windows(cleanup_16.len())
            .any(|window| window == cleanup_16),
        "target-load fault path must release its caller-owned slot"
    );

    let mut exit_pc_low = vec![0xC7, 0x80];
    exit_pc_low.extend_from_slice(&(X86_GUEST_EXIT_PC_OFFSET as u32).to_le_bytes());
    exit_pc_low.extend_from_slice(&(call_pc as u32).to_le_bytes());
    assert!(
        bytes
            .windows(exit_pc_low.len())
            .any(|window| window == exit_pc_low),
        "target-load fault must publish the exact CALL PC"
    );
}

#[test]
fn memory_indirect_call_lowering_rejects_disabled_helpers_and_virtual_addresses() {
    let continuation = BlockId(7);
    let architectural = Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rax)));

    let mut disabled = X86_64Lowerer::new();
    disabled.block_guest_pcs.insert(continuation, 0x2000);
    assert!(matches!(
        disabled.emit_jit_call_op(
            &CallTarget::IndirectMem(architectural),
            continuation,
            0x1ffe,
        ),
        Err(LowerError::UnsupportedOp { .. })
    ));

    let mut virtual_address = X86_64Lowerer::new();
    virtual_address.set_mem_helpers(true);
    virtual_address.block_guest_pcs.insert(continuation, 0x2000);
    assert!(matches!(
        virtual_address.emit_jit_call_op(
            &CallTarget::IndirectMem(Address::Direct(VReg::virt(0))),
            continuation,
            0x1ffe,
        ),
        Err(LowerError::UnsupportedOp { .. })
    ));
}

#[test]
fn call_site_pc_requires_exact_instruction_end_provenance() {
    let source = BlockId(3);
    let continuation = BlockId(7);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.block_guest_pcs.insert(continuation, 0x1006);
    lowerer.x86_instruction_bytes.insert(
        (source, 0x1000),
        X86InstructionBytes::new(&[0xFF, 0x15, 0, 0, 0, 0]).unwrap(),
    );
    assert_eq!(
        lowerer.jit_call_site_pc(source, continuation).unwrap(),
        0x1000
    );

    lowerer.x86_instruction_bytes.insert(
        (source, 0x1001),
        X86InstructionBytes::new(&[0x90, 0x90, 0x90, 0x90, 0x90]).unwrap(),
    );
    assert!(matches!(
        lowerer.jit_call_site_pc(source, continuation),
        Err(LowerError::UnsupportedOp { .. })
    ));
    lowerer.x86_instruction_bytes.remove(&(source, 0x1001));

    lowerer.block_guest_pcs.insert(continuation, 0x1007);
    assert!(matches!(
        lowerer.jit_call_site_pc(source, continuation),
        Err(LowerError::UnsupportedOp { .. })
    ));
    lowerer.x86_instruction_bytes.clear();
    assert!(matches!(
        lowerer.jit_call_site_pc(source, continuation),
        Err(LowerError::UnsupportedOp { .. })
    ));
}

#[test]
fn call_site_pc_rejects_non_64_bit_callout_width_and_malformed_opcode() {
    let source = BlockId(3);
    let continuation = BlockId(7);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.block_guest_pcs.insert(continuation, 0x1007);
    lowerer.x86_instruction_bytes.insert(
        (source, 0x1000),
        X86InstructionBytes::new(&[0x66, 0xFF, 0x15, 0, 0, 0, 0]).unwrap(),
    );
    assert!(matches!(
        lowerer.jit_call_site_pc(source, continuation),
        Err(LowerError::UnsupportedOp { .. })
    ));

    lowerer.x86_instruction_bytes.insert(
        (source, 0x1000),
        X86InstructionBytes::new(&[0x0F, 0x1F, 0x80, 0, 0, 0, 0]).unwrap(),
    );
    assert!(matches!(
        lowerer.jit_call_site_pc(source, continuation),
        Err(LowerError::UnsupportedOp { .. })
    ));
}
