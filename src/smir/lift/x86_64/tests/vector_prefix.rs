//! Exhaustive VEX/EVEX map-frontier coverage.

use super::*;

fn assert_incomplete(result: Result<LiftResult, LiftError>, have: usize, need: usize) {
    let debug = format!("{result:?}");
    assert!(
        matches!(
            result,
            Err(LiftError::Incomplete {
                addr: 0x1000,
                have: actual_have,
                need: actual_need,
            }) if actual_have == have && actual_need == need
        ),
        "{debug}"
    );
}

#[test]
fn vex3_non_profile_maps_are_exhaustive_terminal_ud() {
    for map in 0_u8..=0x1F {
        if matches!(map, 1 | 2 | 3) {
            continue;
        }

        // Intel SDM Vol. 2A Table 2-10 assigns maps 1/2/3. Intel ISE
        // 319433-059 additionally assigns MAP5 only to AMX-FP8, which
        // the deterministic RAX CPUID/XSTATE profile does not enumerate.
        let opcode = if map == 5 { 0xFD } else { 0x00 };
        let bytes = [0xC4, 0xE0 | map, 0x78, opcode, 0xC0];
        let result =
            lift_single(&bytes).unwrap_or_else(|error| panic!("VEX map {map:#04x}: {error:?}"));
        assert_invalid_opcode_trap(&result, 4);
    }
}

#[test]
fn evex_reserved_maps_are_terminal_after_the_opcode_byte() {
    for map in [0_u8, 7] {
        let bytes = [0x62, 0xF0 | map, 0x7C, 0x08, 0x00, 0xC0];
        let result =
            lift_single(&bytes).unwrap_or_else(|error| panic!("EVEX map {map:#04x}: {error:?}"));
        assert_invalid_opcode_trap(&result, 5);
    }
}

#[test]
fn evex_reserved_map_fixed_bit_faults_precede_the_opcode_fetch() {
    for map in [0_u8, 7] {
        for (name, p0, p1) in [
            ("P0[3]", 0xF8 | map, 0x7C),
            ("P1[2]", 0xF0 | map, 0x78),
            ("both", 0xF8 | map, 0x78),
        ] {
            let bytes = [0x62, p0, p1, 0x08];
            let result = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("EVEX map {map:#04x} invalid {name}: {error:?}"));
            assert_invalid_opcode_trap(&result, 4);
        }
    }
}

#[test]
fn invalid_vector_maps_preserve_fetch_and_legacy_prefix_frontiers() {
    assert_incomplete(lift_single(&[0xC4, 0xE4]), 2, 3);
    assert_incomplete(lift_single(&[0xC4, 0xE4, 0x78]), 3, 4);
    assert_incomplete(lift_single(&[0x62, 0xF0, 0x7C]), 3, 4);
    assert_incomplete(lift_single(&[0x62, 0xF0, 0x7C, 0x08]), 4, 5);
    assert_incomplete(lift_single(&[0x67, 0x64, 0xC4, 0xE4]), 4, 5);
    assert_incomplete(lift_single(&[0x67, 0x64, 0xC4, 0xE4, 0x78]), 5, 6);
    assert_incomplete(lift_single(&[0x65, 0x67, 0x62, 0xF0, 0x7C]), 5, 6);
    assert_incomplete(lift_single(&[0x65, 0x67, 0x62, 0xF0, 0x7C, 0x08]), 6, 7);

    let prefixed_vex = lift_single(&[0x67, 0x64, 0xC4, 0xE4, 0x78, 0x00, 0xC0])
        .expect("address-size and FS prefixes before a reserved VEX map");
    assert_invalid_opcode_trap(&prefixed_vex, 6);

    let prefixed_evex = lift_single(&[0x65, 0x67, 0x62, 0xF7, 0x7C, 0x08, 0x00, 0xC0])
        .expect("GS and address-size prefixes before a reserved EVEX map");
    assert_invalid_opcode_trap(&prefixed_evex, 7);
}
