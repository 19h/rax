//! Direct/JIT state-parity tests for `MOVDQ2Q` and `MOVQ2DQ`.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn test_vcpu(code: &[u8]) -> X86_64Vcpu {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cr0 = 0x21;
    vcpu.sregs.cr4 = 1 << 9;
    vcpu.regs.rflags = 0x2 | 0x08D5;
    vcpu
}

fn seed(vcpu: &mut X86_64Vcpu) {
    vcpu.regs.mm =
        std::array::from_fn(|index| 0x1100_0000_0000_0000 | ((index as u64) << 48) | index as u64);
    vcpu.regs.xmm = std::array::from_fn(|index| {
        [
            0x2200_0000_0000_0000 | ((index as u64) << 48),
            0x3300_0000_0000_0000 | ((index as u64) << 48),
        ]
    });
    vcpu.fpu.tag_word = 0xFFFF;
}

#[test]
fn direct_cross_file_transfers_commit_values_and_mmx_tag_after_success() {
    // movdq2q mm7,xmm14; movq2dq xmm15,mm3
    let mut vcpu = test_vcpu(&[0xF2, 0x41, 0x0F, 0xD6, 0xFE, 0xF3, 0x44, 0x0F, 0xD6, 0xFB]);
    seed(&mut vcpu);
    let initial_mm = vcpu.regs.mm;
    let initial_xmm = vcpu.regs.xmm;

    assert!(vcpu.step().unwrap().is_none());
    assert_eq!(vcpu.regs.mm[7], initial_xmm[14][0]);
    assert_eq!(&vcpu.regs.mm[..7], &initial_mm[..7]);
    assert_eq!(vcpu.regs.xmm, initial_xmm);
    assert_eq!(vcpu.fpu.tag_word, 0);
    assert_eq!(vcpu.regs.rip, 5);

    vcpu.fpu.tag_word = 0xFFFF;
    assert!(vcpu.step().unwrap().is_none());
    assert_eq!(vcpu.regs.xmm[15], [initial_mm[3], 0]);
    assert_eq!(vcpu.regs.mm[3], initial_mm[3]);
    assert_eq!(vcpu.fpu.tag_word, 0);
    assert_eq!(vcpu.regs.rip, 10);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn verified_native_region_matches_direct_private_mmx_state() {
    // movdq2q mm7,xmm14; movq2dq xmm15,mm3; jmp next; ret
    let mut vcpu = test_vcpu(&[
        0xF2, 0x41, 0x0F, 0xD6, 0xFE, 0xF3, 0x44, 0x0F, 0xD6, 0xFB, 0xEB, 0x00, 0xC3,
    ]);
    seed(&mut vcpu);
    vcpu.set_jit_call(false);
    vcpu.set_jit_mem(false);

    let region = vcpu
        .jit_compile_region()
        .unwrap()
        .expect("MMX/XMM transfers must be JIT eligible");
    assert!(region.uses_mmx);
    assert!(region.uses_xmm_state);
    assert!(region.uses_x87_tag_state);
    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.regs.rip, 12);
    assert_eq!(vcpu.fpu.tag_word, 0);
}
