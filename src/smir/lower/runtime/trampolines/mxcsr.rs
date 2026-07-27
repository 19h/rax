//! Native x86 MXCSR state detection.

use crate::smir::ir::SmirFunction;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{BlockId, GuestAddr};

/// Whether an executed region directly reads or writes architectural MXCSR.
///
/// This state channel is independent of native vector admission: helper-backed
/// `STMXCSR` reads the marshalled value with scalar host instructions, and
/// `LDMXCSR` commits a validated value through the same field.
pub(crate) fn uses_x86_mxcsr_state_excluding(
    function: &SmirFunction,
    excluded: &std::collections::HashMap<BlockId, GuestAddr>,
) -> bool {
    function
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .flat_map(|block| &block.ops)
        .any(|op| {
            matches!(
                op.kind,
                OpKind::X86LoadMxcsr { .. } | OpKind::X86StoreMxcsr { .. }
            )
        })
}
