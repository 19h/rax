//! AMD SSE4A native-JIT and dynamic-guard regressions.

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

#[test]
fn jit_sse4a_movnt_matches_direct_for_both_widths_and_extended_xmm() {
    const DATA: u64 = 0x20_0000;
    let code = [
        0xBE, 0x78, 0x56, 0x34, 0x12, // mov esi,0x12345678
        0xF3, 0x0F, 0x2B, 0x0F, // movntss [rdi],xmm1
        0xF2, 0x44, 0x0F, 0x2B, 0x4F, 0x08, // movntsd [rdi+8],xmm9
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
        seed(vcpu, true);
        memory.write_slice(&[0xCC; 24], GuestAddress(DATA)).unwrap();
        let mut regs = vcpu.get_regs().unwrap();
        regs.rdi = DATA;
        regs.xmm[1] = [0x1122_3344_5566_7788, 0xA1A2_A3A4_A5A6_A7A8];
        regs.xmm[9] = [0x8877_6655_4433_2211, 0xB1B2_B3B4_B5B6_B7B8];
        vcpu.set_regs(&regs).unwrap();
    };

    let (mut direct, direct_memory) = make_vcpu_mem(&code);
    setup(&mut direct, &direct_memory);
    run_interp(&mut direct);
    let expected_regs = direct.get_regs().unwrap();
    let mut expected_memory = [0u8; 24];
    direct_memory
        .read_slice(&mut expected_memory, GuestAddress(DATA))
        .unwrap();

    let (mut jit, jit_memory) = make_vcpu_mem(&code);
    setup(&mut jit, &jit_memory);
    jit.set_jit_call(false);
    jit.set_jit_mem(true);
    assert!(
        jit.jit_try_block().expect("SSE4A scalar MOVNT JIT"),
        "MOVNTSS/MOVNTSD must enter the native tier:\n{}",
        jit.jit_dump_region(LOAD_ADDR)
    );
    run_interp(&mut jit);
    let actual_regs = jit.get_regs().unwrap();
    let mut actual_memory = [0u8; 24];
    jit_memory
        .read_slice(&mut actual_memory, GuestAddress(DATA))
        .unwrap();

    assert_eq!(actual_memory, expected_memory);
    assert_eq!(&actual_memory[..4], &[0x88, 0x77, 0x66, 0x55]);
    assert_eq!(&actual_memory[4..8], &[0xCC; 4]);
    assert_eq!(
        &actual_memory[8..16],
        &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]
    );
    assert_eq!(actual_regs.xmm, expected_regs.xmm);
    assert_eq!(actual_regs.ymm_high, expected_regs.ymm_high);
    assert_eq!(actual_regs.zmm_high, expected_regs.zmm_high);
    assert_eq!(actual_regs.rsi, expected_regs.rsi);
    assert_eq!(actual_regs.rdi, expected_regs.rdi);
    assert_eq!(actual_regs.rflags, expected_regs.rflags);
    assert_eq!(actual_regs.rip, expected_regs.rip);
}

#[test]
fn jit_sse4a_movnt_guard_and_helper_deopt_are_precise_and_noncommitting() {
    const DATA: u64 = 0x20_0000;
    let code = [
        0xBE, 0x78, 0x56, 0x34, 0x12, // mov esi,0x12345678
        0xF3, 0x0F, 0x2B, 0x0F, // movntss [rdi],xmm1
        0xBA, 0xEF, 0xBE, 0xAD, 0xDE, // mov edx,0xdeadbeef
        0xF4,
    ];

    for (name, enabled, cr0, cr4, vector) in [
        ("feature absent", false, 0x21, 0x20 | CR4_OSFXSR, 6),
        ("CR0.TS", true, 0x21 | (1 << 3), 0x20 | CR4_OSFXSR, 7),
        (
            "CR0.EM precedes CR0.TS",
            true,
            0x21 | (1 << 2) | (1 << 3),
            0x20 | CR4_OSFXSR,
            6,
        ),
    ] {
        let (mut jit, memory) = make_vcpu_mem(&code);
        seed(&mut jit, true);
        let mut regs = jit.get_regs().unwrap();
        regs.rdi = DATA;
        jit.set_regs(&regs).unwrap();
        jit.set_jit_call(false);
        jit.set_jit_mem(true);
        assert!(jit.jit_try_block().expect("prime SSE4A MOVNT region"));

        seed(&mut jit, enabled);
        let mut reset = jit.get_regs().unwrap();
        reset.rip = LOAD_ADDR;
        reset.rdi = DATA;
        reset.rdx = 0xA5A5_A5A5_A5A5_A5A5;
        jit.set_regs(&reset).unwrap();
        let mut sregs = jit.get_sregs().unwrap();
        sregs.cr0 = cr0;
        sregs.cr4 = cr4;
        jit.set_sregs(&sregs).unwrap();
        memory.write_slice(&[0xCC; 8], GuestAddress(DATA)).unwrap();

        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error}"))
        );
        let after_guard = jit.get_regs().unwrap();
        let mut stored = [0u8; 8];
        memory.read_slice(&mut stored, GuestAddress(DATA)).unwrap();
        assert_eq!(stored, [0xCC; 8], "{name}: memory commit");
        assert_eq!(after_guard.rip, LOAD_ADDR + 5, "{name}: exact frontier");
        assert_eq!(after_guard.rsi, 0x1234_5678, "{name}: native prefix");
        assert_eq!(after_guard.rdx, reset.rdx, "{name}: following instruction");
        assert_eq!(after_guard.xmm, reset.xmm, "{name}: XMM state");
        assert_eq!(after_guard.rflags, INITIAL_FLAGS, "{name}: flags");

        let error = format!("{:#}", jit.step().expect_err(name));
        assert!(
            error.contains(&format!("IDT entry {vector} not present")),
            "{name}: {error}"
        );
        memory.read_slice(&mut stored, GuestAddress(DATA)).unwrap();
        assert_eq!(stored, [0xCC; 8], "{name}: direct replay memory");
        assert_eq!(
            jit.get_regs().unwrap().rip,
            LOAD_ADDR + 5,
            "{name}: replay RIP"
        );
    }

    let (mut unmapped, _) = make_vcpu_mem(&code);
    seed(&mut unmapped, true);
    let mut regs = unmapped.get_regs().unwrap();
    regs.rdi = MEM_SIZE + 0x100;
    regs.rdx = 0xA5A5_A5A5_A5A5_A5A5;
    unmapped.set_regs(&regs).unwrap();
    unmapped.set_jit_call(false);
    unmapped.set_jit_mem(true);
    assert!(unmapped.jit_try_block().expect("unmapped MOVNT deopt"));
    let after = unmapped.get_regs().unwrap();
    assert_eq!(after.rip, LOAD_ADDR + 5, "helper deopt frontier");
    assert_eq!(after.rsi, 0x1234_5678, "native prefix");
    assert_eq!(
        after.rdx, regs.rdx,
        "following instruction must not execute"
    );
    assert_eq!(after.xmm, regs.xmm, "helper deopt XMM state");
    assert_eq!(after.rflags, INITIAL_FLAGS, "helper deopt flags");
}

#[test]
fn jit_sse4a_movnt_observes_prior_native_vector_result() {
    if !std::is_x86_feature_detected!("avx512f") {
        return;
    }

    const DATA: u64 = 0x20_0000;
    let code = [
        0x66, 0x0F, 0xEF, 0xC9, // pxor xmm1,xmm1
        0xF3, 0x0F, 0x2B, 0x0F, // movntss [rdi],xmm1
        0xF4,
    ];
    let (mut jit, memory) = make_vcpu_mem(&code);
    seed(&mut jit, true);
    memory.write_slice(&[0xCC; 8], GuestAddress(DATA)).unwrap();
    let mut regs = jit.get_regs().unwrap();
    regs.rdi = DATA;
    regs.xmm[1] = [u64::MAX; 2];
    jit.set_regs(&regs).unwrap();
    jit.set_jit_call(false);
    jit.set_jit_mem(true);

    assert!(jit.jit_try_block().expect("mixed vector/SSE4A MOVNT JIT"));
    let mut stored = [0u8; 8];
    memory.read_slice(&mut stored, GuestAddress(DATA)).unwrap();
    assert_eq!(&stored[..4], &[0; 4], "helper must observe native PXOR");
    assert_eq!(&stored[4..], &[0xCC; 4], "scalar width");
}
