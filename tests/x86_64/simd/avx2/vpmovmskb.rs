use crate::common::{run_until_hlt, setup_vm};
use rax::cpu::Registers;
use vm_memory::{Bytes, GuestAddress};

// VPMOVMSKB - Move Byte Mask to Integer

const ALIGNED_ADDR: u64 = 0x3000;

#[test]
fn test_vpmovmskb_xmm2_xmm0_xmm1() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm3_xmm1_xmm2() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm4_xmm2_xmm3() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm5_xmm3_xmm4() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm6_xmm4_xmm5() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm7_xmm5_xmm6() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm8_xmm6_xmm7() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm9_xmm7_xmm8() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm10_xmm8_xmm9() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm11_xmm9_xmm10() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm12_xmm10_xmm11() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm13_xmm11_xmm12() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm14_xmm12_xmm13() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm15_xmm13_xmm14() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm0_xmm14_xmm15() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_xmm1_xmm15_xmm0() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];  // Placeholder
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_ymm0_ymm1_ymm2() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_ymm1_ymm2_ymm3() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_ymm2_ymm3_ymm4() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_ymm3_ymm4_ymm5() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_ymm4_ymm5_ymm6() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_ymm5_ymm6_ymm7() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_ymm6_ymm7_ymm0() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
#[test]
fn test_vpmovmskb_ymm7_ymm0_ymm1() {
    let code = [0xc5, 0x00, 0x00, 0x00, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
