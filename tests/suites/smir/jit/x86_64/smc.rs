//! Native-region source-page and self-modifying-code regressions.

use super::*;

#[test]
fn explicit_jit_store_to_its_source_page_deopts_before_commit() {
    let target = LOAD_ADDR + 0x300;
    let mut code = vec![0x48, 0xBB]; // mov rbx,target
    code.extend_from_slice(&target.to_le_bytes());
    code.extend_from_slice(&[
        0xC6, 0x03, 0x90, // mov byte ptr [rbx],0x90
        0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,0x12345678
        0xC3, // ret frontier
    ]);
    let (mut vcpu, memory) = make_vcpu_mem(&code);
    memory.write_slice(&[0xCC], GuestAddress(target)).unwrap();
    vcpu.set_jit_call(false);

    assert!(vcpu.jit_try_block().expect("compile source-page store"));
    let native = vcpu.get_regs().unwrap();
    assert_eq!(native.rbx, target, "prefix before deoptimization");
    assert_eq!(
        native.rax, 0,
        "instruction after the store must not execute"
    );
    assert_eq!(native.rip, LOAD_ADDR + 10, "exact store frontier");
    let mut byte = [0_u8; 1];
    memory.read_slice(&mut byte, GuestAddress(target)).unwrap();
    assert_eq!(byte, [0xCC], "native self-modifying store must not commit");

    assert!(vcpu.step().expect("direct store replay").is_none());
    memory.read_slice(&mut byte, GuestAddress(target)).unwrap();
    assert_eq!(byte, [0x90], "interpreter replay commits the store once");
    assert_eq!(vcpu.get_regs().unwrap().rip, LOAD_ADDR + 13);
}

#[test]
fn cross_page_source_instruction_protects_both_pages_before_native_entry() {
    let entry = LOAD_ADDR + 0x0FFE;
    let target = LOAD_ADDR + 0x1200;
    let (mut vcpu, memory) = make_vcpu_mem(&[]);
    // The three-byte store begins in page P and ends in P+1. Its destination is
    // also in P+1, so marking only the entry page would let it commit natively.
    memory
        .write_slice(&[0xC6, 0x03, 0x90, 0xC3], GuestAddress(entry))
        .unwrap();
    memory.write_slice(&[0xCC], GuestAddress(target)).unwrap();
    let mut regs = vcpu.get_regs().unwrap();
    regs.rip = entry;
    regs.rbx = target;
    vcpu.set_regs(&regs).unwrap();
    vcpu.set_jit_call(false);

    assert!(vcpu.jit_try_block().expect("compile cross-page store"));
    let native = vcpu.get_regs().unwrap();
    assert_eq!(native.rip, entry, "cross-page store must deopt at entry");
    let mut byte = [0_u8; 1];
    memory.read_slice(&mut byte, GuestAddress(target)).unwrap();
    assert_eq!(byte, [0xCC], "second source page must be protected");
}

#[test]
fn callout_smc_abandons_the_active_native_continuation() {
    let patch_target = LOAD_ADDR + 0x300;
    let callee_offset = 0x20_u64;
    let mut code = vec![
        0xE8, 0x1B, 0x00, 0x00, 0x00, // call LOAD_ADDR+0x20
        0xB8, 0x44, 0x33, 0x22, 0x11, // native continuation: mov eax,0x11223344
        0xC3, // ret frontier
    ];
    code.resize(callee_offset as usize, 0x90);
    code.extend_from_slice(&[0x48, 0xBB]); // mov rbx,patch_target
    code.extend_from_slice(&patch_target.to_le_bytes());
    code.extend_from_slice(&[
        0xC6, 0x03, 0x90, // mov byte ptr [rbx],0x90
        0xC3, // ret
    ]);

    let (mut vcpu, memory) = make_vcpu_mem(&code);
    memory
        .write_slice(&[0xCC], GuestAddress(patch_target))
        .unwrap();
    let initial_rsp = vcpu.get_regs().unwrap().rsp;

    assert!(vcpu.jit_try_block().expect("compile callout region"));
    let after_smc = vcpu.get_regs().unwrap();
    assert_eq!(after_smc.rip, LOAD_ADDR + callee_offset + 13);
    assert_eq!(after_smc.rax, 0, "stale native continuation must not run");
    assert_eq!(
        after_smc.rsp,
        initial_rsp - 8,
        "the completed CALL remains committed"
    );
    let mut byte = [0_u8; 1];
    memory
        .read_slice(&mut byte, GuestAddress(patch_target))
        .unwrap();
    assert_eq!(byte, [0x90], "callee store completes before deoptimization");

    assert!(vcpu.step().expect("callee RET replay").is_none());
    let after_ret = vcpu.get_regs().unwrap();
    assert_eq!(after_ret.rip, LOAD_ADDR + 5);
    assert_eq!(after_ret.rsp, initial_rsp);
}

#[test]
fn callout_return_push_to_a_source_page_exits_before_native_continuation() {
    let code = [
        0xE8, 0, 0, 0, 0, // call $+5
        0xB8, 0x44, 0x33, 0x22, 0x11, // mov eax,0x11223344
        0xC3, // ret frontier
    ];
    let (mut vcpu, memory) = make_vcpu_mem(&code);
    let initial_rsp = LOAD_ADDR + 0x3F8;
    let pushed_at = initial_rsp - 8;
    let mut regs = vcpu.get_regs().unwrap();
    regs.rsp = initial_rsp;
    vcpu.set_regs(&regs).unwrap();

    assert!(vcpu.jit_try_block().expect("compile CALL $+5 region"));
    let after_call = vcpu.get_regs().unwrap();
    assert_eq!(after_call.rip, LOAD_ADDR + 5);
    assert_eq!(after_call.rax, 0, "native continuation must not execute");
    assert_eq!(after_call.rsp, pushed_at, "CALL stack effect commits once");
    let mut pushed = [0_u8; 8];
    memory
        .read_slice(&mut pushed, GuestAddress(pushed_at))
        .unwrap();
    assert_eq!(u64::from_le_bytes(pushed), LOAD_ADDR + 5);
}
