//! Intel APX paired-stack native-JIT tests.

use super::*;

#[test]
fn jit_helper_backed_apx_push2_pop2_matches_interpreter_and_deopts_precisely() {
    const STACK: u64 = 0x11_0000;
    // LLVM 23:
    //   push2p %rax,%rbx; pop2p %rbp,%rcx
    //   push2 %r20,%r21; pop2 %r21,%r20; hlt
    // The first POP2 exercises a legacy RBP destination; the second pair
    // round-trips extended GPRs while leaving their image in the stack slots.
    let code = [
        0x62, 0xF4, 0xE4, 0x18, 0xFF, 0xF0, 0x62, 0xF4, 0xF4, 0x18, 0x8F, 0xC5, 0x62, 0xFC, 0x54,
        0x10, 0xFF, 0xF4, 0x62, 0xFC, 0x5C, 0x10, 0x8F, 0xC5, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        vcpu.set_apx_enabled(true);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0123_4567_89AB_CDEF;
        regs.rbx = 0xFEDC_BA98_7654_3210;
        regs.rcx = 0x1111_2222_3333_4444;
        regs.rbp = 0x5555_6666_7777_8888;
        regs.r20 = 0xA5A5_5A5A_1357_2468;
        regs.r21 = 0x5A5A_A5A5_8642_7531;
        regs.rsp = STACK;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
    };

    let (mut interp, imem) = make_vcpu_mem(&code);
    imem.write_slice(&[0xCC; 16], GuestAddress(STACK - 16))
        .unwrap();
    setup(&mut interp);
    run_interp(&mut interp);
    let expected = interp.get_regs().unwrap();
    let mut expected_stack = [0u8; 16];
    imem.read_slice(&mut expected_stack, GuestAddress(STACK - 16))
        .unwrap();

    let (mut jit, jmem) = make_vcpu_mem(&code);
    jmem.write_slice(&[0xCC; 16], GuestAddress(STACK - 16))
        .unwrap();
    setup(&mut jit);
    jit.set_jit_mem(true);
    assert!(
        jit.jit_try_block().expect("APX paired-stack JIT"),
        "valid PUSH2/POP2 sequences must enter the native tier"
    );
    run_interp(&mut jit);
    let actual = jit.get_regs().unwrap();
    let mut actual_stack = [0u8; 16];
    jmem.read_slice(&mut actual_stack, GuestAddress(STACK - 16))
        .unwrap();

    assert_eq!(actual.rax, expected.rax, "legacy source RAX");
    assert_eq!(actual.rbx, expected.rbx, "legacy source RBX");
    assert_eq!(actual.rcx, expected.rcx, "POP2 low-slot destination");
    assert_eq!(actual.rbp, expected.rbp, "POP2 high-slot RBP destination");
    assert_eq!(actual.r20, expected.r20, "EGPR low-slot round trip");
    assert_eq!(actual.r21, expected.r21, "EGPR high-slot round trip");
    assert_eq!(actual.rsp, expected.rsp, "balanced paired stack pointer");
    assert_eq!(actual.rflags, expected.rflags, "paired stack flags");
    assert_eq!(actual_stack, expected_stack, "paired stack image");
    assert_eq!(
        u64::from_le_bytes(actual_stack[..8].try_into().unwrap()),
        actual.r20,
        "PUSH2 ModRM B operand occupies the lower qword"
    );
    assert_eq!(
        u64::from_le_bytes(actual_stack[8..].try_into().unwrap()),
        actual.r21,
        "PUSH2 EVEX V operand occupies the upper qword"
    );

    let push2 = [0x62, 0xF4, 0x64, 0x18, 0xFF, 0xF0];
    let pop2 = [0x62, 0xF4, 0x74, 0x18, 0x8F, 0xC5];
    for (name, instruction, apx_enabled, rsp) in [
        ("misaligned PUSH2", &push2[..], true, STACK + 8),
        ("unmapped PUSH2", &push2[..], true, MEM_SIZE + 16),
        ("disabled-APX PUSH2", &push2[..], false, STACK),
        ("unmapped POP2", &pop2[..], true, MEM_SIZE),
    ] {
        let mut fault_code = instruction.to_vec();
        fault_code.extend_from_slice(&[0xBE, 0x01, 0x00, 0x00, 0x00, 0xF4]);
        let (mut fault, memory) = make_vcpu_mem(&fault_code);
        memory
            .write_slice(&[0xCC; 16], GuestAddress(STACK - 16))
            .unwrap();
        fault.set_apx_enabled(apx_enabled);
        let mut before = fault.get_regs().unwrap();
        before.rax = 0x0123_4567_89AB_CDEF;
        before.rbx = 0xFEDC_BA98_7654_3210;
        before.rcx = 0x1111_2222_3333_4444;
        before.rbp = 0x5555_6666_7777_8888;
        before.rsi = 0xDEAD_BEEF_CAFE_BABE;
        before.rsp = rsp;
        before.rflags = 0x2 | 0x8D5;
        fault.set_regs(&before).unwrap();
        fault.set_jit_mem(true);
        assert!(
            fault
                .jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error}")),
            "{name}: exact pair sequence must compile before helper deopt"
        );
        let after = fault.get_regs().unwrap();
        let mut unchanged_stack = [0u8; 16];
        memory
            .read_slice(&mut unchanged_stack, GuestAddress(STACK - 16))
            .unwrap();
        assert_eq!(after.rip, LOAD_ADDR, "{name}: current-instruction PC");
        assert_eq!(after.rsp, before.rsp, "{name}: RSP must not commit");
        assert_eq!(after.rax, before.rax, "{name}: RAX");
        assert_eq!(after.rbx, before.rbx, "{name}: RBX");
        assert_eq!(after.rcx, before.rcx, "{name}: RCX");
        assert_eq!(after.rbp, before.rbp, "{name}: RBP");
        assert_eq!(after.rsi, before.rsi, "{name}: following MOV must not run");
        assert_eq!(after.rflags, before.rflags, "{name}: RFLAGS");
        assert_eq!(unchanged_stack, [0xCC; 16], "{name}: memory commit");
    }
}

#[test]
fn jit_executes_supported_prefix_before_reserved_apx_group_frontiers() {
    // Each reserved APX cell is a terminal #UD known at the ModR/M byte. A
    // strict lift must preserve the preceding native work and hand off at the
    // invalid instruction, never parse its apparent address or execute the
    // following MOV.
    for (name, invalid) in [
        (
            "POP2 reserved /1",
            &[0x62, 0xF4, 0x7C, 0x18, 0x8F, 0xCB][..],
        ),
        (
            "POP2 memory requiring SIB",
            &[0x62, 0xF4, 0x7C, 0x18, 0x8F, 0x04][..],
        ),
        (
            "Group 4 reserved /2",
            &[0x62, 0xF4, 0x64, 0x18, 0xFE, 0xD3][..],
        ),
        (
            "Group 5 reserved /2",
            &[0x62, 0xF4, 0x64, 0x18, 0xFF, 0xD3][..],
        ),
        (
            "PUSH2 memory requiring SIB",
            &[0x62, 0xF4, 0x64, 0x18, 0xFF, 0x34][..],
        ),
        (
            "PUSH2 reserved NF",
            &[0x62, 0xF4, 0x64, 0x1C, 0xFF, 0xF0][..],
        ),
    ] {
        let mut code = vec![0xBE, 0x78, 0x56, 0x34, 0x12]; // mov esi,0x12345678
        code.extend_from_slice(invalid);
        code.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00, 0xF4]);

        let mut vcpu = make_vcpu_code(&code);
        vcpu.set_apx_enabled(true);
        let mut before = vcpu.get_regs().unwrap();
        before.rdi = 0xDEAD_BEEF_CAFE_BABE;
        before.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&before).unwrap();

        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error}")),
            "{name}: reserved APX frontier must retain the native prefix"
        );

        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rsi, 0x1234_5678, "{name}: native prefix result");
        assert_eq!(
            after.rdi, before.rdi,
            "{name}: instruction after #UD must not execute"
        );
        assert_eq!(after.rflags, before.rflags, "{name}: flags");
        assert_eq!(
            after.rip,
            LOAD_ADDR + 5,
            "{name}: handoff at reserved APX instruction"
        );
    }
}
