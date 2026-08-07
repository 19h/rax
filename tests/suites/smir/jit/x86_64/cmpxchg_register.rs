//! End-to-end native-JIT coverage for register-destination `CMPXCHG`.

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

#[derive(Clone, Copy)]
enum CaseKind {
    DirectMatch,
    DirectMismatch,
    SelfMismatch32,
    StackMatch8,
    ApxMatch,
}

fn seed(vcpu: &mut X86_64Vcpu, kind: CaseKind) {
    let mut registers = vcpu.get_regs().unwrap();
    registers.rax = 0x0123_4567_89AB_CDEF;
    registers.rcx = 0x1357_9BDF_2468_ACE0;
    registers.rdx = 0x0F1E_2D3C_4B5A_6978;
    registers.rbx = 0xFEDC_BA98_7654_3210;
    registers.rbp = 0x9999_AAAA_BBBB_CC55;
    registers.r8 = 0x0101_0202_0303_0404;
    registers.r9 = 0x0505_0606_0707_0808;
    registers.r16 = 0x2121_2222_2323_2424;
    registers.r17 = 0x2525_2626_2727_2828;
    registers.rflags = 0x2 | 0x8D5;

    match kind {
        CaseKind::DirectMatch => registers.rax = registers.rdx,
        CaseKind::DirectMismatch => registers.rax = registers.rdx ^ 1,
        CaseKind::SelfMismatch32 => registers.rax = (registers.r8 as u32 ^ 1) as u64,
        CaseKind::StackMatch8 => {
            registers.rsp = (registers.rsp & !0xFF) | 0x7F;
            registers.rax = (registers.rax & !0xFF) | 0x7F;
        }
        CaseKind::ApxMatch => registers.rax = registers.r16,
    }
    vcpu.set_regs(&registers).unwrap();
}

#[test]
fn jit_register_cmpxchg_matches_direct_for_state_alias_and_apx_paths() {
    for (name, instruction, kind, apx) in [
        (
            "CMPXCHG RDX,RCX direct match",
            &[0x48, 0x0F, 0xB1, 0xCA][..],
            CaseKind::DirectMatch,
            false,
        ),
        (
            "CMPXCHG RDX,RCX direct mismatch",
            &[0x48, 0x0F, 0xB1, 0xCA][..],
            CaseKind::DirectMismatch,
            false,
        ),
        (
            "CMPXCHG R8D,R8D self mismatch",
            &[0x45, 0x0F, 0xB1, 0xC0][..],
            CaseKind::SelfMismatch32,
            false,
        ),
        (
            "CMPXCHG SPL,BPL state match",
            &[0x40, 0x0F, 0xB0, 0xEC][..],
            CaseKind::StackMatch8,
            false,
        ),
        (
            "CMPXCHG R16,R17 APX match",
            &[0xD5, 0xD8, 0xB1, 0xC8][..],
            CaseKind::ApxMatch,
            true,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4); // HLT is the exact native-to-interpreter frontier.

        let mut direct = make_vcpu_code(&code);
        direct.set_apx_enabled(apx);
        seed(&mut direct, kind);
        assert!(
            direct
                .step()
                .unwrap_or_else(|error| panic!("{name}: direct: {error}"))
                .is_none(),
            "{name}: direct instruction must fall through"
        );
        let expected = direct.get_regs().unwrap();

        let mut jit = make_vcpu_code(&code);
        jit.set_apx_enabled(apx);
        seed(&mut jit, kind);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: JIT: {error}")),
            "{name}: register CMPXCHG must enter the native tier:\n{}",
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();

        assert_eq!(gprs(&actual), gprs(&expected), "{name}: GPRs");
        assert_eq!(actual.rflags, expected.rflags, "{name}: RFLAGS");
        assert_eq!(actual.rip, expected.rip, "{name}: RIP");
        assert_eq!(actual.xmm, expected.xmm, "{name}: XMM state");
        assert_eq!(actual.mm, expected.mm, "{name}: MMX state");
    }
}
