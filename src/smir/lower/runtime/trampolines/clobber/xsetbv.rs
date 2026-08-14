//! Reachable-prefix selection for the XSETBV native frontier.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{BlockId, GuestAddr};
use crate::smir::ir::{SmirBlock, Terminator, X86InstructionBytes};
use crate::smir::lower::x86_64::x86_xsetbv_resume_pc;

/// Select the part of a block reachable before XSETBV returns to the runtime.
///
/// The full block remains the validation input because a following SMIR PC, if
/// present, must agree with the exact byte-derived handoff boundary. Once that
/// invariant holds, operations after XSETBV and the original terminator cannot
/// execute in this native region.
pub(super) fn x86_xsetbv_reachable_prefix(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> Result<Option<SmirBlock>, ()> {
    let Some(index) = block
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86XSetBv { .. }))
    else {
        return Ok(None);
    };
    x86_xsetbv_resume_pc(block, index, instruction_bytes).ok_or(())?;

    let mut prefix = block.clone();
    prefix.ops.truncate(index + 1);
    prefix.terminator = Terminator::Return { values: vec![] };
    Ok(Some(prefix))
}
