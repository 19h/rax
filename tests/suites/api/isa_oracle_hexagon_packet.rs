#[path = "../../support/hexagon.rs"]
mod lifter_stack;
use rax::oracle::{OracleIsa, OracleOptions, decode_to_json};
fn packet(words: &[u32]) -> serde_json::Value {
    packet_endian(words, rax::config::Endianness::Little)
}

fn packet_endian(words: &[u32], endian: rax::config::Endianness) -> serde_json::Value {
    let code: Vec<_> = words
        .iter()
        .flat_map(|word| match endian {
            rax::config::Endianness::Little => word.to_le_bytes(),
            rax::config::Endianness::Big => word.to_be_bytes(),
        })
        .collect();
    lifter_stack::run_on_large_lifter_stack("Hexagon oracle packet contract", || {
        decode_to_json(
            &code,
            &OracleOptions {
                isa: OracleIsa::Hexagon,
                pc: 0x1000,
                hexagon_endian: endian,
                ..Default::default()
            },
        )
        .unwrap()
    })
}
#[test]
fn extended_packet_includes_its_memory_consumer() {
    let value = packet(&[0x00004000, 0x9181c203]);
    assert_eq!(value["smir"]["bytes_consumed"], 8);
    assert!(
        value["smir"]["ops"]
            .as_array()
            .unwrap()
            .iter()
            .any(|op| op["opcode"] == "load")
    );
}
#[test]
fn packet_retains_the_final_branch() {
    let value = packet(&[0x7f004000, 0x5800c004]);
    assert_eq!(value["smir"]["branch_targets"][0], "0x1008");
}
#[test]
fn later_unsupported_word_rejects_the_whole_ir_result() {
    let value = packet(&[0x78004024, 0x00120043]);
    assert!(
        value["smir"]["error"]
            .as_str()
            .is_some_and(|error| error.contains("duplex"))
    );
    assert!(value["smir"].get("ops").is_none());
}
#[test]
fn operation_ids_and_packet_fault_pc_cover_all_words() {
    let value = packet(&[0x78004024, 0x00004000, 0x9181c203]);
    assert_eq!(value["smir"]["bytes_consumed"], 12);
    let operations = value["smir"]["ops"].as_array().unwrap();
    assert!(operations.len() >= 5);
    for (index, op) in operations.iter().enumerate() {
        assert_eq!(op["id"], index);
    }
    assert_eq!(
        operations
            .iter()
            .find(|op| op["opcode"] == "check_memory_alignment")
            .unwrap()["guest_pc"],
        "0x1000"
    );
}

#[test]
fn both_guest_byte_orders_publish_identical_complete_packet_ir() {
    let words = [0x78004024, 0x00004000, 0x9181c203];
    let little = packet_endian(&words, rax::config::Endianness::Little);
    let big = packet_endian(&words, rax::config::Endianness::Big);
    assert_eq!(little["smir"]["bytes_consumed"], 12);
    assert_eq!(big["smir"], little["smir"]);
}

#[test]
fn static_and_seeded_lifting_cover_the_same_packet() {
    use rax::oracle::{OracleMemorySeed, OracleSeed, decode_to_json_with_seed};
    let words = [0x78004024_u32, 0x00004000, 0x9181c203];
    let code: Vec<_> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let options = OracleOptions {
        isa: OracleIsa::Hexagon,
        pc: 0x1000,
        ..Default::default()
    };
    let seed = OracleSeed {
        regs: vec![("r1".into(), 0x2000), ("r3".into(), 0x55), ("r4".into(), 0)],
        memory: vec![OracleMemorySeed {
            addr: 0x2010,
            bytes: vec![0x44, 0x33, 0x22, 0x11],
        }],
        ..Default::default()
    };
    let value = lifter_stack::run_on_large_lifter_stack("Hexagon seeded packet contract", || {
        decode_to_json_with_seed(&code, &options, Some(&seed)).unwrap()
    });
    assert_eq!(value["smir"]["bytes_consumed"], 12);
    assert_eq!(value["side_effects"]["bytes_consumed"], 12);
    assert_eq!(value["side_effects"]["final_regs"]["r3"], "0x11223344");
    assert_eq!(value["side_effects"]["final_regs"]["r4"], "0x1");
    assert!(
        value["side_effects"]["changed_memory"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

fn assert_rejected(value: &serde_json::Value, message: &str) {
    assert_eq!(value["smir"]["available"], true);
    assert!(
        value["smir"]["error"]
            .as_str()
            .is_some_and(|error| error.contains(message)),
        "{}",
        value["smir"]
    );
    assert!(value["smir"].get("ops").is_none());
}

#[test]
fn incomplete_packet_has_no_partial_ir() {
    let value = packet(&[0x7f004000]);
    assert_eq!(value["packet_flags"]["complete"], false);
    assert_rejected(&value, "incomplete");
}

#[test]
fn loop_end_packet_keeps_decoded_metadata_and_rejects_unmodeled_execution_ir() {
    let value = packet(&[0x7f008000, 0x7f00c000]);
    assert_eq!(value["packet_flags"]["loop0_end"], true);
    assert_rejected(&value, "hardware-loop");
}

#[test]
fn packet_longer_than_the_architectural_four_word_limit_has_no_ir() {
    let value = packet(&[0x7f004000, 0x7f004000, 0x7f004000, 0x7f004000, 0x7f00c000]);
    assert_rejected(&value, "longer than four words");
}

#[test]
fn bytes_after_the_first_packet_are_not_added_to_its_ir() {
    let value = packet(&[0x7800c024, 0x9181c203]);
    assert_eq!(value["smir"]["bytes_consumed"], 4);
    let operations = value["smir"]["ops"].as_array().unwrap();
    assert!(
        operations
            .iter()
            .any(|op| op["kind"]["dst"]["name"] == "r4")
    );
    assert!(operations.iter().all(|op| op["opcode"] != "load"));
}

#[test]
fn an_early_branch_keeps_the_later_packet_data_operation() {
    let value = packet(&[0x58004004, 0x7800c024]);
    assert_eq!(value["smir"]["bytes_consumed"], 8);
    assert_eq!(value["smir"]["branch_targets"][0], "0x1008");
    assert!(
        value["smir"]["ops"]
            .as_array()
            .unwrap()
            .iter()
            .any(|op| op["kind"]["dst"]["name"] == "r4")
    );
}

#[test]
fn multiple_control_transfers_have_no_partial_ir() {
    let value = packet(&[0x58004004, 0x5800c004]);
    assert_rejected(&value, "multiple Hexagon packet control-flow");
}

#[test]
fn a_call_before_packet_end_defers_until_packet_link_effects_are_modeled() {
    // Fixed LLVM 23 V68: { call #8; r4=#1 }.
    let value = packet(&[0x5a004004, 0x7800c024]);
    assert_rejected(&value, "call/return/trap before packet end");
}

#[test]
fn public_packet_ir_exposes_ordered_commit_boundaries() {
    let value = packet(&[0x78004038, 0xa181c200]);
    let operations = value["smir"]["ops"].as_array().unwrap();
    assert_eq!(
        operations.first().unwrap()["opcode"],
        "hexagon_packet_begin"
    );
    assert_eq!(
        operations.last().unwrap()["opcode"],
        "hexagon_packet_commit"
    );
    assert_eq!(operations.first().unwrap()["memory"]["writes"], false);
    assert_eq!(operations.last().unwrap()["memory"]["writes"], true);
}
