use crate::common::{run_until_hlt, setup_vm};
use rax::cpu::Registers;
use vm_memory::{Bytes, GuestAddress};

// VMOVSS - Move Scalar Single-Precision Floating-Point

const ALIGNED_ADDR: u64 = 0x3000;

#[test]
fn test_vmovss_xmm2_xmm0_xmm1() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm3_xmm1_xmm2() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm4_xmm2_xmm3() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm5_xmm3_xmm4() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm6_xmm4_xmm5() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm7_xmm5_xmm6() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm8_xmm6_xmm7() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm9_xmm7_xmm8() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm10_xmm8_xmm9() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm11_xmm9_xmm10() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm12_xmm10_xmm11() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm13_xmm11_xmm12() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm14_xmm12_xmm13() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm15_xmm13_xmm14() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm0_xmm14_xmm15() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovss_xmm1_xmm15_xmm0() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
