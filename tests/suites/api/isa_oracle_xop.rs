use rax::oracle::{OracleIsa, OracleOptions, decode_to_json};

fn decode(bytes: &[u8]) -> serde_json::Value {
    let mut options = OracleOptions::default();
    options.isa = OracleIsa::X86_64;
    decode_to_json(bytes, &options).expect("decode XOP instruction")
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
