//! Native ordinary PUSH/POP differential, width-precedence, and fault tests.

use super::*;

const STACK: u64 = 0x11_2345;
const GROUP5_RSP_PREFIXES: &[&[u8]] = &[
    &[],
    &[0x66],
    &[0xF2],
    &[0xF3],
    &[0x67],
    &[0x64],
    &[0x65],
    &[0x48],
    &[0x44],
    &[0x66, 0x48],
    &[0xF2, 0x48],
    &[0xF3, 0x48],
];

fn setup_stack_case(vcpu: &mut X86_64Vcpu, rax: u64, rbx: u64) {
    let mut regs = vcpu.get_regs().unwrap();
    regs.rax = rax;
    regs.rbx = rbx;
    regs.rcx = 0xDEAD_BEEF_CAFE_BABE;
    regs.rsp = STACK;
    regs.rflags = 0x2 | 0x8D5;
    vcpu.set_regs(&regs).unwrap();
}

fn run_native(code: &[u8], rax: u64, rbx: u64, label: &str) -> Registers {
    let mut jit = make_vcpu_code(code);
    setup_stack_case(&mut jit, rax, rbx);
    jit.set_jit_mem(true);
    assert!(
        jit.jit_try_block()
            .unwrap_or_else(|error| panic!("{label}: {error:?}")),
        "{label}: exact ordinary stack sequence must enter the native tier\n{}",
        jit.jit_dump_region(LOAD_ADDR)
    );
    run_interp(&mut jit);
    jit.get_regs().unwrap()
}

fn run_direct(code: &[u8], rax: u64, rbx: u64) -> Registers {
    let mut direct = make_vcpu_code(code);
    setup_stack_case(&mut direct, rax, rbx);
    run_interp(&mut direct);
    direct.get_regs().unwrap()
}

#[test]
fn jit_group5_push_rsp_scanner_images_use_the_predecrement_source() {
    let mut images = 0usize;
    for prefix in GROUP5_RSP_PREFIXES {
        let word = *prefix == [0x66];
        let mut code = prefix.to_vec();
        code.extend_from_slice(&[0xFF, 0xF4]); // PUSH RSP/SP
        if word {
            code.extend_from_slice(&[0x66, 0x58]); // POP AX
        } else {
            code.push(0x58); // POP RAX
        }
        code.push(0xF4);

        let expected = run_direct(&code, 0, 0xA5A5_5A5A_1234_5678);
        let actual = run_native(&code, 0, 0xA5A5_5A5A_1234_5678, &format!("{code:02X?}"));
        let expected_source = if word { STACK & 0xFFFF } else { STACK };

        assert_eq!(expected.rax, expected_source, "{code:02X?}: direct source");
        assert_eq!(actual.rax, expected.rax, "{code:02X?}: source");
        assert_eq!(actual.rsp, expected.rsp, "{code:02X?}: RSP");
        assert_eq!(actual.rsp, STACK, "{code:02X?}: balanced stack");
        assert_eq!(actual.rbx, expected.rbx, "{code:02X?}: unrelated GPR");
        assert_eq!(actual.rcx, expected.rcx, "{code:02X?}: following state");
        assert_eq!(actual.rflags, expected.rflags, "{code:02X?}: RFLAGS");
        images += 1;
    }
    assert_eq!(images, 12);
}

#[test]
fn jit_ordinary_stack_rex_w_precedence_and_prefix_order_match_direct_execution() {
    const SOURCE: u64 = 0x0123_4567_89AB_CDEF;
    const INITIAL_RBX: u64 = 0xA5A5_5A5A_1357_2468;
    let cases = [
        (
            "66 REX.W PUSH RAX",
            &[0x66, 0x48, 0x50, 0x5B, 0xF4][..],
            SOURCE,
        ),
        (
            "66 REX.W PUSH imm32",
            &[0x66, 0x48, 0x68, 0x80, 0xFF, 0xFF, 0xFF, 0x5B, 0xF4][..],
            (-128_i64) as u64,
        ),
        (
            "66 REX.W PUSH imm8",
            &[0x66, 0x48, 0x6A, 0x80, 0x5B, 0xF4][..],
            (-128_i64) as u64,
        ),
        (
            "66 REX.W Group5 PUSH RAX",
            &[0x66, 0x48, 0xFF, 0xF0, 0x5B, 0xF4][..],
            SOURCE,
        ),
        (
            "66 REX.W POP RBX",
            &[0x50, 0x66, 0x48, 0x8F, 0xC3, 0xF4][..],
            SOURCE,
        ),
        (
            "REX before 66 PUSH AX",
            &[0x48, 0x66, 0x50, 0x66, 0x5B, 0xF4][..],
            (INITIAL_RBX & !0xFFFF) | (SOURCE & 0xFFFF),
        ),
        (
            "REX before 66 PUSH imm16",
            &[0x48, 0x66, 0x68, 0x80, 0xFF, 0x66, 0x5B, 0xF4][..],
            (INITIAL_RBX & !0xFFFF) | 0xFF80,
        ),
        (
            "REX before 66 Group5 PUSH AX",
            &[0x48, 0x66, 0xFF, 0xF0, 0x66, 0x5B, 0xF4][..],
            (INITIAL_RBX & !0xFFFF) | (SOURCE & 0xFFFF),
        ),
        (
            "REX before 66 POP BX",
            &[0x66, 0x50, 0x48, 0x66, 0x8F, 0xC3, 0xF4][..],
            (INITIAL_RBX & !0xFFFF) | (SOURCE & 0xFFFF),
        ),
    ];

    for (name, code, expected_rbx) in cases {
        let expected = run_direct(code, SOURCE, INITIAL_RBX);
        let actual = run_native(code, SOURCE, INITIAL_RBX, name);

        assert_eq!(expected.rbx, expected_rbx, "{name}: direct result");
        assert_eq!(actual.rbx, expected.rbx, "{name}: RBX");
        assert_eq!(actual.rax, expected.rax, "{name}: RAX");
        assert_eq!(actual.rsp, expected.rsp, "{name}: RSP");
        assert_eq!(actual.rsp, STACK, "{name}: balanced stack");
        assert_eq!(actual.rcx, expected.rcx, "{name}: RCX");
        assert_eq!(actual.rflags, expected.rflags, "{name}: RFLAGS");
        assert_eq!(actual.rip, expected.rip, "{name}: RIP");
    }
}

#[test]
fn jit_group5_memory_push_reads_old_rsp_at_the_operand_width() {
    const SOURCE: u64 = 0x0123_4567_89AB_CDEF;
    const INITIAL_RBX: u64 = 0xA5A5_5A5A_1357_2468;
    for (name, code, width) in [
        (
            "PUSH qword [RSP]",
            &[0xFF, 0x34, 0x24, 0x5B, 0xF4][..],
            8usize,
        ),
        (
            "PUSH word [RSP]",
            &[0x66, 0xFF, 0x34, 0x24, 0x66, 0x5B, 0xF4][..],
            2,
        ),
        (
            "66 REX.W PUSH qword [RSP]",
            &[0x66, 0x48, 0xFF, 0x34, 0x24, 0x5B, 0xF4][..],
            8,
        ),
    ] {
        let run = |native: bool| {
            let (mut vcpu, memory) = make_vcpu_mem(code);
            memory
                .write_slice(&[0xCC; 8], GuestAddress(STACK - 8))
                .unwrap();
            memory
                .write_slice(&SOURCE.to_le_bytes(), GuestAddress(STACK))
                .unwrap();
            setup_stack_case(&mut vcpu, 0, INITIAL_RBX);
            if native {
                vcpu.set_jit_mem(true);
                assert!(
                    vcpu.jit_try_block()
                        .unwrap_or_else(|error| panic!("{name}: {error:?}")),
                    "{name}: native admission"
                );
            }
            run_interp(&mut vcpu);
            let regs = vcpu.get_regs().unwrap();
            let mut stack = [0u8; 8];
            memory
                .read_slice(&mut stack, GuestAddress(STACK - 8))
                .unwrap();
            (regs, stack)
        };

        let (expected, expected_stack) = run(false);
        let (actual, actual_stack) = run(true);
        let expected_rbx = if width == 2 {
            (INITIAL_RBX & !0xFFFF) | (SOURCE & 0xFFFF)
        } else {
            SOURCE
        };

        assert_eq!(expected.rbx, expected_rbx, "{name}: direct source width");
        assert_eq!(actual.rbx, expected.rbx, "{name}: source value");
        assert_eq!(actual.rsp, expected.rsp, "{name}: RSP");
        assert_eq!(actual.rsp, STACK, "{name}: balanced stack");
        assert_eq!(actual.rflags, expected.rflags, "{name}: RFLAGS");
        assert_eq!(actual_stack, expected_stack, "{name}: stack image");
        if width == 2 {
            assert_eq!(&actual_stack[6..], &(SOURCE as u16).to_le_bytes());
            assert_eq!(&actual_stack[..6], &[0xCC; 6]);
        } else {
            assert_eq!(actual_stack, SOURCE.to_le_bytes());
        }
    }
}

#[test]
fn jit_group5_rsp_push_fault_is_precise_and_noncommitting() {
    for (name, instruction, rsp) in [
        ("PUSH RSP", &[0xFF, 0xF4][..], MEM_SIZE + 4),
        ("PUSH SP", &[0x66, 0xFF, 0xF4][..], MEM_SIZE + 1),
        (
            "66 REX.W PUSH RSP",
            &[0x66, 0x48, 0xFF, 0xF4][..],
            MEM_SIZE + 4,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0xB9, 0x01, 0x00, 0x00, 0x00, 0xF4]);
        let mut jit = make_vcpu_code(&code);
        let mut before = jit.get_regs().unwrap();
        before.rax = 0x0123_4567_89AB_CDEF;
        before.rcx = 0xDEAD_BEEF_CAFE_BABE;
        before.rsp = rsp;
        before.rflags = 0x2 | 0x8D5;
        jit.set_regs(&before).unwrap();
        jit.set_jit_mem(true);

        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error:?}")),
            "{name}: compile before helper fault"
        );
        let after = jit.get_regs().unwrap();
        assert_eq!(after.rip, LOAD_ADDR, "{name}: restart PC");
        assert_eq!(after.rsp, before.rsp, "{name}: RSP commit");
        assert_eq!(after.rax, before.rax, "{name}: RAX");
        assert_eq!(after.rcx, before.rcx, "{name}: following MOV");
        assert_eq!(after.rflags, before.rflags, "{name}: RFLAGS");
    }
}

#[test]
fn jit_group5_memory_source_fault_precedes_stack_decrement_and_store() {
    let code = [0xFF, 0x34, 0x24, 0xB9, 0x01, 0x00, 0x00, 0x00, 0xF4];
    let (mut jit, memory) = make_vcpu_mem(&code);
    memory
        .write_slice(&[0xCC; 8], GuestAddress(MEM_SIZE - 8))
        .unwrap();
    let mut before = jit.get_regs().unwrap();
    before.rax = 0x0123_4567_89AB_CDEF;
    before.rcx = 0xDEAD_BEEF_CAFE_BABE;
    before.rsp = MEM_SIZE;
    before.rflags = 0x2 | 0x8D5;
    jit.set_regs(&before).unwrap();
    jit.set_jit_mem(true);

    assert!(
        jit.jit_try_block().expect("faulting memory PUSH JIT"),
        "memory PUSH must compile before its source-read fault"
    );
    let after = jit.get_regs().unwrap();
    let mut destination = [0u8; 8];
    memory
        .read_slice(&mut destination, GuestAddress(MEM_SIZE - 8))
        .unwrap();
    assert_eq!(after.rip, LOAD_ADDR, "restart at source-reading PUSH");
    assert_eq!(after.rsp, before.rsp, "source fault must not decrement RSP");
    assert_eq!(after.rax, before.rax, "source fault RAX");
    assert_eq!(after.rcx, before.rcx, "following MOV must not execute");
    assert_eq!(after.rflags, before.rflags, "source fault RFLAGS");
    assert_eq!(destination, [0xCC; 8], "destination store must not occur");
}
