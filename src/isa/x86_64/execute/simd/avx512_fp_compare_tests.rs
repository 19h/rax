//! Direct-execution regressions for EVEX floating-point mask comparisons.

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::VCpu;

const CODE: u64 = 0x1000;
const UNMAPPED: u64 = 0x2_0000;

fn vcpu(code: &[u8]) -> X86_64Vcpu {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(CODE)).unwrap();
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.regs.rip = CODE;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.db = false;
    vcpu
}

fn encoding(elem_size: usize, scalar: bool, ll: u8, suppress_exceptions: bool) -> [u8; 7] {
    assert!(matches!(elem_size, 2 | 4 | 8));
    assert!(ll < 4);
    let map = if elem_size == 2 { 3 } else { 1 };
    let pp = match (elem_size, scalar) {
        (2 | 4, false) => 0,
        (8, false) => 1,
        (2 | 4, true) => 2,
        (8, true) => 3,
        _ => unreachable!(),
    };
    let w = elem_size == 8;
    let source1 = 1u8;
    let source2 = 2u8;
    let destination = 1u8;
    [
        0x62,
        0xF0 | map,
        (((!source1) & 0x0F) << 3) | 0x04 | pp | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08 | if suppress_exceptions { 0x10 } else { 0 },
        0xC2,
        0xC0 | (destination << 3) | source2,
        0,
    ]
}

fn as_memory(mut code: [u8; 7]) -> [u8; 7] {
    code[5] &= 0x38;
    code
}

fn assert_reserved_ud(code: &[u8]) {
    let mut vcpu = vcpu(code);
    vcpu.regs.rax = UNMAPPED;
    vcpu.regs.k[1] = 0xA55A_3CC3_F00F_9696;
    let before = vcpu.regs.clone();
    let mxcsr_before = vcpu.mxcsr;
    let error = vcpu.step().expect_err("reserved EVEX FP compare must #UD");
    assert!(
        format!("{error:?}").contains("IDT entry 6 not present"),
        "wrong exception for {code:02X?}: {error:?}"
    );
    assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: RIP");
    assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: RFLAGS");
    assert_eq!(vcpu.regs.rax, before.rax, "{code:02X?}: RAX");
    assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM");
    assert_eq!(vcpu.regs.ymm_high, before.ymm_high, "{code:02X?}: YMM");
    assert_eq!(vcpu.regs.zmm_high, before.zmm_high, "{code:02X?}: ZMM");
    assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "{code:02X?}: ZMM16-31");
    assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmasks");
    assert_eq!(vcpu.mxcsr, mxcsr_before, "{code:02X?}: MXCSR");
}

#[test]
fn evex_scalar_fp_compare_accepts_exact_llig_sae_control_domain() {
    for elem_size in [2, 4, 8] {
        for ll in 0..4 {
            for suppress_exceptions in [false, true] {
                let code = encoding(elem_size, true, ll, suppress_exceptions);
                if ll == 3 && !suppress_exceptions {
                    assert_reserved_ud(&code);
                    continue;
                }
                let mut vcpu = vcpu(&code);
                vcpu.regs.k[1] = u64::MAX;
                assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
                assert_eq!(vcpu.regs.k[1], 1, "{code:02X?}");
                assert_eq!(vcpu.regs.rip, CODE + code.len() as u64, "{code:02X?}");
            }
        }
    }
}

#[test]
fn evex_packed_fp_compare_uses_exact_width_and_sae_controls() {
    for elem_size in [2, 4, 8] {
        for ll in 0..3 {
            let code = encoding(elem_size, false, ll, false);
            let mut vcpu = vcpu(&code);
            vcpu.regs.k[1] = u64::MAX;
            assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
            let lanes = [16usize, 32, 64][ll as usize] / elem_size;
            assert_eq!(vcpu.regs.k[1], (1u64 << lanes) - 1, "{code:02X?}");
        }

        for ll in 0..4 {
            let sae = encoding(elem_size, false, ll, true);
            let mut vcpu = vcpu(&sae);
            vcpu.regs.k[1] = u64::MAX;
            assert!(vcpu.step().unwrap().is_none(), "{sae:02X?}");
            let lanes = 64 / elem_size;
            assert_eq!(vcpu.regs.k[1], (1u64 << lanes) - 1, "{sae:02X?}");
        }
    }
}

#[test]
fn evex_fp_compare_rejects_reserved_controls_before_memory_or_state_access() {
    for elem_size in [2, 4, 8] {
        assert_reserved_ud(&as_memory(encoding(elem_size, true, 3, false)));
        assert_reserved_ud(&as_memory(encoding(elem_size, true, 0, true)));
        assert_reserved_ud(&encoding(elem_size, false, 3, false));
        assert_reserved_ud(&as_memory(encoding(elem_size, false, 3, true)));
    }

    let valid = encoding(4, true, 2, false);
    let mut zeroing = valid;
    zeroing[3] |= 0x80;
    assert_reserved_ud(&zeroing);
    let mut extended_destination_r = valid;
    extended_destination_r[1] &= !0x80;
    assert_reserved_ud(&extended_destination_r);
    let mut extended_destination_r_prime = valid;
    extended_destination_r_prime[1] &= !0x10;
    assert_reserved_ud(&extended_destination_r_prime);
    let mut reserved_immediate = valid;
    reserved_immediate[6] = 0x20;
    assert_reserved_ud(&reserved_immediate);
}
