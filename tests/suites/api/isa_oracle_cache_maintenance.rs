//! Privileged cache operations retain their identity and instruction boundary.

use super::*;

#[test]
fn cache_maintenance_json_preserves_kind_side_effects_and_next_pc() {
    let mut opts = OracleOptions::default();
    opts.isa = OracleIsa::X86_64;
    for (bytes, kind) in [
        (&[0x0f, 0x08][..], "invd"),
        (&[0x0f, 0x09][..], "wbinvd"),
        (&[0xf3, 0x0f, 0x09][..], "wbnoinvd"),
    ] {
        let value = decode_to_json(bytes, &opts).unwrap();
        assert_eq!(value["smir"]["available"], true);
        assert_eq!(value["smir"]["bytes_consumed"], bytes.len());
        let op = &value["smir"]["ops"][0];
        assert_eq!(op["kind"]["opcode"], "x86_cache_maintenance");
        assert_eq!(op["kind"]["kind"], kind);
        assert_eq!(
            op["kind"]["next_pc"],
            format!("0x{:x}", opts.pc + bytes.len() as u64)
        );
        assert_eq!(op["side_effects"], true);
        assert!(op["writes"].as_array().unwrap().is_empty());
        assert_eq!(op["memory"]["reads"], false);
        assert_eq!(op["memory"]["writes"], false);
    }
}
