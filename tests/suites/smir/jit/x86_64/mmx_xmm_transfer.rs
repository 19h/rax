//! End-to-end native-JIT coverage for legacy SSE2 MMX/XMM transfers.

use super::*;

#[test]
fn jit_movq2dq_movdq2q_matches_direct_and_preserves_unwritten_state() {
    // mov esi,0x12345678; movdq2q mm7,xmm14; movq2dq xmm15,mm3; hlt
    // REX.B extends only the MOVDQ2Q XMM source; REX.R extends only the
    // MOVQ2DQ XMM destination.
    let code = [
        0xBE, 0x78, 0x56, 0x34, 0x12, 0xF2, 0x41, 0x0F, 0xD6, 0xFE, 0xF3, 0x44, 0x0F, 0xD6, 0xFB,
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.cr4 |= 1 << 9; // CR4.OSFXSR
        vcpu.set_sregs(&sregs).unwrap();

        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xA1A2_A3A4_A5A6_A7A8;
        regs.rsi = 0xFFFF_FFFF_0000_0000;
        regs.rflags = 0x2 | 0x08D5 | (1 << 10);
        regs.mm = std::array::from_fn(|index| {
            0x1100_0000_0000_0000 | ((index as u64) << 48) | index as u64
        });
        for index in 0..16 {
            regs.xmm[index] = [
                0x2200_0000_0000_0000 | ((index as u64) << 48),
                0x3300_0000_0000_0000 | ((index as u64) << 48),
            ];
            regs.ymm_high[index] = [
                0x4400_0000_0000_0000 | index as u64,
                0x5500_0000_0000_0000 | index as u64,
            ];
            regs.zmm_high[index] = [
                0x6600_0000_0000_0000 | index as u64,
                0x7700_0000_0000_0000 | index as u64,
                0x8800_0000_0000_0000 | index as u64,
                0x9900_0000_0000_0000 | index as u64,
            ];
        }
        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut direct, _) = make_vcpu_mem(&code);
    let initial = setup(&mut direct);
    run_interp(&mut direct);
    let expected = direct.get_regs().unwrap();

    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    jit.set_jit_call(false);
    jit.set_jit_mem(false);
    assert!(
        jit.jit_try_block().expect("MMX/XMM transfer JIT"),
        "register-only MOVDQ2Q/MOVQ2DQ sequence must enter the native tier:\n{}",
        jit.jit_dump_region(LOAD_ADDR)
    );
    assert_eq!(
        jit.get_regs().unwrap().rip,
        LOAD_ADDR + code.len() as u64 - 1,
        "HLT must remain the exact interpreter frontier"
    );
    run_interp(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_eq!(actual.mm, expected.mm, "MMX state");
    assert_eq!(actual.xmm, expected.xmm, "low XMM state");
    assert_eq!(actual.ymm_high, expected.ymm_high, "YMM upper state");
    assert_eq!(actual.zmm_high, expected.zmm_high, "ZMM upper state");
    assert_eq!(actual.zmm_ext, expected.zmm_ext, "extended ZMM state");
    assert_eq!(
        actual.rax, expected.rax,
        "state-pointer scratch preservation"
    );
    assert_eq!(actual.rsi, expected.rsi, "native scalar prefix");
    assert_eq!(actual.rflags, expected.rflags, "architectural flags");
    assert_eq!(actual.rip, expected.rip, "final PC");

    assert_eq!(actual.rsi, 0x1234_5678);
    assert_eq!(actual.mm[7], initial.xmm[14][0]);
    assert_eq!(&actual.mm[..7], &initial.mm[..7]);
    assert_eq!(actual.xmm[14], initial.xmm[14], "MOVDQ2Q source");
    assert_eq!(actual.xmm[15], [initial.mm[3], 0]);
    assert_eq!(actual.ymm_high[15], initial.ymm_high[15]);
    assert_eq!(actual.zmm_high[15], initial.zmm_high[15]);
}
