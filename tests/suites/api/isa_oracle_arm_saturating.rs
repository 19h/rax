//! Oracle contracts for saturation shape, indexed sources, and QC effects.

use rax::oracle::{ArmState, OracleIsa, OracleOptions, decode_to_json};

#[test]
fn saturating_json_preserves_arithmetic_shape_and_implicit_status_writes() {
    let cases = [
        (0x4e22_0c20u32, "Add", "I8", "Vector128", true, Some("v2")),
        (
            0x6e22_2c20,
            "Subtract",
            "I8",
            "Vector128",
            false,
            Some("v2"),
        ),
        (
            0x4e20_3820,
            "MixedAccumulate",
            "I8",
            "Vector128",
            true,
            None,
        ),
        (
            0x6e20_3820,
            "MixedAccumulate",
            "I8",
            "Vector128",
            false,
            None,
        ),
        (0x5e20_7820, "Absolute", "I8", "Scalar", true, None),
        (0x6e60_7820, "Negate", "I16", "Vector128", true, None),
        (
            0x0f08_7420,
            "ShiftImmediate { amount: 0 }",
            "I8",
            "Vector64",
            true,
            None,
        ),
        (
            0x6f0f_6420,
            "ShiftSignedToUnsigned { amount: 7 }",
            "I8",
            "Vector128",
            false,
            None,
        ),
        (
            0x7ee2_5c20,
            "ShiftRegister { rounding: true }",
            "I64",
            "Scalar",
            false,
            Some("v2"),
        ),
        (
            0x4f72_d820,
            "DoublingMultiplyHigh { rounding: true, lane: Some(7) }",
            "I16",
            "Vector128",
            true,
            Some("v2"),
        ),
    ];
    for (instruction, kind, element, form, signed, second_source) in cases {
        let mut options = OracleOptions::default();
        options.isa = OracleIsa::Arm;
        options.arm_state = ArmState::Aarch64;
        let value = decode_to_json(&instruction.to_le_bytes(), &options).unwrap();
        assert_eq!(
            value["smir"]["available"], true,
            "{instruction:#010x}: {value}"
        );
        assert_eq!(value["smir"]["control_flow"]["kind"], "fallthrough");
        let operations = value["smir"]["ops"].as_array().unwrap();
        assert_eq!(operations.len(), 1);
        let operation = &operations[0];
        assert_eq!(operation["kind"]["opcode"], "arm_simd_saturating");
        assert_eq!(operation["kind"]["kind"], kind);
        assert_eq!(operation["kind"]["elem"], element);
        assert_eq!(operation["kind"]["form"], form);
        assert_eq!(operation["kind"]["signed"], signed);
        assert_eq!(operation["kind"]["dst"]["name"], "v0");
        assert_eq!(operation["kind"]["src1"]["name"], "v1");
        match second_source {
            Some(name) => assert_eq!(operation["kind"]["src2"]["name"], name),
            None => assert!(operation["kind"]["src2"].is_null()),
        }
        assert_eq!(operation["side_effects"], true);
        let writes = operation["writes"].as_array().unwrap();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0]["name"], "v0");
        assert_eq!(writes[1]["name"], "fpsr");
        assert_eq!(operation["memory"]["reads"], false);
        assert_eq!(operation["memory"]["writes"], false);
    }
}
