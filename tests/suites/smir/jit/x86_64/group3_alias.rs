//! Native differential coverage for legacy Group 3 `/1` TEST aliases.

use super::*;

#[test]
fn jit_group3_slash1_memory_test_matches_direct_for_every_width() {
    const DATA: u64 = 0x20_0000;
    const SOURCE: u64 = 0xFEDC_BA98_7654_3210;

    for (name, instruction) in [
        ("TEST byte [rbx],imm8", &[0xF6, 0x0B, 0x81][..]),
        ("TEST word [rbx],imm16", &[0x66, 0xF7, 0x0B, 0x34, 0x80][..]),
        (
            "TEST dword [rbx],imm32",
            &[0xF7, 0x0B, 0x78, 0x56, 0x34, 0x80][..],
        ),
        (
            "TEST qword [rbx],sign-extended imm32",
            &[0x48, 0xF7, 0x0B, 0x78, 0x56, 0x34, 0x80][..],
        ),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
            memory.write_obj(SOURCE, GuestAddress(DATA)).unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = 0xA5A5_5A5A_A5A5_5A5A;
            regs.rbx = DATA;
            regs.rflags = 0x2 | 0x8D5;
            vcpu.set_regs(&regs).unwrap();
        };

        let (mut direct, direct_mem) = make_vcpu_mem(&code);
        setup(&mut direct, &direct_mem);
        assert!(
            direct
                .step()
                .unwrap_or_else(|error| panic!("{name}: {error}"))
                .is_none()
        );
        let expected = direct.get_regs().unwrap();

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error}")),
            "{name}: compatibility alias must enter the native tier"
        );
        let actual = jit.get_regs().unwrap();

        assert_eq!(actual.rax, expected.rax, "{name}: RAX");
        assert_eq!(actual.rbx, expected.rbx, "{name}: RBX");
        assert_eq!(actual.rsp, expected.rsp, "{name}: RSP");
        assert_eq!(actual.rflags, expected.rflags, "{name}: RFLAGS");
        assert_eq!(actual.rip, expected.rip, "{name}: RIP");
        assert_eq!(
            jit_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
            SOURCE,
            "{name}: TEST must not modify source memory"
        );
    }
}
