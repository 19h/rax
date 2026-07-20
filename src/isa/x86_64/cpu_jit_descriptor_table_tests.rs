//! Direct/native x86-64 JIT differentials for SGDT/SIDT stores and faults.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.sregs.gdt.limit = 0x1357;
    vcpu.sregs.gdt.base = 0x0123_4567_89AB_CDEF;
    vcpu.sregs.idt.limit = 0x2468;
    vcpu.sregs.idt.base = 0xFEDC_BA98_7654_3210;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..32 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step().expect("direct SGDT/SIDT instruction").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct execution did not reach {target:#x}");
}

fn exception_without_idt(vcpu: &mut X86_64Vcpu) -> String {
    format!(
        "{:#}",
        vcpu.step()
            .expect_err("exception delivery must fail against the empty test IDT")
    )
}

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

#[test]
fn jit_sgdt_sidt_memory_forms_match_direct_for_legacy_stack_and_egpr_addresses() {
    let code = [
        0x0F, 0x01, 0x43, 0x02, // SGDT [RBX+2]
        0x48, 0x0F, 0x01, 0x4C, 0x4C, 0x04, // SIDT [RSP+RCX*2+4]
        0xD5, 0xB3, 0x01, 0x04, 0xD1, // SGDT [R25+R26*8]
        0xEB, 0x00, 0xF4,
    ];
    let direct_memory = memory_with_code(&code);
    let native_memory = memory_with_code(&code);
    let mut direct = test_vcpu(direct_memory.clone());
    let mut native = test_vcpu(native_memory.clone());
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_apx_enabled(true);
        vcpu.set_jit_mem(true);
        vcpu.regs.rbx = 0x3000;
        vcpu.regs.rsp = 0x4000;
        vcpu.regs.rcx = 0x10;
        vcpu.regs.r25 = 0x5000;
        vcpu.regs.r26 = 4;
    }
    let addresses = [0x3002, 0x4024, 0x5020];
    for memory in [&direct_memory, &native_memory] {
        for address in addresses {
            memory
                .write_slice(&[0xA5; 12], GuestAddress(address - 1))
                .unwrap();
        }
    }

    run_direct_to(&mut direct, 17);
    let region = native
        .jit_compile_region()
        .expect("compile SGDT/SIDT memory region")
        .expect("helper-backed SGDT/SIDT stores must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(gprs(&native.regs), gprs(&direct.regs));
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 17);
    for (index, address) in addresses.into_iter().enumerate() {
        let mut direct_observed = [0; 12];
        let mut native_observed = [0; 12];
        direct_memory
            .read_slice(&mut direct_observed, GuestAddress(address - 1))
            .unwrap();
        native_memory
            .read_slice(&mut native_observed, GuestAddress(address - 1))
            .unwrap();
        assert_eq!(native_observed, direct_observed, "{address:#x}");
        assert_eq!(native_observed[0], 0xA5);
        assert_eq!(native_observed[11], 0xA5);
        let (limit, base) = if index == 1 {
            (0x2468_u16, 0xFEDC_BA98_7654_3210_u64)
        } else {
            (0x1357, 0x0123_4567_89AB_CDEF)
        };
        assert_eq!(&native_observed[1..3], &limit.to_le_bytes());
        assert_eq!(&native_observed[3..11], &base.to_le_bytes());
    }
}

#[test]
fn jit_descriptor_store_apx_umip_and_memory_fault_priority_is_precise() {
    for (name, apx_enabled, umip, expected_vector) in [
        ("APX", false, true, Some(6)),
        ("UMIP", true, true, Some(13)),
        ("memory", true, false, None),
    ] {
        let memory = memory_with_code(&[
            0xD5, 0x91, 0x01, 0x00, // SGDT [R24]
            0xEB, 0x00, 0xF4,
        ]);
        let mut vcpu = test_vcpu(memory);
        vcpu.set_apx_enabled(apx_enabled);
        vcpu.set_jit_mem(true);
        vcpu.sregs.cs.selector = 3;
        vcpu.sregs.cr4 = if umip { 1 << 11 } else { 0 };
        vcpu.sregs.idt.base = 0;
        vcpu.sregs.idt.limit = 0;
        vcpu.regs.r24 = 0x20_000;
        let before = vcpu.regs.clone();

        let region = vcpu
            .jit_compile_region()
            .expect("compile dynamically guarded SGDT")
            .expect("dynamic SGDT fault must not block admission");
        vcpu.jit_run_region_native(&region);
        assert_eq!(gprs(&vcpu.regs), gprs(&before), "{name}");
        assert_eq!(vcpu.regs.rflags, before.rflags, "{name}");
        assert_eq!(vcpu.regs.rip, 0, "{name}");

        let error = exception_without_idt(&mut vcpu);
        if let Some(vector) = expected_vector {
            assert!(
                error.contains(&format!("IDT entry {vector} not present")),
                "{name} fault priority changed: {error}"
            );
        } else {
            assert!(
                !error.contains("IDT entry 13 not present"),
                "UMIP-clear execution must reach the memory fault: {error}"
            );
        }
    }
}

#[test]
fn descriptor_store_transaction_and_jit_helper_are_noncommitting_on_cross_region_fault() {
    use crate::smir::lower::runtime::GuestRegs;

    let memory = Arc::new(
        GuestMemoryMmap::<()>::from_ranges(&[
            (GuestAddress(0), 0x1000),
            (GuestAddress(0x2000), 0x1000),
        ])
        .unwrap(),
    );
    let mut vcpu = test_vcpu(memory.clone());
    memory.write_slice(&[0xCC; 4], GuestAddress(0xFFC)).unwrap();
    assert!(
        vcpu.write_descriptor_table_mem(0xFFC, 0x1357, 0x0123_4567_89AB_CDEF)
            .is_err()
    );
    let mut observed = [0; 4];
    memory
        .read_slice(&mut observed, GuestAddress(0xFFC))
        .unwrap();
    assert_eq!(observed, [0xCC; 4]);

    let mut state = GuestRegs::default();
    state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;
    vcpu.jit_mem_log = Some(Vec::new());
    assert_eq!(
        unsafe { rax_jit_descriptor_table_store(&mut state, 0xFFC, 0) },
        0
    );
    memory
        .read_slice(&mut observed, GuestAddress(0xFFC))
        .unwrap();
    assert_eq!(observed, [0xCC; 4]);
    assert!(vcpu.jit_mem_log.is_none());
}

#[test]
fn jit_descriptor_store_deopts_before_touching_either_covered_code_page() {
    use crate::smir::lower::runtime::GuestRegs;

    let memory = memory_with_code(&[0xF4]);
    let mut vcpu = test_vcpu(memory.clone());
    let address = 0x2FFC;
    let protected = [0xA5; 10];
    memory
        .write_slice(&protected, GuestAddress(address))
        .unwrap();
    vcpu.mmu.mark_code_page(0x3000);

    let mut state = GuestRegs::default();
    state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;
    assert_eq!(
        unsafe { rax_jit_descriptor_table_store(&mut state, address, 0) },
        0
    );

    let mut observed = [0; 10];
    memory
        .read_slice(&mut observed, GuestAddress(address))
        .unwrap();
    assert_eq!(observed, protected);
}

#[test]
fn jit_verify_descriptor_store_trace_and_undo_match_direct() {
    let memory = memory_with_code(&[
        0x0F, 0x01, 0x03, // SGDT [RBX]
        0xEB, 0x00, // JMP HLT
        0xF4,
    ]);
    memory
        .write_slice(&[0xA5; 10], GuestAddress(0x3000))
        .unwrap();
    let mut vcpu = test_vcpu(memory.clone());
    vcpu.set_jit_mem(true);
    vcpu.regs.rbx = 0x3000;
    let region = vcpu
        .jit_compile_region()
        .unwrap()
        .expect("compile verifier SGDT region");
    vcpu.jit_run_region_verified(&region);
    assert_eq!(vcpu.regs.rip, 5);
    let mut observed = [0; 10];
    memory
        .read_slice(&mut observed, GuestAddress(0x3000))
        .unwrap();
    assert_eq!(&observed[..2], &0x1357_u16.to_le_bytes());
    assert_eq!(&observed[2..], &0x0123_4567_89AB_CDEF_u64.to_le_bytes());
}

#[test]
fn jit_rejects_descriptor_store_outside_cs_l_and_direct_keeps_six_byte_form() {
    let memory = memory_with_code(&[0x0F, 0x01, 0x03, 0xF4]);
    let mut compatibility = test_vcpu(memory.clone());
    compatibility.sregs.cs.l = false;
    compatibility.sregs.cs.db = true;
    compatibility.regs.rbx = 0x3000;
    memory
        .write_slice(&[0xA5; 8], GuestAddress(0x3000))
        .unwrap();
    assert!(compatibility.jit_compile_region().unwrap().is_none());
    assert!(compatibility.step().unwrap().is_none());
    let mut observed = [0; 8];
    memory
        .read_slice(&mut observed, GuestAddress(0x3000))
        .unwrap();
    assert_eq!(&observed[..2], &0x1357_u16.to_le_bytes());
    assert_eq!(&observed[2..6], &0x89AB_CDEF_u32.to_le_bytes());
    assert_eq!(&observed[6..], &[0xA5; 2]);
}
