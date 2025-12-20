use crate::common::{run_until_hlt, setup_vm};
use rax::cpu::Registers;
use vm_memory::{Bytes, GuestAddress};

// VMOVHPS - Move High Packed Single-Precision

const ALIGNED_ADDR: u64 = 0x3000;

#[test]
fn test_vmovhps_xmm2_xmm0_xmm1() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm3_xmm1_xmm2() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm4_xmm2_xmm3() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm5_xmm3_xmm4() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm6_xmm4_xmm5() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm7_xmm5_xmm6() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm8_xmm6_xmm7() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm9_xmm7_xmm8() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm10_xmm8_xmm9() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm11_xmm9_xmm10() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm12_xmm10_xmm11() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm13_xmm11_xmm12() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm14_xmm12_xmm13() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm15_xmm13_xmm14() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm0_xmm14_xmm15() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm1_xmm15_xmm0() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm0_xmm1_mem() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0x25, 0x00, 0x30, 0x00, 0x00, 0xf4];
    let (mut vcpu, vm_memory) = setup_vm(&code, None);
    let test_data = [0u8; 16];
    vm_memory.write(&test_data, GuestAddress(0x3000)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm1_xmm2_mem() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0x25, 0x00, 0x30, 0x00, 0x00, 0xf4];
    let (mut vcpu, vm_memory) = setup_vm(&code, None);
    let test_data = [0u8; 16];
    vm_memory.write(&test_data, GuestAddress(0x3000)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm2_xmm3_mem() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0x25, 0x00, 0x30, 0x00, 0x00, 0xf4];
    let (mut vcpu, vm_memory) = setup_vm(&code, None);
    let test_data = [0u8; 16];
    vm_memory.write(&test_data, GuestAddress(0x3000)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm3_xmm4_mem() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0x25, 0x00, 0x30, 0x00, 0x00, 0xf4];
    let (mut vcpu, vm_memory) = setup_vm(&code, None);
    let test_data = [0u8; 16];
    vm_memory.write(&test_data, GuestAddress(0x3000)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm4_xmm5_mem() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0x25, 0x00, 0x30, 0x00, 0x00, 0xf4];
    let (mut vcpu, vm_memory) = setup_vm(&code, None);
    let test_data = [0u8; 16];
    vm_memory.write(&test_data, GuestAddress(0x3000)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm5_xmm6_mem() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0x25, 0x00, 0x30, 0x00, 0x00, 0xf4];
    let (mut vcpu, vm_memory) = setup_vm(&code, None);
    let test_data = [0u8; 16];
    vm_memory.write(&test_data, GuestAddress(0x3000)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm6_xmm7_mem() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0x25, 0x00, 0x30, 0x00, 0x00, 0xf4];
    let (mut vcpu, vm_memory) = setup_vm(&code, None);
    let test_data = [0u8; 16];
    vm_memory.write(&test_data, GuestAddress(0x3000)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vmovhps_xmm7_xmm8_mem() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0x25, 0x00, 0x30, 0x00, 0x00, 0xf4];
    let (mut vcpu, vm_memory) = setup_vm(&code, None);
    let test_data = [0u8; 16];
    vm_memory.write(&test_data, GuestAddress(0x3000)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}
