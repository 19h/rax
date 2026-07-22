//! Shared terminal invalid-opcode construction for Intel APX lifting.

use super::*;

impl X86_64Lifter {
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
