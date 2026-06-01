//! (bitmanip) Hexagon instructions — direct opcode-dispatch semantic handlers.
//! Filled in by the implementation effort; verified against the qemu-hexagon
//! oracle (tests/hexagon_diff.rs). See sem/alu.rs for the established pattern.

use super::super::opcode::{DecodedOp, Opcode};
use super::SemCtx;

/// Execute a bitmanip-class opcode. Returns `false` if `op` is not in this class.
pub fn exec(op: Opcode, d: &DecodedOp, ctx: &mut SemCtx) -> bool {
    let _ = (op, d, ctx);
    false
}
