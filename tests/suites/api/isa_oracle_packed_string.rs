//! Oracle regression coverage for x86 packed-string SMIR contracts.

use rax::oracle::{OracleIsa, OracleOptions, decode_to_json};

#[test]
fn emits_exact_vex_packed_string_smir_contract() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let mask = decode_to_json(&[0xC4, 0xE3, 0x79, 0x60, 0xD1, 0x00], &opts).unwrap();
    let op = &mask["smir"]["ops"][0]["kind"];
    assert_eq!(op["opcode"], "x86_packed_string_compare");
    assert_eq!(op["kind"], "ExplicitMask");
    assert_eq!(op["dst"]["name"], "xmm0");
    assert_eq!(op["src1"]["name"], "xmm2");
    assert_eq!(op["src2"]["name"], "xmm1");
    assert_eq!(op["length_width"], "W32");
    assert_eq!(op["zero_upper"], true);

    let index = decode_to_json(&[0xC4, 0xE3, 0xF9, 0x61, 0xD1, 0xFD], &opts).unwrap();
    let op = &index["smir"]["ops"][0]["kind"];
    assert_eq!(op["kind"], "ExplicitIndex");
    assert_eq!(op["dst"]["name"], "rcx");
    assert_eq!(op["len1"]["name"], "rax");
    assert_eq!(op["len2"]["name"], "rdx");
    assert_eq!(op["length_width"], "W64");
    assert_eq!(op["imm"], 0xFD);
    assert_eq!(op["zero_upper"], false);
}
