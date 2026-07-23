//! Native-JIT differential and precise-frontier tests for scalar MOVRS.

use super::*;

const DATA: u64 = 0x20_0000;
const SOURCE: u64 = 0x0123_4567_89AB_CDEF;
const INITIAL_DESTINATION: u64 = 0xA1B2_C3D4_E5F6_7788;
const INITIAL_FLAGS: u64 = 0xCD7;

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn seed_registers(vcpu: &mut X86_64Vcpu, apx: bool) {
    vcpu.set_apx_enabled(apx);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rax = 0x0101_0101_0101_0101;
    regs.rcx = 0x0202_0202_0202_0202;
    regs.rdx = 0x0303_0303_0303_0303;
    regs.rbx = DATA;
    regs.rsp = 0x11_0000;
    regs.rbp = 0x0606_0606_0606_0606;
    regs.rsi = 0x0707_0707_0707_0707;
    regs.rdi = 0x0808_0808_0808_0808;
    regs.r8 = INITIAL_DESTINATION;
    regs.r9 = 0x0A0A_0A0A_0A0A_0A0A;
    regs.r10 = 0x0B0B_0B0B_0B0B_0B0B;
    regs.r11 = 0x0C0C_0C0C_0C0C_0C0C;
    regs.r12 = 0x0D0D_0D0D_0D0D_0D0D;
    regs.r13 = 0x0E0E_0E0E_0E0E_0E0E;
    regs.r14 = 0x0F0F_0F0F_0F0F_0F0F;
    regs.r15 = 0x1010_1010_1010_1010;
    regs.r16 = INITIAL_DESTINATION;
    regs.r17 = DATA - 0x20;
    regs.r18 = 0;
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
    regs.rflags = INITIAL_FLAGS;
    vcpu.set_regs(&regs).unwrap();
}

#[test]
fn jit_movrs_all_widths_and_byte_namespaces_match_direct_execution() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
    }

    let cases = [
        Case {
            name: "legacy r8b",
            instruction: &[0x44, 0x0F, 0x38, 0x8A, 0x03],
            apx: false,
        },
        Case {
            name: "legacy r8w",
            instruction: &[0x66, 0x44, 0x0F, 0x38, 0x8B, 0x03],
            apx: false,
        },
        Case {
            name: "legacy r8d",
            instruction: &[0x44, 0x0F, 0x38, 0x8B, 0x03],
            apx: false,
        },
        Case {
            name: "legacy r8",
            instruction: &[0x4C, 0x0F, 0x38, 0x8B, 0x03],
            apx: false,
        },
        Case {
            name: "legacy AH",
            instruction: &[0x0F, 0x38, 0x8A, 0x23],
            apx: false,
        },
        Case {
            name: "legacy SPL",
            instruction: &[0x40, 0x0F, 0x38, 0x8A, 0x23],
            apx: false,
        },
        Case {
            name: "legacy BP",
            instruction: &[0x66, 0x0F, 0x38, 0x8B, 0x2B],
            apx: false,
        },
        Case {
            name: "legacy RBP",
            instruction: &[0x48, 0x0F, 0x38, 0x8B, 0x2B],
            apx: false,
        },
        Case {
            name: "APX r16b",
            instruction: &[0x62, 0xEC, 0x78, 0x08, 0x8A, 0x44, 0x91, 0x20],
            apx: true,
        },
        Case {
            name: "APX r16w",
            instruction: &[0x62, 0xEC, 0x79, 0x08, 0x8B, 0x44, 0x91, 0x20],
            apx: true,
        },
        Case {
            name: "APX r16d",
            instruction: &[0x62, 0xEC, 0x78, 0x08, 0x8B, 0x44, 0x91, 0x20],
            apx: true,
        },
        Case {
            name: "APX r16",
            instruction: &[0x62, 0xEC, 0xF8, 0x08, 0x8B, 0x44, 0x91, 0x20],
            apx: true,
        },
        Case {
            name: "APX ESP",
            instruction: &[0x62, 0xF4, 0x78, 0x08, 0x8B, 0x23],
            apx: true,
        },
        Case {
            name: "APX RBP",
            instruction: &[0x62, 0xF4, 0xF8, 0x08, 0x8B, 0x2B],
            apx: true,
        },
    ];

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let (mut direct, direct_memory) = make_vcpu_mem(&code);
        direct_memory
            .write_slice(&SOURCE.to_le_bytes(), GuestAddress(DATA))
            .unwrap();
        seed_registers(&mut direct, case.apx);
        assert!(
            direct
                .step()
                .unwrap_or_else(|error| panic!("{} direct: {error:?}", case.name))
                .is_none(),
            "{} direct exit",
            case.name
        );
        let expected = direct.get_regs().unwrap();

        let (mut jit, jit_memory) = make_vcpu_mem(&code);
        jit_memory
            .write_slice(&SOURCE.to_le_bytes(), GuestAddress(DATA))
            .unwrap();
        seed_registers(&mut jit, case.apx);
        jit.set_jit_call(false);
        jit.set_jit_mem(true);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{} JIT: {error:?}", case.name)),
            "{} must enter the helper-backed native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();

        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

#[test]
fn jit_apx_movrs_guard_is_dynamic_precise_and_precedes_memory() {
    let instruction = [0x62, 0xEC, 0xF8, 0x08, 0x8B, 0x44, 0x91, 0x20];
    let mut code = vec![0xBE, 0x78, 0x56, 0x34, 0x12]; // MOV ESI,0x12345678
    code.extend_from_slice(&instruction);
    code.push(0xF4);
    let (mut vcpu, memory) = make_vcpu_mem(&code);
    memory
        .write_slice(&SOURCE.to_le_bytes(), GuestAddress(DATA))
        .unwrap();
    seed_registers(&mut vcpu, true);
    vcpu.set_jit_call(false);
    vcpu.set_jit_mem(true);

    assert!(vcpu.jit_try_block().expect("enabled APX MOVRS JIT"));
    assert_eq!(vcpu.get_regs().unwrap().r16, SOURCE);

    let mut regs = vcpu.get_regs().unwrap();
    regs.rip = LOAD_ADDR;
    regs.rsi = 0;
    regs.r16 = INITIAL_DESTINATION;
    regs.r17 = MEM_SIZE + 0x1000;
    regs.r18 = 0;
    vcpu.set_regs(&regs).unwrap();
    vcpu.set_apx_enabled(false);

    assert!(vcpu.jit_try_block().expect("cached disabled-APX MOVRS JIT"));
    let after_guard = vcpu.get_regs().unwrap();
    assert_eq!(after_guard.rsi, 0x1234_5678, "native prefix committed");
    assert_eq!(after_guard.r16, INITIAL_DESTINATION);
    assert_eq!(after_guard.rflags, INITIAL_FLAGS);
    assert_eq!(after_guard.rip, LOAD_ADDR + 5, "exact APX guard frontier");

    let before_step = gprs(&after_guard);
    let error = format!("{:#}", vcpu.step().expect_err("disabled APX MOVRS"));
    assert!(error.contains("IDT entry 6 not present"), "{error}");
    let after_step = vcpu.get_regs().unwrap();
    assert_eq!(gprs(&after_step), before_step);
    assert_eq!(after_step.rip, LOAD_ADDR + 5);
}

#[test]
fn jit_reserved_apx_movrs_is_an_exact_terminal_handoff() {
    for (name, invalid) in [
        ("ND", &[0x62, 0xEC, 0x78, 0x18, 0x8B][..]),
        ("NF", &[0x62, 0xEC, 0x78, 0x0C, 0x8B]),
        ("payload", &[0x62, 0xEC, 0x78, 0x09, 0x8B]),
        ("byte W", &[0x62, 0xEC, 0xF8, 0x08, 0x8A]),
    ] {
        let mut code = vec![0xBE, 0x78, 0x56, 0x34, 0x12];
        code.extend_from_slice(invalid);
        code.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00, 0xF4]);
        let mut vcpu = make_vcpu_code(&code);
        seed_registers(&mut vcpu, true);
        let mut before = vcpu.get_regs().unwrap();
        before.rdi = 0xDEAD_BEEF_CAFE_BABE;
        vcpu.set_regs(&before).unwrap();

        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error:?}")),
            "{name}: native prefix must be retained"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rsi, 0x1234_5678, "{name}: prefix result");
        assert_eq!(after.rdi, before.rdi, "{name}: following instruction");
        assert_eq!(after.rflags, before.rflags, "{name}: flags");
        assert_eq!(after.rip, LOAD_ADDR + 5, "{name}: exact handoff PC");
    }
}

#[test]
fn jit_movrs_memory_faults_never_commit_any_destination_class() {
    for (name, instruction, apx) in [
        ("legacy R8", &[0x4C, 0x0F, 0x38, 0x8B, 0x03][..], false),
        ("legacy AH", &[0x0F, 0x38, 0x8A, 0x23], false),
        ("legacy RBP", &[0x48, 0x0F, 0x38, 0x8B, 0x2B], false),
        (
            "APX R16",
            &[0x62, 0xEC, 0xF8, 0x08, 0x8B, 0x44, 0x91, 0x20],
            true,
        ),
        ("APX RSP", &[0x62, 0xF4, 0xF8, 0x08, 0x8B, 0x23], true),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let mut vcpu = make_vcpu_code(&code);
        seed_registers(&mut vcpu, apx);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rbx = MEM_SIZE + 0x1000;
        regs.r17 = MEM_SIZE + 0x1000;
        regs.r18 = 0;
        vcpu.set_regs(&regs).unwrap();
        vcpu.set_jit_call(false);
        vcpu.set_jit_mem(true);
        let before = vcpu.get_regs().unwrap();

        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error:?}")),
            "{name}: faulting MOVRS must still enter native helper path"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(gprs(&after), gprs(&before), "{name}: noncommit GPRs");
        assert_eq!(after.rflags, before.rflags, "{name}: noncommit flags");
        assert_eq!(after.rip, LOAD_ADDR, "{name}: restart PC");
    }
}
