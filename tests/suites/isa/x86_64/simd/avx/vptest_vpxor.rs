use crate::common::*;

#[test]
fn test_vpxor_xmm_value() {
    // VPXOR XMM0, XMM1, XMM2
    let code = [
        0xc5, 0xf1, 0xef, 0xc2, // VPXOR XMM0, XMM1, XMM2
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let src1 = 0xf0f0_f0f0_aaaa_5555_ffff_0000_1234_5678u128;
    let src2 = 0x0f0f_0f0f_5555_aaaa_0000_ffff_8765_4321u128;
    let mut regs = vcpu.get_regs().unwrap();
    regs.xmm[1] = [src1 as u64, (src1 >> 64) as u64];
    regs.xmm[2] = [src2 as u64, (src2 >> 64) as u64];
    regs.ymm_high[0] = [0xfeed_face_dead_beef, 0x0123_4567_89ab_cdef];
    vcpu.set_regs(&regs).unwrap();

    run_until_hlt(&mut vcpu).unwrap();

    let regs = vcpu.get_regs().unwrap();
    assert_eq!(get_xmm(&regs, 0), src1 ^ src2);
    assert_eq!(
        regs.ymm_high[0],
        [0, 0],
        "VEX.128 VPXOR must clear the destination upper YMM state"
    );
}

#[test]
fn test_vptest_xmm_flags() {
    // VPTEST XMM0, XMM1
    let code = [
        0xc4, 0xe2, 0x79, 0x17, 0xc1, // VPTEST XMM0, XMM1
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let mut regs = vcpu.get_regs().unwrap();
    regs.xmm[0] = [0x00ff, 0];
    regs.xmm[1] = [0x00ff, 0];
    regs.rflags |= 0x8d5; // CF, PF, AF, ZF, SF, OF set before VPTEST.
    vcpu.set_regs(&regs).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert!(cf_set(regs.rflags), "src1 & !src2 is zero");
    assert!(!zf_set(regs.rflags), "src1 & src2 is non-zero");
    assert!(!af_set(regs.rflags));
    assert!(!of_set(regs.rflags));
    assert!(!pf_set(regs.rflags));
    assert!(!sf_set(regs.rflags));
}
