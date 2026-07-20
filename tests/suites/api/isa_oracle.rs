use rax::isa::riscv::Xlen;
use rax::oracle::{
    ArmState, MAX_ORACLE_SEED_MEMORY_SIZE, OracleIsa, OracleMemorySeed, OracleOptions, OracleSeed,
    RiscVIsaProfile, decode_to_json, decode_to_json_with_seed, parse_hex_bytes,
};

#[test]
fn parses_hex_bytes_with_prefixes_and_separators() {
    let bytes = parse_hex_bytes("0x90, 48-b8").unwrap();
    assert_eq!(bytes, vec![0x90, 0x48, 0xb8]);
}

#[test]
fn decodes_hexagon_packet() {
    let word = 0x5400c000u32.to_le_bytes();
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::Hexagon;

    let value = decode_to_json(&word, &opts).unwrap();
    assert_eq!(value["isa"], "hexagon");
    assert_eq!(value["packet_flags"]["end_seen"], true);
    assert_eq!(value["decoded_ops"][0]["opcode"], "J2_trap0");
}

#[test]
fn decodes_riscv_instruction() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::RiscV;
    opts.riscv_xlen = Xlen::Rv64;
    opts.riscv_isa = RiscVIsaProfile::Rv64Gc;

    let value = decode_to_json(&[0x93, 0x00, 0x10, 0x00], &opts).unwrap();
    assert_eq!(value["isa"], "riscv");
    assert_eq!(value["decoded_ops"][0]["op"], "Addi");
}

#[test]
fn decodes_arm_aarch64_instruction() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::Arm;
    opts.arm_state = ArmState::Aarch64;

    let value = decode_to_json(&[0x20, 0x00, 0x80, 0xd2], &opts).unwrap();
    assert_eq!(value["isa"], "arm");
    assert_eq!(value["decoded_ops"][0]["mnemonic"], "movz");
}

#[test]
fn decodes_arm_aarch64_non_temporal_pairs() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::Arm;
    opts.arm_state = ArmState::Aarch64;

    let cases = [
        (0x2840_0820u32, "ldnp"),
        (0xa840_0820u32, "ldnp"),
        (0x2800_0820u32, "stnp"),
        (0xa800_0820u32, "stnp"),
        (0x6940_0820u32, "ldpsw"),
        (0x6840_0820u32, "unknown"),
    ];

    for (raw, mnemonic) in cases {
        let value = decode_to_json(&raw.to_le_bytes(), &opts).unwrap();
        assert_eq!(value["decoded_ops"][0]["mnemonic"], mnemonic);
    }
}

#[test]
fn decodes_arm_aarch64_bti_hints() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::Arm;
    opts.arm_state = ArmState::Aarch64;

    let bti = [
        0xd503_241fu32,
        0xd503_245fu32,
        0xd503_249fu32,
        0xd503_24dfu32,
    ];
    for raw in bti {
        let value = decode_to_json(&raw.to_le_bytes(), &opts).unwrap();
        assert_eq!(value["decoded_ops"][0]["mnemonic"], "bti");
    }

    for raw in [0xd503_231fu32, 0xd503_235fu32] {
        let value = decode_to_json(&raw.to_le_bytes(), &opts).unwrap();
        assert_eq!(value["decoded_ops"][0]["mnemonic"], "hint");
    }
}

#[test]
fn decodes_arm_aarch64_prfm_literal() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::Arm;
    opts.arm_state = ArmState::Aarch64;

    let cases = [
        (0xd800_0000u32, "Prfop(PLDL1KEEP)", "Label(0)"),
        (0xd800_0075u32, "Prfop(PSTL3STRM)", "Label(12)"),
        (0xd8ff_ffffu32, "Prfop(Raw(31))", "Label(-4)"),
    ];

    for (raw, prfop, label) in cases {
        let value = decode_to_json(&raw.to_le_bytes(), &opts).unwrap();
        let op = &value["decoded_ops"][0];
        assert_eq!(op["mnemonic"], "prfm");
        assert_eq!(op["operands"][0], prfop);
        assert_eq!(op["operands"][1], label);
    }
}

#[test]
fn decodes_arm_aarch64_prfm_memory_forms() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::Arm;
    opts.arm_state = ArmState::Aarch64;

    let cases = [
        (0xf980_0020u32, "Prfop(PLDL1KEEP)", "offset: Imm(0)"),
        (0xf980_0c35u32, "Prfop(PSTL3STRM)", "offset: Imm(24)"),
        (0xf89f_8022u32, "Prfop(PLDL2KEEP)", "offset: Imm(-8)"),
        (0xf8a2_5824u32, "Prfop(PLDL3KEEP)", "extend_type: UXTW"),
        (0xf8a2_e832u32, "Prfop(PSTL2KEEP)", "extend_type: SXTX"),
    ];

    for (raw, prfop, mem_fragment) in cases {
        let value = decode_to_json(&raw.to_le_bytes(), &opts).unwrap();
        let op = &value["decoded_ops"][0];
        assert_eq!(op["mnemonic"], "prfm");
        assert_eq!(op["operands"][0], prfop);
        let mem = op["operands"][1].as_str().unwrap();
        assert!(mem.contains(mem_fragment), "{mem}");
    }

    for raw in [
        0xf880_8420u32, // post-index prefetch pattern is undefined
        0xf880_8c20u32, // pre-index prefetch pattern is undefined
        0xf8a2_0820u32, // register-offset sub-word extend is undefined
        0xf9c0_0020u32, // size=11/opc=11 is undefined
    ] {
        let value = decode_to_json(&raw.to_le_bytes(), &opts).unwrap();
        assert_eq!(value["decoded_ops"][0]["mnemonic"], "unknown");
    }
}

#[test]
fn decodes_arm_aarch64_ldst_register_offset_forms() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::Arm;
    opts.arm_state = ArmState::Aarch64;

    let cases = [
        (0xf862_5820u32, "ldr", "extend_type: UXTW", "shift: 3"),
        (0xb822_c820u32, "str", "extend_type: SXTW", "shift: 0"),
        (0x38a2_e820u32, "ldrsb", "extend_type: SXTX", "shift: 0"),
        (0x78e2_5820u32, "ldrsh", "extend_type: UXTW", "shift: 1"),
        (0xf840_8820u32, "ldtr", "offset: Imm(8)", "mode: Offset"),
    ];

    for (raw, mnemonic, mem_fragment, shift_fragment) in cases {
        let value = decode_to_json(&raw.to_le_bytes(), &opts).unwrap();
        let op = &value["decoded_ops"][0];
        assert_eq!(op["mnemonic"], mnemonic);
        let mem = op["operands"][1].as_str().unwrap();
        assert!(mem.contains(mem_fragment), "{mem}");
        assert!(mem.contains(shift_fragment), "{mem}");
    }

    let value = decode_to_json(&0xf862_0820u32.to_le_bytes(), &opts).unwrap();
    assert_eq!(value["decoded_ops"][0]["mnemonic"], "unknown");
}

#[test]
fn decodes_x86_with_smir_lift() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let value = decode_to_json(&[0x90], &opts).unwrap();
    assert_eq!(value["isa"], "x86_64");
    assert_eq!(value["smir"]["available"], true);
}

#[test]
fn reports_x86_ud0_as_an_exact_two_byte_invalid_opcode_trap() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let value = decode_to_json(&[0x0F, 0xFF, 0x04, 0x25, 0x78, 0x56, 0x34, 0x12], &opts).unwrap();
    let smir = &value["smir"];
    assert_eq!(smir["available"], true);
    assert_eq!(smir["bytes_consumed"], 2);
    assert_eq!(smir["ops"].as_array().unwrap().len(), 0);
    assert_eq!(smir["control_flow"]["kind"], "trap");
    assert_eq!(smir["control_flow"]["trap"], "InvalidOpcode");
    assert_eq!(smir["ends_block"], true);
    assert_eq!(smir["ends_function"], true);
}

#[test]
fn reports_exact_hypercall_hint_and_unsupported_alias_profiles() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    for bytes in [&[0x0F, 0x01, 0xC1][..], &[0x0F, 0x01, 0xD9]] {
        let value = decode_to_json(bytes, &opts).unwrap();
        let smir = &value["smir"];
        assert_eq!(smir["available"], true, "{bytes:02X?}");
        assert_eq!(smir["bytes_consumed"], bytes.len(), "{bytes:02X?}");
        assert_eq!(smir["ops"], serde_json::json!([]), "{bytes:02X?}");
        assert_eq!(smir["control_flow"]["kind"], "fallthrough");
        assert_eq!(smir["ends_block"], false);
        assert_eq!(smir["ends_function"], false);
    }

    for bytes in [
        &[0xF2, 0x0F, 0x01, 0xD9][..],
        &[0xF3, 0x0F, 0x01, 0xD9],
        &[0xD5, 0x80, 0x01, 0xD9],
    ] {
        let value = decode_to_json(bytes, &opts).unwrap();
        let smir = &value["smir"];
        assert_eq!(smir["available"], true, "{bytes:02X?}");
        assert_eq!(smir["bytes_consumed"], bytes.len(), "{bytes:02X?}");
        assert_eq!(smir["ops"], serde_json::json!([]), "{bytes:02X?}");
        assert_eq!(smir["control_flow"]["kind"], "trap");
        assert_eq!(smir["control_flow"]["trap"], "InvalidOpcode");
        assert_eq!(smir["ends_block"], true);
        assert_eq!(smir["ends_function"], true);
    }
}

#[test]
fn reports_disabled_vmx_controls_as_exact_invalid_opcode_traps() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    for bytes in [
        &[0x0F, 0x01, 0xC2][..], // VMLAUNCH
        &[0x0F, 0x01, 0xC3],     // VMRESUME
        &[0x0F, 0x01, 0xC4],     // VMXOFF
        &[0x0F, 0x01, 0xD4],     // VMFUNC
        &[0xD5, 0x80, 0x01, 0xC2],
        &[0xD5, 0x80, 0x01, 0xC3],
        &[0xD5, 0x80, 0x01, 0xC4],
        &[0xD5, 0x80, 0x01, 0xD4],
    ] {
        let value = decode_to_json(bytes, &opts).unwrap();
        let smir = &value["smir"];
        assert_eq!(smir["available"], true, "{bytes:02X?}");
        assert_eq!(smir["bytes_consumed"], bytes.len(), "{bytes:02X?}");
        assert_eq!(smir["ops"], serde_json::json!([]), "{bytes:02X?}");
        assert_eq!(smir["control_flow"]["kind"], "trap", "{bytes:02X?}");
        assert_eq!(
            smir["control_flow"]["trap"], "InvalidOpcode",
            "{bytes:02X?}"
        );
        assert_eq!(smir["ends_block"], true, "{bytes:02X?}");
        assert_eq!(smir["ends_function"], true, "{bytes:02X?}");
    }
}

#[test]
fn reports_disabled_svm_controls_as_exact_invalid_opcode_traps() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    for modrm in [0xD8, 0xDA, 0xDB, 0xDC, 0xDD, 0xDE, 0xDF] {
        for bytes in [vec![0x0F, 0x01, modrm], vec![0xD5, 0x80, 0x01, modrm]] {
            let value = decode_to_json(&bytes, &opts).unwrap();
            let smir = &value["smir"];
            assert_eq!(smir["available"], true, "{bytes:02X?}");
            assert_eq!(smir["bytes_consumed"], bytes.len(), "{bytes:02X?}");
            assert_eq!(smir["ops"], serde_json::json!([]), "{bytes:02X?}");
            assert_eq!(smir["control_flow"]["kind"], "trap", "{bytes:02X?}");
            assert_eq!(
                smir["control_flow"]["trap"], "InvalidOpcode",
                "{bytes:02X?}"
            );
            assert_eq!(smir["ends_block"], true, "{bytes:02X?}");
            assert_eq!(smir["ends_function"], true, "{bytes:02X?}");
        }
    }
}

#[test]
fn reports_disabled_sgx_roots_as_exact_invalid_opcode_traps() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    for modrm in [0xC0, 0xCF, 0xD7] {
        for bytes in [vec![0x0F, 0x01, modrm], vec![0xD5, 0x80, 0x01, modrm]] {
            let value = decode_to_json(&bytes, &opts).unwrap();
            let smir = &value["smir"];
            assert_eq!(smir["available"], true, "{bytes:02X?}");
            assert_eq!(smir["bytes_consumed"], bytes.len(), "{bytes:02X?}");
            assert_eq!(smir["ops"], serde_json::json!([]), "{bytes:02X?}");
            assert_eq!(smir["control_flow"]["kind"], "trap", "{bytes:02X?}");
            assert_eq!(
                smir["control_flow"]["trap"], "InvalidOpcode",
                "{bytes:02X?}"
            );
            assert_eq!(smir["ends_block"], true, "{bytes:02X?}");
            assert_eq!(smir["ends_function"], true, "{bytes:02X?}");
        }
    }
}

#[test]
fn reports_disabled_pconfig_as_an_exact_invalid_opcode_trap() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    for bytes in [
        &[0x0F, 0x01, 0xC5][..],
        &[0x66, 0x0F, 0x01, 0xC5],
        &[0xF2, 0x0F, 0x01, 0xC5],
        &[0xF3, 0x0F, 0x01, 0xC5],
        &[0xC5, 0xF8, 0x01, 0xC5],
        &[0xC4, 0xE1, 0x78, 0x01, 0xC5],
        &[0x62, 0xF1, 0x7C, 0x08, 0x01, 0xC5],
        &[0xD5, 0x80, 0x01, 0xC5],
    ] {
        let value = decode_to_json(bytes, &opts).unwrap();
        let smir = &value["smir"];
        assert_eq!(smir["available"], true, "{bytes:02X?}");
        assert_eq!(smir["bytes_consumed"], bytes.len(), "{bytes:02X?}");
        assert_eq!(smir["ops"], serde_json::json!([]), "{bytes:02X?}");
        assert_eq!(smir["control_flow"]["kind"], "trap", "{bytes:02X?}");
        assert_eq!(
            smir["control_flow"]["trap"], "InvalidOpcode",
            "{bytes:02X?}"
        );
        assert_eq!(smir["ends_block"], true, "{bytes:02X?}");
        assert_eq!(smir["ends_function"], true, "{bytes:02X?}");
    }
}

#[test]
fn reports_disabled_msr_extensions_as_exact_invalid_opcode_traps() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    for bytes in [
        &[0x0F, 0x01, 0xC6][..],
        &[0xF2, 0x0F, 0x01, 0xC6],
        &[0xF3, 0x0F, 0x01, 0xC6],
        &[0x66, 0x0F, 0x01, 0xC6],
        &[0xC5, 0xF8, 0x01, 0xC6],
        &[0xC4, 0xE1, 0x78, 0x01, 0xC6],
        &[0x62, 0xF1, 0x7C, 0x08, 0x01, 0xC6],
        &[0xD5, 0x80, 0x01, 0xC6],
    ] {
        let value = decode_to_json(bytes, &opts).unwrap();
        let smir = &value["smir"];
        assert_eq!(smir["available"], true, "{bytes:02X?}");
        assert_eq!(smir["bytes_consumed"], bytes.len(), "{bytes:02X?}");
        assert_eq!(smir["ops"], serde_json::json!([]), "{bytes:02X?}");
        assert_eq!(smir["control_flow"]["kind"], "trap", "{bytes:02X?}");
        assert_eq!(
            smir["control_flow"]["trap"], "InvalidOpcode",
            "{bytes:02X?}"
        );
        assert_eq!(smir["ends_block"], true, "{bytes:02X?}");
        assert_eq!(smir["ends_function"], true, "{bytes:02X?}");
    }
}

#[test]
fn emits_structured_smir_ops() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let value = decode_to_json(&[0xb8, 0x34, 0x12, 0x00, 0x00], &opts).unwrap();
    let op = &value["smir"]["ops"][0];

    assert_eq!(op["opcode"], "mov");
    assert_eq!(op["kind"]["opcode"], "mov");
    assert_eq!(op["kind"]["dst"]["kind"], "arch");
    assert_eq!(op["kind"]["dst"]["name"], "rax");
    assert_eq!(op["kind"]["src"]["kind"], "imm");
    assert_eq!(op["kind"]["src"]["value"], 0x1234);
    assert!(op.get("debug").is_none());
}

#[test]
fn emits_exact_fsgsbase_state_direction_width_and_apx_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let read = decode_to_json(&[0xF3, 0xD5, 0x99, 0xAE, 0xCF], &opts).unwrap();
    assert_eq!(read["smir"]["available"], true);
    assert_eq!(read["smir"]["bytes_consumed"], 5);
    let op = &read["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_fsgsbase");
    assert_eq!(op["kind"]["operand"]["name"], "r31");
    assert_eq!(op["kind"]["base"]["name"], "gs_base");
    assert_eq!(op["kind"]["write"], false);
    assert_eq!(op["kind"]["width"], "W64");
    assert_eq!(op["kind"]["requires_apx"], true);
    assert_eq!(op["writes"][0]["name"], "r31");
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);

    let write = decode_to_json(&[0xF3, 0x0F, 0xAE, 0xD0], &opts).unwrap();
    let op = &write["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_fsgsbase");
    assert_eq!(op["kind"]["operand"]["name"], "rax");
    assert_eq!(op["kind"]["base"]["name"], "fs_base");
    assert_eq!(op["kind"]["write"], true);
    assert_eq!(op["kind"]["width"], "W32");
    assert_eq!(op["kind"]["requires_apx"], false);
    assert_eq!(op["writes"][0]["name"], "fs_base");
}

#[test]
fn emits_exact_pkru_state_direction_and_implicit_register_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let read = decode_to_json(&[0x0F, 0x01, 0xEE], &opts).unwrap();
    assert_eq!(read["smir"]["available"], true);
    assert_eq!(read["smir"]["bytes_consumed"], 3);
    let op = &read["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_pkru");
    assert_eq!(op["kind"]["eax"]["name"], "rax");
    assert_eq!(op["kind"]["ecx"]["name"], "rcx");
    assert_eq!(op["kind"]["edx"]["name"], "rdx");
    assert_eq!(op["kind"]["pkru"]["name"], "pkru");
    assert_eq!(op["kind"]["write"], false);
    assert_eq!(op["writes"][0]["name"], "rax");
    assert_eq!(op["writes"][1]["name"], "rdx");
    assert_eq!(op["side_effects"], true);

    let write = decode_to_json(&[0x0F, 0x01, 0xEF], &opts).unwrap();
    let op = &write["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_pkru");
    assert_eq!(op["kind"]["write"], true);
    assert_eq!(op["writes"][0]["name"], "pkru");
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
}

#[test]
fn emits_exact_clac_stac_guest_ac_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    for (modrm, value) in [(0xCA, false), (0xCB, true)] {
        let decoded = decode_to_json(&[0x0F, 0x01, modrm], &opts).unwrap();
        assert_eq!(decoded["smir"]["available"], true);
        assert_eq!(decoded["smir"]["bytes_consumed"], 3);
        let op = &decoded["smir"]["ops"][0];
        assert_eq!(op["kind"]["opcode"], "set_ac");
        assert_eq!(op["kind"]["value"], value);
        assert_eq!(op["writes"].as_array().unwrap().len(), 0);
        assert_eq!(op["memory"]["reads"], false);
        assert_eq!(op["memory"]["writes"], false);
        assert_eq!(op["side_effects"], true);
    }
}

#[test]
fn emits_exact_clts_guest_cr0_state_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let decoded = decode_to_json(&[0x0F, 0x06], &opts).unwrap();
    assert_eq!(decoded["smir"]["available"], true);
    assert_eq!(decoded["smir"]["bytes_consumed"], 2);
    let op = &decoded["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_clts");
    assert!(op["reads"].is_null());
    assert_eq!(op["writes"].as_array().unwrap().len(), 0);
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);
}

#[test]
fn emits_exact_rdmsr_wrmsr_metadata_and_frontiers() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let read = decode_to_json(&[0x0F, 0x32], &opts).unwrap();
    assert_eq!(read["smir"]["available"], true);
    assert_eq!(read["smir"]["bytes_consumed"], 2);
    let op = &read["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_msr");
    assert_eq!(op["kind"]["eax"]["name"], "rax");
    assert_eq!(op["kind"]["ecx"]["name"], "rcx");
    assert_eq!(op["kind"]["edx"]["name"], "rdx");
    assert_eq!(op["kind"]["write"], false);
    assert_eq!(op["kind"]["next_pc"], 0x1002);
    assert_eq!(op["writes"][0]["name"], "rax");
    assert_eq!(op["writes"][1]["name"], "rdx");
    assert_eq!(op["side_effects"], true);

    let write = decode_to_json(&[0x66, 0x0F, 0x30], &opts).unwrap();
    assert_eq!(write["smir"]["available"], true);
    assert_eq!(write["smir"]["bytes_consumed"], 3);
    let op = &write["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_msr");
    assert_eq!(op["kind"]["write"], true);
    assert_eq!(op["kind"]["next_pc"], 0x1003);
    assert_eq!(op["writes"].as_array().unwrap().len(), 0);
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);
}

#[test]
fn emits_exact_mov_from_control_register_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let decoded = decode_to_json(&[0x45, 0x0F, 0x20, 0xC7], &opts).unwrap();
    assert_eq!(decoded["smir"]["available"], true);
    assert_eq!(decoded["smir"]["bytes_consumed"], 4);
    let op = &decoded["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_read_control");
    assert_eq!(op["kind"]["control"], "Cr8");
    assert_eq!(op["kind"]["dst"]["arch"], "x86_64");
    assert_eq!(op["kind"]["dst"]["name"], "r15");
    assert!(op["reads"].is_null());
    assert_eq!(op["writes"].as_array().unwrap().len(), 1);
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);
}

#[test]
fn emits_exact_smsw_register_and_memory_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let register = decode_to_json(&[0xD5, 0x91, 0x01, 0xE7], &opts).unwrap();
    assert_eq!(register["smir"]["available"], true);
    assert_eq!(register["smir"]["bytes_consumed"], 4);
    let op = &register["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_smsw");
    assert_eq!(op["kind"]["target"]["kind"], "register");
    assert_eq!(op["kind"]["target"]["dst"]["name"], "r31");
    assert_eq!(op["kind"]["target"]["width"], "W32");
    assert_eq!(op["kind"]["requires_apx"], true);
    assert_eq!(op["writes"][0]["name"], "r31");
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);

    let memory = decode_to_json(&[0x48, 0x0F, 0x01, 0x60, 0x08], &opts).unwrap();
    assert_eq!(memory["smir"]["available"], true);
    assert_eq!(memory["smir"]["bytes_consumed"], 5);
    let op = &memory["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_smsw");
    assert_eq!(op["kind"]["target"]["kind"], "memory");
    assert_eq!(op["kind"]["requires_apx"], false);
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], true);
    assert_eq!(op["side_effects"], true);
}

#[test]
fn emits_exact_sldt_str_selector_target_width_and_apx_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let register = decode_to_json(&[0xD5, 0x91, 0x00, 0xCF], &opts).unwrap();
    assert_eq!(register["smir"]["available"], true);
    assert_eq!(register["smir"]["bytes_consumed"], 4);
    let op = &register["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_system_selector_store");
    assert_eq!(op["kind"]["selector"], "Tr");
    assert_eq!(op["kind"]["target"]["kind"], "register");
    assert_eq!(op["kind"]["target"]["dst"]["name"], "r31");
    assert_eq!(op["kind"]["target"]["width"], "W32");
    assert_eq!(op["kind"]["requires_apx"], true);
    assert_eq!(op["writes"][0]["name"], "r31");
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);

    let memory = decode_to_json(&[0x48, 0x0F, 0x00, 0x40, 0x08], &opts).unwrap();
    assert_eq!(memory["smir"]["available"], true);
    assert_eq!(memory["smir"]["bytes_consumed"], 5);
    let op = &memory["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_system_selector_store");
    assert_eq!(op["kind"]["selector"], "Ldtr");
    assert_eq!(op["kind"]["target"]["kind"], "memory");
    assert_eq!(op["kind"]["target"]["width"], "B2");
    assert_eq!(op["kind"]["target"]["addr"]["kind"], "base_offset");
    assert_eq!(op["kind"]["target"]["addr"]["base"]["name"], "rax");
    assert_eq!(op["kind"]["target"]["addr"]["offset"], 8);
    assert_eq!(op["kind"]["requires_apx"], false);
    assert_eq!(op["writes"].as_array().unwrap().len(), 0);
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], true);
    assert_eq!(op["side_effects"], true);
}

#[test]
fn emits_exact_lldt_ltr_source_hidden_state_handoff_busy_and_apx_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let register = decode_to_json(&[0xD5, 0x91, 0x00, 0xD7], &opts).unwrap();
    assert_eq!(register["smir"]["available"], true);
    assert_eq!(register["smir"]["bytes_consumed"], 4);
    let op = &register["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_system_selector_load");
    assert_eq!(op["kind"]["selector"], "Ldtr");
    assert_eq!(op["kind"]["source"]["kind"], "register");
    assert_eq!(op["kind"]["source"]["src"]["name"], "r31");
    assert_eq!(op["kind"]["source"]["width"], "W16");
    assert_eq!(op["kind"]["requires_apx"], true);
    assert_eq!(op["kind"]["next_pc"], 0x1004);
    assert!(op["reads"].is_null());
    assert_eq!(op["writes"].as_array().unwrap().len(), 0);
    assert_eq!(op["memory"]["reads"], true);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);

    let memory = decode_to_json(&[0x48, 0x0F, 0x00, 0x50, 0x08], &opts).unwrap();
    assert_eq!(memory["smir"]["available"], true);
    assert_eq!(memory["smir"]["bytes_consumed"], 5);
    let op = &memory["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_system_selector_load");
    assert_eq!(op["kind"]["selector"], "Ldtr");
    assert_eq!(op["kind"]["source"]["kind"], "memory");
    assert_eq!(op["kind"]["source"]["width"], "B2");
    assert_eq!(op["kind"]["source"]["addr"]["kind"], "base_offset");
    assert_eq!(op["kind"]["source"]["addr"]["base"]["name"], "rax");
    assert_eq!(op["kind"]["source"]["addr"]["offset"], 8);
    assert_eq!(op["kind"]["requires_apx"], false);
    assert_eq!(op["kind"]["next_pc"], 0x1005);
    assert_eq!(op["memory"]["reads"], true);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);

    let ltr = decode_to_json(&[0x48, 0x0F, 0x00, 0x58, 0x08], &opts).unwrap();
    assert_eq!(ltr["smir"]["available"], true);
    assert_eq!(ltr["smir"]["bytes_consumed"], 5);
    let op = &ltr["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_system_selector_load");
    assert_eq!(op["kind"]["selector"], "Tr");
    assert_eq!(op["kind"]["source"]["kind"], "memory");
    assert_eq!(op["kind"]["source"]["width"], "B2");
    assert_eq!(op["kind"]["source"]["addr"]["kind"], "base_offset");
    assert_eq!(op["kind"]["source"]["addr"]["base"]["name"], "rax");
    assert_eq!(op["kind"]["source"]["addr"]["offset"], 8);
    assert_eq!(op["kind"]["requires_apx"], false);
    assert_eq!(op["kind"]["next_pc"], 0x1005);
    assert_eq!(op["memory"]["reads"], true);
    assert_eq!(op["memory"]["writes"], true);
    assert_eq!(op["side_effects"], true);
}

#[test]
fn emits_exact_descriptor_table_memory_table_handoff_and_apx_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let sidt = decode_to_json(&[0x48, 0x0F, 0x01, 0x48, 0x08], &opts).unwrap();
    assert_eq!(sidt["smir"]["available"], true);
    assert_eq!(sidt["smir"]["bytes_consumed"], 5);
    let op = &sidt["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_descriptor_table_store");
    assert_eq!(op["kind"]["table"], "Idt");
    assert_eq!(op["kind"]["addr"]["kind"], "base_offset");
    assert_eq!(op["kind"]["addr"]["base"]["name"], "rax");
    assert_eq!(op["kind"]["addr"]["offset"], 8);
    assert_eq!(op["kind"]["requires_apx"], false);
    assert!(op["reads"].is_null());
    assert_eq!(op["writes"].as_array().unwrap().len(), 0);
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], true);
    assert_eq!(op["side_effects"], true);

    let sgdt = decode_to_json(&[0xD5, 0xB3, 0x01, 0x04, 0xD1], &opts).unwrap();
    assert_eq!(sgdt["smir"]["available"], true);
    let op = &sgdt["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_descriptor_table_store");
    assert_eq!(op["kind"]["table"], "Gdt");
    assert_eq!(op["kind"]["requires_apx"], true);
    assert_eq!(op["kind"]["addr"]["base"]["name"], "r25");
    assert_eq!(op["kind"]["addr"]["index"]["name"], "r26");
    assert_eq!(op["kind"]["addr"]["scale"], 8);

    let lidt = decode_to_json(&[0x48, 0x0F, 0x01, 0x58, 0x08], &opts).unwrap();
    assert_eq!(lidt["smir"]["available"], true);
    assert_eq!(lidt["smir"]["bytes_consumed"], 5);
    let op = &lidt["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_descriptor_table_load");
    assert_eq!(op["kind"]["table"], "Idt");
    assert_eq!(op["kind"]["addr"]["kind"], "base_offset");
    assert_eq!(op["kind"]["addr"]["base"]["name"], "rax");
    assert_eq!(op["kind"]["addr"]["offset"], 8);
    assert_eq!(op["kind"]["requires_apx"], false);
    assert_eq!(op["kind"]["next_pc"], 0x1005);
    assert!(op["reads"].is_null());
    assert_eq!(op["writes"].as_array().unwrap().len(), 0);
    assert_eq!(op["memory"]["reads"], true);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);

    let lgdt = decode_to_json(&[0xD5, 0xB3, 0x01, 0x14, 0xD1], &opts).unwrap();
    assert_eq!(lgdt["smir"]["available"], true);
    assert_eq!(lgdt["smir"]["bytes_consumed"], 5);
    let op = &lgdt["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_descriptor_table_load");
    assert_eq!(op["kind"]["table"], "Gdt");
    assert_eq!(op["kind"]["requires_apx"], true);
    assert_eq!(op["kind"]["next_pc"], 0x1005);
    assert_eq!(op["kind"]["addr"]["base"]["name"], "r25");
    assert_eq!(op["kind"]["addr"]["index"]["name"], "r26");
    assert_eq!(op["kind"]["addr"]["scale"], 8);
    assert_eq!(op["memory"]["reads"], true);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);
}

#[test]
fn emits_exact_lmsw_register_memory_and_handoff_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let register = decode_to_json(&[0xD5, 0x91, 0x01, 0xF7], &opts).unwrap();
    assert_eq!(register["smir"]["available"], true);
    assert_eq!(register["smir"]["bytes_consumed"], 4);
    let op = &register["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_lmsw");
    assert_eq!(op["kind"]["source"]["kind"], "register");
    assert_eq!(op["kind"]["source"]["src"]["name"], "r31");
    assert_eq!(op["kind"]["source"]["width"], "W16");
    assert_eq!(op["kind"]["requires_apx"], true);
    assert_eq!(op["kind"]["next_pc"], 0x1004);
    assert!(op["reads"].is_null());
    assert_eq!(op["writes"].as_array().unwrap().len(), 0);
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);

    let memory = decode_to_json(&[0x48, 0x0F, 0x01, 0x70, 0x08], &opts).unwrap();
    assert_eq!(memory["smir"]["available"], true);
    assert_eq!(memory["smir"]["bytes_consumed"], 5);
    let op = &memory["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_lmsw");
    assert_eq!(op["kind"]["source"]["kind"], "memory");
    assert_eq!(op["kind"]["source"]["width"], "B2");
    assert_eq!(op["kind"]["source"]["addr"]["kind"], "base_offset");
    assert_eq!(op["kind"]["source"]["addr"]["base"]["name"], "rax");
    assert_eq!(op["kind"]["source"]["addr"]["offset"], 8);
    assert_eq!(op["kind"]["requires_apx"], false);
    assert_eq!(op["kind"]["next_pc"], 0x1005);
    assert_eq!(op["memory"]["reads"], true);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);
}

#[test]
fn emits_exact_mov_to_control_register_metadata_and_handoff_frontier() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let decoded = decode_to_json(&[0x49, 0x0F, 0x22, 0xE6], &opts).unwrap();
    assert_eq!(decoded["smir"]["available"], true);
    assert_eq!(decoded["smir"]["bytes_consumed"], 4);
    let op = &decoded["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_write_control");
    assert_eq!(op["kind"]["control"], "Cr4");
    assert_eq!(op["kind"]["src"]["arch"], "x86_64");
    assert_eq!(op["kind"]["src"]["name"], "r14");
    assert_eq!(op["kind"]["next_pc"], 0x1004);
    assert!(op["reads"].is_null());
    assert_eq!(op["writes"].as_array().unwrap().len(), 0);
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);
}

#[test]
fn emits_exact_mov_from_debug_register_metadata_and_instruction_boundary() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let decoded = decode_to_json(&[0x49, 0x0F, 0x21, 0xFE], &opts).unwrap();
    assert_eq!(decoded["smir"]["available"], true);
    assert_eq!(decoded["smir"]["bytes_consumed"], 4);
    let op = &decoded["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_read_debug");
    assert_eq!(op["kind"]["debug"], "Dr7");
    assert_eq!(op["kind"]["dst"]["arch"], "x86_64");
    assert_eq!(op["kind"]["dst"]["name"], "r14");
    assert!(op["reads"].is_null());
    assert_eq!(op["writes"].as_array().unwrap().len(), 1);
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);
}

#[test]
fn emits_exact_mov_to_debug_register_metadata_and_instruction_boundary() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let decoded = decode_to_json(&[0x49, 0x0F, 0x23, 0xFE], &opts).unwrap();
    assert_eq!(decoded["smir"]["available"], true);
    assert_eq!(decoded["smir"]["bytes_consumed"], 4);
    let op = &decoded["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_write_debug");
    assert_eq!(op["kind"]["debug"], "Dr7");
    assert_eq!(op["kind"]["src"]["arch"], "x86_64");
    assert_eq!(op["kind"]["src"]["name"], "r14");
    assert!(op["reads"].is_null());
    assert_eq!(op["writes"].as_array().unwrap().len(), 0);
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);
}

#[test]
fn emits_exact_rdtsc_and_rdtscp_destination_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let rdtsc = decode_to_json(&[0x0F, 0x31], &opts).unwrap();
    assert_eq!(rdtsc["smir"]["available"], true);
    assert_eq!(rdtsc["smir"]["bytes_consumed"], 2);
    let op = &rdtsc["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_read_tsc");
    assert_eq!(op["kind"]["dst_lo"]["name"], "rax");
    assert_eq!(op["kind"]["dst_hi"]["name"], "rdx");
    assert!(op["kind"]["dst_aux"].is_null());
    assert_eq!(op["writes"][0]["name"], "rax");
    assert_eq!(op["writes"][1]["name"], "rdx");
    assert_eq!(op["side_effects"], true);

    let rdtscp = decode_to_json(&[0x0F, 0x01, 0xF9], &opts).unwrap();
    assert_eq!(rdtscp["smir"]["available"], true);
    assert_eq!(rdtscp["smir"]["bytes_consumed"], 3);
    let op = &rdtscp["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_read_tsc");
    assert_eq!(op["kind"]["dst_lo"]["name"], "rax");
    assert_eq!(op["kind"]["dst_hi"]["name"], "rdx");
    assert_eq!(op["kind"]["dst_aux"]["name"], "rcx");
    assert_eq!(op["writes"][0]["name"], "rax");
    assert_eq!(op["writes"][1]["name"], "rdx");
    assert_eq!(op["writes"][2]["name"], "rcx");
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);
}

#[test]
fn emits_exact_rdpmc_selector_and_destination_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let decoded = decode_to_json(&[0x0F, 0x33], &opts).unwrap();
    assert_eq!(decoded["smir"]["available"], true);
    assert_eq!(decoded["smir"]["bytes_consumed"], 2);
    let op = &decoded["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_read_pmc");
    assert_eq!(op["kind"]["selector"]["name"], "rcx");
    assert_eq!(op["kind"]["dst_lo"]["name"], "rax");
    assert_eq!(op["kind"]["dst_hi"]["name"], "rdx");
    assert!(op["reads"].is_null());
    assert_eq!(op["writes"][0]["name"], "rax");
    assert_eq!(op["writes"][1]["name"], "rdx");
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);
}

#[test]
fn emits_and_executes_exact_swapgs_state_dataflow() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let decoded = decode_to_json(&[0x0F, 0x01, 0xF8], &opts).unwrap();
    assert_eq!(decoded["smir"]["available"], true);
    assert_eq!(decoded["smir"]["bytes_consumed"], 3);
    let op = &decoded["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_swapgs");
    assert_eq!(op["kind"]["gs_base"]["name"], "gs_base");
    assert_eq!(op["kind"]["kernel_gs_base"]["name"], "kernel_gs_base");
    assert_eq!(op["writes"][0]["name"], "gs_base");
    assert_eq!(op["writes"][1]["name"], "kernel_gs_base");
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);

    let seed = OracleSeed {
        regs: vec![
            ("gs_base".to_string(), 0x0000_7FFF_1234_5000),
            ("kernel_gs_base".to_string(), 0xFFFF_8000_ABCD_E000),
        ],
        memory: vec![],
        memory_size: None,
    };
    let executed = decode_to_json_with_seed(&[0x0F, 0x01, 0xF8], &opts, Some(&seed)).unwrap();
    assert_eq!(executed["side_effects"]["available"], true);
    assert_eq!(
        executed["side_effects"]["changed_regs"]["gs_base"]["after"],
        "0xffff8000abcde000"
    );
    assert_eq!(
        executed["side_effects"]["changed_regs"]["kernel_gs_base"]["after"],
        "0x7fff12345000"
    );
}

#[test]
fn emits_and_executes_exact_monitor_mwait_semantics() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let monitor = decode_to_json(&[0x0F, 0x01, 0xC8], &opts).unwrap();
    assert_eq!(monitor["smir"]["available"], true);
    assert_eq!(monitor["smir"]["bytes_consumed"], 3);
    let op = &monitor["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_monitor_mwait");
    assert_eq!(op["kind"]["rcx"]["name"], "rcx");
    assert_eq!(op["kind"]["hint"]["name"], "rdx");
    assert_eq!(op["kind"]["stack_segment"], false);
    assert!(!op["kind"]["addr"].is_null());
    assert_eq!(op["writes"].as_array().unwrap().len(), 0);
    assert_eq!(op["memory"]["reads"], true);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);

    let mwait = decode_to_json(&[0x0F, 0x01, 0xC9], &opts).unwrap();
    let op = &mwait["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_monitor_mwait");
    assert_eq!(op["kind"]["rcx"]["name"], "rcx");
    assert_eq!(op["kind"]["hint"]["name"], "rax");
    assert_eq!(op["kind"]["stack_segment"], false);
    assert!(op["kind"]["addr"].is_null());
    assert_eq!(op["memory"]["reads"], false);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);

    let ss_monitor = decode_to_json(&[0x36, 0x0F, 0x01, 0xC8], &opts).unwrap();
    assert_eq!(ss_monitor["smir"]["ops"][0]["kind"]["stack_segment"], true);

    let seed = OracleSeed {
        regs: vec![
            ("rax".to_string(), 0x20),
            ("rcx".to_string(), 0),
            ("rdx".to_string(), 0xA5A5),
        ],
        memory: vec![OracleMemorySeed {
            addr: 0x20,
            bytes: vec![0x5A],
        }],
        memory_size: Some(0x100),
    };
    let executed = decode_to_json_with_seed(&[0x0F, 0x01, 0xC8], &opts, Some(&seed)).unwrap();
    assert_eq!(executed["side_effects"]["available"], true);
    assert_eq!(
        executed["side_effects"]["changed_regs"]
            .as_object()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        executed["side_effects"]["changed_memory"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn emits_exact_legacy_pcmpxstrx_semantics() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let explicit = decode_to_json(&[0x66, 0x4D, 0x0F, 0x3A, 0x61, 0xD1, 0xFD], &opts).unwrap();
    assert_eq!(explicit["smir"]["available"], true);
    assert_eq!(explicit["smir"]["bytes_consumed"], 7);
    let op = explicit["smir"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["kind"]["opcode"] == "x86_packed_string_compare")
        .unwrap();
    assert_eq!(op["kind"]["dst"]["name"], "rcx");
    assert_eq!(op["kind"]["src1"]["name"], "xmm10");
    assert_eq!(op["kind"]["src2"]["name"], "xmm9");
    assert_eq!(op["kind"]["len1"]["name"], "rax");
    assert_eq!(op["kind"]["len2"]["name"], "rdx");
    assert_eq!(op["kind"]["length_width"], "W64");
    assert_eq!(op["kind"]["kind"], "ExplicitIndex");
    assert_eq!(op["kind"]["imm"], 0xFD);
    assert_eq!(op["writes"][0]["name"], "rcx");

    let implicit = decode_to_json(&[0x66, 0x0F, 0x3A, 0x62, 0xD1, 0x40], &opts).unwrap();
    assert_eq!(implicit["smir"]["bytes_consumed"], 6);
    let op = implicit["smir"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["kind"]["opcode"] == "x86_packed_string_compare")
        .unwrap();
    assert_eq!(op["kind"]["dst"]["name"], "xmm0");
    assert_eq!(op["kind"]["len1"], serde_json::Value::Null);
    assert_eq!(op["kind"]["len2"], serde_json::Value::Null);
    assert_eq!(op["kind"]["kind"], "ImplicitMask");
}

#[test]
fn emits_complete_non_transactional_rtm_semantics() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let xabort = decode_to_json(&[0xc6, 0xf8, 0x42], &opts).unwrap();
    assert_eq!(xabort["smir"]["bytes_consumed"], 3);
    assert_eq!(xabort["smir"]["control_flow"]["kind"], "fallthrough");
    assert_eq!(xabort["smir"]["ops"], serde_json::json!([]));

    let xbegin_rel32 = decode_to_json(&[0xc7, 0xf8, 0x05, 0x00, 0x00, 0x00], &opts).unwrap();
    assert_eq!(xbegin_rel32["smir"]["bytes_consumed"], 6);
    assert_eq!(xbegin_rel32["smir"]["control_flow"]["kind"], "branch");
    assert_eq!(xbegin_rel32["smir"]["control_flow"]["target"], "0x100b");
    assert_eq!(xbegin_rel32["smir"]["ops"][0]["kind"]["opcode"], "mov");
    assert_eq!(xbegin_rel32["smir"]["ops"][0]["kind"]["dst"]["name"], "rax");
    assert_eq!(xbegin_rel32["smir"]["ops"][0]["kind"]["src"]["value"], 0);
    assert_eq!(xbegin_rel32["smir"]["ops"][0]["kind"]["width"], "W32");

    let xbegin_rel16 = decode_to_json(&[0x66, 0xc7, 0xf8, 0xfb, 0xff], &opts).unwrap();
    assert_eq!(xbegin_rel16["smir"]["bytes_consumed"], 5);
    assert_eq!(xbegin_rel16["smir"]["control_flow"]["target"], "0x1000");

    let xtest = decode_to_json(&[0x0f, 0x01, 0xd6], &opts).unwrap();
    assert_eq!(xtest["smir"]["bytes_consumed"], 3);
    assert_eq!(xtest["smir"]["ops"][0]["kind"]["opcode"], "x86_xtest");
    assert_eq!(xtest["smir"]["ops"][0]["side_effects"], true);

    let xend = decode_to_json(&[0x0f, 0x01, 0xd5], &opts).unwrap();
    assert_eq!(xend["smir"]["bytes_consumed"], 3);
    assert_eq!(xend["smir"]["control_flow"]["kind"], "trap");
    assert_eq!(xend["smir"]["control_flow"]["trap"], "GeneralProtection");
    assert_eq!(xend["smir"]["ops"], serde_json::json!([]));

    opts.pc = 0x0000_7fff_ffff_fffa;
    let noncanonical_xbegin = decode_to_json(&[0xc7, 0xf8, 0x00, 0x00, 0x00, 0x00], &opts).unwrap();
    assert_eq!(noncanonical_xbegin["smir"]["bytes_consumed"], 6);
    assert_eq!(
        noncanonical_xbegin["smir"]["control_flow"]["trap"],
        "GeneralProtection"
    );
    assert_eq!(noncanonical_xbegin["smir"]["ops"], serde_json::json!([]));
}

#[test]
fn emits_evex_sqrt_embedded_rounding_and_sae_semantics() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    for (p2, round) in [
        (0x18, "RoundNearest"),
        (0x38, "RoundDown"),
        (0x58, "RoundUp"),
        (0x78, "RoundTowardZero"),
    ] {
        let value = decode_to_json(&[0x62, 0xF1, 0x7C, p2, 0x51, 0xC1], &opts).unwrap();
        assert_eq!(value["smir"]["available"], true);
        assert_eq!(value["smir"]["bytes_consumed"], 6);
        assert_eq!(value["smir"]["control_flow"]["kind"], "fallthrough");
        let op = value["smir"]["ops"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["kind"]["opcode"] == "x86_sqrt")
            .unwrap();
        assert_eq!(op["kind"]["dst"]["name"], "zmm0");
        assert_eq!(op["kind"]["src"]["name"], "zmm1");
        assert_eq!(op["kind"]["elem"], "F32");
        assert_eq!(op["kind"]["lanes"], 16);
        assert_eq!(op["kind"]["round"], round);
        assert_eq!(op["kind"]["suppress_exceptions"], true);
        assert_eq!(op["side_effects"], false);
    }

    let scalar = decode_to_json(&[0x62, 0xF1, 0xFF, 0x18, 0x51, 0xC1], &opts).unwrap();
    let op = scalar["smir"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["kind"]["opcode"] == "x86_sqrt")
        .unwrap();
    assert_eq!(op["kind"]["src"]["name"], "xmm1");
    assert_eq!(op["kind"]["elem"], "F64");
    assert_eq!(op["kind"]["lanes"], 1);
    assert_eq!(op["kind"]["round"], "RoundNearest");
    assert_eq!(op["kind"]["suppress_exceptions"], true);
}

#[test]
fn emits_evex_scalar_binary_embedded_rounding_and_sae_semantics() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    for (p2, round) in [
        (0x18, "RoundNearest"),
        (0x38, "RoundDown"),
        (0x58, "RoundUp"),
        (0x78, "RoundTowardZero"),
    ] {
        let value = decode_to_json(&[0x62, 0xF1, 0x7E, p2, 0x58, 0xD1], &opts).unwrap();
        assert_eq!(value["smir"]["available"], true);
        assert_eq!(value["smir"]["bytes_consumed"], 6);
        let op = value["smir"]["ops"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["kind"]["opcode"] == "x86_fp_binary")
            .unwrap();
        assert_eq!(op["kind"]["src1"]["name"], "xmm0");
        assert_eq!(op["kind"]["src2"]["name"], "xmm1");
        assert_eq!(op["kind"]["elem"], "F32");
        assert_eq!(op["kind"]["lanes"], 1);
        assert_eq!(op["kind"]["op"], "Add");
        assert_eq!(op["kind"]["round"], round);
        assert_eq!(op["kind"]["suppress_exceptions"], true);
        assert_eq!(op["side_effects"], false);
    }

    let sae = decode_to_json(&[0x62, 0xF1, 0xFF, 0x18, 0x5F, 0xD1], &opts).unwrap();
    let op = sae["smir"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["kind"]["opcode"] == "x86_fp_binary")
        .unwrap();
    assert_eq!(op["kind"]["elem"], "F64");
    assert_eq!(op["kind"]["op"], "Max");
    assert_eq!(op["kind"]["round"], "Dynamic");
    assert_eq!(op["kind"]["suppress_exceptions"], true);
}

#[test]
fn emits_atomic_register_movd_q_and_keeps_memory_effects_explicit() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    for (bytes, dst, src, width, zero_upper) in [
        (&[0x66, 0x0F, 0x6E, 0xC1][..], "xmm0", "rcx", "W32", false),
        (
            &[0xC4, 0xE1, 0xF9, 0x7E, 0xC1][..],
            "rcx",
            "xmm0",
            "W64",
            false,
        ),
        (
            &[0x62, 0xC1, 0x7D, 0x08, 0x6E, 0xC8][..],
            "xmm17",
            "r8",
            "W32",
            true,
        ),
    ] {
        let value = decode_to_json(bytes, &opts).unwrap();
        let kind = &value["smir"]["ops"][0]["kind"];
        assert_eq!(kind["opcode"], "x86_movd_q");
        assert_eq!(kind["dst"]["name"], dst);
        assert_eq!(kind["src"]["name"], src);
        assert_eq!(kind["width"], width);
        assert_eq!(kind["zero_upper"], zero_upper);
    }

    let memory = decode_to_json(&[0x66, 0x0F, 0x6E, 0x00], &opts).unwrap();
    let opcodes = memory["smir"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|op| op["kind"]["opcode"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&"load"));
    assert!(!opcodes.contains(&"x86_movd_q"));
}

#[test]
fn emits_riscv_architectural_smir_destinations() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::RiscV;
    opts.riscv_xlen = Xlen::Rv64;
    opts.riscv_isa = RiscVIsaProfile::Rv64Gc;

    // addi x1, x0, 1
    let value = decode_to_json(&[0x93, 0x00, 0x10, 0x00], &opts).unwrap();
    let op = &value["smir"]["ops"][0];

    assert_eq!(op["kind"]["dst"]["kind"], "arch");
    assert_eq!(op["kind"]["dst"]["arch"], "riscv");
    assert_eq!(op["kind"]["dst"]["name"], "x1");
    assert_eq!(value["smir"]["arch_outputs"], serde_json::json!([]));
}

#[test]
fn reports_seeded_side_effects() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let seed = OracleSeed {
        regs: vec![("rax".to_string(), 0)],
        memory: vec![],
        memory_size: None,
    };
    let value =
        decode_to_json_with_seed(&[0xb8, 0x34, 0x12, 0x00, 0x00], &opts, Some(&seed)).unwrap();

    assert_eq!(value["side_effects"]["available"], true);
    assert_eq!(
        value["side_effects"]["changed_regs"]["rax"]["after"],
        "0x1234"
    );
}

#[test]
fn reports_seeded_riscv_side_effects() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::RiscV;
    opts.riscv_xlen = Xlen::Rv64;
    opts.riscv_isa = RiscVIsaProfile::Rv64Gc;

    let seed = OracleSeed {
        regs: vec![("x1".to_string(), 5)],
        memory: vec![],
        memory_size: None,
    };
    // addi x1, x1, 1
    let value = decode_to_json_with_seed(&[0x93, 0x80, 0x10, 0x00], &opts, Some(&seed)).unwrap();

    assert_eq!(value["side_effects"]["available"], true);
    assert_eq!(value["side_effects"]["changed_regs"]["x1"]["after"], "0x6");
}

#[test]
fn rejects_oversized_oracle_seed_memory_size() {
    let value = serde_json::json!({
        "memory_size": MAX_ORACLE_SEED_MEMORY_SIZE + 1,
    });
    let err = OracleSeed::from_json(&value).unwrap_err();
    assert!(err.contains("seed.memory_size"), "{err}");
    assert!(err.contains("oracle seed memory limit"), "{err}");
}

#[test]
fn rejects_sparse_oracle_seed_memory_span_before_allocation() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let seed = OracleSeed {
        regs: vec![],
        memory: vec![
            OracleMemorySeed {
                addr: 0,
                bytes: vec![0],
            },
            OracleMemorySeed {
                addr: (MAX_ORACLE_SEED_MEMORY_SIZE as u64) + 1,
                bytes: vec![0],
            },
        ],
        memory_size: None,
    };
    let value = decode_to_json_with_seed(&[0x90], &opts, Some(&seed)).unwrap();
    let err = value["side_effects"]["error"].as_str().unwrap();
    assert!(err.contains("seeded memory span"), "{err}");
    assert!(err.contains("oracle seed memory limit"), "{err}");
}
