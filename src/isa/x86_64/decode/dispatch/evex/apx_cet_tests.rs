//! Direct-execution regressions for profile-disabled APX CET shadow-stack stores.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::VCpu;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const MEMORY_SIZE: usize = 0x10000;

fn tail_vcpu(code: &[u8]) -> X86_64Vcpu {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), MEMORY_SIZE)]).unwrap());
    let rip = (MEMORY_SIZE - code.len()) as u64;
    memory.write_slice(code, GuestAddress(rip)).unwrap();

    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.regs.rip = rip;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
    vcpu.regs.rbx = 0xFEDC_BA98_7654_3210;
    vcpu.regs.rflags = 0x2 | 0x8D5;
    vcpu.set_apx_enabled(true);
    vcpu
}

#[test]
fn direct_profile_disabled_apx_wrss_and_wruss_raise_ud_before_modrm_fetch() {
    // Intel APX Revision 7.0 encodes WRSSD/WRSSQ at NP.MAP4 66 and
    // WRUSSD/WRUSSQ at 66.MAP4 65. CPUID CET is absent from RAX's fixed
    // profile, so each opcode is #UD even when no ModR/M byte is mapped.
    for (code, name) in [
        (&[0x62, 0xF4, 0x7C, 0x08, 0x66][..], "WRSSD"),
        (&[0x62, 0xF4, 0xFC, 0x08, 0x66][..], "WRSSQ"),
        (&[0x62, 0xF4, 0x7D, 0x08, 0x65][..], "WRUSSD"),
        (&[0x62, 0xF4, 0xFD, 0x08, 0x65][..], "WRUSSQ"),
    ] {
        let mut vcpu = tail_vcpu(code);
        let before = vcpu.regs.clone();
        let error = vcpu.step().expect_err(name);

        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "{name}: {error:?}"
        );
        assert_eq!(vcpu.regs.rax, before.rax, "{name}: RAX");
        assert_eq!(vcpu.regs.rbx, before.rbx, "{name}: RBX");
        assert_eq!(vcpu.regs.rsp, before.rsp, "{name}: RSP");
        assert_eq!(vcpu.regs.rflags, before.rflags, "{name}: RFLAGS");
        assert_eq!(vcpu.regs.rip, before.rip, "{name}: fault RIP");
    }
}
