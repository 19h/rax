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
}
