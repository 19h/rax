//! Oracle control-flow metadata for the dedicated x86 `INT3` encoding.

use super::*;

#[test]
fn emits_exact_int3_breakpoint_handoff_metadata() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let value = decode_to_json(&[0x66, 0xF3, 0x2E, 0x48, 0xCC], &opts).unwrap();
    let smir = &value["smir"];
    assert_eq!(smir["available"], true);
    assert_eq!(smir["bytes_consumed"], 5);
    assert_eq!(smir["ops"], serde_json::json!([]));
    assert_eq!(smir["control_flow"]["kind"], "trap");
    assert_eq!(
        smir["control_flow"]["trap"],
        "X86Breakpoint { fault_pc: 4096, return_pc: 4101, requires_apx: false }"
    );
    assert_eq!(smir["ends_block"], true);
    assert_eq!(smir["ends_function"], true);
}

#[test]
fn reports_int3_and_int_vector_three_as_distinct_terminal_events() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;

    let int3 = decode_to_json(&[0xCC], &opts).unwrap();
    let int_3 = decode_to_json(&[0xCD, 0x03], &opts).unwrap();
    assert_eq!(
        int3["smir"]["control_flow"]["trap"],
        "X86Breakpoint { fault_pc: 4096, return_pc: 4097, requires_apx: false }"
    );
    assert_eq!(
        int_3["smir"]["control_flow"]["trap"],
        "X86SoftwareInterrupt { vector: 3, fault_pc: 4096, return_pc: 4098, requires_apx: false }"
    );
}
