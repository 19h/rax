//! End-to-end native-JIT differential coverage for LAHF and SAHF.

use super::*;

const STATUS_MASK: u64 = 0xD5;
const OF: u64 = 1 << 11;
const DF: u64 = 1 << 10;
const IF: u64 = 1 << 9;
const IOPL: u64 = 3 << 12;
const AC: u64 = 1 << 18;
const ID: u64 = 1 << 21;
const PRESERVED_FLAGS: u64 = 0x2 | OF | DF | IF | IOPL | AC | ID;

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn seed(vcpu: &mut X86_64Vcpu, apx: bool, rax: u64, rflags: u64) -> Registers {
    vcpu.set_apx_enabled(apx);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rax = rax;
    regs.rcx = 0x0202_0202_0202_0202;
    regs.rdx = 0x0303_0303_0303_0303;
    regs.rbx = 0x0404_0404_0404_0404;
    regs.rsp = 0x11_0000;
    regs.rbp = 0x0606_0606_0606_0606;
    regs.rsi = 0x0707_0707_0707_0707;
    regs.rdi = 0x0808_0808_0808_0808;
    regs.r8 = 0x0909_0909_0909_0909;
    regs.r9 = 0x0A0A_0A0A_0A0A_0A0A;
    regs.r10 = 0x0B0B_0B0B_0B0B_0B0B;
    regs.r11 = 0x0C0C_0C0C_0C0C_0C0C;
    regs.r12 = 0x0D0D_0D0D_0D0D_0D0D;
    regs.r13 = 0x0E0E_0E0E_0E0E_0E0E;
    regs.r14 = 0x0F0F_0F0F_0F0F_0F0F;
    regs.r15 = 0x1010_1010_1010_1010;
    regs.r16 = 0x1111_1111_1111_1111;
    regs.r17 = 0x1212_1212_1212_1212;
    regs.r18 = 0x1313_1313_1313_1313;
    regs.r19 = 0x1414_1414_1414_1414;
    regs.r20 = 0x1515_1515_1515_1515;
    regs.r21 = 0x1616_1616_1616_1616;
    regs.r22 = 0x1717_1717_1717_1717;
    regs.r23 = 0x1818_1818_1818_1818;
    regs.r24 = 0x1919_1919_1919_1919;
    regs.r25 = 0x1A1A_1A1A_1A1A_1A1A;
    regs.r26 = 0x1B1B_1B1B_1B1B_1B1B;
    regs.r27 = 0x1C1C_1C1C_1C1C_1C1C;
    regs.r28 = 0x1D1D_1D1D_1D1D_1D1D;
    regs.r29 = 0x1E1E_1E1E_1E1E_1E1E;
    regs.r30 = 0x1F1F_1F1F_1F1F_1F1F;
    regs.r31 = 0x2020_2020_2020_2020;
    regs.rip = LOAD_ADDR;
    regs.rflags = rflags;
    vcpu.set_regs(&regs).unwrap();
    regs
}

fn expected(opcode: u8, initial: &Registers, instruction_len: usize) -> Registers {
    let mut expected = initial.clone();
    match opcode {
        0x9E => {
            let ah = (initial.rax >> 8) & 0xFF;
            expected.rflags = (initial.rflags & !STATUS_MASK) | (ah & STATUS_MASK) | 0x2;
        }
        0x9F => {
            let ah = ((initial.rflags & STATUS_MASK) | 0x2) << 8;
            expected.rax = (initial.rax & !0xFF00) | ah;
        }
        _ => unreachable!("not LAHF/SAHF"),
    }
    expected.rip = LOAD_ADDR + instruction_len as u64;
    expected
}

fn assert_manual_result(
    name: &str,
    opcode: u8,
    initial: &Registers,
    actual: &Registers,
    len: usize,
) {
    let manual = expected(opcode, initial, len);
    assert_eq!(gprs(actual), gprs(&manual), "{name}: manual GPRs");
    assert_eq!(actual.rflags, manual.rflags, "{name}: manual RFLAGS");
    assert_eq!(actual.rip, manual.rip, "{name}: manual RIP");
}

fn assert_direct_and_jit(name: &str, instruction: &[u8], apx: bool, rax: u64, rflags: u64) {
    let opcode = *instruction.last().expect("instruction opcode");
    let mut code = instruction.to_vec();
    code.push(0xF4);

    let mut direct = make_vcpu_code(&code);
    let initial = seed(&mut direct, apx, rax, rflags);
    assert!(
        direct
            .step()
            .unwrap_or_else(|error| panic!("{name} direct: {error:?}"))
            .is_none(),
        "{name}: direct execution exited"
    );
    let direct_regs = direct.get_regs().unwrap();
    assert_manual_result(name, opcode, &initial, &direct_regs, instruction.len());

    let mut jit = make_vcpu_code(&code);
    seed(&mut jit, apx, rax, rflags);
    jit.set_jit_call(false);
    jit.set_jit_mem(false);
    assert!(
        jit.jit_try_block()
            .unwrap_or_else(|error| panic!("{name} JIT: {error:?}")),
        "{name}: LAHF/SAHF must enter the native tier:\n{}",
        jit.jit_dump_region(LOAD_ADDR)
    );
    let jit_regs = jit.get_regs().unwrap();
    assert_eq!(gprs(&jit_regs), gprs(&direct_regs), "{name}: JIT GPRs");
    assert_eq!(jit_regs.rflags, direct_regs.rflags, "{name}: JIT RFLAGS");
    assert_eq!(jit_regs.rip, direct_regs.rip, "{name}: JIT RIP");
    assert_eq!(jit_regs.xmm, direct_regs.xmm, "{name}: JIT XMM state");
    assert_eq!(jit_regs.mm, direct_regs.mm, "{name}: JIT MMX state");
}

#[test]
fn jit_lahf_sahf_all_scanned_legacy_prefixes_match_direct_and_manual_semantics() {
    const PREFIXES: &[&[u8]] = &[
        &[],
        &[0x66],
        &[0xF2],
        &[0xF3],
        &[0x67],
        &[0x64],
        &[0x65],
        &[0x48],
        &[0x44],
        &[0x41],
        &[0x4D],
        &[0x66, 0x48],
        &[0xF2, 0x48],
        &[0xF3, 0x48],
    ];

    for opcode in [0x9E, 0x9F] {
        for (index, prefix) in PREFIXES.iter().enumerate() {
            let mut instruction = prefix.to_vec();
            instruction.push(opcode);
            let rax = 0x0123_4567_89AB_CDEFu64.rotate_left(index as u32 * 3);
            let status = ((index * 13 + usize::from(opcode)) as u64) & STATUS_MASK;
            assert_direct_and_jit(
                &format!("opcode={opcode:#04X}, prefix={prefix:02X?}"),
                &instruction,
                false,
                rax,
                PRESERVED_FLAGS | status,
            );
        }
    }
}

#[test]
fn jit_lahf_exhausts_five_status_inputs_and_sahf_exhausts_every_ah_value() {
    let rax = 0xA1B2_C3D4_E5F6_0718;
    let status_bits = [1u64 << 0, 1 << 2, 1 << 4, 1 << 6, 1 << 7];
    for pattern in 0u64..32 {
        let status = status_bits
            .into_iter()
            .enumerate()
            .fold(0, |value, (index, bit)| {
                value | (((pattern >> index) & 1) * bit)
            });
        assert_direct_and_jit(
            &format!("LAHF status pattern={pattern:#04X}"),
            &[0x9F],
            false,
            rax,
            PRESERVED_FLAGS | status,
        );
    }

    for ah in 0u64..=0xFF {
        let initial_rax = (rax & !0xFF00) | (ah << 8);
        assert_direct_and_jit(
            &format!("SAHF AH={ah:#04X}"),
            &[0x9E],
            false,
            initial_rax,
            PRESERVED_FLAGS | STATUS_MASK,
        );
    }
}

#[test]
fn jit_rex2_lahf_sahf_payload_bounds_match_direct_execution() {
    for opcode in [0x9E, 0x9F] {
        for payload in [0x00, 0x7F] {
            let instruction = [0xD5, payload, opcode];
            assert_direct_and_jit(
                &format!("REX2 opcode={opcode:#04X}, payload={payload:#04X}"),
                &instruction,
                true,
                0x8877_6655_4433_2211,
                PRESERVED_FLAGS | STATUS_MASK,
            );
        }
    }
}

#[test]
fn jit_rex2_lahf_sahf_guard_is_dynamic_precise_and_noncommitting() {
    for opcode in [0x9E, 0x9F] {
        let code = [0xD5, 0x00, opcode, 0xF4];
        let initial_rax = 0x0123_4567_89AB_CDEF;
        let initial_flags = PRESERVED_FLAGS | STATUS_MASK;
        let mut vcpu = make_vcpu_code(&code);
        let enabled_initial = seed(&mut vcpu, true, initial_rax, initial_flags);
        vcpu.set_jit_call(false);
        vcpu.set_jit_mem(false);
        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("opcode={opcode:#04X}: {error:?}")),
            "opcode={opcode:#04X}: enabled APX region"
        );
        let enabled = vcpu.get_regs().unwrap();
        assert_manual_result("enabled REX2", opcode, &enabled_initial, &enabled, 3);

        let disabled_initial = seed(&mut vcpu, false, initial_rax, initial_flags);
        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("opcode={opcode:#04X}: {error:?}")),
            "opcode={opcode:#04X}: cached disabled-APX region"
        );
        let guarded = vcpu.get_regs().unwrap();
        assert_eq!(gprs(&guarded), gprs(&disabled_initial));
        assert_eq!(guarded.rflags, disabled_initial.rflags);
        assert_eq!(guarded.rip, disabled_initial.rip);

        let error = format!(
            "{:#}",
            vcpu.step()
                .expect_err("disabled REX2 LAHF/SAHF must inject #UD")
        );
        assert!(error.contains("IDT entry 6 not present"), "{error}");
        let after_step = vcpu.get_regs().unwrap();
        assert_eq!(gprs(&after_step), gprs(&guarded));
        assert_eq!(after_step.rflags, guarded.rflags);
        assert_eq!(after_step.rip, guarded.rip);
    }
}
