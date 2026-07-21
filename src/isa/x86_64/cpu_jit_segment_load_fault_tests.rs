//! Accessed-bit store fault differentials for `MOV Sreg,r/m`.

use super::*;

#[test]
fn jit_mov_sreg_read_only_accessed_store_deopts_then_direct_faults_without_commit() {
    const PML4: u64 = 0x9000;
    const PDPT: u64 = 0xA000;
    const PD: u64 = 0xB000;
    const PT: u64 = 0xC000;
    const PAGE_FLAGS: u64 = 0x7; // Present | writable | user-accessible.
    const DESCRIPTOR_ADDR: u64 = 0x2000;

    let memory = memory_with_code(&[0x8E, 0xD8, 0xF4]); // MOV DS,AX; HLT
    let descriptor = data_descriptor(0x1234_5000, 0xFFFF, 0, true, 0x2, false);
    memory
        .write_slice(&descriptor, GuestAddress(DESCRIPTOR_ADDR))
        .unwrap();
    for (address, entry) in [
        (PML4, PDPT | PAGE_FLAGS),
        (PDPT, PD | PAGE_FLAGS),
        (PD, PT | PAGE_FLAGS),
    ] {
        memory
            .write_slice(&entry.to_le_bytes(), GuestAddress(address))
            .unwrap();
    }
    for page in 0..16_u64 {
        let flags = if page == DESCRIPTOR_ADDR >> 12 {
            PAGE_FLAGS & !0x2
        } else {
            PAGE_FLAGS
        };
        memory
            .write_slice(
                &(page * 0x1000 | flags).to_le_bytes(),
                GuestAddress(PT + page * 8),
            )
            .unwrap();
    }

    let mut vcpu = test_vcpu(memory.clone());
    vcpu.sregs.gdt.base = DESCRIPTOR_ADDR - 0x10;
    vcpu.sregs.gdt.limit = 0x1F;
    vcpu.sregs.cr0 |= 1 << 31;
    vcpu.sregs.cr3 = PML4;
    vcpu.sregs.cr4 |= 1 << 5;
    vcpu.sregs.efer |= 1 << 8;
    vcpu.set_jit_mem(true);
    vcpu.regs.rflags &= !flags::bits::AF;
    vcpu.regs.rax = 0x10;
    let before_ds = segment_fingerprint(&vcpu.sregs.ds);
    let before_regs = vcpu.regs.clone();

    let region = vcpu
        .jit_compile_region()
        .expect("compile MOV DS,AX with a dynamic descriptor-store fault")
        .expect("the faulting accessed-bit transition must remain native eligible");
    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.rip, 0);
    assert_eq!(gprs(&vcpu.regs), gprs(&before_regs));
    assert_eq!(vcpu.regs.rflags, before_regs.rflags);
    assert_eq!(segment_fingerprint(&vcpu.sregs.ds), before_ds);
    let mut observed = [0_u8; 8];
    memory
        .read_slice(&mut observed, GuestAddress(DESCRIPTOR_ADDR))
        .unwrap();
    assert_eq!(observed, descriptor);

    assert!(matches!(
        vcpu.step(),
        Err(crate::error::Error::PageFault {
            vaddr: DESCRIPTOR_ADDR,
            error_code: 0x3,
        })
    ));
    assert_eq!(segment_fingerprint(&vcpu.sregs.ds), before_ds);
    memory
        .read_slice(&mut observed, GuestAddress(DESCRIPTOR_ADDR))
        .unwrap();
    assert_eq!(observed, descriptor);
}
