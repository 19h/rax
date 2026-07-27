//! CPU-level native x86-64 JIT differentials for STMXCSR/VSTMXCSR.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const DEST: u64 = 0x4000;
const STACK: u64 = 0x8000;
const MXCSR: u32 = 0xFFE5;

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.sregs.cr0 = 0x21;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = STACK;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rbx = DEST;
    vcpu.regs.rax = 0xA5A5_5A5A_DEAD_BEEF;
    vcpu.regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    vcpu.mxcsr = MXCSR;
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);
    vcpu
}

fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..32 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step().expect("direct MXCSR sequence").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct execution did not reach {target:#x}");
}

fn read_u32(memory: &GuestMemoryMmap, addr: u64) -> u32 {
    let mut bytes = [0u8; 4];
    memory.read_slice(&mut bytes, GuestAddress(addr)).unwrap();
    u32::from_le_bytes(bytes)
}

#[test]
fn jit_mxcsr_stores_match_direct_for_legacy_and_both_vex_wig_encodings() {
    for (name, instruction) in [
        ("legacy", &[0x0F, 0xAE, 0x1B][..]),
        ("VEX.W0", &[0xC5, 0xF8, 0xAE, 0x1B][..]),
        ("VEX.W1", &[0xC4, 0xE1, 0xF8, 0xAE, 0x1B][..]),
    ] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
        let frontier = code.len() as u64 - 1;
        let direct_memory = memory_with_code(&code);
        let native_memory = memory_with_code(&code);
        let mut direct = test_vcpu(direct_memory.clone());
        let mut native = test_vcpu(native_memory.clone());

        run_direct_to(&mut direct, frontier);
        let region = native
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("{name}: compile error: {error:?}"))
            .unwrap_or_else(|| panic!("{name}: MXCSR store must be native eligible"));
        assert!(!region.uses_vector, "{name}");
        assert!(!region.uses_xmm_state, "{name}");
        assert!(region.uses_mxcsr_state, "{name}");
        native.jit_run_region_native(&region);

        assert_eq!(read_u32(&direct_memory, DEST), MXCSR, "{name}: direct");
        assert_eq!(
            read_u32(&native_memory, DEST),
            read_u32(&direct_memory, DEST),
            "{name}: native store"
        );
        assert_eq!(native.mxcsr, direct.mxcsr, "{name}: MXCSR");
        assert_eq!(native.regs.rax, direct.regs.rax, "{name}: RAX");
        assert_eq!(native.regs.rbx, direct.regs.rbx, "{name}: RBX");
        assert_eq!(native.regs.rflags, direct.regs.rflags, "{name}: RFLAGS");
        assert_eq!(native.regs.rip, frontier, "{name}: frontier");
    }
}

#[test]
fn jit_mxcsr_store_fault_is_precise_and_noncommitting() {
    let code = [0x0F, 0xAE, 0x1B, 0xEB, 0x00, 0xF4];
    let memory = memory_with_code(&code);
    let mut native = test_vcpu(memory);
    native.regs.rbx = 0x20_000;
    let before = (
        native.mxcsr,
        native.regs.rax,
        native.regs.rbx,
        native.regs.rflags,
    );

    let region = native
        .jit_compile_region()
        .expect("compile faulting MXCSR-store region")
        .expect("dynamic memory fault must not block admission");
    assert!(region.uses_mxcsr_state);
    native.jit_run_region_native(&region);

    assert_eq!(native.regs.rip, 0, "fault must restart STMXCSR");
    assert_eq!(
        (
            native.mxcsr,
            native.regs.rax,
            native.regs.rbx,
            native.regs.rflags,
        ),
        before,
        "faulting STMXCSR committed architectural state"
    );
    assert!(
        native.step().is_err(),
        "direct replay must deliver the guest memory fault"
    );
    assert_eq!(native.regs.rip, 0);
}

#[test]
fn jit_mxcsr_state_is_coherent_across_interpreter_callouts() {
    // call 100h; stmxcsr [rbx]; jmp hlt; hlt
    let code = [
        0xE8, 0xFB, 0x00, 0x00, 0x00, 0x0F, 0xAE, 0x1B, 0xEB, 0x00, 0xF4,
    ];
    let direct_memory = memory_with_code(&code);
    let native_memory = memory_with_code(&code);
    // ldmxcsr [rbx+4]; ret
    for memory in [&direct_memory, &native_memory] {
        memory
            .write_slice(&[0x0F, 0xAE, 0x53, 0x04, 0xC3], GuestAddress(0x100))
            .unwrap();
        memory
            .write_slice(&MXCSR.to_le_bytes(), GuestAddress(DEST + 4))
            .unwrap();
    }

    let mut direct = test_vcpu(direct_memory.clone());
    let mut native = test_vcpu(native_memory.clone());
    direct.mxcsr = 0x1F80;
    native.mxcsr = 0x1F80;
    native.set_jit_call(true);
    run_direct_to(&mut direct, 10);

    let region = native
        .jit_compile_region()
        .expect("compile MXCSR callout region")
        .expect("MXCSR callout region must be native eligible");
    assert!(region.uses_mxcsr_state);
    assert!(!region.uses_vector);
    native.jit_run_region_native(&region);

    assert_eq!(native.regs.rip, 10);
    assert_eq!(native.regs.rsp, STACK);
    assert_eq!(native.mxcsr, MXCSR, "callee MXCSR update was not imported");
    assert_eq!(read_u32(&native_memory, DEST), MXCSR);
    assert_eq!(
        read_u32(&native_memory, DEST),
        read_u32(&direct_memory, DEST)
    );
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rax, direct.regs.rax);
}
