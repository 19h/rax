//! AMD SSE4A EXTRQ/INSERTQ native-JIT and dynamic-guard regressions.

use super::*;

const CR4_OSFXSR: u64 = 1 << 9;
const INITIAL_FLAGS: u64 = 0x2 | 0x08D5 | (1 << 10);

fn seed(vcpu: &mut X86_64Vcpu, enabled: bool) {
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cr4 |= CR4_OSFXSR;
    vcpu.set_sregs(&sregs).unwrap();
    vcpu.set_sse4a_enabled(enabled);

    let mut regs = vcpu.get_regs().unwrap();
    regs.rflags = INITIAL_FLAGS;
    regs.rsi = 0;
    regs.xmm[1] = [0xFEDC_BA98_7654_3210, 0x1112_1314_1516_1718];
    regs.xmm[2] = [0xA5, 0x2122_2324_2526_2728];
    regs.xmm[3] = [0xFFFF_FFFF_FFFF_100C, 0x3132_3334_3536_3738];
    regs.xmm[4] = [0xFFFF_0000_FFFF_0000, 0x4142_4344_4546_4748];
    regs.xmm[5] = [0x5A, 0x5152_5354_5556_5758];
    regs.xmm[6] = [0xAAAA_BBBB_CCCC_DDDD, 0x6162_6364_6566_6768];
    regs.xmm[7] = [0xE7, 0xFFFF_FFFF_FFFF_2008];
    regs.xmm[9] = [0x8877_6655_4433_2211, 0x8182_8384_8586_8788];
    regs.xmm[10] = [(4 << 8) | 8, 0x9192_9394_9596_9798];
    for index in [1_usize, 2, 4, 6, 9] {
        regs.ymm_high[index] = [
            0xA100_0000_0000_0000 | index as u64,
            0xA200_0000_0000_0000 | index as u64,
        ];
        regs.zmm_high[index] = [
            0xB100_0000_0000_0000 | index as u64,
            0xB200_0000_0000_0000 | index as u64,
            0xB300_0000_0000_0000 | index as u64,
            0xB400_0000_0000_0000 | index as u64,
        ];
    }
    vcpu.set_regs(&regs).unwrap();
}

fn assert_dynamic_guard_fault(
    code: &[u8],
    enabled: bool,
    cr0: u64,
    cr4: u64,
    vector: u8,
    name: &str,
) {
    let mut jit = make_vcpu_code(code);
    seed(&mut jit, true);
    jit.set_jit_call(false);
    assert!(jit.jit_try_block().expect("prime SSE4A JIT cache"));

    seed(&mut jit, enabled);
    let mut reset = jit.get_regs().unwrap();
    reset.rip = LOAD_ADDR;
    jit.set_regs(&reset).unwrap();
    let mut sregs = jit.get_sregs().unwrap();
    sregs.cr0 = cr0;
    sregs.cr4 = cr4;
    jit.set_sregs(&sregs).unwrap();

    assert!(
        jit.jit_try_block()
            .unwrap_or_else(|error| panic!("{name} cached JIT: {error:?}")),
        "{name}: cached region must execute its dynamic guard"
    );
    let guarded = jit.get_regs().unwrap();
    assert_eq!(guarded.rsi, 0x1234_5678, "{name}: native prefix");
    assert_eq!(guarded.xmm[1], reset.xmm[1], "{name}: EXTRQ commit");
    assert_eq!(guarded.rflags, INITIAL_FLAGS, "{name}: flags commit");
    assert_eq!(guarded.rip, LOAD_ADDR + 5, "{name}: exact frontier");

    let error = match jit.step() {
        Err(error) => format!("{error:#}"),
        Ok(exit) => panic!("{name} direct replay unexpectedly succeeded: {exit:?}"),
    };
    assert!(
        error.contains(&format!("IDT entry {vector} not present")),
        "{name}: {error}"
    );
    let after_fault = jit.get_regs().unwrap();
    assert_eq!(
        after_fault.xmm[1], reset.xmm[1],
        "{name}: direct XMM commit"
    );
    assert_eq!(
        after_fault.rflags, INITIAL_FLAGS,
        "{name}: direct flags commit"
    );
    assert_eq!(after_fault.rip, LOAD_ADDR + 5, "{name}: direct RIP commit");
}

#[test]
fn jit_sse4a_bitfield_matches_direct_and_guard_is_dynamic_noncommitting() {
    let code = [
        0xBE, 0x78, 0x56, 0x34, 0x12, // mov esi,0x12345678
        0x66, 0x0F, 0x78, 0xC1, 0xC8, 0xC4, // extrq xmm1,8,4 (high imm bits ignored)
        0x66, 0x0F, 0x79, 0xD3, // extrq xmm2,xmm3
        0xF2, 0x0F, 0x78, 0xE5, 0x08, 0x10, // insertq xmm4,xmm5,8,16
        0xF2, 0x0F, 0x79, 0xF7, // insertq xmm6,xmm7
        0x66, 0x45, 0x0F, 0x79, 0xCA, // extrq xmm9,xmm10
        0xF4,
    ];

    let mut direct = make_vcpu_code(&code);
    seed(&mut direct, true);
    for _ in 0..6 {
        assert!(direct.step().expect("direct SSE4A instruction").is_none());
    }
    let expected = direct.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    seed(&mut jit, true);
    jit.set_jit_call(false);
    assert!(
        jit.jit_try_block().expect("enabled SSE4A JIT"),
        "SSE4A bitfields must enter the native tier:\n{}",
        jit.jit_dump_region(LOAD_ADDR)
    );
    let actual = jit.get_regs().unwrap();
    assert_eq!(actual.xmm, expected.xmm);
    assert_eq!(actual.ymm_high, expected.ymm_high);
    assert_eq!(actual.zmm_high, expected.zmm_high);
    assert_eq!(actual.rsi, expected.rsi);
    assert_eq!(actual.rflags, expected.rflags);
    assert_eq!(actual.rip, expected.rip);

    for (name, enabled, cr0, cr4, vector) in [
        ("feature absent", false, 0x21, 0x20 | CR4_OSFXSR, 6),
        ("CR0.EM", true, 0x21 | (1 << 2), 0x20 | CR4_OSFXSR, 6),
        ("CR0.TS", true, 0x21 | (1 << 3), 0x20 | CR4_OSFXSR, 7),
        ("CR4.OSFXSR absent", true, 0x21, 0x20, 6),
        (
            "feature absence precedes CR0.TS",
            false,
            0x21 | (1 << 3),
            0x20 | CR4_OSFXSR,
            6,
        ),
        (
            "CR0.EM precedes CR0.TS",
            true,
            0x21 | (1 << 2) | (1 << 3),
            0x20 | CR4_OSFXSR,
            6,
        ),
        (
            "CR4.OSFXSR absence precedes CR0.TS",
            true,
            0x21 | (1 << 3),
            0x20,
            6,
        ),
    ] {
        assert_dynamic_guard_fault(&code, enabled, cr0, cr4, vector, name);
    }
}
