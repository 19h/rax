//! Fault barriers expose their access intent without claiming a memory effect.
use super::*;

#[test]
fn hexagon_alignment_json_is_structured_and_precedes_the_access() {
    let mut options = OracleOptions::default();
    options.isa = OracleIsa::Hexagon;
    options.pc = 0x1000;
    for (word, access) in [(0x9181c003_u32, "read"), (0xa181c200, "write")] {
        let value = decode_to_json(&word.to_le_bytes(), &options).unwrap();
        assert_eq!(value["smir"]["available"], true);
        let operations = value["smir"]["ops"].as_array().unwrap();
        let index = operations
            .iter()
            .position(|op| op["opcode"] == "check_memory_alignment")
            .unwrap();
        let guard = &operations[index];
        assert_eq!(guard["guest_pc"], "0x1000");
        assert_eq!(guard["kind"]["access"], access);
        assert!(guard["kind"]["addr"].is_object());
        assert!(guard["kind"].get("width").is_some());
        assert!(guard["kind"]["cond"].is_null());
        assert_eq!(guard["side_effects"], true);
        assert_eq!(guard["memory"]["reads"], false);
        assert_eq!(guard["memory"]["writes"], false);
        assert!(
            operations[..index]
                .iter()
                .all(|op| op["memory"]["reads"] == false && op["memory"]["writes"] == false)
        );
        assert!(
            operations[index + 1..]
                .iter()
                .any(|op| op["memory"]["reads"] == true || op["memory"]["writes"] == true)
        );
    }
}
