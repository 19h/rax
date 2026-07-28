//! Full x86 VCPU-state marshalling coverage for AMD TBM on an AArch64 host.

use super::*;

const INITIAL_FLAGS: u64 = 0xCD7;
const CF: u64 = 1 << 0;
const ZF: u64 = 1 << 6;
const SF: u64 = 1 << 7;
const OF: u64 = 1 << 11;

fn xop_p1(destination: u8, width64: bool) -> u8 {
    (u8::from(width64) << 7) | (((!destination) & 0x0F) << 3)
}

fn map9_register(opcode: u8, extension: u8, width64: bool) -> Vec<u8> {
    vec![
        0x8F,
        0xE9,
        xop_p1(4, width64), // destination RSP
        opcode,
        0xC0 | (extension << 3) | 5, // source RBP
    ]
}

fn immediate_bextr_register(width64: bool, control: u32) -> Vec<u8> {
    let mut bytes = vec![
        0x8F,
        0xEA,
        xop_p1(0, width64), // BEXTR reserves decoded XOP.vvvv=0
        0x10,
        0xE5, // destination RSP, source RBP
    ];
    bytes.extend_from_slice(&control.to_le_bytes());
    bytes
}

fn vex_bextr_register(width64: bool) -> Vec<u8> {
    vec![
        0xC4,
        0xE2,
        if width64 { 0xF8 } else { 0x78 }, // control RAX, L=0, pp=00
        0xF7,
        0xE5, // destination RSP, source RBP
    ]
}

fn finite_region(mut instruction: Vec<u8>, result: u64) -> Vec<u8> {
    assert!(instruction.len() <= 125);
    let displacement = -((instruction.len() + 2) as i8);
    instruction.extend_from_slice(&[
        if result == 0 { 0x75 } else { 0x74 }, // false JNZ/JZ
        displacement as u8,
        0xF4,
    ]);
    instruction
}

fn tbm_reference(opcode: u8, extension: u8, source: u64, width64: bool) -> (u64, bool) {
    let mask = if width64 {
        u64::MAX
    } else {
        u64::from(u32::MAX)
    };
    let source = source & mask;
    let incremented = source.wrapping_add(1) & mask;
    let decremented = source.wrapping_sub(1) & mask;
    let result = match (opcode, extension) {
        (0x01, 1) => source & incremented,
        (0x01, 2) => source | decremented,
        (0x01, 3) => source | incremented,
        (0x01, 4) => !source & decremented,
        (0x01, 5) => !source & incremented,
        (0x01, 6) => !source | decremented,
        (0x01, 7) => !source | incremented,
        (0x02, 1) => source ^ incremented,
        (0x02, 6) => source | !incremented,
        _ => unreachable!("validated scalar TBM encoding"),
    } & mask;
    let carry = if matches!((opcode, extension), (0x01, 2 | 4 | 6)) {
        source == 0
    } else {
        source == mask
    };
    (result, carry)
}

fn bextr_reference(source: u64, control: u32, width64: bool) -> u64 {
    let bits = if width64 { 64 } else { 32 };
    let mask = if width64 {
        u64::MAX
    } else {
        u64::from(u32::MAX)
    };
    let start = control & 0xFF;
    let length = (control >> 8) & 0xFF;
    if start >= bits || length == 0 {
        return 0;
    }
    let shifted = (source & mask) >> start;
    let field_bits = length.min(bits - start);
    if field_bits == 64 {
        shifted
    } else {
        shifted & ((1_u64 << field_bits) - 1)
    }
}

fn expected_tbm_flags(result: u64, carry: bool, width64: bool) -> u64 {
    let sign = if width64 { 1_u64 << 63 } else { 1_u64 << 31 };
    (INITIAL_FLAGS & !(CF | ZF | SF | OF))
        | (u64::from(carry) * CF)
        | (u64::from(result == 0) * ZF)
        | (u64::from(result & sign != 0) * SF)
}

fn run_native_case(label: &str, code: &[u8], source: u64) -> Registers {
    let setup = |vcpu: &mut X86_64Vcpu| {
        vcpu.set_tbm_enabled(true);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0804;
        regs.rbp = source;
        regs.rflags = INITIAL_FLAGS;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .unwrap_or_else(|error| panic!("{label}: JIT attempt: {error:?}")),
        "{label}: block must enter the AArch64 native tier"
    );
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, label);
    actual
}

#[test]
fn x86_tbm_all_scalar_forms_cross_the_complete_aarch64_vcpu_bridge() {
    let operations = [
        ("BLCFILL", 0x01, 1),
        ("BLSFILL", 0x01, 2),
        ("BLCS", 0x01, 3),
        ("TZMSK", 0x01, 4),
        ("BLCIC", 0x01, 5),
        ("BLSIC", 0x01, 6),
        ("T1MSKC", 0x01, 7),
        ("BLCMSK", 0x02, 1),
        ("BLCI", 0x02, 6),
    ];

    for (mnemonic, opcode, extension) in operations {
        for width64 in [false, true] {
            let mask = if width64 {
                u64::MAX
            } else {
                u64::from(u32::MAX)
            };
            for source in [0, 0xFEDC_BA98_7654_3210, mask] {
                let (result, carry) = tbm_reference(opcode, extension, source, width64);
                let code = finite_region(map9_register(opcode, extension, width64), result);
                let width = if width64 { 64 } else { 32 };
                let label = format!("{mnemonic} RSP,RBP W{width} source={source:#018x}");
                let actual = run_native_case(&label, &code, source);

                assert_eq!(actual.rsp, result, "{label}: destination");
                assert_eq!(actual.rbp, source, "{label}: source");
                assert_eq!(
                    actual.rflags,
                    expected_tbm_flags(result, carry, width64),
                    "{label}: defined flags and deterministic PF/AF preservation"
                );
            }
        }
    }
}

#[test]
fn x86_tbm_immediate_bextr_crosses_complete_aarch64_vcpu_bridge() {
    let source = 0xFEDC_BA98_7654_3210;
    for width64 in [false, true] {
        for control in [0, 0x0804, 0x0840, 0x4004] {
            let result = bextr_reference(source, control, width64);
            let code = finite_region(immediate_bextr_register(width64, control), result);
            let width = if width64 { 64 } else { 32 };
            let label = format!("BEXTR RSP,RBP,{control:#06x} W{width}");
            let actual = run_native_case(&label, &code, source);
            let expected_flags = (INITIAL_FLAGS & !(CF | ZF | OF)) | (u64::from(result == 0) * ZF);

            assert_eq!(actual.rsp, result, "{label}: destination");
            assert_eq!(actual.rbp, source, "{label}: source");
            assert_eq!(
                actual.rflags, expected_flags,
                "{label}: CF/ZF/OF and preserved SF/PF/AF"
            );
        }
    }
}

#[test]
fn x86_bmi_bextr_register_control_uses_aarch64_rsp_rbp_identity_slots() {
    let source = 0xFEDC_BA98_7654_3210;
    let control = 0x0804;
    for width64 in [false, true] {
        let result = bextr_reference(source, control, width64);
        let code = finite_region(vex_bextr_register(width64), result);
        let width = if width64 { 64 } else { 32 };
        let label = format!("BMI1 BEXTR RSP,RBP,RAX W{width}");
        let actual = run_native_case(&label, &code, source);
        let expected_flags = (INITIAL_FLAGS & !(CF | ZF | OF)) | (u64::from(result == 0) * ZF);

        assert_eq!(actual.rax, u64::from(control), "{label}: control");
        assert_eq!(actual.rsp, result, "{label}: destination");
        assert_eq!(actual.rbp, source, "{label}: source");
        assert_eq!(actual.rflags, expected_flags, "{label}: flags");
    }
}

#[test]
fn x86_tbm_aarch64_guard_marshals_feature_and_mode_without_committing() {
    let instruction = map9_register(0x02, 6, true);
    let mut code = instruction.clone();
    code.push(0xF4);
    let source = 0x0123_4567_89AB_CDEF;

    let mut unavailable = make_vcpu_code(&code);
    unavailable.set_tbm_enabled(false);
    let mut regs = unavailable.get_regs().unwrap();
    regs.rbp = source;
    regs.rflags = INITIAL_FLAGS;
    unavailable.set_regs(&regs).unwrap();
    let before = unavailable.get_regs().unwrap();

    assert!(
        unavailable
            .jit_try_block()
            .expect("disabled-TBM AArch64 JIT attempt"),
        "the dynamic TBM guard must not block native tier admission"
    );
    assert_mapped_state_eq(
        &unavailable.get_regs().unwrap(),
        &before,
        "disabled TBM guard",
    );
    let error = format!(
        "{:#}",
        unavailable
            .step()
            .expect_err("disabled TBM direct replay must raise #UD")
    );
    assert!(error.contains("IDT entry 6 not present"), "{error}");
    assert_mapped_state_eq(
        &unavailable.get_regs().unwrap(),
        &before,
        "disabled TBM direct replay",
    );

    let mut compatibility = make_vcpu_code(&code);
    compatibility.set_tbm_enabled(true);
    let mut regs = compatibility.get_regs().unwrap();
    regs.rbp = source;
    regs.rflags = INITIAL_FLAGS;
    compatibility.set_regs(&regs).unwrap();
    let mut sregs = compatibility.get_sregs().unwrap();
    sregs.cs.l = false;
    sregs.cs.db = true;
    compatibility.set_sregs(&sregs).unwrap();
    let before = compatibility.get_regs().unwrap();

    assert!(
        compatibility
            .jit_try_block()
            .expect("compatibility-mode TBM AArch64 JIT attempt"),
        "the mode guard must form an admitted native frontier"
    );
    assert_mapped_state_eq(
        &compatibility.get_regs().unwrap(),
        &before,
        "compatibility-mode TBM guard",
    );
    assert!(
        compatibility
            .step()
            .expect("compatibility-mode direct replay")
            .is_none()
    );
    let replayed = compatibility.get_regs().unwrap();
    let (result, carry) = tbm_reference(0x02, 6, source, false);
    assert_eq!(replayed.rsp, result, "XOP.W is WIG outside 64-bit mode");
    assert_eq!(replayed.rbp, source);
    assert_eq!(replayed.rflags, expected_tbm_flags(result, carry, false));
    assert_eq!(replayed.rip, LOAD_ADDR + instruction.len() as u64);
}
