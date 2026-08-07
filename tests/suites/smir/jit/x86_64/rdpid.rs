//! End-to-end native-JIT coverage for state-backed RDPID destinations.

use super::*;

fn gprs(registers: &Registers) -> [u64; 32] {
    [
        registers.rax,
        registers.rcx,
        registers.rdx,
        registers.rbx,
        registers.rsp,
        registers.rbp,
        registers.rsi,
        registers.rdi,
        registers.r8,
        registers.r9,
        registers.r10,
        registers.r11,
        registers.r12,
        registers.r13,
        registers.r14,
        registers.r15,
        registers.r16,
        registers.r17,
        registers.r18,
        registers.r19,
        registers.r20,
        registers.r21,
        registers.r22,
        registers.r23,
        registers.r24,
        registers.r25,
        registers.r26,
        registers.r27,
        registers.r28,
        registers.r29,
        registers.r30,
        registers.r31,
    ]
}

fn write_tsc_aux_prefix(tsc_aux: u32) -> Vec<u8> {
    let mut code = vec![0xB9]; // mov ecx,IA32_TSC_AUX
    code.extend_from_slice(&0xC000_0103u32.to_le_bytes());
    code.push(0xB8); // mov eax,tsc_aux
    code.extend_from_slice(&tsc_aux.to_le_bytes());
    code.extend_from_slice(&[0x31, 0xD2, 0x0F, 0x30]); // xor edx,edx; wrmsr
    code
}

fn run_tsc_aux_setup(vcpu: &mut X86_64Vcpu) {
    for _ in 0..4 {
        assert!(vcpu.step().expect("RDPID setup instruction").is_none());
    }
}

fn seed_state(vcpu: &mut X86_64Vcpu) {
    let mut registers = vcpu.get_regs().unwrap();
    registers.rax = 0x0123_4567_89AB_CDEF;
    registers.rcx = 0x1357_9BDF_2468_ACE0;
    registers.rdx = 0x0F1E_2D3C_4B5A_6978;
    registers.rbx = 0xFEDC_BA98_7654_3210;
    registers.rsp = 0x11_0000;
    registers.rbp = 0x9999_AAAA_BBBB_CCCC;
    registers.rsi = 0x1111_2222_3333_4444;
    registers.rdi = 0x5555_6666_7777_8888;
    registers.r8 = 0x0101_0202_0303_0404;
    registers.r9 = 0x0505_0606_0707_0808;
    registers.r16 = 0x2121_2222_2323_2424;
    registers.r31 = 0xF1F1_F2F2_F3F3_F4F4;
    registers.rflags = 0x2 | 0x8D5;
    registers.xmm[0] = [0x8000_0000_0000_0001, 0x7FF8_1234_5678_9ABC];
    registers.ymm_high[0] = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
    registers.zmm_high[0] = [1, 2, 3, 4];
    registers.zmm_ext[0] = [5, 6, 7, 8, 9, 10, 11, 12];
    registers.k[1] = 0xA5A5_5A5A_C3C3_3C3C;
    registers.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
    vcpu.set_regs(&registers).unwrap();
}

/// RDPID must read the emulated IA32_TSC_AUX field rather than the host
/// thread's processor identifier, zero-extend the 32-bit result, and leave
/// RFLAGS unchanged before the loop's DEC.
#[test]
fn jit_rdpid_reads_guest_tsc_aux_matches_interpreter() {
    const TSC_AUX: u32 = 0xA5C3_7E91;
    const ITERATIONS: u32 = 100;
    // Setup (interpreted once):
    //   mov ecx,0xc0000103; mov eax,TSC_AUX; xor edx,edx; wrmsr
    //   mov ecx,ITERATIONS
    // loop: rdpid r8d; dec ecx; jnz loop; hlt
    let mut code = write_tsc_aux_prefix(TSC_AUX);
    code.push(0xB9);
    code.extend_from_slice(&ITERATIONS.to_le_bytes());
    code.extend_from_slice(&[
        0xF3, 0x41, 0x0F, 0xC7, 0xF8, // rdpid r8d
        0xFF, 0xC9, // dec ecx
        0x75, 0xF7, // jnz loop
        0xF4,
    ]);

    let setup = |vcpu: &mut X86_64Vcpu| {
        for _ in 0..5 {
            assert!(vcpu.step().expect("RDPID setup instruction").is_none());
        }
        let mut regs = vcpu.get_regs().unwrap();
        regs.r8 = u64::MAX;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block().expect("JIT RDPID loop"),
        "state-backed RDPID loop must enter the native tier"
    );
    run_interp(&mut jit);

    let expected = interp.get_regs().unwrap();
    let actual = jit.get_regs().unwrap();
    assert_eq!(actual.r8, u64::from(TSC_AUX));
    assert_eq!(actual.r8, expected.r8);
    assert_eq!(actual.rcx, 0);
    assert_eq!(actual.rflags, expected.rflags);
}

#[test]
fn jit_rdpid_stack_destinations_cover_all_four_scanner_cells_and_preserve_full_state() {
    const TSC_AUX: u32 = 0xA5C3_7E91;
    for (name, instruction, destination) in [
        ("RDPID ESP", &[0xF3, 0x0F, 0xC7, 0xFC][..], 4usize),
        ("RDPID EBP", &[0xF3, 0x0F, 0xC7, 0xFD][..], 5usize),
        (
            "REX.W RDPID RSP",
            &[0xF3, 0x48, 0x0F, 0xC7, 0xFC][..],
            4usize,
        ),
        (
            "REX.W RDPID RBP",
            &[0xF3, 0x48, 0x0F, 0xC7, 0xFD][..],
            5usize,
        ),
    ] {
        let mut code = write_tsc_aux_prefix(TSC_AUX);
        code.extend_from_slice(instruction);
        code.push(0xF4);

        let mut direct = make_vcpu_code(&code);
        run_tsc_aux_setup(&mut direct);
        seed_state(&mut direct);
        assert!(
            direct
                .step()
                .unwrap_or_else(|error| panic!("{name}: direct: {error}"))
                .is_none(),
            "{name}: direct instruction must fall through"
        );
        let expected = direct.get_regs().unwrap();
        assert_eq!(gprs(&expected)[destination], u64::from(TSC_AUX), "{name}");

        let mut jit = make_vcpu_code(&code);
        run_tsc_aux_setup(&mut jit);
        seed_state(&mut jit);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: JIT: {error}")),
            "{name}: state-backed RDPID must enter the native tier:\n{}",
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();

        assert_eq!(gprs(&actual), gprs(&expected), "{name}: GPRs");
        assert_eq!(actual.rflags, expected.rflags, "{name}: RFLAGS");
        assert_eq!(actual.rip, expected.rip, "{name}: RIP");
        assert_eq!(actual.xmm, expected.xmm, "{name}: XMM state");
        assert_eq!(actual.ymm_high, expected.ymm_high, "{name}: YMM state");
        assert_eq!(actual.zmm_high, expected.zmm_high, "{name}: ZMM state");
        assert_eq!(actual.zmm_ext, expected.zmm_ext, "{name}: ZMM16-31 state");
        assert_eq!(actual.k, expected.k, "{name}: opmask state");
        assert_eq!(actual.mm, expected.mm, "{name}: MMX state");
    }
}
