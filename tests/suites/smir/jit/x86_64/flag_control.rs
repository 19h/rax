//! Native-JIT differential and precise-guard tests for x86 flag controls.

use super::*;

const CF: u64 = 1 << 0;
const DF: u64 = 1 << 10;
const DATA: u64 = 0x20_0000;
const INITIAL_RDI: u64 = 0x0808_0808_0808_0808;

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn seed_registers(vcpu: &mut X86_64Vcpu, apx: bool, rflags: u64) -> [u64; 32] {
    vcpu.set_apx_enabled(apx);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rax = 0x0101_0101_0101_0101;
    regs.rcx = 0x0202_0202_0202_0202;
    regs.rdx = 0x0303_0303_0303_0303;
    regs.rbx = 0x0404_0404_0404_0404;
    regs.rsp = 0x11_0000;
    regs.rbp = 0x0606_0606_0606_0606;
    regs.rsi = 0x0707_0707_0707_0707;
    regs.rdi = INITIAL_RDI;
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
    gprs(&regs)
}

fn initial_flags(opcode: u8, discriminator: usize) -> u64 {
    let pattern = ((discriminator * 29 + usize::from(opcode)) & 0x7F) as u64;
    let mut flags = 0x2;
    for (index, bit) in [0, 2, 4, 6, 7, 11].into_iter().enumerate() {
        flags |= ((pattern >> index) & 1) << bit;
    }
    flags |= ((pattern >> 6) & 1) << 10;

    match opcode {
        0xF5 | 0xF8 => flags | CF,
        0xF9 => flags & !CF,
        0xFC => flags | DF,
        0xFD => flags & !DF,
        _ => unreachable!("not a flag-control opcode"),
    }
}

fn expected_flags(opcode: u8, initial: u64) -> u64 {
    match opcode {
        0xF5 => initial ^ CF,
        0xF8 => initial & !CF,
        0xF9 => initial | CF,
        0xFC => initial & !DF,
        0xFD => initial | DF,
        _ => unreachable!("not a flag-control opcode"),
    }
}

fn assert_direct_and_jit(name: &str, instruction: &[u8], apx: bool, initial: u64) {
    let mut code = instruction.to_vec();
    code.push(0xF4);
    let expected_rflags = expected_flags(*instruction.last().unwrap(), initial);
    let expected_rip = LOAD_ADDR + instruction.len() as u64;

    let mut direct = make_vcpu_code(&code);
    let initial_gprs = seed_registers(&mut direct, apx, initial);
    assert!(
        direct
            .step()
            .unwrap_or_else(|error| panic!("{name} direct: {error:?}"))
            .is_none(),
        "{name}: direct execution exited"
    );
    let direct_regs = direct.get_regs().unwrap();
    assert_eq!(gprs(&direct_regs), initial_gprs, "{name}: direct GPRs");
    assert_eq!(direct_regs.rflags, expected_rflags, "{name}: direct RFLAGS");
    assert_eq!(direct_regs.rip, expected_rip, "{name}: direct RIP");

    let mut jit = make_vcpu_code(&code);
    seed_registers(&mut jit, apx, initial);
    jit.set_jit_call(false);
    jit.set_jit_mem(false);
    assert!(
        jit.jit_try_block()
            .unwrap_or_else(|error| panic!("{name} JIT: {error:?}")),
        "{name}: flag control must enter the native tier:\n{}",
        jit.jit_dump_region(LOAD_ADDR)
    );
    let jit_regs = jit.get_regs().unwrap();
    assert_eq!(gprs(&jit_regs), initial_gprs, "{name}: JIT GPRs");
    assert_eq!(jit_regs.rflags, expected_rflags, "{name}: JIT RFLAGS");
    assert_eq!(jit_regs.rip, expected_rip, "{name}: JIT RIP");
}

#[test]
fn jit_flag_controls_all_scanned_legacy_prefixes_match_direct_execution() {
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

    for opcode in [0xF5, 0xF8, 0xF9, 0xFC, 0xFD] {
        for (prefix_index, prefix) in PREFIXES.iter().enumerate() {
            let mut instruction = prefix.to_vec();
            instruction.push(opcode);
            let name = format!("opcode={opcode:#04X}, prefix={prefix:02X?}");
            assert_direct_and_jit(
                &name,
                &instruction,
                false,
                initial_flags(opcode, prefix_index),
            );
        }
    }
}

#[test]
fn jit_rex2_flag_controls_match_direct_at_payload_bounds() {
    for opcode in [0xF5, 0xF8, 0xF9, 0xFC, 0xFD] {
        for (index, payload) in [0x00, 0x7F].into_iter().enumerate() {
            let instruction = [0xD5, payload, opcode];
            let name = format!("REX2 opcode={opcode:#04X}, payload={payload:#04X}");
            assert_direct_and_jit(&name, &instruction, true, initial_flags(opcode, index));
        }
    }
}

#[test]
fn jit_std_survives_a_rust_memory_helper_boundary_and_handoff() {
    let code = [0xFD, 0x8B, 0x03, 0xF4]; // STD; MOV EAX,[RBX]; HLT
    let value = 0x89AB_CDEFu32;
    let initial = initial_flags(0xFD, 0);

    let (mut direct, direct_memory) = make_vcpu_mem(&code);
    direct_memory
        .write_slice(&value.to_le_bytes(), GuestAddress(DATA))
        .unwrap();
    seed_registers(&mut direct, false, initial);
    let mut regs = direct.get_regs().unwrap();
    regs.rbx = DATA;
    direct.set_regs(&regs).unwrap();
    assert!(direct.step().expect("direct STD").is_none());
    assert!(direct.step().expect("direct MOV load").is_none());
    let expected = direct.get_regs().unwrap();

    let (mut jit, jit_memory) = make_vcpu_mem(&code);
    jit_memory
        .write_slice(&value.to_le_bytes(), GuestAddress(DATA))
        .unwrap();
    seed_registers(&mut jit, false, initial);
    let mut regs = jit.get_regs().unwrap();
    regs.rbx = DATA;
    jit.set_regs(&regs).unwrap();
    jit.set_jit_call(false);
    jit.set_jit_mem(true);
    assert!(jit.jit_try_block().expect("STD plus memory-helper JIT"));
    let actual = jit.get_regs().unwrap();

    assert_eq!(gprs(&actual), gprs(&expected));
    assert_eq!(actual.rflags, expected.rflags);
    assert_ne!(actual.rflags & DF, 0, "STD must survive helper handoff");
    assert_eq!(actual.rip, expected.rip);
}

#[test]
fn jit_rex2_flag_control_guard_is_dynamic_precise_and_noncommitting() {
    for (index, opcode) in [0xF5, 0xF8, 0xF9, 0xFC, 0xFD].into_iter().enumerate() {
        let initial = initial_flags(opcode, index);
        let mut code = vec![0xBE, 0x78, 0x56, 0x34, 0x12];
        code.extend_from_slice(&[0xD5, 0x00, opcode]);
        code.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00, 0xF4]);
        let mut vcpu = make_vcpu_code(&code);
        seed_registers(&mut vcpu, true, initial);
        vcpu.set_jit_call(false);
        vcpu.set_jit_mem(false);

        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("opcode={opcode:#04X}: {error:?}")),
            "opcode={opcode:#04X}: enabled APX region"
        );
        let enabled = vcpu.get_regs().unwrap();
        assert_eq!(enabled.rsi, 0x1234_5678);
        assert_eq!(enabled.rdi, 1);
        assert_eq!(enabled.rflags, expected_flags(opcode, initial));

        let mut expected_gprs = seed_registers(&mut vcpu, false, initial);
        expected_gprs[6] = 0x1234_5678;
        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("opcode={opcode:#04X}: {error:?}")),
            "opcode={opcode:#04X}: cached disabled-APX region"
        );
        let guarded = vcpu.get_regs().unwrap();
        assert_eq!(gprs(&guarded), expected_gprs, "opcode={opcode:#04X}: GPRs");
        assert_eq!(guarded.rflags, initial, "opcode={opcode:#04X}: RFLAGS");
        assert_eq!(
            guarded.rip,
            LOAD_ADDR + 5,
            "opcode={opcode:#04X}: exact guard frontier"
        );

        let before_step = guarded;
        let error = format!(
            "{:#}",
            vcpu.step()
                .expect_err("disabled REX2 flag control must inject #UD")
        );
        assert!(error.contains("IDT entry 6 not present"), "{error}");
        let after_step = vcpu.get_regs().unwrap();
        assert_eq!(gprs(&after_step), gprs(&before_step));
        assert_eq!(after_step.rflags, before_step.rflags);
        assert_eq!(after_step.rip, before_step.rip);
    }
}
