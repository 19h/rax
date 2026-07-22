//! Public oracle shape coverage for legacy and APX-promoted INVPCID.

use super::*;

#[test]
fn emits_exact_invpcid_type_address_fault_class_and_handoff_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let legacy = decode_to_json(&[0x66, 0x0F, 0x38, 0x82, 0x7C, 0x48, 0x08], &opts).unwrap();
    assert_eq!(legacy["smir"]["available"], true);
    assert_eq!(legacy["smir"]["bytes_consumed"], 7);
    let op = &legacy["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_invpcid");
    assert_eq!(op["kind"]["invpcid_type"]["name"], "rdi");
    assert_eq!(op["kind"]["addr"]["kind"], "base_index_scale");
    assert_eq!(op["kind"]["addr"]["base"]["name"], "rax");
    assert_eq!(op["kind"]["addr"]["index"]["name"], "rcx");
    assert_eq!(op["kind"]["addr"]["scale"], 2);
    assert_eq!(op["kind"]["addr"]["disp"], 8);
    assert_eq!(op["kind"]["requires_apx"], false);
    assert_eq!(op["kind"]["stack_segment"], false);
    assert_eq!(op["kind"]["next_pc"], 0x1007);
    assert_eq!(op["memory"]["reads"], true);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);
    assert_eq!(op["writes"].as_array().unwrap().len(), 0);

    // LLVM 23: `{evex} invpcid r31, [r20 + 8*r29 + 64]`. R20 selects SS by
    // default, which must survive lifting for noncanonical-source #SS(0).
    let apx = decode_to_json(&[0x62, 0x2C, 0x7A, 0x08, 0xF2, 0x7C, 0xEC, 0x40], &opts).unwrap();
    assert_eq!(apx["smir"]["available"], true);
    assert_eq!(apx["smir"]["bytes_consumed"], 8);
    let op = &apx["smir"]["ops"][0];
    assert_eq!(op["kind"]["opcode"], "x86_invpcid");
    assert_eq!(op["kind"]["invpcid_type"]["name"], "r31");
    assert_eq!(op["kind"]["addr"]["base"]["name"], "r20");
    assert_eq!(op["kind"]["addr"]["index"]["name"], "r29");
    assert_eq!(op["kind"]["addr"]["scale"], 8);
    assert_eq!(op["kind"]["addr"]["disp"], 0x40);
    assert_eq!(op["kind"]["requires_apx"], true);
    assert_eq!(op["kind"]["stack_segment"], true);
    assert_eq!(op["kind"]["next_pc"], 0x1008);
    assert_eq!(op["memory"]["reads"], true);
    assert_eq!(op["memory"]["writes"], false);
    assert_eq!(op["side_effects"], true);
}
