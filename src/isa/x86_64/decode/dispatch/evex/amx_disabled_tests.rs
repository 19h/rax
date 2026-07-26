//! Direct-execution regressions for profile-disabled Intel AMX-AVX512 forms.

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
    vcpu
}

#[test]
fn every_assigned_amx_avx512_cell_raises_ud_before_modrm_fetch() {
    let mut cases = Vec::new();

    // EVEX.512.{66,F3}.0F38.W0 4A and every mandatory-prefix variant of
    // EVEX.512.0F38.W0 6D.
    for (opcode, pp_values) in [(0x4A, &[1_u8, 2_u8][..]), (0x6D, &[0, 1, 2, 3][..])] {
        for &pp in pp_values {
            cases.push((2_u8, pp, opcode));
        }
    }

    // Every mandatory-prefix variant of EVEX.512.0F3A.W0 07, plus the F3/F2
    // variants at opcode 77.
    for (opcode, pp_values) in [(0x07, &[0_u8, 1, 2, 3][..]), (0x77, &[2, 3][..])] {
        for &pp in pp_values {
            cases.push((3_u8, pp, opcode));
        }
    }

    assert_eq!(cases.len(), 12);
    for (map, pp, opcode) in cases {
        let code = [0x62, 0xF0 | map, 0x7C | pp, 0x48, opcode];
        let mut vcpu = tail_vcpu(&code);
        let before = vcpu.regs.clone();
        let error = vcpu
            .step()
            .expect_err("profile-disabled AMX-AVX512 must raise #UD");

        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "map={map} pp={pp} opcode={opcode:02X}: {error:?}"
        );
        assert_eq!(vcpu.regs.rax, before.rax);
        assert_eq!(vcpu.regs.rbx, before.rbx);
        assert_eq!(vcpu.regs.rsp, before.rsp);
        assert_eq!(vcpu.regs.rflags, before.rflags);
        assert_eq!(vcpu.regs.rip, before.rip);
    }
}
