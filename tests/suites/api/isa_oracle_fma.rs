//! Oracle regression coverage for exact x86 FMA3/FMA4 SMIR contracts.

use rax::oracle::{OracleIsa, OracleOptions, decode_to_json};

fn fma_op(value: &serde_json::Value) -> &serde_json::Value {
    value["smir"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["kind"]["opcode"] == "x86_fma")
        .expect("x86 FMA operation")
}

#[test]
fn emits_exact_vex_and_evex_fma3_smir_contracts() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let vex = decode_to_json(&[0xC4, 0xE2, 0x75, 0x98, 0xD3], &opts).unwrap();
    let op = fma_op(&vex);
    assert_eq!(op["kind"]["src1"]["name"], "ymm2");
    assert_eq!(op["kind"]["src2"]["name"], "ymm1");
    assert_eq!(op["kind"]["src3"]["name"], "ymm3");
    assert_eq!(op["kind"]["mask"], serde_json::Value::Null);
    assert_eq!(op["kind"]["elem"], "F32");
    assert_eq!(op["kind"]["kind"], "Add");
    assert_eq!(op["kind"]["order"], "Order132");
    assert_eq!(op["kind"]["round"], "Dynamic");
    assert_eq!(op["kind"]["lanes"], 8);
    assert_eq!(op["side_effects"], true);
    assert!(op["x86_hint"].as_str().unwrap().contains("V256"));

    // EVEX.b with a register source repurposes L'L as RC. RC=01 selects
    // round-down while the operation width remains 512 bits.
    let evex = decode_to_json(&[0x62, 0xF2, 0x75, 0x3B, 0x98, 0xC2], &opts).unwrap();
    let op = fma_op(&evex);
    assert_eq!(op["kind"]["mask"]["name"], "k3");
    assert_eq!(op["kind"]["round"], "RoundDown");
    assert_eq!(op["kind"]["lanes"], 16);
    assert_eq!(op["side_effects"], false);
    assert!(op["x86_hint"].as_str().unwrap().contains("V512"));
}

#[test]
fn emits_exact_vex_fma4_source_order_width_and_status_contract() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    // VFMADDPS ymm1,ymm2,ymm4,ymm3. W=0 places r/m in arithmetic source 2.
    let w0 = decode_to_json(&[0xC4, 0xE3, 0x6D, 0x68, 0xCC, 0x3F], &opts).unwrap();
    let op = fma_op(&w0);
    assert_eq!(op["kind"]["src1"]["name"], "ymm2");
    assert_eq!(op["kind"]["src2"]["name"], "ymm4");
    assert_eq!(op["kind"]["src3"]["name"], "ymm3");
    assert_eq!(op["kind"]["mask"], serde_json::Value::Null);
    assert_eq!(op["kind"]["elem"], "F32");
    assert_eq!(op["kind"]["kind"], "Add");
    assert_eq!(op["kind"]["order"], "Order123");
    assert_eq!(op["kind"]["round"], "Dynamic");
    assert_eq!(op["kind"]["lanes"], 8);
    assert_eq!(op["side_effects"], true);
    assert!(op["x86_hint"].as_str().unwrap().contains("V256"));

    // Scalar L=1 is ignored. W=1 swaps /is4 and r/m; the low /is4 nibble is X.
    let w1_scalar = decode_to_json(&[0xC4, 0xE3, 0xED, 0x6B, 0xCC, 0x30], &opts).unwrap();
    let op = fma_op(&w1_scalar);
    assert_eq!(op["kind"]["src1"]["name"], "xmm2");
    assert_eq!(op["kind"]["src2"]["name"], "xmm3");
    assert_eq!(op["kind"]["src3"]["name"], "xmm4");
    assert_eq!(op["kind"]["elem"], "F64");
    assert_eq!(op["kind"]["order"], "Order123");
    assert_eq!(op["kind"]["lanes"], 1);
    assert!(op["x86_hint"].as_str().unwrap().contains("V128"));
}
