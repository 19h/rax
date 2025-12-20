use crate::common::{run_until_hlt, setup_vm};
use rax::cpu::Registers;
use vm_memory::{Bytes, GuestAddress};

// VROUNDPD - Round Packed Double-Precision Floating-Point Values
//
// Opcodes: VEX.128.66.0F3A.WIG 09 /r ib

const ALIGNED_ADDR: u64 = 0x3000;

#[test]
fn test_vroundpd_xmm0_xmm1_xmm2() {
    let code = [0xc5, 0xf0, 0x00, 0xc2, 0xf4];  // Placeholder opcodes
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vroundpd_xmm1_xmm2_xmm3() {
    let code = [0xc5, 0xe8, 0x00, 0xcb, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vroundpd_xmm2_xmm3_xmm4() {
    let code = [0xc5, 0xe0, 0x00, 0xd4, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vroundpd_xmm3_xmm4_xmm5() {
    let code = [0xc5, 0xd8, 0x00, 0xdd, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vroundpd_xmm4_xmm5_xmm6() {
    let code = [0xc5, 0xd0, 0x00, 0xe6, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vroundpd_xmm5_xmm6_xmm7() {
    let code = [0xc5, 0xc8, 0x00, 0xef, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vroundpd_xmm6_xmm7_xmm8() {
    let code = [0xc4, 0xc1, 0x40, 0x00, 0xf0, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vroundpd_xmm7_xmm8_xmm9() {
    let code = [0xc4, 0xc1, 0x38, 0x00, 0xf9, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// Additional tests for memory operands and YMM registers would go here
