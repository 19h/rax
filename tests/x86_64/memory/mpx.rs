use crate::common::TestCase;

// BNDCL - Check Lower Bound
// Compares address with lower bound in bounds register

#[test]
fn test_bndcl_r32() {
    TestCase::from("f3 0f 1a 08").check();
}

#[test]
fn test_bndcl_r64() {
    TestCase::from("f3 0f 1a 08").check();
}

#[test]
fn test_bndcl_bnd0_eax() {
    TestCase::from("f3 0f 1a 00").check();
}

#[test]
fn test_bndcl_bnd1_ecx() {
    TestCase::from("f3 0f 1a 09").check();
}

#[test]
fn test_bndcl_bnd2_edx() {
    TestCase::from("f3 0f 1a 12").check();
}

#[test]
fn test_bndcl_bnd3_ebx() {
    TestCase::from("f3 0f 1a 1b").check();
}

#[test]
fn test_bndcl_bnd0_mem32() {
    TestCase::from("f3 0f 1a 00").check();
}

#[test]
fn test_bndcl_bnd1_mem32() {
    TestCase::from("f3 0f 1a 09").check();
}

#[test]
fn test_bndcl_bnd0_rax() {
    TestCase::from("f3 48 0f 1a 00").check();
}

#[test]
fn test_bndcl_bnd1_rcx() {
    TestCase::from("f3 48 0f 1a 09").check();
}

#[test]
fn test_bndcl_bnd2_rdx() {
    TestCase::from("f3 48 0f 1a 12").check();
}

#[test]
fn test_bndcl_bnd3_rbx() {
    TestCase::from("f3 48 0f 1a 1b").check();
}

// BNDCU/BNDCN - Check Upper Bound

#[test]
fn test_bndcu_r32() {
    TestCase::from("f2 0f 1a 08").check();
}

#[test]
fn test_bndcu_r64() {
    TestCase::from("f2 0f 1a 08").check();
}

#[test]
fn test_bndcu_bnd0_eax() {
    TestCase::from("f2 0f 1a 00").check();
}

#[test]
fn test_bndcu_bnd1_ecx() {
    TestCase::from("f2 0f 1a 09").check();
}

#[test]
fn test_bndcu_bnd2_edx() {
    TestCase::from("f2 0f 1a 12").check();
}

#[test]
fn test_bndcu_bnd3_ebx() {
    TestCase::from("f2 0f 1a 1b").check();
}

#[test]
fn test_bndcn_bnd0_rax() {
    TestCase::from("f2 48 0f 1a 00").check();
}

#[test]
fn test_bndcn_bnd1_rcx() {
    TestCase::from("f2 48 0f 1a 09").check();
}

#[test]
fn test_bndcn_bnd2_rdx() {
    TestCase::from("f2 48 0f 1a 12").check();
}

#[test]
fn test_bndcn_bnd3_rbx() {
    TestCase::from("f2 48 0f 1a 1b").check();
}

// BNDMK - Make Bounds
// Creates bounds from memory operand

#[test]
fn test_bndmk_bnd0_m32() {
    TestCase::from("f3 0f 1b 00").check();
}

#[test]
fn test_bndmk_bnd1_m32() {
    TestCase::from("f3 0f 1b 08").check();
}

#[test]
fn test_bndmk_bnd2_m32() {
    TestCase::from("f3 0f 1b 10").check();
}

#[test]
fn test_bndmk_bnd3_m32() {
    TestCase::from("f3 0f 1b 18").check();
}

#[test]
fn test_bndmk_bnd0_m64() {
    TestCase::from("f3 48 0f 1b 00").check();
}

#[test]
fn test_bndmk_bnd1_m64() {
    TestCase::from("f3 48 0f 1b 08").check();
}

#[test]
fn test_bndmk_bnd2_m64() {
    TestCase::from("f3 48 0f 1b 10").check();
}

#[test]
fn test_bndmk_bnd3_m64() {
    TestCase::from("f3 48 0f 1b 18").check();
}

#[test]
fn test_bndmk_bnd0_rax_base() {
    TestCase::from("f3 48 0f 1b 00").check();
}

#[test]
fn test_bndmk_bnd1_rcx_base() {
    TestCase::from("f3 48 0f 1b 09").check();
}

#[test]
fn test_bndmk_bnd2_rdx_base() {
    TestCase::from("f3 48 0f 1b 12").check();
}

#[test]
fn test_bndmk_bnd3_rbx_base() {
    TestCase::from("f3 48 0f 1b 1b").check();
}

// BNDMOV - Move Bounds
// Moves bounds between bound registers or memory

#[test]
fn test_bndmov_bnd0_bnd1() {
    TestCase::from("66 0f 1a c1").check();
}

#[test]
fn test_bndmov_bnd1_bnd0() {
    TestCase::from("66 0f 1a c8").check();
}

#[test]
fn test_bndmov_bnd2_bnd3() {
    TestCase::from("66 0f 1a d3").check();
}

#[test]
fn test_bndmov_bnd3_bnd2() {
    TestCase::from("66 0f 1a da").check();
}

#[test]
fn test_bndmov_bnd0_m64() {
    TestCase::from("66 0f 1a 00").check();
}

#[test]
fn test_bndmov_bnd1_m64() {
    TestCase::from("66 0f 1a 08").check();
}

#[test]
fn test_bndmov_bnd2_m64() {
    TestCase::from("66 0f 1a 10").check();
}

#[test]
fn test_bndmov_bnd3_m64() {
    TestCase::from("66 0f 1a 18").check();
}

#[test]
fn test_bndmov_m64_bnd0() {
    TestCase::from("66 0f 1b 00").check();
}

#[test]
fn test_bndmov_m64_bnd1() {
    TestCase::from("66 0f 1b 08").check();
}

#[test]
fn test_bndmov_m64_bnd2() {
    TestCase::from("66 0f 1b 10").check();
}

#[test]
fn test_bndmov_m64_bnd3() {
    TestCase::from("66 0f 1b 18").check();
}

#[test]
fn test_bndmov_bnd0_m128() {
    TestCase::from("66 48 0f 1a 00").check();
}

#[test]
fn test_bndmov_bnd1_m128() {
    TestCase::from("66 48 0f 1a 08").check();
}

#[test]
fn test_bndmov_m128_bnd0() {
    TestCase::from("66 48 0f 1b 00").check();
}

#[test]
fn test_bndmov_m128_bnd1() {
    TestCase::from("66 48 0f 1b 08").check();
}

// BNDLDX - Load Extended Bounds Using Address Translation

#[test]
fn test_bndldx_bnd0() {
    TestCase::from("0f 1a 00").check();
}

#[test]
fn test_bndldx_bnd1() {
    TestCase::from("0f 1a 08").check();
}

#[test]
fn test_bndldx_bnd2() {
    TestCase::from("0f 1a 10").check();
}

#[test]
fn test_bndldx_bnd3() {
    TestCase::from("0f 1a 18").check();
}

#[test]
fn test_bndldx_bnd0_rax_rbx() {
    TestCase::from("0f 1a 04 18").check();
}

#[test]
fn test_bndldx_bnd1_rcx_rdx() {
    TestCase::from("0f 1a 0c 11").check();
}

#[test]
fn test_bndldx_bnd2_rsi_rdi() {
    TestCase::from("0f 1a 14 3e").check();
}

#[test]
fn test_bndldx_bnd3_r8_r9() {
    TestCase::from("44 0f 1a 1c 08").check();
}

// BNDSTX - Store Extended Bounds Using Address Translation

#[test]
fn test_bndstx_bnd0() {
    TestCase::from("0f 1b 00").check();
}

#[test]
fn test_bndstx_bnd1() {
    TestCase::from("0f 1b 08").check();
}

#[test]
fn test_bndstx_bnd2() {
    TestCase::from("0f 1b 10").check();
}

#[test]
fn test_bndstx_bnd3() {
    TestCase::from("0f 1b 18").check();
}

#[test]
fn test_bndstx_rax_rbx_bnd0() {
    TestCase::from("0f 1b 04 18").check();
}

#[test]
fn test_bndstx_rcx_rdx_bnd1() {
    TestCase::from("0f 1b 0c 11").check();
}

#[test]
fn test_bndstx_rsi_rdi_bnd2() {
    TestCase::from("0f 1b 14 3e").check();
}

#[test]
fn test_bndstx_r8_r9_bnd3() {
    TestCase::from("44 0f 1b 1c 08").check();
}
