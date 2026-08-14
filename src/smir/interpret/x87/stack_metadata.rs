//! Payload-independent x87 stack-metadata semantics.

use crate::smir::ir::context::X86X87State;

/// Mark logical ST(i) empty and optionally apply FFREEP's stack pop.
///
/// FFREE and the compatibility FFREEP opcode leave C0-C3 undefined. The
/// deterministic SMIR profile preserves all four bits.
pub(super) fn free(state: &mut X86X87State, st: u8, pop: bool) {
    let target = state.physical_index(st);
    state.set_physical_tag(target, 3);
    if pop {
        let old_top = state.physical_index(0);
        state.set_physical_tag(old_top, 3);
        state.set_top(state.top().wrapping_add(1));
    }
}

/// Apply FINCSTP/FDECSTP's modulo-eight TOP rotation and defined C1=0.
pub(super) fn rotate_top(state: &mut X86X87State, increment: bool) {
    state.status_word &= !0x0200;
    let next = if increment {
        state.top().wrapping_add(1)
    } else {
        state.top().wrapping_sub(1)
    };
    state.set_top(next);
}
