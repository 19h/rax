use rax::oracle::{OracleIsa, OracleOptions, decode_to_json};

fn decode(bytes: &[u8]) -> serde_json::Value {
    let mut options = OracleOptions::default();
    options.isa = OracleIsa::X86_64;
    decode_to_json(bytes, &options).expect("decode XOP instruction")
}

const VPCOM_OPCODES: &[(u8, &str, u64, bool)] = &[
    (0xCC, "I8", 16, true),
    (0xCD, "I16", 8, true),
    (0xCE, "I32", 4, true),
    (0xCF, "I64", 2, true),
    (0xEC, "I8", 16, false),
    (0xED, "I16", 8, false),
    (0xEE, "I32", 4, false),
    (0xEF, "I64", 2, false),
];

fn vpcom_condition(immediate: u8, signed: bool) -> &'static str {
    match (immediate & 7, signed) {
        (0, true) => "Lt",
        (1, true) => "Le",
        (2, true) => "Gt",
        (3, true) => "Ge",
        (0, false) => "Ltu",
        (1, false) => "Leu",
        (2, false) => "Gtu",
        (3, false) => "Geu",
        (4, _) => "Eq",
        (5, _) => "Ne",
        (6, _) => "False",
        (7, _) => "True",
        _ => unreachable!(),
    }
}

#[test]
fn emits_exact_vpcmov_register_guard_roles_width_and_ignored_immediate_bits() {
    // VPCMOV XMM3,XMM2,XMM1,XMM0. The low immediate nibble is ignored.
    let value = decode(&[0x8F, 0xE8, 0x68, 0xA2, 0xD9, 0x0F]);
    let smir = &value["smir"];
    assert_eq!(smir["available"], true);
    assert_eq!(smir["bytes_consumed"], 6);
    assert_eq!(smir["control_flow"]["kind"], "fallthrough");
    assert_eq!(smir["ops"].as_array().unwrap().len(), 2);
    assert_eq!(smir["ops"][0]["opcode"], "x86_require_xop");
    assert_eq!(smir["ops"][0]["side_effects"], true);

    let select = &smir["ops"][1];
    assert_eq!(select["opcode"], "vbitselect");
    assert_eq!(select["kind"]["width"], "V128");
    assert_eq!(select["kind"]["dst"]["name"], "xmm3");
    assert_eq!(select["kind"]["src_true"]["name"], "xmm2");
    assert_eq!(select["kind"]["src_false"]["name"], "xmm1");
    assert_eq!(select["kind"]["mask"]["name"], "xmm0");
    assert_eq!(select["side_effects"], false);
    assert_eq!(select["memory"]["reads"], false);
    assert_eq!(select["memory"]["writes"], false);
}

#[test]
fn emits_exact_vpcmov_memory_access_size_alignment_and_w_selected_role() {
    for w in [false, true] {
        for l in [false, true] {
            let width = if l { "V256" } else { "V128" };
            let access_size = if l { 32 } else { 16 };
            let p1 = (u8::from(w) << 7) | 0x68 | (u8::from(l) << 2);
            // VPCMOV {X,Y}MM1,{X,Y}MM2,{[RAX],reg4},{reg4,[RAX]}.
            let value = decode(&[0x8F, 0xE8, p1, 0xA2, 0x08, 0x4F]);
            let ops = value["smir"]["ops"].as_array().unwrap();
            assert_eq!(ops.len(), 4, "W={w}, L={l}");
            assert_eq!(ops[0]["opcode"], "x86_require_xop");

            let alignment = &ops[1];
            assert_eq!(alignment["opcode"], "x86_check_alignment_ac");
            assert_eq!(alignment["kind"]["access_size"], access_size);
            assert_eq!(alignment["kind"]["alignment"], 16);
            assert_eq!(alignment["kind"]["stack_segment"], false);
            assert_eq!(alignment["kind"]["addr"]["kind"], "direct");
            assert_eq!(alignment["kind"]["addr"]["reg"]["name"], "rax");
            assert_eq!(alignment["side_effects"], true);

            let load = &ops[2];
            assert_eq!(load["opcode"], "vload");
            assert_eq!(load["kind"]["width"], width);
            assert_eq!(load["memory"]["reads"], true);
            assert_eq!(load["memory"]["writes"], false);
            assert_eq!(load["x86_hint"], "VecAlign(Aligned)");

            let select = &ops[3];
            assert_eq!(select["opcode"], "vbitselect");
            assert_eq!(select["kind"]["width"], width);
            assert_eq!(
                select["kind"]["dst"]["name"],
                if l { "ymm1" } else { "xmm1" }
            );
            assert_eq!(
                select["kind"]["src_true"]["name"],
                if l { "ymm2" } else { "xmm2" }
            );
            if w {
                assert_eq!(select["kind"]["mask"]["kind"], "virtual");
                assert_eq!(
                    select["kind"]["src_false"]["name"],
                    if l { "ymm4" } else { "xmm4" }
                );
            } else {
                assert_eq!(
                    select["kind"]["mask"]["name"],
                    if l { "ymm4" } else { "xmm4" }
                );
                assert_eq!(select["kind"]["src_false"]["kind"], "virtual");
            }
        }
    }
}

#[test]
fn emits_exact_vpcom_register_family_predicate_roles_and_high_registers() {
    for &(opcode, elem, lanes, signed) in VPCOM_OPCODES {
        for immediate in 0..=u8::MAX {
            // VPCOM* XMM3,XMM2,XMM1,imm8.
            let value = decode(&[0x8F, 0xE8, 0x68, opcode, 0xD9, immediate]);
            let smir = &value["smir"];
            assert_eq!(smir["available"], true);
            assert_eq!(smir["bytes_consumed"], 6);
            assert_eq!(smir["control_flow"]["kind"], "fallthrough");
            let ops = smir["ops"].as_array().unwrap();
            assert_eq!(ops.len(), 2, "opcode={opcode:#04x}, imm={immediate:#04x}");
            assert_eq!(ops[0]["opcode"], "x86_require_xop");
            assert_eq!(ops[0]["side_effects"], true);

            let compare = &ops[1];
            assert_eq!(compare["opcode"], "vcmp");
            assert_eq!(compare["kind"]["dst"]["name"], "xmm3");
            assert_eq!(compare["kind"]["src1"]["name"], "xmm2");
            assert_eq!(compare["kind"]["src2"]["name"], "xmm1");
            assert_eq!(compare["kind"]["elem"], elem);
            assert_eq!(compare["kind"]["lanes"], lanes);
            assert_eq!(compare["kind"]["cond"], vpcom_condition(immediate, signed));
            assert_eq!(compare["side_effects"], false);
            assert_eq!(compare["memory"]["reads"], false);
            assert_eq!(compare["memory"]["writes"], false);
            assert_eq!(compare["x86_hint"], "XopVpcom");
        }
    }

    // ~R=0 and ~B=0 select XMM11 and XMM9; decoded vvvv selects XMM10.
    let value = decode(&[0x8F, 0x48, 0x28, 0xEF, 0xD9, 0x82]);
    let compare = &value["smir"]["ops"][1];
    assert_eq!(compare["kind"]["dst"]["name"], "xmm11");
    assert_eq!(compare["kind"]["src1"]["name"], "xmm10");
    assert_eq!(compare["kind"]["src2"]["name"], "xmm9");
    assert_eq!(compare["kind"]["cond"], "Gtu");
    assert_eq!(compare["kind"]["elem"], "I64");
    assert_eq!(compare["kind"]["lanes"], 2);
}

#[test]
fn emits_exact_vpcom_memory_access_alignment_and_constant_dependencies() {
    for &(opcode, elem, lanes, signed) in VPCOM_OPCODES {
        for immediate in 0..8 {
            // VPCOM* XMM1,XMM2,[RAX],imm8.
            let value = decode(&[0x8F, 0xE8, 0x68, opcode, 0x08, 0xF0 | immediate]);
            let smir = &value["smir"];
            assert_eq!(smir["available"], true);
            assert_eq!(smir["bytes_consumed"], 6);
            let ops = smir["ops"].as_array().unwrap();
            assert_eq!(ops.len(), 4, "opcode={opcode:#04x}, imm={immediate}");
            assert_eq!(ops[0]["opcode"], "x86_require_xop");

            let alignment = &ops[1];
            assert_eq!(alignment["opcode"], "x86_check_alignment_ac");
            assert_eq!(alignment["kind"]["access_size"], 16);
            assert_eq!(alignment["kind"]["alignment"], 16);
            assert_eq!(alignment["kind"]["stack_segment"], false);
            assert_eq!(alignment["kind"]["addr"]["kind"], "direct");
            assert_eq!(alignment["kind"]["addr"]["reg"]["name"], "rax");
            assert_eq!(alignment["side_effects"], true);

            let load = &ops[2];
            assert_eq!(load["opcode"], "vload");
            assert_eq!(load["kind"]["width"], "V128");
            assert_eq!(load["kind"]["addr"], alignment["kind"]["addr"]);
            assert_eq!(load["memory"]["reads"], true);
            assert_eq!(load["memory"]["writes"], false);
            assert_eq!(load["x86_hint"], "VecAlign(Aligned)");

            let compare = &ops[3];
            assert_eq!(compare["opcode"], "vcmp");
            assert_eq!(compare["kind"]["dst"]["name"], "xmm1");
            assert_eq!(compare["kind"]["src1"]["name"], "xmm2");
            assert_eq!(compare["kind"]["src2"]["kind"], "virtual");
            assert_eq!(compare["kind"]["src2"], load["kind"]["dst"]);
            assert_eq!(compare["kind"]["elem"], elem);
            assert_eq!(compare["kind"]["lanes"], lanes);
            assert_eq!(compare["kind"]["cond"], vpcom_condition(immediate, signed));
            assert_eq!(compare["side_effects"], false);
            assert_eq!(compare["memory"]["reads"], false);
            assert_eq!(compare["memory"]["writes"], false);
            assert_eq!(compare["x86_hint"], "XopVpcom");
        }
    }
}
