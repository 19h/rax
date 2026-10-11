//! Oracle contracts for A64 width, part, source, and cumulative QC metadata.

use rax::oracle::{ArmState, OracleIsa, OracleOptions, decode_to_json};
use serde_json::Value;

fn decode_resize(instruction: u32) -> Value {
    let mut options = OracleOptions::default();
    options.isa = OracleIsa::Arm;
    options.arm_state = ArmState::Aarch64;
    let value = decode_to_json(&instruction.to_le_bytes(), &options)
        .unwrap_or_else(|error| panic!("{instruction:#010x}: {error}"));
    assert_eq!(
        value["smir"]["available"], true,
        "{instruction:#010x}: {value}"
    );
    assert_eq!(value["smir"]["control_flow"]["kind"], "fallthrough");
    assert_eq!(value["smir"]["ops"].as_array().unwrap().len(), 1);
    value
}

fn assert_sources_and_status(operation: &Value, second_source: Option<&str>, updates_qc: bool) {
    assert_eq!(operation["kind"]["dst"]["name"], "v0");
    assert_eq!(operation["kind"]["src1"]["name"], "v1");
    match second_source {
        Some(name) => assert_eq!(operation["kind"]["src2"]["name"], name),
        None => assert!(operation["kind"]["src2"].is_null()),
    }
    assert_eq!(operation["side_effects"], updates_qc);
    let writes = operation["writes"].as_array().unwrap();
    assert_eq!(writes.len(), if updates_qc { 2 } else { 1 });
    assert_eq!(writes[0]["name"], "v0");
    if updates_qc {
        assert_eq!(writes[1]["name"], "fpsr");
    }
    assert_eq!(operation["memory"]["reads"], false);
    assert_eq!(operation["memory"]["writes"], false);
}

#[test]
fn long_resize_json_preserves_kind_element_part_signedness_and_qc() {
    let cases = [
        (
            0x0e22_0020,
            "Add",
            "I8",
            "LowerHalf",
            true,
            Some("v2"),
            false,
        ),
        (
            0x6e62_8020,
            "MultiplyAccumulate",
            "I16",
            "UpperHalf",
            false,
            Some("v2"),
            false,
        ),
        (
            0x5e62_d020,
            "SaturatingDoublingMultiply",
            "I16",
            "Scalar",
            true,
            Some("v2"),
            true,
        ),
        (
            0x6f0f_a420,
            "ShiftLeft { amount: 7 }",
            "I8",
            "UpperHalf",
            false,
            None,
            false,
        ),
        (
            0x4e20_6820,
            "PairwiseAdd { width: V128, accumulate: true }",
            "I8",
            "LowerHalf",
            true,
            None,
            false,
        ),
    ];
    for (instruction, kind, element, part, signed, second_source, updates_qc) in cases {
        let value = decode_resize(instruction);
        let operation = &value["smir"]["ops"][0];
        assert_eq!(operation["kind"]["opcode"], "arm_simd_long");
        assert_eq!(operation["kind"]["kind"], kind, "{instruction:#010x}");
        assert_eq!(operation["kind"]["elem"], element);
        assert_eq!(operation["kind"]["part"], part);
        assert_eq!(operation["kind"]["signed"], signed);
        assert_sources_and_status(operation, second_source, updates_qc);
    }
}

#[test]
fn narrow_resize_json_preserves_clipping_rounding_part_and_qc() {
    let cases = [
        (
            0x4e21_2820,
            "Extract { mode: Truncate }",
            "UpperHalf",
            None,
            false,
        ),
        (
            0x2e21_2820,
            "Extract { mode: SignedToUnsigned }",
            "LowerHalf",
            None,
            true,
        ),
        (
            0x6e22_4020,
            "HighArithmetic { subtract: false, rounding: true }",
            "UpperHalf",
            Some("v2"),
            false,
        ),
        (
            0x7f0f_9c20,
            "ShiftRight { amount: 1, rounding: true, mode: UnsignedSaturate }",
            "Scalar",
            None,
            true,
        ),
        (
            0x0f08_8420,
            "ShiftRight { amount: 8, rounding: false, mode: Truncate }",
            "LowerHalf",
            None,
            false,
        ),
    ];
    for (instruction, kind, part, second_source, updates_qc) in cases {
        let value = decode_resize(instruction);
        let operation = &value["smir"]["ops"][0];
        assert_eq!(operation["kind"]["opcode"], "arm_simd_narrow");
        assert_eq!(operation["kind"]["kind"], kind, "{instruction:#010x}");
        assert_eq!(operation["kind"]["elem"], "I8");
        assert_eq!(operation["kind"]["part"], part);
        assert_sources_and_status(operation, second_source, updates_qc);
    }
}
