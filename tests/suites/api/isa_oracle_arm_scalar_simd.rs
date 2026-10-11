//! Existing operation JSON must expose scalar and comparison-zero shapes.

use rax::oracle::{ArmState, OracleIsa, OracleOptions, decode_to_json};

#[test]
fn scalar_integer_and_zero_compare_json_preserves_shape_and_status_contracts() {
    for (instruction, opcode, lanes) in [
        (0x5ee2_8420u32, "vadd", 1),
        (0x7ee2_8420, "vsub", 1),
        (0x5ee0_b820, "vunary", 1),
        (0x7ee0_b820, "vunary", 1),
        (0x5ee0_9820, "vcmp", 1),
        (0x4ee0_9820, "vcmp", 2),
        (0x5ef1_b820, "vreduce", 2),
    ] {
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
        assert_eq!(operation["kind"]["opcode"], opcode);
        assert_eq!(operation["kind"]["elem"], "I64");
        assert_eq!(operation["kind"]["lanes"], lanes);
        assert_eq!(operation["kind"]["dst"]["name"], "v0");
        assert_eq!(operation["side_effects"], false);
        let writes = operation["writes"].as_array().unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0]["name"], "v0");
        if opcode == "vcmp" {
            assert_eq!(operation["kind"]["cond"], "Eq");
            assert_eq!(operation["kind"]["src2"]["kind"], "imm");
            assert_eq!(operation["kind"]["src2"]["value"], 0);
        }
    }
}
