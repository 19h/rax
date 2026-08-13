//! End-to-end native-JIT coverage for register `XCHG`, including the complete
//! low-byte namespace and APX EGPRs.

use super::*;

const PRESERVED_RFLAGS: u64 =
    0x2 | 0x08D5 | (1 << 9) | (1 << 10) | (3 << 12) | (1 << 18) | (1 << 21);
const LEGACY_PREFIXES: [&[u8]; 7] = [&[], &[0x66], &[0xF2], &[0xF3], &[0x67], &[0x64], &[0x65]];
const SCANNER_REX_PREFIXES: [&[u8]; 7] = [
    &[0x48],
    &[0x44],
    &[0x41],
    &[0x4D],
    &[0x66, 0x48],
    &[0xF2, 0x48],
    &[0xF3, 0x48],
];

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

fn set_gprs(registers: &mut Registers, values: [u64; 32]) {
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
    ] = values;
}

fn seed(vcpu: &mut X86_64Vcpu, apx: bool) -> Registers {
    let mut registers = vcpu.get_regs().unwrap();
    let mut values = [0u64; 32];
    for (index, value) in values.iter_mut().enumerate() {
        *value = 0xA1A2_0000_0000_0011u64
            .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0125));
    }
    values[4] = 0x11_0044;
    set_gprs(&mut registers, values);
    registers.rip = LOAD_ADDR;
    registers.rflags = PRESERVED_RFLAGS;
    vcpu.set_regs(&registers).unwrap();
    vcpu.set_apx_enabled(apx);
    registers
}

fn expected_byte_xchg(
    initial: &Registers,
    reg1: usize,
    reg2: usize,
    instruction_len: usize,
) -> Registers {
    let mut expected = initial.clone();
    let mut values = gprs(initial);
    let old1 = values[reg1];
    let old2 = values[reg2];
    values[reg1] = (old1 & !0xFF) | (old2 & 0xFF);
    values[reg2] = (old2 & !0xFF) | (old1 & 0xFF);
    set_gprs(&mut expected, values);
    expected.rip = LOAD_ADDR + instruction_len as u64;
    expected
}

fn assert_byte_direct_manual_and_jit(instruction: &[u8], apx: bool, reg1: usize, reg2: usize) {
    let mut code = instruction.to_vec();
    code.push(0xF4);

    let mut direct = make_vcpu_code(&code);
    let initial = seed(&mut direct, apx);
    let manual = expected_byte_xchg(&initial, reg1, reg2, instruction.len());
    assert!(
        direct
            .step()
            .unwrap_or_else(|error| panic!("direct {instruction:02X?}: {error}"))
            .is_none(),
        "direct {instruction:02X?} must fall through"
    );
    let direct_result = direct.get_regs().unwrap();
    assert_eq!(
        gprs(&direct_result),
        gprs(&manual),
        "direct GPRs {instruction:02X?}"
    );
    assert_eq!(
        direct_result.rflags, manual.rflags,
        "direct RFLAGS {instruction:02X?}"
    );
    assert_eq!(
        direct_result.rip, manual.rip,
        "direct RIP {instruction:02X?}"
    );

    let mut jit = make_vcpu_code(&code);
    seed(&mut jit, apx);
    jit.set_jit_call(false);
    jit.set_jit_mem(false);
    assert!(
        jit.jit_try_block()
            .unwrap_or_else(|error| panic!("JIT {instruction:02X?}: {error}")),
        "byte register XCHG must enter the native tier {instruction:02X?}:\n{}",
        jit.jit_dump_region(LOAD_ADDR)
    );
    let actual = jit.get_regs().unwrap();
    assert_eq!(gprs(&actual), gprs(&manual), "JIT GPRs {instruction:02X?}");
    assert_eq!(
        actual.rflags, manual.rflags,
        "JIT RFLAGS {instruction:02X?}"
    );
    assert_eq!(actual.rip, manual.rip, "JIT RIP {instruction:02X?}");
    assert_eq!(
        actual.xmm, direct_result.xmm,
        "JIT XMM state {instruction:02X?}"
    );
    assert_eq!(
        actual.mm, direct_result.mm,
        "JIT MMX state {instruction:02X?}"
    );
}

#[test]
fn jit_all_560_legacy_scanner_byte_xchg_gaps_match_a_manual_oracle() {
    let mut cases = 0usize;
    for prefix in LEGACY_PREFIXES {
        for reg in 0u8..4 {
            for rm in 0u8..4 {
                let mut instruction = prefix.to_vec();
                instruction.extend_from_slice(&[0x86, 0xC0 | (reg << 3) | rm]);
                assert_byte_direct_manual_and_jit(
                    &instruction,
                    false,
                    usize::from(rm),
                    usize::from(reg),
                );
                cases += 1;
            }
        }
    }

    for prefix in SCANNER_REX_PREFIXES {
        let rex = *prefix.last().expect("REX-bearing scanner prefix");
        let reg_ext = (rex & 0x04) << 1;
        let rm_ext = (rex & 0x01) << 3;
        for modrm in 0xC0_u8..=0xFF {
            let mut instruction = prefix.to_vec();
            instruction.extend_from_slice(&[0x86, modrm]);
            assert_byte_direct_manual_and_jit(
                &instruction,
                false,
                usize::from((modrm & 7) | rm_ext),
                usize::from(((modrm >> 3) & 7) | reg_ext),
            );
            cases += 1;
        }
    }

    assert_eq!(cases, 7 * 16 + 7 * 64);
    assert_eq!(cases, 560);
}

#[test]
fn jit_rex2_byte_xchg_matches_manual_oracle_for_all_1024_gpr_pairs() {
    let mut cases = 0usize;
    for reg1 in 0u8..32 {
        for reg2 in 0u8..32 {
            let payload = u8::from(reg2 & 16 != 0) * 0x40
                | u8::from(reg1 & 16 != 0) * 0x10
                | u8::from(reg2 & 8 != 0) * 0x04
                | u8::from(reg1 & 8 != 0);
            let modrm = 0xC0 | ((reg2 & 7) << 3) | (reg1 & 7);
            let instruction = [0xD5, payload, 0x86, modrm];
            assert_byte_direct_manual_and_jit(
                &instruction,
                true,
                usize::from(reg1),
                usize::from(reg2),
            );
            cases += 1;
        }
    }
    assert_eq!(cases, 32 * 32);
}

#[test]
fn jit_rex2_byte_xchg_guard_is_dynamic_precise_and_noncommitting() {
    // XCHG BPL,R16B exercises both the REX2 low-byte namespace and the x86
    // host's state-backed RBP/EGPR lowering.
    let instruction = [0xD5, 0x40, 0x86, 0xC5];
    let mut code = instruction.to_vec();
    code.push(0xF4);
    let mut vcpu = make_vcpu_code(&code);

    let enabled_initial = seed(&mut vcpu, true);
    vcpu.set_jit_call(false);
    vcpu.set_jit_mem(false);
    assert!(vcpu.jit_try_block().unwrap(), "enabled APX native region");
    let enabled = vcpu.get_regs().unwrap();
    let enabled_expected = expected_byte_xchg(&enabled_initial, 5, 16, instruction.len());
    assert_eq!(gprs(&enabled), gprs(&enabled_expected));
    assert_eq!(enabled.rflags, enabled_expected.rflags);
    assert_eq!(enabled.rip, enabled_expected.rip);

    let disabled_initial = seed(&mut vcpu, false);
    assert!(vcpu.jit_try_block().unwrap(), "cached disabled-APX region");
    let guarded = vcpu.get_regs().unwrap();
    assert_eq!(gprs(&guarded), gprs(&disabled_initial));
    assert_eq!(guarded.rflags, disabled_initial.rflags);
    assert_eq!(guarded.rip, disabled_initial.rip);

    let error = format!(
        "{:#}",
        vcpu.step()
            .expect_err("disabled REX2 byte XCHG must inject #UD")
    );
    assert!(error.contains("IDT entry 6 not present"), "{error}");
    let after_step = vcpu.get_regs().unwrap();
    assert_eq!(gprs(&after_step), gprs(&guarded));
    assert_eq!(after_step.rflags, guarded.rflags);
    assert_eq!(after_step.rip, guarded.rip);
}

/// Register XCHG is flag-neutral. Word exchanges preserve both upper register
/// portions, dword exchanges zero-extend, and full-width exchanges swap all bits.
#[test]
fn jit_xchg_preserves_width_semantics_and_flags() {
    const STATUS_MASK: u64 = 0x08D5;
    for (name, instruction, rax, r8, expected_rax, expected_r8) in [
        (
            "xchg ax,r8w",
            &[0x66, 0x44, 0x87, 0xC0][..],
            0x1122_3344_5566_1234,
            0xAABB_CCDD_EEFF_7788,
            0x1122_3344_5566_7788,
            0xAABB_CCDD_EEFF_1234,
        ),
        (
            "xchg eax,eax",
            &[0x87, 0xC0][..],
            0xAABB_CCDD_1234_5678,
            0x0123_4567_89AB_CDEF,
            0x1234_5678,
            0x0123_4567_89AB_CDEF,
        ),
        (
            "xchg rax,r8",
            &[0x4C, 0x87, 0xC0][..],
            0x1122_3344_5566_7788,
            0xAABB_CCDD_EEFF_1234,
            0xAABB_CCDD_EEFF_1234,
            0x1122_3344_5566_7788,
        ),
    ] {
        let mut code = vec![0xFF, 0xC9, 0x75, 0xFC, 0x45, 0x31, 0xC9];
        code.extend_from_slice(instruction);
        code.push(0xF4);

        let setup = |vcpu: &mut X86_64Vcpu| {
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = rax;
            regs.rcx = 200;
            regs.r8 = r8;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp);
        run_interp(&mut interp);

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(jit.jit_try_block().unwrap(), "{name} native tier");
        run_interp(&mut jit);

        let expected = interp.get_regs().unwrap();
        let after = jit.get_regs().unwrap();
        assert_eq!(after.rax, expected_rax, "{name}: architectural RAX");
        assert_eq!(after.r8, expected_r8, "{name}: architectural R8");
        assert_eq!(after.rax, expected.rax, "{name}: RAX vs interpreter");
        assert_eq!(after.r8, expected.r8, "{name}: R8 vs interpreter");
        assert_eq!(after.rcx & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(after.rflags & STATUS_MASK, 0x44, "{name}: status flags");
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected.rflags & STATUS_MASK,
            "{name}"
        );
    }
}

#[test]
fn jit_state_backed_gpr_xchg_matches_interpreter_without_memory_helpers() {
    for (name, instruction, apx) in [
        (
            "REX2 XCHG CX,R16W",
            &[0x66, 0xD5, 0x10, 0x87, 0xC8][..],
            true,
        ),
        ("REX2 XCHG EBP,R17D", &[0xD5, 0x10, 0x87, 0xE9][..], true),
        ("REX2 XCHG RSP,R31", &[0xD5, 0x19, 0x87, 0xE7][..], true),
        ("XCHG SP,BP", &[0x66, 0x87, 0xE5][..], false),
        ("REX2 XCHG R16D,R16D", &[0xD5, 0x50, 0x87, 0xC0][..], true),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let mut direct = make_vcpu_code(&code);
        seed(&mut direct, apx);
        assert!(direct.step().unwrap().is_none(), "{name}: direct");
        let expected = direct.get_regs().unwrap();

        let mut jit = make_vcpu_code(&code);
        seed(&mut jit, apx);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error}")),
            "{name} must enter the register-only native tier:\n{}",
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{name}: GPR file");
        assert_eq!(actual.rflags, expected.rflags, "{name}: RFLAGS");
        assert_eq!(actual.rip, expected.rip, "{name}: RIP");
    }
}
