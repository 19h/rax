//! Terminal #UD coverage for forbidden legacy prefixes before VEX/EVEX.

use super::*;

const ALLOWED_PREFIXES: [u8; 7] = [0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x67];
const VECTOR_PREFIXES: [(u8, usize); 3] = [(0xC5, 2), (0xC4, 3), (0x62, 4)];

fn lift_nonstrict(bytes: &[u8]) -> Result<LiftResult, LiftError> {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    lifter.lift_insn(0x1000, bytes, &mut ctx)
}

fn assert_terminal_ud(bytes: &[u8]) {
    let strict =
        lift_single(bytes).unwrap_or_else(|error| panic!("strict {bytes:02X?}: {error:?}"));
    assert_invalid_opcode_trap(&strict, bytes.len());

    let nonstrict =
        lift_nonstrict(bytes).unwrap_or_else(|error| panic!("nonstrict {bytes:02X?}: {error:?}"));
    assert_invalid_opcode_trap(&nonstrict, bytes.len());
}

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
fn every_forbidden_legacy_prefix_is_terminal_at_each_vector_lead() {
    for (lead, _) in VECTOR_PREFIXES {
        for forbidden in [0xF0_u8, 0x66, 0xF2, 0xF3] {
            assert_terminal_ud(&[forbidden, lead]);
            for allowed in ALLOWED_PREFIXES {
                assert_terminal_ud(&[allowed, forbidden, lead]);
                assert_terminal_ud(&[forbidden, allowed, lead]);
            }
        }

        for rex in 0x40_u8..=0x4F {
            assert_terminal_ud(&[rex, lead]);
            for allowed in ALLOWED_PREFIXES {
                // A later legacy prefix clears decoded REX state. The raw byte
                // stream must still make the earlier REX architecturally fatal.
                assert_terminal_ud(&[rex, allowed, lead]);
                assert_terminal_ud(&[allowed, rex, lead]);
            }
        }

        // REX2 makes the vector lead a reserved APX legacy-map opcode. Its
        // existing three-byte #UD frontier must remain distinct from VEX/EVEX.
        assert_terminal_ud(&[0xD5, 0x00, lead]);
    }
}

#[test]
fn address_size_and_segment_prefixes_still_reach_vector_payload_decode() {
    for (lead, prefix_width) in VECTOR_PREFIXES {
        for prefixes in ALLOWED_PREFIXES.iter().map(|prefix| vec![*prefix]).chain(
            ALLOWED_PREFIXES
                .iter()
                .filter(|prefix| **prefix != 0x67)
                .map(|segment| vec![*segment, 0x67]),
        ) {
            let mut bytes = prefixes;
            bytes.push(lead);
            assert_incomplete(
                lift_single(&bytes),
                bytes.len(),
                bytes.len() - 1 + prefix_width,
            );
        }
    }
}
