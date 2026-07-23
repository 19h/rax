//! Shared terminal invalid-opcode construction for Intel APX lifting.

use super::*;

impl X86_64Lifter {
    /// Return whether Intel APX Architecture Specification revision 7.0
    /// assigns this opcode byte in EVEX MAP4. This is the section 3.1.5 table
    /// plus the later-added MOVRS rows in section 6.38. Prefix and ModR/M
    /// restrictions are classified by the owning family after this frontier.
    pub(super) fn apx_map4_opcode_is_assigned(opcode: u8) -> bool {
        matches!(
            opcode,
            0x00..=0x03
                | 0x08..=0x0B
                | 0x10..=0x13
                | 0x18..=0x1B
                | 0x20..=0x24
                | 0x28..=0x2C
                | 0x30..=0x33
                | 0x38..=0x3B
                | 0x40..=0x4F
                | 0x60
                | 0x61
                | 0x65
                | 0x66
                | 0x69
                | 0x6B
                | 0x80
                | 0x81
                | 0x83..=0x85
                | 0x88
                | 0x8A
                | 0x8B
                | 0x8F
                | 0xA5
                | 0xAD
                | 0xAF
                | 0xC0
                | 0xC1
                | 0xD0..=0xD3
                | 0xF0..=0xF2
                | 0xF4..=0xF9
                | 0xFC
                | 0xFE
                | 0xFF
        )
    }

    /// Construct a terminal #UD once an APX encoding is known to be reserved.
    /// The caller supplies the exact encoding frontier inspected to establish
    /// the fault; no apparent addressing or immediate bytes are consumed.
    pub(super) fn apx_invalid_opcode(bytes_consumed: usize) -> LiftResult {
        LiftResult {
            ops: Vec::new(),
            bytes_consumed,
            control_flow: ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode,
            },
            branch_targets: Vec::new(),
        }
    }

    pub(super) fn apx_modrm_invalid_opcode(prefix: ApxEvexPrefix) -> LiftResult {
        Self::apx_invalid_opcode(prefix.bytes + 2)
    }

    /// Read the first operand byte while retaining full-instruction lengths in
    /// an `Incomplete` diagnostic. APX family lifters receive a slice beginning
    /// at ModR/M, whereas `ApxEvexPrefix::bytes` is measured from the original
    /// instruction start.
    pub(super) fn apx_operand_modrm_byte(
        prefix: ApxEvexPrefix,
        bytes: &[u8],
        pc: u64,
    ) -> Result<u8, LiftError> {
        bytes.first().copied().ok_or(LiftError::Incomplete {
            addr: pc,
            have: prefix.bytes + 1,
            need: prefix.bytes + 2,
        })
    }
}
