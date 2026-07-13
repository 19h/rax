//! SMIR optimization passes.
//!
//! This module implements optimization passes for SMIR to improve execution performance.
//! The most impactful optimization for x86 is dead flag elimination, which removes
//! flag updates that are never read.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagState, FlagUpdate};
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86OpHint, X86RepMode, X86StringKind, X86VecAlign, X86X87DataKind,
};
use crate::smir::ir::types::{
    Address, ArchReg, ArmReg, BlockId, HexagonReg, MemWidth, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator};

// ============================================================================
// Optimization Level
// ============================================================================

/// Optimization level
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OptLevel {
    /// No optimization (for debugging)
    #[default]
    O0,

    /// Basic optimizations (fast compile, some speedup)
    O1,

    /// Full optimization (slower compile, best runtime)
    O2,
}

// ============================================================================
// Optimization Statistics
// ============================================================================

/// Statistics from optimization passes
#[derive(Clone, Debug, Default)]
pub struct OptStats {
    /// Dead flag updates eliminated
    pub dead_flags_eliminated: usize,

    /// Constants propagated
    pub constants_propagated: usize,

    /// Expressions folded
    pub expressions_folded: usize,

    /// Dead ops eliminated
    pub dead_ops_eliminated: usize,

    /// Strength reductions applied
    pub strength_reductions: usize,

    /// Blocks merged
    pub blocks_merged: usize,

    /// Redundant loads eliminated
    pub redundant_loads_eliminated: usize,

    /// Vector alignment hints inferred
    pub vector_alignments_inferred: usize,

    /// Copy-propagation operand rewrites
    pub copies_propagated: usize,

    /// Branch foldings / unreachable blocks removed
    pub branches_folded: usize,
}

impl OptStats {
    /// Create new empty stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge stats from another run
    pub fn merge(&mut self, other: &OptStats) {
        self.dead_flags_eliminated += other.dead_flags_eliminated;
        self.constants_propagated += other.constants_propagated;
        self.expressions_folded += other.expressions_folded;
        self.dead_ops_eliminated += other.dead_ops_eliminated;
        self.strength_reductions += other.strength_reductions;
        self.blocks_merged += other.blocks_merged;
        self.redundant_loads_eliminated += other.redundant_loads_eliminated;
        self.vector_alignments_inferred += other.vector_alignments_inferred;
        self.copies_propagated += other.copies_propagated;
        self.branches_folded += other.branches_folded;
    }

    /// Total optimizations applied
    pub fn total(&self) -> usize {
        self.dead_flags_eliminated
            + self.constants_propagated
            + self.expressions_folded
            + self.dead_ops_eliminated
            + self.strength_reductions
            + self.blocks_merged
            + self.redundant_loads_eliminated
            + self.vector_alignments_inferred
            + self.copies_propagated
            + self.branches_folded
    }
}

// ============================================================================
// Liveness analysis (registers + flags), frontier-aware
// ============================================================================
//
// The SMIR optimizer runs on *regions* that the JIT (or the differential
// harness) hands back to the interpreter at their exits. At such an exit EVERY
// architectural register and flag is live-out: the interpreter resumes and may
// read any of them. Only an *internal* `Branch`/`CondBranch`/`Switch` edge
// (whose targets are blocks present in this function) propagates a precise
// live set from the successor. Treating block boundaries as "nothing live"
// (the classic compiler default) would let DCE / dead-flag-elim delete the
// final architectural writes — which is why this analysis exists and why every
// flag-effect-dropping transform is gated on the op's flags already being dead.
//
// x86 partial-register semantics: a write of width >= 32 bits zero-extends and
// thus *fully* overwrites the 64-bit GPR; an 8/16-bit write preserves the upper
// bits and is therefore read-modify-write (it keeps the prior definition live).

/// Destination width of an integer op, when it has architecturally-meaningful
/// width (used for x86 partial-register liveness). `None` for ops without a
/// single integer result width (vectors, memory, etc.).
fn op_out_width(kind: &OpKind) -> Option<OpWidth> {
    match kind {
        OpKind::Add { width, .. }
        | OpKind::Sub { width, .. }
        | OpKind::Adc { width, .. }
        | OpKind::Sbb { width, .. }
        | OpKind::Neg { width, .. }
        | OpKind::Inc { width, .. }
        | OpKind::Dec { width, .. }
        | OpKind::And { width, .. }
        | OpKind::Or { width, .. }
        | OpKind::Xor { width, .. }
        | OpKind::AndNot { width, .. }
        | OpKind::Not { width, .. }
        | OpKind::Shl { width, .. }
        | OpKind::Shr { width, .. }
        | OpKind::Sar { width, .. }
        | OpKind::Shld { width, .. }
        | OpKind::Shrd { width, .. }
        | OpKind::X86NddDoubleShift { width, .. }
        | OpKind::Rol { width, .. }
        | OpKind::Ror { width, .. }
        | OpKind::Rcl { width, .. }
        | OpKind::Rcr { width, .. }
        | OpKind::MulU { width, .. }
        | OpKind::MulS { width, .. }
        | OpKind::DivU { width, .. }
        | OpKind::DivS { width, .. }
        | OpKind::Mov { width, .. }
        | OpKind::CMove { width, .. }
        | OpKind::Cwd { width, .. }
        | OpKind::Bsf { width, .. }
        | OpKind::Bsr { width, .. }
        | OpKind::Clz { width, .. }
        | OpKind::Ctz { width, .. }
        | OpKind::Popcnt { width, .. }
        | OpKind::X86Count { width, .. }
        | OpKind::Bswap { width, .. }
        | OpKind::Bt { width, .. }
        | OpKind::Bts { width, .. }
        | OpKind::Btr { width, .. }
        | OpKind::Btc { width, .. } => Some(*width),
        // ZeroExtend / SignExtend write the *destination* (to) width.
        OpKind::ZeroExtend { to_width, .. } | OpKind::SignExtend { to_width, .. } => {
            Some(*to_width)
        }
        // LEA computes a full pointer; SETcc writes a single byte.
        OpKind::Lea { .. } => Some(OpWidth::W64),
        OpKind::SetCC { .. } => Some(OpWidth::W8),
        OpKind::VBitSelect { width, .. } => match width {
            VecWidth::V64 => Some(OpWidth::W64),
            VecWidth::V128 => Some(OpWidth::W128),
            _ => None,
        },
        _ => None,
    }
}

/// True if executing `op` fully overwrites every architectural register it
/// defines, so an earlier definition of the same register becomes dead.
/// Conservative: returns false when unsure (the register stays live — this is
/// the safe direction; it can only cost a missed optimization, never delete a
/// live definition).
fn op_fully_defines(kind: &OpKind) -> bool {
    let dests = kind.dests();
    if dests.is_empty() {
        return true;
    }
    // SSA virtual temporaries are defined in full by their (single) writer.
    if dests.iter().all(|d| matches!(d, VReg::Virtual(_))) {
        return true;
    }
    matches!(
        op_out_width(kind),
        Some(OpWidth::W32) | Some(OpWidth::W64) | Some(OpWidth::W128)
    )
}

/// Registers read by a terminator (used at the block's exit point).
fn terminator_reg_uses(term: &Terminator) -> Vec<VReg> {
    let mut v = Vec::new();
    match term {
        Terminator::CondBranch { cond, .. } => v.push(*cond),
        Terminator::Switch { index, .. } => v.push(*index),
        Terminator::IndirectBranch { target, .. } => v.push(*target),
        Terminator::IndirectBranchMem { addr, .. } => v.extend(addr.regs()),
        Terminator::Return { values } => v.extend(values.iter().copied()),
        Terminator::Call { target, args, .. } | Terminator::TailCall { target, args } => {
            if let CallTarget::Indirect(reg) = target {
                v.push(*reg);
            }
            if let CallTarget::IndirectMem(addr) = target {
                v.extend(addr.regs());
            }
            v.extend(args.iter().copied());
        }
        _ => {}
    }
    v
}

/// Does this terminator hand control out of the region (back to the
/// interpreter, a callee, or an unknown target)? Anything that is not an
/// internal branch whose every target is a block present in `func`.
fn terminator_is_exit(func: &SmirFunction, term: &Terminator) -> bool {
    let in_func = |id: BlockId| func.blocks.iter().any(|b| b.id == id);
    match term {
        Terminator::Branch { target } => !in_func(*target),
        Terminator::CondBranch {
            true_target,
            false_target,
            ..
        } => !in_func(*true_target) || !in_func(*false_target),
        Terminator::Switch {
            targets, default, ..
        } => !in_func(*default) || targets.iter().any(|t| !in_func(*t)),
        // Indirect branches (incomplete target lists), calls (escape to a
        // callee), tail calls, traps, returns, unreachable: all exits.
        _ => true,
    }
}

/// Per-block live-out sets after a frontier-aware backward dataflow fixpoint.
struct FuncLiveness {
    reg_out: HashMap<BlockId, HashSet<VReg>>,
    flag_out: HashMap<BlockId, FlagSet>,
}

/// Backward transfer through one block: given the live-out reg/flag sets,
/// returns the live-in sets. Handles x86 partial-register RMW.
fn block_transfer(
    block: &SmirBlock,
    mut rlive: HashSet<VReg>,
    mut flive: FlagSet,
) -> (HashSet<VReg>, FlagSet) {
    for op in block.ops.iter().rev() {
        let full = op_fully_defines(&op.kind);
        let dests = op.kind.dests();
        if full {
            for d in &dests {
                rlive.remove(d);
            }
        }
        for s in op.kind.source_vregs() {
            rlive.insert(s);
        }
        if !full {
            // Partial-width write reads the destination it merges into.
            for d in &dests {
                rlive.insert(*d);
            }
        }
        flive = flive
            .difference(op.kind.flags_must_write())
            .union(op.kind.flags_read());
    }
    (rlive, flive)
}

/// Compute per-block register + flag live-out for a function, with all
/// architectural state live at frontier exits.
fn compute_liveness(func: &SmirFunction) -> FuncLiveness {
    // Universe of architectural registers touched anywhere in the function —
    // the set that is live-out at any region exit.
    let mut universe: HashSet<VReg> = HashSet::new();
    let mut note = |v: VReg, set: &mut HashSet<VReg>| {
        if matches!(v, VReg::Arch(_)) {
            set.insert(v);
        }
    };
    for block in &func.blocks {
        for op in &block.ops {
            for d in op.kind.dests() {
                note(d, &mut universe);
            }
            for s in op.kind.source_vregs() {
                note(s, &mut universe);
            }
        }
        for u in terminator_reg_uses(&block.terminator) {
            note(u, &mut universe);
        }
    }

    let mut reg_in: HashMap<BlockId, HashSet<VReg>> =
        func.blocks.iter().map(|b| (b.id, HashSet::new())).collect();
    let mut flag_in: HashMap<BlockId, FlagSet> =
        func.blocks.iter().map(|b| (b.id, FlagSet::EMPTY)).collect();

    // Iterate to fixpoint (live sets grow monotonically).
    let max_iters = func.blocks.len() + 2;
    for _ in 0..max_iters {
        let mut changed = false;
        for block in &func.blocks {
            let mut rout: HashSet<VReg> = HashSet::new();
            let mut fout = FlagSet::EMPTY;
            if terminator_is_exit(func, &block.terminator) {
                rout.extend(universe.iter().copied());
                fout = FlagSet::ALL_X86;
            }
            for s in block.terminator.successors() {
                if let Some(ri) = reg_in.get(&s) {
                    rout.extend(ri.iter().copied());
                }
                if let Some(fi) = flag_in.get(&s) {
                    fout = fout.union(*fi);
                }
            }
            for u in terminator_reg_uses(&block.terminator) {
                rout.insert(u);
            }
            let (rin, fin) = block_transfer(block, rout, fout);
            if reg_in[&block.id] != rin {
                reg_in.insert(block.id, rin);
                changed = true;
            }
            if flag_in[&block.id] != fin {
                flag_in.insert(block.id, fin);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Materialize live-out per block from the converged live-in sets.
    let mut reg_out = HashMap::new();
    let mut flag_out = HashMap::new();
    for block in &func.blocks {
        let mut rout: HashSet<VReg> = HashSet::new();
        let mut fout = FlagSet::EMPTY;
        if terminator_is_exit(func, &block.terminator) {
            rout.extend(universe.iter().copied());
            fout = FlagSet::ALL_X86;
        }
        for s in block.terminator.successors() {
            if let Some(ri) = reg_in.get(&s) {
                rout.extend(ri.iter().copied());
            }
            if let Some(fi) = flag_in.get(&s) {
                fout = fout.union(*fi);
            }
        }
        for u in terminator_reg_uses(&block.terminator) {
            rout.insert(u);
        }
        reg_out.insert(block.id, rout);
        flag_out.insert(block.id, fout);
    }

    FuncLiveness { reg_out, flag_out }
}

// ============================================================================
// Main Optimization Entry Point
// ============================================================================

/// Run optimization pipeline on a function
pub fn optimize_function(func: &mut SmirFunction, level: OptLevel) -> OptStats {
    optimize_function_with_stats(func, level)
}

/// Run optimization pipeline on a function, returning statistics.
///
/// Block-level passes are run to a fixpoint (they enable one another and change
/// liveness); liveness is recomputed each round so dead-flag and dead-code
/// elimination always see correct, frontier-aware live-out sets. This is the
/// only entry point that is safe to use on JIT regions / against KVM — the bare
/// per-block passes assume a caller-supplied live-out and must not be used
/// directly on architectural regions.
pub fn optimize_function_with_stats(func: &mut SmirFunction, level: OptLevel) -> OptStats {
    let mut stats = OptStats::new();
    if level == OptLevel::O0 {
        return stats;
    }
    let o2 = level == OptLevel::O2;

    let max_rounds = 8;
    for _ in 0..max_rounds {
        let live = compute_liveness(func);
        let mut round_changes = 0usize;
        for block in &mut func.blocks {
            let flag_out = live
                .flag_out
                .get(&block.id)
                .copied()
                .unwrap_or(FlagSet::ALL_X86);
            let empty_regs;
            let reg_out = match live.reg_out.get(&block.id) {
                Some(r) => r,
                None => {
                    empty_regs = HashSet::new();
                    &empty_regs
                }
            };

            let n = dead_flag_elimination_with(block, flag_out);
            stats.dead_flags_eliminated += n;
            round_changes += n;

            let n = constant_propagation(block);
            stats.constants_propagated += n;
            round_changes += n;

            let n = copy_propagation(block);
            stats.copies_propagated += n;
            round_changes += n;

            if o2 {
                let n = constant_folding(block);
                stats.expressions_folded += n;
                round_changes += n;

                let n = strength_reduction(block);
                stats.strength_reductions += n;
                round_changes += n;
            }

            let n = dead_code_elimination_with(block, reg_out);
            stats.dead_ops_eliminated += n;
            round_changes += n;
        }

        if o2 {
            let n = branch_folding(func);
            stats.branches_folded += n;
            round_changes += n;

            let n = block_merging(func);
            stats.blocks_merged += n;
            round_changes += n;

            let n = redundant_load_elimination(func);
            stats.redundant_loads_eliminated += n;
            round_changes += n;
        }

        if round_changes == 0 {
            break;
        }
    }

    if o2 {
        // Hint-only pass (no IR mutation) — run once at the end.
        stats.vector_alignments_inferred += vector_alignment_inference(func);
    }

    stats
}

// ============================================================================
// Dead Flag Elimination
// ============================================================================

/// Eliminate dead flag updates.
///
/// This is the most impactful optimization for x86 - removes flag updates that
/// are never read. Uses backward analysis to find live flags.
///
/// Returns the number of flag updates eliminated.
pub fn dead_flag_elimination(block: &mut SmirBlock) -> usize {
    // Bare per-block use: approximate live-out from the terminator only (a
    // CondBranch needs the status flags; any other terminator is assumed to
    // leave no flag live). This is the legacy block-local contract; JIT regions
    // must go through `optimize_function`, which supplies a frontier-aware
    // live-out via `dead_flag_elimination_with`.
    let live_out = if matches!(block.terminator, Terminator::CondBranch { .. }) {
        FlagSet::NZCV
    } else {
        FlagSet::EMPTY
    };
    dead_flag_elimination_with(block, live_out)
}

/// Eliminate dead flag updates given the flags live on block exit.
///
/// A flag-writing op has its `FlagUpdate` cleared to `None` when none of the
/// flags it writes are live after it — either because a later op in this block
/// overwrites them before any read, or because they are not in `live_out`.
/// Returns the number of flag updates eliminated.
pub fn dead_flag_elimination_with(block: &mut SmirBlock, live_out: FlagSet) -> usize {
    if block.ops.is_empty() {
        return 0;
    }

    // Backward pass: liveness[i] = flags live immediately AFTER op i.
    let mut liveness = vec![FlagSet::EMPTY; block.ops.len()];
    let mut current_live = live_out;
    for i in (0..block.ops.len()).rev() {
        liveness[i] = current_live;
        let op = &block.ops[i];
        let reads = op.kind.flags_read();
        // Only flags DEFINITELY written kill upstream liveness.
        let kills = op.kind.flags_must_write();
        // live_in = (live_out - must_write) | reads
        current_live = current_live.difference(kills).union(reads);
    }

    // Forward pass: eliminate dead flag updates.
    let mut eliminated = 0;
    for i in 0..block.ops.len() {
        let live = liveness[i];
        if let Some(flags) = block.ops[i].kind.flags_written_mut() {
            let written = flags.as_set();
            if !written.is_empty() && live.intersection(written).is_empty() {
                *flags = FlagUpdate::None;
                eliminated += 1;
            }
        }
    }

    eliminated
}

// ============================================================================
// Constant Propagation
// ============================================================================

/// Constant propagation within a block.
///
/// Tracks known constant values through the block and replaces register
/// operands with immediate values when possible.
///
/// Returns the number of constants propagated.
pub fn constant_propagation(block: &mut SmirBlock) -> usize {
    // Tracked values are the FULL architectural register value (for x86, a W32
    // definition zero-extends, so we mask to 32 bits and store the
    // zero-extended 64-bit value). Partial-width (8/16-bit) definitions leave
    // the upper bits unknown, so they are NOT tracked.
    let mut constants: HashMap<VReg, i64> = HashMap::new();
    let mut propagated = 0;

    // Mask a value to a width (the register value the op produces / sees).
    fn m(v: i64, w: OpWidth) -> i64 {
        ((v as u64) & w.mask()) as i64
    }
    // Only W32/W64 definitions fully overwrite the destination register.
    fn trackable(w: OpWidth) -> bool {
        matches!(w, OpWidth::W32 | OpWidth::W64)
    }

    for op in &mut block.ops {
        // Discriminants read before the mutable borrow of `op.kind` below.
        let alu = alu_tag(&op.kind);
        let is_shl = matches!(op.kind, OpKind::Shl { .. });
        match &mut op.kind {
            OpKind::Mov { dst, src, width } => {
                if let SrcOperand::Imm(imm) = src {
                    if trackable(*width) {
                        constants.insert(*dst, m(*imm, *width));
                    } else {
                        constants.remove(dst);
                    }
                } else if let SrcOperand::Reg(r) = src {
                    if let Some(&val) = constants.get(r) {
                        *src = SrcOperand::Imm(m(val, *width));
                        propagated += 1;
                        if trackable(*width) {
                            constants.insert(*dst, m(val, *width));
                        } else {
                            constants.remove(dst);
                        }
                    } else {
                        constants.remove(dst);
                    }
                } else {
                    constants.remove(dst);
                }
            }

            OpKind::Add {
                dst,
                src1,
                src2,
                width,
                ..
            }
            | OpKind::Sub {
                dst,
                src1,
                src2,
                width,
                ..
            }
            | OpKind::And {
                dst,
                src1,
                src2,
                width,
                ..
            }
            | OpKind::Or {
                dst,
                src1,
                src2,
                width,
                ..
            }
            | OpKind::Xor {
                dst,
                src1,
                src2,
                width,
                ..
            } => {
                // Substitute a known constant for the register second operand.
                if let SrcOperand::Reg(r) = src2 {
                    if let Some(&val) = constants.get(r) {
                        *src2 = SrcOperand::Imm(m(val, *width));
                        propagated += 1;
                    }
                }
                // Fold the result if both operands are now known constants.
                let folded = if let (Some(&v1), SrcOperand::Imm(v2)) = (constants.get(src1), &*src2)
                {
                    let a = (v1 as u64) & width.mask();
                    let b = (*v2 as u64) & width.mask();
                    let r = match alu {
                        AluTag::Add => a.wrapping_add(b),
                        AluTag::Sub => a.wrapping_sub(b),
                        AluTag::And => a & b,
                        AluTag::Or => a | b,
                        AluTag::Xor => a ^ b,
                    } & width.mask();
                    Some(r as i64)
                } else {
                    None
                };
                match (folded, trackable(*width)) {
                    (Some(r), true) => {
                        constants.insert(*dst, r);
                    }
                    _ => {
                        constants.remove(dst);
                    }
                }
            }

            OpKind::Shl {
                dst,
                src,
                amount,
                width,
                ..
            }
            | OpKind::Shr {
                dst,
                src,
                amount,
                width,
                ..
            } => {
                if let SrcOperand::Reg(r) = amount {
                    if let Some(&val) = constants.get(r) {
                        *amount = SrcOperand::Imm(val);
                        propagated += 1;
                    }
                }
                let folded = if let (Some(&v), SrcOperand::Imm(a)) = (constants.get(src), &*amount)
                {
                    let count_mask = (width.bits() - 1) as u64;
                    let cnt = (*a as u64) & count_mask;
                    let base = (v as u64) & width.mask();
                    let r = if is_shl { base << cnt } else { base >> cnt } & width.mask();
                    Some(r as i64)
                } else {
                    None
                };
                match (folded, trackable(*width)) {
                    (Some(r), true) => {
                        constants.insert(*dst, r);
                    }
                    _ => {
                        constants.remove(dst);
                    }
                }
            }

            OpKind::Load { dst, .. } | OpKind::AtomicLoad { dst, .. } => {
                // Loads produce unknown values.
                constants.remove(dst);
            }

            _ => {
                // For other ops, invalidate destinations.
                for dst in op.kind.dests() {
                    constants.remove(&dst);
                }
            }
        }
    }

    propagated
}

/// Small discriminant for the ALU constant-fold in `constant_propagation`.
#[derive(Clone, Copy)]
enum AluTag {
    Add,
    Sub,
    And,
    Or,
    Xor,
}

fn alu_tag(kind: &OpKind) -> AluTag {
    match kind {
        OpKind::Sub { .. } => AluTag::Sub,
        OpKind::And { .. } => AluTag::And,
        OpKind::Or { .. } => AluTag::Or,
        OpKind::Xor { .. } => AluTag::Xor,
        // Add and anything else (the tag is only consulted in the ALU arm).
        _ => AluTag::Add,
    }
}

// ============================================================================
// Constant Folding
// ============================================================================

/// Fold constant expressions at compile time.
///
/// Evaluates operations where all operands are constants and replaces them
/// with simple moves.
///
/// Returns the number of expressions folded.
pub fn constant_folding(block: &mut SmirBlock) -> usize {
    let mut folded = 0;

    for i in 0..block.ops.len() {
        // Rewrites that turn a flag-setting op into a flag-less `Mov` are only
        // legal when the op's flags are dead (`FlagUpdate::None`, established by
        // `dead_flag_elimination`). Shift-by-0 is exempt: x86 leaves flags
        // untouched on a zero count, so the `Mov` is flag-equivalent regardless.
        let new_kind = match &block.ops[i].kind {
            // Add with two immediates
            OpKind::Add {
                dst,
                src1,
                src2: SrcOperand::Imm(v2),
                width,
                flags,
            } if matches!(src1, VReg::Imm(..)) && matches!(flags, FlagUpdate::None) => {
                if let VReg::Imm(v1) = src1 {
                    let result = ((*v1 as u64).wrapping_add(*v2 as u64)) & width.mask();
                    Some(OpKind::Mov {
                        dst: *dst,
                        src: SrcOperand::Imm(result as i64),
                        width: *width,
                    })
                } else {
                    None
                }
            }

            // Sub with two immediates
            OpKind::Sub {
                dst,
                src1,
                src2: SrcOperand::Imm(v2),
                width,
                flags,
            } if matches!(src1, VReg::Imm(..)) && matches!(flags, FlagUpdate::None) => {
                if let VReg::Imm(v1) = src1 {
                    let result = ((*v1 as u64).wrapping_sub(*v2 as u64)) & width.mask();
                    Some(OpKind::Mov {
                        dst: *dst,
                        src: SrcOperand::Imm(result as i64),
                        width: *width,
                    })
                } else {
                    None
                }
            }

            // And with zero -> 0
            OpKind::And {
                dst,
                src2: SrcOperand::Imm(0),
                width,
                flags,
                ..
            } if matches!(flags, FlagUpdate::None) => Some(OpKind::Mov {
                dst: *dst,
                src: SrcOperand::Imm(0),
                width: *width,
            }),

            // And with -1 (all bits) -> mov src1
            OpKind::And {
                dst,
                src1,
                src2: SrcOperand::Imm(-1),
                width,
                flags,
            } if matches!(flags, FlagUpdate::None) => Some(OpKind::Mov {
                dst: *dst,
                src: SrcOperand::Reg(*src1),
                width: *width,
            }),

            // Or with zero -> mov src1
            OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Imm(0),
                width,
                flags,
            } if matches!(flags, FlagUpdate::None) => Some(OpKind::Mov {
                dst: *dst,
                src: SrcOperand::Reg(*src1),
                width: *width,
            }),

            // Xor with zero -> mov src1
            OpKind::Xor {
                dst,
                src1,
                src2: SrcOperand::Imm(0),
                width,
                flags,
            } if matches!(flags, FlagUpdate::None) => Some(OpKind::Mov {
                dst: *dst,
                src: SrcOperand::Reg(*src1),
                width: *width,
            }),

            // Xor of same register -> 0
            OpKind::Xor {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width,
                flags,
            } if src1 == src2 && matches!(flags, FlagUpdate::None) => Some(OpKind::Mov {
                dst: *dst,
                src: SrcOperand::Imm(0),
                width: *width,
            }),

            // Sub of same register -> 0
            OpKind::Sub {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width,
                flags,
            } if src1 == src2 && matches!(flags, FlagUpdate::None) => Some(OpKind::Mov {
                dst: *dst,
                src: SrcOperand::Imm(0),
                width: *width,
            }),

            // And/Or of a register with itself -> mov src1 (idempotent).
            OpKind::And {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width,
                flags,
            }
            | OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width,
                flags,
            } if src1 == src2 && matches!(flags, FlagUpdate::None) => Some(OpKind::Mov {
                dst: *dst,
                src: SrcOperand::Reg(*src1),
                width: *width,
            }),

            // Multiply by 1 -> mov src1 (no high half, flags dead).
            OpKind::MulU {
                dst_lo,
                dst_hi: None,
                src1,
                src2: SrcOperand::Imm(1),
                width,
                flags: FlagUpdate::None,
            }
            | OpKind::MulS {
                dst_lo,
                dst_hi: None,
                src1,
                src2: SrcOperand::Imm(1),
                width,
                flags: FlagUpdate::None,
            } => Some(OpKind::Mov {
                dst: *dst_lo,
                src: SrcOperand::Reg(*src1),
                width: *width,
            }),

            // Multiply by 0 -> mov 0 (no high half, flags dead).
            OpKind::MulU {
                dst_lo,
                dst_hi: None,
                src2: SrcOperand::Imm(0),
                width,
                flags: FlagUpdate::None,
                ..
            }
            | OpKind::MulS {
                dst_lo,
                dst_hi: None,
                src2: SrcOperand::Imm(0),
                width,
                flags: FlagUpdate::None,
                ..
            } => Some(OpKind::Mov {
                dst: *dst_lo,
                src: SrcOperand::Imm(0),
                width: *width,
            }),

            // Shift by zero -> mov src (flags untouched on x86 zero count).
            OpKind::Shl {
                dst,
                src,
                amount: SrcOperand::Imm(0),
                width,
                ..
            }
            | OpKind::Shr {
                dst,
                src,
                amount: SrcOperand::Imm(0),
                width,
                ..
            }
            | OpKind::Sar {
                dst,
                src,
                amount: SrcOperand::Imm(0),
                width,
                ..
            } => Some(OpKind::Mov {
                dst: *dst,
                src: SrcOperand::Reg(*src),
                width: *width,
            }),

            // Add zero -> mov src1
            OpKind::Add {
                dst,
                src1,
                src2: SrcOperand::Imm(0),
                width,
                flags,
            } if matches!(flags, FlagUpdate::None) => Some(OpKind::Mov {
                dst: *dst,
                src: SrcOperand::Reg(*src1),
                width: *width,
            }),

            // Sub zero -> mov src1
            OpKind::Sub {
                dst,
                src1,
                src2: SrcOperand::Imm(0),
                width,
                flags,
            } if matches!(flags, FlagUpdate::None) => Some(OpKind::Mov {
                dst: *dst,
                src: SrcOperand::Reg(*src1),
                width: *width,
            }),

            // VPTERNLOG truth-table projections. With index bits ordered as
            // (src1, src2, src3), AA/CC/F0 select one input unchanged.
            OpKind::X86TernaryLogic {
                dst,
                src1,
                src2,
                src3,
                mask: None,
                imm,
                width,
                ..
            } if matches!(imm, 0xAA | 0xCC | 0xF0) => Some(OpKind::VMov {
                dst: *dst,
                src: match imm {
                    0xAA => *src3,
                    0xCC => *src2,
                    0xF0 => *src1,
                    _ => unreachable!(),
                },
                width: *width,
            }),

            // Immediate packed rotates and funnel shifts reduce their counts
            // modulo the element width. A reduced zero count is an exact copy.
            OpKind::X86PackedRotate {
                dst,
                src,
                count: None,
                mask: None,
                amount,
                width,
                elem,
                ..
            } if u32::from(*amount) % (elem.bytes() * 8) == 0 => Some(OpKind::VMov {
                dst: *dst,
                src: *src,
                width: *width,
            }),

            OpKind::X86PackedFunnelShift {
                dst,
                src,
                count: None,
                mask: None,
                amount,
                width,
                elem,
                ..
            } if u32::from(*amount) % (elem.bytes() * 8) == 0 => Some(OpKind::VMov {
                dst: *dst,
                src: *src,
                width: *width,
            }),

            _ => None,
        };

        if let Some(new_kind) = new_kind {
            block.ops[i].kind = new_kind;
            folded += 1;
        }
    }

    folded
}

// ============================================================================
// Dead Code Elimination
// ============================================================================

/// Eliminate dead code.
///
/// Removes operations whose results are never used and have no side effects.
///
/// Returns the number of operations eliminated.
pub fn dead_code_elimination(block: &mut SmirBlock) -> usize {
    // Bare per-block use: seed only from the terminator (legacy contract). JIT
    // regions must go through `optimize_function`, which supplies a
    // frontier-aware register live-out via `dead_code_elimination_with`.
    dead_code_elimination_with(block, &HashSet::new())
}

/// Eliminate dead operations given the registers live on block exit.
///
/// An op is kept when any of its destinations is still used downstream (live),
/// or it has memory/side-effects, or it still writes a live flag (after
/// `dead_flag_elimination` has cleared the dead ones). x86 partial-register
/// writes are treated as read-modify-write so they keep the prior definition
/// live. Returns the number of operations removed.
pub fn dead_code_elimination_with(block: &mut SmirBlock, live_out: &HashSet<VReg>) -> usize {
    // Values used by something we must keep, seeded with the live-out set and
    // the terminator's own register uses.
    let mut used: HashSet<VReg> = live_out.clone();
    for u in terminator_reg_uses(&block.terminator) {
        used.insert(u);
    }

    // Backward pass to find all used values.
    for op in block.ops.iter().rev() {
        let dests = op.kind.dests();
        let dest_live = dests.is_empty() || dests.iter().any(|d| used.contains(d));
        let keep = dest_live || op.kind.has_side_effects() || !op.kind.flags_written().is_empty();

        if keep {
            for src in op.kind.source_vregs() {
                used.insert(src);
            }
            // A partial-width write merges into (reads) its destination.
            if !op_fully_defines(&op.kind) {
                for d in &dests {
                    used.insert(*d);
                }
            }
        }
    }

    // Remove ops that are neither live, side-effecting, nor flag-producing.
    let before = block.ops.len();
    block.ops.retain(|op| {
        let dests = op.kind.dests();
        dests.is_empty()
            || dests.iter().any(|d| used.contains(d))
            || op.kind.has_side_effects()
            || !op.kind.flags_written().is_empty()
    });

    before - block.ops.len()
}

// ============================================================================
// Strength Reduction
// ============================================================================

/// Strength reduction transformations.
///
/// Replaces expensive operations with cheaper equivalents:
/// - Multiply by power of 2 -> shift left
/// - Unsigned divide by power of 2 -> shift right
///
/// Returns the number of reductions applied.
pub fn strength_reduction(block: &mut SmirBlock) -> usize {
    let mut reductions = 0;

    for op in &mut block.ops {
        let new_kind = match &op.kind {
            // Multiply by power of 2 -> shift. Only legal when there is no
            // high-half result to produce (`dst_hi == None`) and the multiply's
            // flags are dead (a shift's CF/OF differ from MUL/IMUL's), which
            // `dead_flag_elimination` establishes as `FlagUpdate::None`.
            OpKind::MulU {
                dst_lo,
                dst_hi: None,
                src1,
                src2: SrcOperand::Imm(imm),
                width,
                flags: FlagUpdate::None,
            }
            | OpKind::MulS {
                dst_lo,
                dst_hi: None,
                src1,
                src2: SrcOperand::Imm(imm),
                width,
                flags: FlagUpdate::None,
            } if *imm > 0 && (*imm as u64).is_power_of_two() => {
                let shift = (*imm as u64).trailing_zeros() as i64;
                Some(OpKind::Shl {
                    dst: *dst_lo,
                    src: *src1,
                    amount: SrcOperand::Imm(shift),
                    width: *width,
                    flags: FlagUpdate::None,
                })
            }

            // Unsigned divide by power of 2 -> shift right. Only legal when the
            // remainder is not needed (`rem == None`); the quotient of an
            // unsigned divide by 2^k is exactly `src >> k`.
            OpKind::DivU {
                quot,
                rem: None,
                src1,
                src2: SrcOperand::Imm(imm),
                width,
                flags: FlagUpdate::None,
            } if *imm > 0 && (*imm as u64).is_power_of_two() => {
                let shift = (*imm as u64).trailing_zeros() as i64;
                Some(OpKind::Shr {
                    dst: *quot,
                    src: *src1,
                    amount: SrcOperand::Imm(shift),
                    width: *width,
                    flags: FlagUpdate::None,
                })
            }

            _ => None,
        };

        if let Some(new_kind) = new_kind {
            op.kind = new_kind;
            reductions += 1;
        }
    }

    reductions
}

// ============================================================================
// Copy Propagation
// ============================================================================

/// Apply `f` to every PURE-SOURCE register operand of `kind` (operands that are
/// read but never also written — so never a destination or read-modify-write
/// field). Returns how many operands changed. Address operands and RMW fields
/// (Shld/Shrd dst, Xchg, CMove dst, accumulators) are intentionally left
/// untouched: a missed rewrite only forgoes an optimization, never changes
/// semantics.
fn rewrite_pure_src_vregs(kind: &mut OpKind, f: &dyn Fn(VReg) -> VReg) -> usize {
    let mut n = 0usize;
    let mut do_v = |v: &mut VReg, n: &mut usize| {
        let nv = f(*v);
        if nv != *v {
            *v = nv;
            *n += 1;
        }
    };
    let mut do_s = |s: &mut SrcOperand, n: &mut usize| {
        if let SrcOperand::Reg(r) = s {
            let nv = f(*r);
            if nv != *r {
                *s = SrcOperand::Reg(nv);
                *n += 1;
            }
        }
    };
    match kind {
        OpKind::Add { src1, src2, .. }
        | OpKind::Sub { src1, src2, .. }
        | OpKind::Adc { src1, src2, .. }
        | OpKind::Sbb { src1, src2, .. }
        | OpKind::And { src1, src2, .. }
        | OpKind::Or { src1, src2, .. }
        | OpKind::Xor { src1, src2, .. }
        | OpKind::AndNot { src1, src2, .. }
        | OpKind::Cmp { src1, src2, .. }
        | OpKind::Test { src1, src2, .. }
        | OpKind::MulU { src1, src2, .. }
        | OpKind::MulS { src1, src2, .. }
        | OpKind::DivU { src1, src2, .. }
        | OpKind::DivS { src1, src2, .. } => {
            do_v(src1, &mut n);
            do_s(src2, &mut n);
        }
        OpKind::Mov { src, .. } => do_s(src, &mut n),
        OpKind::Neg { src, .. }
        | OpKind::Inc { src, .. }
        | OpKind::Dec { src, .. }
        | OpKind::Not { src, .. }
        | OpKind::Cwd { src, .. }
        | OpKind::Bsf { src, .. }
        | OpKind::Bsr { src, .. }
        | OpKind::Clz { src, .. }
        | OpKind::Ctz { src, .. }
        | OpKind::Popcnt { src, .. }
        | OpKind::X86Count { src, .. }
        | OpKind::Bswap { src, .. }
        | OpKind::Rbit { src, .. }
        | OpKind::ZeroExtend { src, .. }
        | OpKind::SignExtend { src, .. }
        | OpKind::Truncate { src, .. } => do_v(src, &mut n),
        OpKind::Shl { src, amount, .. }
        | OpKind::Shr { src, amount, .. }
        | OpKind::Sar { src, amount, .. }
        | OpKind::Rol { src, amount, .. }
        | OpKind::Ror { src, amount, .. }
        | OpKind::Rcl { src, amount, .. }
        | OpKind::Rcr { src, amount, .. } => {
            do_v(src, &mut n);
            do_s(amount, &mut n);
        }
        // Shld/Shrd: `dst` is read-modify-write (skip); `src` and `amount` are
        // pure sources.
        OpKind::Shld { src, amount, .. } | OpKind::Shrd { src, amount, .. } => {
            do_v(src, &mut n);
            do_s(amount, &mut n);
        }
        OpKind::X86NddDoubleShift {
            base, fill, amount, ..
        } => {
            do_v(base, &mut n);
            do_v(fill, &mut n);
            do_s(amount, &mut n);
        }
        OpKind::CMove { src, .. } => do_v(src, &mut n),
        OpKind::Bt { src, index, .. }
        | OpKind::Bts { src, index, .. }
        | OpKind::Btr { src, index, .. }
        | OpKind::Btc { src, index, .. } => {
            do_v(src, &mut n);
            do_s(index, &mut n);
        }
        OpKind::Select {
            cond,
            src_true,
            src_false,
            ..
        } => {
            do_v(cond, &mut n);
            do_v(src_true, &mut n);
            do_v(src_false, &mut n);
        }
        _ => {}
    }
    n
}

/// Copy propagation within a block.
///
/// For `mov dst, reg(src)` (a full-width register copy), later pure-source uses
/// of `dst` are rewritten to `src` until `dst` or `src` is redefined. This
/// turns the very common lifted pattern `mov vtmp, r; OP _, vtmp` into
/// `OP _, r`, letting dead-code elimination drop the now-unused copy.
///
/// Returns the number of operand rewrites performed.
pub fn copy_propagation(block: &mut SmirBlock) -> usize {
    // `copies[d] = s` means "register d currently holds the same value as s".
    let mut copies: HashMap<VReg, VReg> = HashMap::new();
    let mut count = 0;

    for op in &mut block.ops {
        // 1) Rewrite pure-source uses through the copy map.
        if !copies.is_empty() {
            let map = &copies;
            count += rewrite_pure_src_vregs(&mut op.kind, &|v| map.get(&v).copied().unwrap_or(v));
        }

        // 2) Invalidate copies killed by this op's destinations.
        let dests = op.kind.dests();
        if !dests.is_empty() {
            for d in &dests {
                copies.remove(d);
            }
            copies.retain(|_, val| !dests.contains(val));
        }

        // 3) Record a new register copy. ONLY W64 moves give full-register
        //    equality (`rcx == rax`); a W32 `mov ecx, eax` yields
        //    `ecx == zero_extend(low32(eax))`, which is not equal to `eax` when
        //    its upper bits are set, so substituting it into a 64-bit use would
        //    be wrong. Restrict to W64 to stay correct regardless of use width.
        if let OpKind::Mov {
            dst,
            src: SrcOperand::Reg(s),
            width: OpWidth::W64,
        } = &op.kind
        {
            if dst != s {
                copies.insert(*dst, *s);
            }
        }
    }

    count
}

// ============================================================================
// Branch Folding
// ============================================================================

/// Fold degenerate conditional branches and drop unreachable blocks.
///
/// - A `CondBranch` whose two targets are identical becomes an unconditional
///   `Branch` (the condition no longer matters).
/// - Blocks not reachable from the entry are removed (they can arise after
///   constant folding / block merging).
///
/// Returns the number of transformations applied.
pub fn branch_folding(func: &mut SmirFunction) -> usize {
    let mut changes = 0;

    // Same-target conditional branches -> unconditional.
    for block in &mut func.blocks {
        if let Terminator::CondBranch {
            true_target,
            false_target,
            ..
        } = &block.terminator
        {
            if true_target == false_target {
                let target = *true_target;
                block.set_terminator(Terminator::Branch { target });
                changes += 1;
            }
        }
    }

    // Reachability from the entry.
    let mut reachable: HashSet<BlockId> = HashSet::new();
    let mut stack = vec![func.entry];
    while let Some(id) = stack.pop() {
        if !reachable.insert(id) {
            continue;
        }
        if let Some(b) = func.blocks.iter().find(|b| b.id == id) {
            for s in b.terminator.successors() {
                if !reachable.contains(&s) {
                    stack.push(s);
                }
            }
        }
    }
    let before = func.blocks.len();
    func.blocks.retain(|b| reachable.contains(&b.id));
    changes += before - func.blocks.len();

    changes
}

// ============================================================================
// Block Merging
// ============================================================================

/// Merge adjacent blocks with unconditional jumps.
///
/// When a block ends with an unconditional branch to a block with only one
/// predecessor, merge them together.
///
/// Returns the number of blocks merged.
pub fn block_merging(func: &mut SmirFunction) -> usize {
    if func.blocks.len() < 2 {
        return 0;
    }

    // Build predecessor count
    let mut pred_count: HashMap<BlockId, usize> = HashMap::new();

    for block in &func.blocks {
        match &block.terminator {
            Terminator::Branch { target } => {
                *pred_count.entry(*target).or_default() += 1;
            }
            Terminator::CondBranch {
                true_target,
                false_target,
                ..
            } => {
                *pred_count.entry(*true_target).or_default() += 1;
                *pred_count.entry(*false_target).or_default() += 1;
            }
            Terminator::Switch {
                targets, default, ..
            } => {
                for target in targets {
                    *pred_count.entry(*target).or_default() += 1;
                }
                *pred_count.entry(*default).or_default() += 1;
            }
            _ => {}
        }
    }

    // Find blocks to merge
    let mut merge_pairs: Vec<(BlockId, BlockId)> = Vec::new();

    for block in &func.blocks {
        if let Terminator::Branch { target } = &block.terminator {
            // Only merge if target has single predecessor
            if pred_count.get(target) == Some(&1) && *target != block.id {
                merge_pairs.push((block.id, *target));
            }
        }
    }

    let merged_count = merge_pairs.len();

    // Perform merges
    for (from, to) in merge_pairs {
        let from_idx = func.blocks.iter().position(|b| b.id == from);
        let to_idx = func.blocks.iter().position(|b| b.id == to);

        if let (Some(from_idx), Some(to_idx)) = (from_idx, to_idx) {
            // Get ops and terminator from target block
            let to_ops = func.blocks[to_idx].ops.clone();
            let to_term = func.blocks[to_idx].terminator.clone();

            // Append to source block
            func.blocks[from_idx].ops.extend(to_ops);
            func.blocks[from_idx].terminator = to_term;

            // Mark target block for removal
            func.blocks[to_idx].ops.clear();
            func.blocks[to_idx].terminator = Terminator::Unreachable;
        }
    }

    // Remove empty blocks (but keep entry block)
    func.blocks.retain(|b| {
        b.id == func.entry || !b.ops.is_empty() || !matches!(b.terminator, Terminator::Unreachable)
    });

    merged_count
}

// ============================================================================
// Redundant Load Elimination
// ============================================================================

/// Eliminate redundant loads.
///
/// When a value is loaded from memory and the same address is loaded again
/// (without an intervening store), replace the second load with a move.
///
/// Returns the number of redundant loads eliminated.
pub fn redundant_load_elimination(func: &mut SmirFunction) -> usize {
    let mut eliminated = 0;

    for block in &mut func.blocks {
        eliminated += redundant_load_elimination_block(block);
    }

    eliminated
}

// ============================================================================
// Vector Alignment Inference
// ============================================================================

/// Infer vector alignment hints for VLoad/VStore ops.
pub fn vector_alignment_inference(func: &mut SmirFunction) -> usize {
    let mut inferred = 0;

    for block in &mut func.blocks {
        inferred += vector_alignment_inference_block(block);
    }

    inferred
}

fn vector_alignment_inference_block(block: &mut SmirBlock) -> usize {
    let mut inferred = 0;
    let mut alignments = seed_x86_alignments();

    for op in &mut block.ops {
        inferred += apply_vec_align_hint(op, &alignments);
        update_pointer_alignment(op, &mut alignments);
    }

    inferred
}

fn seed_x86_alignments() -> HashMap<VReg, usize> {
    let mut alignments = HashMap::new();
    alignments.insert(VReg::Arch(ArchReg::X86(X86Reg::Rsp)), 16);
    alignments.insert(VReg::Arch(ArchReg::X86(X86Reg::Rbp)), 16);
    alignments
}

fn apply_vec_align_hint(op: &mut SmirOp, alignments: &HashMap<VReg, usize>) -> usize {
    let (addr, width) = match &op.kind {
        OpKind::VLoad { addr, width, .. } | OpKind::VStore { addr, width, .. } => (addr, width),
        _ => return 0,
    };

    match op.x86_hint {
        None | Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)) => {}
        _ => return 0,
    }

    let required = vec_width_bytes(*width);
    if let Some(alignment) = address_alignment(addr, alignments) {
        if alignment >= required {
            op.x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
            return 1;
        }
    }

    0
}

fn update_pointer_alignment(op: &SmirOp, alignments: &mut HashMap<VReg, usize>) {
    let mut computed = HashMap::new();

    match &op.kind {
        OpKind::Mov { dst, src, width } if *width == OpWidth::W64 => {
            if let Some(src_reg) = src.as_reg() {
                if let Some(&alignment) = alignments.get(&src_reg) {
                    computed.insert(*dst, alignment);
                }
            } else if let Some(imm) = src.as_imm() {
                if imm >= 0 {
                    computed.insert(*dst, alignment_from_addr(imm as u64));
                }
            }
        }
        OpKind::Add {
            dst,
            src1,
            src2,
            width,
            ..
        }
        | OpKind::Sub {
            dst,
            src1,
            src2,
            width,
            ..
        } if *width == OpWidth::W64 => {
            if let Some(&src_align) = alignments.get(src1) {
                if let Some(imm) = src2.as_imm() {
                    computed.insert(*dst, gcd(src_align, imm.unsigned_abs() as usize));
                } else if let Some(src2_reg) = src2.as_reg() {
                    if let Some(&src2_align) = alignments.get(&src2_reg) {
                        computed.insert(*dst, gcd(src_align, src2_align));
                    }
                }
            }
        }
        OpKind::Shl {
            dst,
            src,
            amount,
            width,
            ..
        } if *width == OpWidth::W64 => {
            if let (Some(&src_align), Some(shift)) = (alignments.get(src), amount.as_imm()) {
                if let Ok(shift) = u32::try_from(shift) {
                    if let Some(alignment) = src_align.checked_shl(shift) {
                        computed.insert(*dst, alignment);
                    }
                }
            }
        }
        OpKind::And {
            dst,
            src1,
            src2,
            width,
            ..
        } if *width == OpWidth::W64 => {
            if let Some(imm) = src2.as_imm() {
                let mask = imm as u64;
                let mut alignment = if mask == 0 {
                    1
                } else {
                    1usize << mask.trailing_zeros()
                };
                if let Some(&src_align) = alignments.get(src1) {
                    alignment = alignment.max(src_align);
                }
                computed.insert(*dst, alignment);
            }
        }
        OpKind::CMove {
            dst, src, width, ..
        } if *width == OpWidth::W64 => {
            if let (Some(&dst_align), Some(&src_align)) = (alignments.get(dst), alignments.get(src))
            {
                computed.insert(*dst, gcd(dst_align, src_align));
            }
        }
        OpKind::Select {
            dst,
            src_true,
            src_false,
            width,
            ..
        } if *width == OpWidth::W64 => {
            if let (Some(&a), Some(&b)) = (alignments.get(src_true), alignments.get(src_false)) {
                computed.insert(*dst, gcd(a, b));
            }
        }
        OpKind::Lea { dst, addr } => {
            if let Some(alignment) = address_alignment(addr, alignments) {
                computed.insert(*dst, alignment);
            }
        }
        _ => {}
    }

    for dst in op.kind.dests() {
        if let Some(&alignment) = computed.get(&dst) {
            alignments.insert(dst, alignment);
        } else {
            alignments.remove(&dst);
        }
    }
}

fn vec_width_bytes(width: VecWidth) -> usize {
    match width {
        VecWidth::V64 => 8,
        VecWidth::V128 => 16,
        VecWidth::V256 => 32,
        VecWidth::V512 => 64,
    }
}

fn address_alignment(addr: &Address, alignments: &HashMap<VReg, usize>) -> Option<usize> {
    match addr {
        Address::Direct(base) => alignments.get(base).copied(),
        Address::BaseOffset { base, offset, .. } => {
            let base_align = alignments.get(base).copied()?;
            Some(gcd(base_align, offset.unsigned_abs() as usize))
        }
        Address::BaseIndexScale {
            base,
            index,
            scale,
            disp,
            ..
        } => {
            let index_align = alignments.get(index).copied()?;
            let scaled = index_align.checked_mul(*scale as usize)?;
            let mut alignment = scaled;
            if let Some(base_reg) = base {
                let base_align = alignments.get(base_reg).copied()?;
                alignment = gcd(alignment, base_align);
            }
            alignment = gcd(alignment, (*disp as i64).unsigned_abs() as usize);
            Some(alignment)
        }
        Address::PcRel { offset, base, .. } => {
            let base_addr = match base {
                Some(base_addr) => *base_addr as i128,
                None => return None,
            };
            let target = base_addr + *offset as i128;
            if target < 0 {
                None
            } else {
                Some(alignment_from_addr(target as u64))
            }
        }
        Address::Absolute(addr) => Some(alignment_from_addr(*addr)),
        _ => None,
    }
}

fn alignment_from_addr(addr: u64) -> usize {
    if addr == 0 {
        return 1;
    }
    1usize << addr.trailing_zeros()
}

fn gcd(mut a: usize, mut b: usize) -> usize {
    if a == 0 {
        return b;
    }
    if b == 0 {
        return a;
    }
    while b != 0 {
        let tmp = a % b;
        a = b;
        b = tmp;
    }
    a
}

fn redundant_load_elimination_block(block: &mut SmirBlock) -> usize {
    // Track what's currently in registers from memory
    // Key: (base_vreg, offset, width), Value: VReg holding the loaded value
    let mut mem_to_reg: HashMap<(Option<VReg>, i64, MemWidth), VReg> = HashMap::new();
    let mut eliminated = 0;

    let mut new_ops = Vec::new();

    for op in &block.ops {
        match &op.kind {
            OpKind::Load {
                dst,
                addr,
                width,
                sign: _,
            } => {
                // Only loads from a key-able address (Direct/BaseOffset/Absolute)
                // are candidates. Complex addresses (BaseIndexScale, PcRel) are
                // NOT tracked — a single sentinel key would make distinct
                // addresses (e.g. [rsi+rdx-16] vs [rsi+rdx-8]) collide and
                // wrongly forward one load's value to the other.
                if let Some(key) = address_key(addr, *width) {
                    if let Some(&existing) = mem_to_reg.get(&key) {
                        new_ops.push(SmirOp {
                            id: op.id,
                            guest_pc: op.guest_pc,
                            kind: OpKind::Mov {
                                dst: *dst,
                                src: SrcOperand::Reg(existing),
                                width: width.to_op_width().unwrap_or(OpWidth::W64),
                            },
                            x86_hint: None,
                        });
                        eliminated += 1;
                    } else {
                        mem_to_reg.insert(key, *dst);
                        new_ops.push(op.clone());
                    }
                } else {
                    new_ops.push(op.clone());
                }
            }

            // Any op that writes memory may alias an arbitrary cached address, so
            // conservatively drop every cached load. This is keyed off
            // `writes_memory()` rather than an explicit op list so that store-like
            // ops cannot silently regress by falling through to the default arm:
            // the prior explicit list omitted PredStore, RepStos, RepMovs,
            // StorePair, VStore, and RvVector, any of which could leave a stale
            // cached load alive across a memory write. (#112)
            other if other.writes_memory() => {
                mem_to_reg.clear();
                new_ops.push(op.clone());
            }

            // I/O ports and syscalls don't write guest RAM through a tracked
            // address, and a fence imposes ordering on prior writes — none are
            // covered by `writes_memory()`, but all may have arbitrary memory side
            // effects, so be conservative and also drop the cache.
            OpKind::Fence { .. }
            | OpKind::IoIn { .. }
            | OpKind::IoOut { .. }
            | OpKind::Syscall { .. } => {
                mem_to_reg.clear();
                new_ops.push(op.clone());
            }

            _ => {
                new_ops.push(op.clone());
            }
        }

        // Invalidate any cached load whose BASE register this op redefines: once
        // the base changes, the cached `(base, offset)` no longer names the same
        // memory (e.g. `load [rsi-8]; lea rsi,[rsi-32]; load [rsi-8]` are two
        // different addresses). Without this, the second load would be wrongly
        // forwarded from the first.
        for d in op.kind.dests() {
            mem_to_reg.retain(|key, _| key.0 != Some(d));
        }
    }

    block.ops = new_ops;
    eliminated
}

/// Create a key for memory-address tracking, or `None` for addresses we do not
/// track (complex forms whose equality we cannot cheaply decide). Returning
/// `None` — never a shared sentinel — is what keeps distinct untracked
/// addresses from aliasing each other.
fn address_key(addr: &Address, width: MemWidth) -> Option<(Option<VReg>, i64, MemWidth)> {
    match addr {
        Address::Direct(r) => Some((Some(*r), 0, width)),
        Address::BaseOffset { base, offset, .. } => Some((Some(*base), *offset, width)),
        Address::Absolute(a) => Some((None, *a as i64, width)),
        _ => None,
    }
}

// ============================================================================
// OpKind Helper Methods for Optimization
// ============================================================================

impl OpKind {
    /// Get mutable reference to flag update field
    pub fn flags_written_mut(&mut self) -> Option<&mut FlagUpdate> {
        match self {
            OpKind::Add { flags, .. }
            | OpKind::Sub { flags, .. }
            | OpKind::Adc { flags, .. }
            | OpKind::Sbb { flags, .. }
            | OpKind::Neg { flags, .. }
            | OpKind::Inc { flags, .. }
            | OpKind::Dec { flags, .. }
            | OpKind::And { flags, .. }
            | OpKind::Or { flags, .. }
            | OpKind::Xor { flags, .. }
            | OpKind::AndNot { flags, .. }
            | OpKind::Shl { flags, .. }
            | OpKind::Shr { flags, .. }
            | OpKind::Sar { flags, .. }
            | OpKind::Shld { flags, .. }
            | OpKind::Shrd { flags, .. }
            | OpKind::X86NddDoubleShift { flags, .. }
            | OpKind::Rol { flags, .. }
            | OpKind::Ror { flags, .. }
            | OpKind::Rcl { flags, .. }
            | OpKind::Rcr { flags, .. }
            | OpKind::Bsf { flags, .. }
            | OpKind::Bsr { flags, .. }
            | OpKind::X86Count { flags, .. }
            | OpKind::Bextr { flags, .. }
            | OpKind::Bzhi { flags, .. }
            | OpKind::MulU { flags, .. }
            | OpKind::MulS { flags, .. } => Some(flags),
            _ => None,
        }
    }

    /// Get the flags written by this operation (the flags it may define).
    pub fn flags_written(&self) -> FlagSet {
        match self {
            OpKind::Add { flags, .. }
            | OpKind::Sub { flags, .. }
            | OpKind::Adc { flags, .. }
            | OpKind::Sbb { flags, .. }
            | OpKind::Neg { flags, .. }
            | OpKind::And { flags, .. }
            | OpKind::Or { flags, .. }
            | OpKind::Xor { flags, .. }
            | OpKind::AndNot { flags, .. }
            | OpKind::Shl { flags, .. }
            | OpKind::Shr { flags, .. }
            | OpKind::Sar { flags, .. }
            | OpKind::Shld { flags, .. }
            | OpKind::Shrd { flags, .. }
            | OpKind::X86NddDoubleShift { flags, .. }
            | OpKind::Bsf { flags, .. }
            | OpKind::Bsr { flags, .. }
            | OpKind::X86Count { flags, .. }
            | OpKind::Bextr { flags, .. }
            | OpKind::Bzhi { flags, .. }
            | OpKind::MulU { flags, .. }
            | OpKind::MulS { flags, .. } => flags.as_set(),

            OpKind::Rol { flags, .. }
            | OpKind::Ror { flags, .. }
            | OpKind::Rcl { flags, .. }
            | OpKind::Rcr { flags, .. } => {
                flags.as_set().intersection(FlagSet::CF.union(FlagSet::OF))
            }

            // INC/DEC update OF/SF/ZF/AF/PF but PRESERVE CF (their defining
            // difference from ADD/SUB by 1). Never report CF as written.
            OpKind::Inc { flags, .. } | OpKind::Dec { flags, .. } => {
                flags.as_set().difference(FlagSet::CF)
            }

            // Cmp, Test, and CMPccXADD update all x86 arithmetic flags.
            OpKind::Cmp { .. } | OpKind::Test { .. } | OpKind::AtomicCmpXadd { .. } => {
                FlagSet::ALL_X86
            }

            OpKind::X86FpCompare { .. } => FlagSet::ALL_X86,

            OpKind::X86Cmpxchg8b16b { .. } => FlagSet::ZF,

            OpKind::X86Random { .. } => FlagSet::ALL_X86,

            OpKind::X86X87Data {
                kind: X86X87DataKind::Compare { eflags: true, .. },
                ..
            } => FlagSet::ALL_X86,

            // Bit test updates CF
            OpKind::Bt { .. } => FlagSet::CF,

            // SCAS and CMPS may update all arithmetic flags. With REP, an
            // initial count of zero performs no comparison and preserves them.
            OpKind::X86String {
                kind: X86StringKind::Scas | X86StringKind::Cmps,
                ..
            } => FlagSet::ALL_X86,

            OpKind::SetCF { .. } | OpKind::CmcCF => FlagSet::CF,

            _ => FlagSet::EMPTY,
        }
    }

    /// Flags this op DEFINITELY writes a defined value to, regardless of its
    /// operands — the set safe to treat as "killed" (overwritten) in backward
    /// flag-liveness. Conservatively smaller than `flags_written` for ops whose
    /// flag effect is operand-conditional or partly undefined: a shift/rotate
    /// by a variable count writes nothing when the count is 0, and MUL/IMUL and
    /// BSF/BSR leave most flags undefined. Using a smaller must-write set can
    /// only keep more upstream flags live (safe), never delete a needed one.
    pub fn flags_must_write(&self) -> FlagSet {
        match self {
            OpKind::Shl { .. }
            | OpKind::Shr { .. }
            | OpKind::Sar { .. }
            | OpKind::Shld { .. }
            | OpKind::Shrd { .. }
            | OpKind::X86NddDoubleShift { .. }
            | OpKind::Rol { .. }
            | OpKind::Ror { .. }
            | OpKind::Rcl { .. }
            | OpKind::Rcr { .. }
            | OpKind::MulU { .. }
            | OpKind::MulS { .. }
            | OpKind::Bsf { .. }
            | OpKind::Bsr { .. } => FlagSet::EMPTY,
            OpKind::X86String {
                kind: X86StringKind::Scas | X86StringKind::Cmps,
                rep,
                ..
            } if *rep != X86RepMode::None => FlagSet::EMPTY,
            OpKind::X86X87Data {
                kind: X86X87DataKind::Compare { eflags: true, .. },
                ..
            } => FlagSet::OF.union(FlagSet::SF).union(FlagSet::AF),
            _ => self.flags_written(),
        }
    }

    /// Get the flags read by this operation
    pub fn flags_read(&self) -> FlagSet {
        match self {
            // Add/Sub with carry read CF
            OpKind::Adc { .. } | OpKind::Sbb { .. } | OpKind::Rcl { .. } | OpKind::Rcr { .. } => {
                FlagSet::CF
            }

            // Conditional move reads flags based on condition
            OpKind::CMove { cond, .. } | OpKind::SetCC { cond, .. } => {
                FlagState::required_flags(*cond)
            }

            // TestCondition reads flags
            OpKind::TestCondition { cond, .. } => FlagState::required_flags(*cond),

            OpKind::X86X87Data {
                kind: X86X87DataKind::ConditionalMove(cond),
                ..
            } => FlagState::required_flags(*cond),

            // AArch64 conditional-compare lifting materializes ArmReg::Nzcv
            // from the immediately preceding flag-producing compare op.
            OpKind::Mov {
                src: SrcOperand::Reg(VReg::Arch(ArchReg::Arm(ArmReg::Nzcv))),
                ..
            } => FlagSet::NZCV,

            // Complement carry reads CF
            OpKind::CmcCF => FlagSet::CF,

            // ReadFlags reads all flags
            OpKind::ReadFlags { .. } => FlagSet::ALL_X86,

            _ => FlagSet::EMPTY,
        }
    }

    /// Get source registers used by this operation
    pub fn source_vregs(&self) -> Vec<VReg> {
        let mut result = Vec::new();

        match self {
            OpKind::Add { src1, src2, .. }
            | OpKind::Sub { src1, src2, .. }
            | OpKind::Adc { src1, src2, .. }
            | OpKind::Sbb { src1, src2, .. }
            | OpKind::And { src1, src2, .. }
            | OpKind::Or { src1, src2, .. }
            | OpKind::Xor { src1, src2, .. }
            | OpKind::AndNot { src1, src2, .. }
            | OpKind::Cmp { src1, src2, .. }
            | OpKind::Test { src1, src2, .. } => {
                result.push(*src1);
                if let SrcOperand::Reg(r) = src2 {
                    result.push(*r);
                }
            }

            OpKind::Shld {
                dst: src1,
                src: src3,
                amount: src2,
                ..
            }
            | OpKind::Shrd {
                dst: src1,
                src: src3,
                amount: src2,
                ..
            } => {
                result.push(*src1);
                result.push(*src3);
                if let SrcOperand::Reg(r) = src2 {
                    result.push(*r);
                }
            }

            OpKind::X86NddDoubleShift {
                base, fill, amount, ..
            } => {
                result.push(*base);
                result.push(*fill);
                if let SrcOperand::Reg(r) = amount {
                    result.push(*r);
                }
            }

            OpKind::MulU { src1, src2, .. }
            | OpKind::MulS { src1, src2, .. }
            | OpKind::DivU { src1, src2, .. }
            | OpKind::DivS { src1, src2, .. } => {
                result.push(*src1);
                if let SrcOperand::Reg(r) = src2 {
                    result.push(*r);
                }
            }

            OpKind::MulAdd {
                acc, src1, src2, ..
            }
            | OpKind::MulSub {
                acc, src1, src2, ..
            } => {
                result.push(*acc);
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::Bextr { src, control, .. } => {
                result.push(*src);
                result.push(*control);
            }

            OpKind::Bzhi { src, index, .. } => {
                result.push(*src);
                result.push(*index);
            }

            OpKind::Pdep { src, mask, .. } | OpKind::Pext { src, mask, .. } => {
                result.push(*src);
                result.push(*mask);
            }

            OpKind::Neg { src, .. }
            | OpKind::Inc { src, .. }
            | OpKind::Dec { src, .. }
            | OpKind::Not { src, .. }
            | OpKind::Cwd { src, .. }
            | OpKind::Bsf { src, .. }
            | OpKind::Bsr { src, .. }
            | OpKind::Clz { src, .. }
            | OpKind::Ctz { src, .. }
            | OpKind::Popcnt { src, .. }
            | OpKind::X86Count { src, .. }
            | OpKind::Bswap { src, .. }
            | OpKind::Rbit { src, .. } => {
                result.push(*src);
            }

            OpKind::Leave => {
                result.push(VReg::Arch(ArchReg::X86(X86Reg::Rbp)));
            }

            OpKind::Shl { src, amount, .. }
            | OpKind::Shr { src, amount, .. }
            | OpKind::Sar { src, amount, .. }
            | OpKind::Rol { src, amount, .. }
            | OpKind::Ror { src, amount, .. }
            | OpKind::Rcl { src, amount, .. }
            | OpKind::Rcr { src, amount, .. } => {
                result.push(*src);
                if let SrcOperand::Reg(r) = amount {
                    result.push(*r);
                }
            }

            // Bidirectional shift: both `src` and `amount` are SrcOperand.
            OpKind::BidirShift { src, amount, .. } => {
                if let SrcOperand::Reg(r) = src {
                    result.push(*r);
                }
                if let SrcOperand::Reg(r) = amount {
                    result.push(*r);
                }
            }

            // Saturating clamp: `src` is a SrcOperand (register or immediate).
            OpKind::SatN { src, .. } => {
                if let SrcOperand::Reg(r) = src {
                    result.push(*r);
                }
            }

            OpKind::Bt { src, index, .. }
            | OpKind::Bts { src, index, .. }
            | OpKind::Btr { src, index, .. }
            | OpKind::Btc { src, index, .. } => {
                result.push(*src);
                if let SrcOperand::Reg(r) = index {
                    result.push(*r);
                }
            }

            OpKind::Bfx { src, .. } => {
                result.push(*src);
            }

            OpKind::Bfi { src, dst_in, .. } => {
                result.push(*src);
                result.push(*dst_in);
            }

            OpKind::Mov { src, .. } => {
                if let SrcOperand::Reg(r) = src {
                    result.push(*r);
                }
            }

            OpKind::CMove { src, .. } => {
                result.push(*src);
            }

            OpKind::Select {
                cond,
                src_true,
                src_false,
                ..
            } => {
                result.push(*cond);
                result.push(*src_true);
                result.push(*src_false);
            }

            OpKind::ZeroExtend { src, .. }
            | OpKind::SignExtend { src, .. }
            | OpKind::Truncate { src, .. } => {
                result.push(*src);
            }

            OpKind::Lea { addr, .. } => {
                result.extend(addr.regs());
            }

            OpKind::Xchg { reg1, reg2, .. } => {
                result.push(*reg1);
                result.push(*reg2);
            }

            OpKind::Load { addr, .. }
            | OpKind::AtomicLoad { addr, .. }
            | OpKind::LoadExclusive { addr, .. } => {
                result.extend(addr.regs());
            }

            OpKind::Store { src, addr, .. } | OpKind::AtomicStore { src, addr, .. } => {
                result.push(*src);
                result.extend(addr.regs());
            }

            // Predicated load: reads the predicate `cond` and the address base
            // register(s). The `dst` is conditionally written (in dests()).
            OpKind::PredLoad { cond, addr, .. } => {
                result.push(*cond);
                result.extend(addr.regs());
            }

            // Predicated store: reads the predicate `cond`, the source operand
            // (when a register), and the address base register(s).
            OpKind::PredStore {
                src, cond, addr, ..
            } => {
                result.push(*cond);
                if let SrcOperand::Reg(r) = src {
                    result.push(*r);
                }
                result.extend(addr.regs());
            }

            OpKind::RepStos {
                dst, src, count, ..
            } => {
                result.push(*dst);
                result.push(*src);
                result.push(*count);
            }

            OpKind::RepMovs {
                dst, src, count, ..
            } => {
                result.push(*dst);
                result.push(*src);
                result.push(*count);
            }

            OpKind::LoadPair { addr, .. } => {
                result.extend(addr.regs());
            }

            OpKind::StorePair {
                src1, src2, addr, ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.extend(addr.regs());
            }

            OpKind::AtomicRmw { addr, src, .. } => {
                result.extend(addr.regs());
                result.push(*src);
            }

            OpKind::Cas {
                addr,
                expected,
                new_val,
                ..
            } => {
                result.extend(addr.regs());
                result.push(*expected);
                result.push(*new_val);
            }

            OpKind::AtomicCmpXadd { addr, cmp, add, .. } => {
                result.extend(addr.regs());
                result.push(*cmp);
                result.push(*add);
            }

            OpKind::StoreExclusive { src, addr, .. } => {
                result.push(*src);
                result.extend(addr.regs());
            }

            OpKind::IoIn { port, .. } => {
                result.push(*port);
            }

            OpKind::IoOut { port, value, .. } => {
                result.push(*port);
                result.push(*value);
            }

            OpKind::WriteFlags { src } | OpKind::WriteSysReg { src, .. } => {
                result.push(*src);
            }

            OpKind::Syscall { num, args } => {
                result.push(*num);
                result.extend(args.iter().copied());
            }

            // FP operations
            OpKind::FAdd { src1, src2, .. }
            | OpKind::FSub { src1, src2, .. }
            | OpKind::FMul { src1, src2, .. }
            | OpKind::FDiv { src1, src2, .. }
            | OpKind::FMin { src1, src2, .. }
            | OpKind::FMax { src1, src2, .. }
            | OpKind::FCmp { src1, src2, .. }
            | OpKind::X86FpCompare { src1, src2, .. }
            | OpKind::HexFp { src1, src2, .. }
            | OpKind::HexFpRecip { src1, src2, .. }
            | OpKind::HexCabacDecBin { src1, src2, .. }
            | OpKind::HexTlbMatch { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::FFma {
                src1, src2, src3, ..
            }
            | OpKind::HexFp3 {
                src1, src2, src3, ..
            }
            | OpKind::HexFpDf {
                src1, src2, src3, ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.push(*src3);
            }

            OpKind::HexFpScFma {
                src1,
                src2,
                src3,
                scale,
                ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.push(*src3);
                result.push(*scale);
            }

            OpKind::RvFp {
                src1,
                src2,
                src3,
                fcsr_src,
                ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.push(*src3);
                result.push(*fcsr_src);
            }

            OpKind::RvIntCrypto { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::RvVector {
                rs1, rs2, state, ..
            } => {
                result.push(*rs1);
                result.push(*rs2);
                result.extend(state.x_srcs.iter().copied().filter(|r| !r.is_imm()));
                result.extend(state.f_srcs.iter().copied().filter(|r| !r.is_imm()));
                result.extend(
                    [
                        state.fcsr_src,
                        state.vl_src,
                        state.vtype_src,
                        state.vstart_src,
                        state.vcsr_src,
                    ]
                    .into_iter()
                    .filter(|r| !r.is_imm()),
                );
            }

            OpKind::FAbs { src, .. }
            | OpKind::FNeg { src, .. }
            | OpKind::FSqrt { src, .. }
            | OpKind::FConvert { src, .. }
            | OpKind::IntToFp { src, .. }
            | OpKind::FpToInt { src, .. }
            | OpKind::X86FpToInt { src, .. }
            | OpKind::FRound { src, .. } => {
                result.push(*src);
            }

            OpKind::X86IntToFp { merge, src, .. } => {
                result.push(*merge);
                result.push(*src);
            }

            OpKind::X86FpConvert { merge, src, .. } => {
                result.push(*merge);
                result.push(*src);
            }

            OpKind::X86Round { merge, src, .. } => {
                result.push(*merge);
                result.push(*src);
            }

            OpKind::X86VectorFpCompare {
                src1, src2, mask, ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.extend(mask.iter().copied());
            }

            OpKind::X86PackedFpConvert { src, mask, .. } => {
                result.push(*src);
                result.extend(mask.iter().copied());
            }

            // Vector operations
            OpKind::VAdd { src1, src2, .. }
            | OpKind::VSub { src1, src2, .. }
            | OpKind::VAddSubSat { src1, src2, .. }
            | OpKind::VMax { src1, src2, .. }
            | OpKind::VX86MinMax { src1, src2, .. }
            | OpKind::VMul { src1, src2, .. }
            | OpKind::VDiv { src1, src2, .. }
            | OpKind::VLane { src1, src2, .. }
            | OpKind::VAnd { src1, src2, .. }
            | OpKind::VAndNot { src1, src2, .. }
            | OpKind::VOr { src1, src2, .. }
            | OpKind::VXor { src1, src2, .. }
            | OpKind::VFMinMaxNm { src1, src2, .. }
            | OpKind::VPermute2 { src1, src2, .. }
            | OpKind::VCmp { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::VBitSelect {
                mask,
                src_true,
                src_false,
                ..
            } => {
                result.push(*mask);
                result.push(*src_true);
                result.push(*src_false);
            }

            OpKind::VPermute {
                src1,
                src2,
                indices,
                ..
            } => {
                result.push(*src1);
                result.extend(src2.iter().copied());
                result.push(*indices);
            }

            OpKind::X86PermuteBytesWords {
                dst,
                table1,
                table2,
                indices,
                mask,
                zeroing,
                ..
            } => {
                if mask.is_some() && !zeroing {
                    result.push(*dst);
                }
                result.push(*table1);
                result.extend(table2.iter().copied());
                result.push(*indices);
                result.extend(mask.iter().copied());
            }

            OpKind::VMultiplyAdd52 {
                acc,
                src1,
                src2,
                mask,
                ..
            } => {
                result.push(*acc);
                result.push(*src1);
                result.push(*src2);
                result.extend(mask.iter().copied());
            }

            OpKind::VUnary { src, .. } | OpKind::VReduce { src, .. } => {
                result.push(*src);
            }

            OpKind::VConflict { src, mask, .. } => {
                result.push(*src);
                result.extend(mask.iter().copied());
            }

            OpKind::VPopcnt { src, mask, .. } | OpKind::VLeadingZeros { src, mask, .. } => {
                result.push(*src);
                result.extend(mask.iter().copied());
            }

            OpKind::X86Aes { src1, src2, .. } => {
                result.push(*src1);
                result.extend(src2.iter().copied());
            }

            OpKind::VTableLookup {
                dst,
                table,
                num_tables,
                index,
                is_tbx,
                ..
            } => {
                result.push(*index);
                // The table is `num_tables` consecutive registers from `table`.
                if let VReg::Arch(ArchReg::Arm(ArmReg::V(base))) = table {
                    for i in 0..*num_tables {
                        result.push(VReg::Arch(ArchReg::Arm(ArmReg::V((base + i) % 32))));
                    }
                } else {
                    result.push(*table);
                }
                // TBX reads the destination (out-of-range indices keep it).
                if *is_tbx {
                    result.push(*dst);
                }
            }

            OpKind::VShift { src, amount, .. } => {
                result.push(*src);
                if let SrcOperand::Reg(r) = amount {
                    result.push(*r);
                }
            }

            OpKind::VWidenMul {
                src1,
                src2,
                dst_lo,
                dst_hi,
                acc,
                ..
            }
            | OpKind::VWidenAddSub {
                src1,
                src2,
                dst_lo,
                dst_hi,
                acc,
                ..
            } => {
                result.push(*src1);
                result.push(*src2);
                if *acc {
                    // accumulating form reads the existing destination pair
                    result.push(*dst_lo);
                    result.push(*dst_hi);
                }
            }

            OpKind::VLaneUnary { src, .. } => {
                result.push(*src);
            }

            OpKind::VNavg { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::VShiftAcc {
                src, amount, dst, ..
            } => {
                result.push(*src);
                // shift-accumulate reads the existing destination lane
                result.push(*dst);
                if let SrcOperand::Reg(r) = amount {
                    result.push(*r);
                }
            }

            OpKind::VPairReduceMul {
                src_lo,
                src_hi,
                src2,
                dst_lo,
                dst_hi,
                acc,
                ..
            }
            | OpKind::VSlideReduceMul {
                src_lo,
                src_hi,
                src2,
                dst_lo,
                dst_hi,
                acc,
                ..
            }
            | OpKind::VRotReduceMulPair {
                src_lo,
                src_hi,
                src2,
                dst_lo,
                dst_hi,
                acc,
                ..
            } => {
                result.push(*src_lo);
                result.push(*src_hi);
                result.push(*src2);
                if *acc {
                    result.push(*dst_lo);
                    result.push(*dst_hi);
                }
            }

            OpKind::VPairPairReduceMul {
                src_lo,
                src_hi,
                src2_lo,
                src2_hi,
                ..
            } => {
                result.push(*src_lo);
                result.push(*src_hi);
                result.push(*src2_lo);
                result.push(*src2_hi);
            }

            OpKind::VReduceMul {
                src1,
                src2,
                dst,
                acc,
                ..
            }
            | OpKind::VMulEvenWiden {
                src1,
                src2,
                dst,
                acc,
                ..
            }
            | OpKind::VMulSubLane {
                src1,
                src2,
                dst,
                acc,
                ..
            } => {
                result.push(*src1);
                result.push(*src2);
                if *acc {
                    result.push(*dst);
                }
            }

            OpKind::VWidenExt { src, .. } => {
                result.push(*src);
            }

            OpKind::VLut {
                src_idx,
                table,
                sel,
                dst,
                oracc,
                ..
            } => {
                result.push(*src_idx);
                result.push(*table);
                if let SrcOperand::Reg(r) = sel {
                    result.push(*r);
                }
                if *oracc {
                    result.push(*dst);
                }
            }

            OpKind::VLut16 {
                src_idx,
                table,
                sel,
                dst_lo,
                dst_hi,
                oracc,
                ..
            } => {
                result.push(*src_idx);
                result.push(*table);
                if let SrcOperand::Reg(r) = sel {
                    result.push(*r);
                }
                if *oracc {
                    result.push(*dst_lo);
                    result.push(*dst_hi);
                }
            }

            OpKind::VShuffVdd {
                src_lo,
                src_hi,
                amount,
                ..
            }
            | OpKind::VDealVdd {
                src_lo,
                src_hi,
                amount,
                ..
            } => {
                result.push(*src_lo);
                result.push(*src_hi);
                if let SrcOperand::Reg(r) = amount {
                    result.push(*r);
                }
            }

            OpKind::VShuffleEOPair { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            // In-place dual-register shuffle/deal reads AND writes both Vy and Vx.
            OpKind::VShuffleDeal {
                dst_y,
                dst_x,
                amount,
                ..
            } => {
                result.push(*dst_y);
                result.push(*dst_x);
                if let SrcOperand::Reg(r) = amount {
                    result.push(*r);
                }
            }

            // vunpacko OR-accumulates the source into the existing dst pair.
            OpKind::VUnpackOAcc {
                src,
                dst_lo,
                dst_hi,
                ..
            } => {
                result.push(*src);
                result.push(*dst_lo);
                result.push(*dst_hi);
            }

            OpKind::VInsertWordR { dst, scalar } => {
                // read-modify-write: preserves the other words of dst.
                result.push(*dst);
                result.push(*scalar);
            }

            OpKind::VExtractWord { src, sel, .. } => {
                result.push(*src);
                result.push(*sel);
            }

            OpKind::VLut4 { src, table, .. } => {
                result.push(*src);
                result.push(*table);
            }

            OpKind::VRotr { src, amount, .. } => {
                result.push(*src);
                result.push(*amount);
            }

            OpKind::VAddSubMixedSat { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::VSetPredQ { scalar, .. } => {
                result.push(*scalar);
            }

            OpKind::VShuffEqQ { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            // vmpa(Vx, Vu, Rtt):sat reads the dst (Vx) accumulator, Vu, and Rtt.
            OpKind::VMpaHhSat {
                dst, src, table, ..
            } => {
                result.push(*dst);
                result.push(*src);
                result.push(*table);
            }

            // vmpyhsat_acc accumulates into the existing dst pair.
            OpKind::VMpyHsatAcc {
                dst_lo,
                dst_hi,
                src,
                scalar,
            } => {
                result.push(*dst_lo);
                result.push(*dst_hi);
                result.push(*src);
                result.push(*scalar);
            }

            // vasr_into shifts Vu into the running accumulator pair (read+write).
            OpKind::VAsrInto {
                dst_lo,
                dst_hi,
                src,
                amount,
            } => {
                result.push(*dst_lo);
                result.push(*dst_hi);
                result.push(*src);
                result.push(*amount);
            }

            OpKind::V6Mpy {
                src_lo,
                src_hi,
                src2_lo,
                src2_hi,
                dst_lo,
                dst_hi,
                acc,
                ..
            } => {
                result.push(*src_lo);
                result.push(*src_hi);
                result.push(*src2_lo);
                result.push(*src2_hi);
                if *acc {
                    result.push(*dst_lo);
                    result.push(*dst_hi);
                }
            }

            OpKind::VDelta { src, control, .. } => {
                result.push(*src);
                result.push(*control);
            }

            OpKind::VPack { src1, src2, .. }
            | OpKind::VPackSat { src1, src2, .. }
            | OpKind::VShuffleEO { src1, src2, .. }
            | OpKind::VDealB4W { src1, src2, .. }
            | OpKind::VMulSubLaneFrac { src1, src2, .. }
            | OpKind::VMulSubLaneSh { src1, src2, .. }
            | OpKind::VMulShiftSat { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::VMulWord64Pair {
                src1,
                src2,
                dst_lo,
                dst_hi,
                mode,
            } => {
                result.push(*src1);
                result.push(*src2);
                // mode 1 (vmpyowh_64_acc) reads the existing dst pair.
                if *mode == 1 {
                    result.push(*dst_lo);
                    result.push(*dst_hi);
                }
            }

            OpKind::VShuffle2 { src, .. } => {
                result.push(*src);
            }

            OpKind::VAlign {
                src1, src2, amount, ..
            } => {
                result.push(*src1);
                result.push(*src2);
                if let SrcOperand::Reg(r) = amount {
                    result.push(*r);
                }
            }

            OpKind::VShiftV { src, amount, .. } => {
                result.push(*src);
                result.push(*amount);
            }

            OpKind::VNarrowShiftSat {
                src_lo,
                src_hi,
                amount,
                ..
            } => {
                result.push(*src_lo);
                result.push(*src_hi);
                if let SrcOperand::Reg(r) = amount {
                    result.push(*r);
                }
            }

            OpKind::VSatDW { src_lo, src_hi, .. } => {
                result.push(*src_lo);
                result.push(*src_hi);
            }

            OpKind::VNarrowShiftV {
                src_lo,
                src_hi,
                amount,
                ..
            } => {
                result.push(*src_lo);
                result.push(*src_hi);
                result.push(*amount);
            }

            OpKind::VCmpToQ {
                dst,
                src1,
                src2,
                accumulate,
                ..
            } => {
                result.push(*src1);
                result.push(*src2);
                if accumulate.is_some() {
                    result.push(*dst);
                }
            }

            OpKind::VBlend {
                mask_q,
                src_true,
                src_false,
                ..
            } => {
                result.push(*mask_q);
                result.push(*src_true);
                result.push(*src_false);
            }

            OpKind::VMaskZero {
                mask_q,
                src,
                dst,
                oracc,
                ..
            } => {
                result.push(*mask_q);
                result.push(*src);
                // oracc (vandqrt_acc) OR-accumulates into the existing dst.
                if *oracc {
                    result.push(*dst);
                }
            }

            OpKind::VQFromVAndR {
                src1,
                src2,
                dst,
                oracc,
            } => {
                result.push(*src1);
                result.push(*src2);
                // oracc (vandvrt_acc) OR-accumulates into the existing dst Q.
                if *oracc {
                    result.push(*dst);
                }
            }

            // Q-predicated conditional add/sub: dst is read-modify-written.
            OpKind::VLaneCond {
                dst, src, mask_q, ..
            } => {
                result.push(*dst);
                result.push(*src);
                result.push(*mask_q);
            }

            // Carry add/sub: reads both vectors; reads the carry Q when it has a
            // carry-in (carry / carrysat forms).
            OpKind::VCarry {
                src1,
                src2,
                q_inout,
                has_cin,
                ..
            } => {
                result.push(*src1);
                result.push(*src2);
                if *has_cin {
                    result.push(*q_inout);
                }
            }

            OpKind::VSwap {
                mask_q, src1, src2, ..
            } => {
                result.push(*mask_q);
                result.push(*src1);
                result.push(*src2);
            }

            // Scalar-predicate-gated move/combine: when the gate is false the
            // dest(s) keep their prior value, so they are read; also reads the
            // predicate and the candidate sources.
            OpKind::VCondMove {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                pred,
                ..
            } => {
                result.push(*pred);
                result.push(*src_lo);
                result.push(*dst_lo);
                if let Some(hi) = dst_hi {
                    result.push(*src_hi);
                    result.push(*hi);
                }
            }

            OpKind::VPrefixSumQ { mask_q, .. } => {
                result.push(*mask_q);
            }

            // The histogram family read-modify-writes the WHOLE V0..V31 file and
            // reads the input vector from memory (the `.tmp` load's address) plus
            // the q-mask for the q-forms.
            OpKind::VHist {
                input,
                mask_q,
                use_q,
                ..
            } => {
                result.extend(input.regs());
                if *use_q {
                    result.push(*mask_q);
                }
                for n in 0..32u8 {
                    result.push(VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n))));
                }
            }

            OpKind::VMov { src, .. } | OpKind::VBroadcast { scalar: src, .. } => {
                result.push(*src);
            }

            OpKind::VInsertLane { vec, scalar, .. } => {
                result.push(*vec);
                result.push(*scalar);
            }

            OpKind::VExtractLane { vec, .. } => {
                result.push(*vec);
            }

            OpKind::VShuffle {
                src1,
                src2,
                indices,
                ..
            } => {
                result.push(*src1);
                if let Some(s2) = src2 {
                    result.push(*s2);
                }
                result.push(*indices);
            }

            OpKind::VByteShuffle { src, control, .. } => {
                result.push(*src);
                result.push(*control);
            }

            OpKind::VHorizontalBin { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::VLoad { addr, .. } => {
                result.extend(addr.regs());
            }

            OpKind::X86LoadMxcsr { addr } | OpKind::X86StoreMxcsr { addr } => {
                result.extend(addr.regs());
            }

            OpKind::X86CacheControl { addr, .. } | OpKind::X86CheckAlignment { addr, .. } => {
                result.extend(addr.regs());
            }

            OpKind::X86FxSave { addr, .. } | OpKind::X86FxRstor { addr, .. } => {
                result.extend(addr.regs());
            }

            OpKind::X86XSave {
                addr,
                src_low,
                src_high,
                ..
            }
            | OpKind::X86XRstor {
                addr,
                src_low,
                src_high,
                ..
            } => {
                result.extend(addr.regs());
                result.extend([*src_low, *src_high]);
            }

            OpKind::X86XGetBv { selector, .. } => result.push(*selector),
            OpKind::X86XSetBv {
                selector,
                src_low,
                src_high,
            } => result.extend([*selector, *src_low, *src_high]),

            OpKind::X86Cmpxchg8b16b {
                addr,
                compare_lo,
                compare_hi,
                new_lo,
                new_hi,
                ..
            } => {
                result.extend(addr.regs());
                result.extend([*compare_lo, *compare_hi, *new_lo, *new_hi]);
            }

            OpKind::X86X87Control {
                addr: Some(addr), ..
            } => {
                result.extend(addr.regs());
            }

            OpKind::X86X87Data {
                addr: Some(addr), ..
            } => {
                result.extend(addr.regs());
            }

            OpKind::VStore { src, addr, .. } => {
                result.push(*src);
                result.extend(addr.regs());
            }

            // Operations with no source registers
            OpKind::ReadFlags { .. }
            | OpKind::SetCF { .. }
            | OpKind::SetDF { .. }
            | OpKind::X86ReadTsc { .. }
            | OpKind::X86Random { .. }
            | OpKind::X86ReadPid { .. }
            | OpKind::X86X87Control { addr: None, .. }
            | OpKind::X86X87Data { addr: None, .. }
            | OpKind::CmcCF
            | OpKind::MaterializeFlags
            | OpKind::TestCondition { .. }
            | OpKind::SetCC { .. }
            | OpKind::ClearExclusive
            | OpKind::Prefetch { .. }
            | OpKind::Fence { .. }
            | OpKind::IoIn { .. }
            | OpKind::IoOut { .. }
            | OpKind::Swi { .. }
            | OpKind::ReadSysReg { .. }
            | OpKind::Nop
            | OpKind::Undefined { .. }
            | OpKind::Breakpoint => {}

            // AVX10 operations - extract source registers
            OpKind::VMin { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::VFma {
                src1, src2, acc, ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.push(*acc);
            }

            OpKind::VDotProductBF16 {
                acc,
                src1,
                src2,
                mask,
                ..
            } => {
                result.push(*acc);
                result.push(*src1);
                result.push(*src2);
                result.extend(mask.iter().copied());
            }

            OpKind::VDotProduct {
                acc,
                src1,
                src2,
                mask,
                ..
            } => {
                result.push(*acc);
                result.push(*src1);
                result.push(*src2);
                result.extend(mask.iter().copied());
            }

            OpKind::VMultiplyAdd52 {
                acc,
                src1,
                src2,
                mask,
                ..
            } => {
                result.push(*acc);
                result.push(*src1);
                result.push(*src2);
                result.extend(mask.iter().copied());
            }

            OpKind::VCvtBF16ToFP32 { src, .. } => {
                result.push(*src);
            }

            OpKind::VConflict { src, mask, .. } => {
                result.push(*src);
                result.extend(mask.iter().copied());
            }

            OpKind::VPopcnt { src, mask, .. } | OpKind::VLeadingZeros { src, mask, .. } => {
                result.push(*src);
                result.extend(mask.iter().copied());
            }

            OpKind::VPermute {
                src1,
                src2,
                indices,
                ..
            } => {
                result.push(*src1);
                if let Some(s2) = src2 {
                    result.push(*s2);
                }
                result.push(*indices);
            }

            OpKind::X86PermuteBytesWords {
                dst,
                table1,
                table2,
                indices,
                mask,
                zeroing,
                ..
            } => {
                if mask.is_some() && !zeroing {
                    result.push(*dst);
                }
                result.push(*table1);
                result.extend(table2.iter().copied());
                result.push(*indices);
                result.extend(mask.iter().copied());
            }

            OpKind::VShuffleBitQM {
                src, indices, mask, ..
            } => {
                result.push(*src);
                result.push(*indices);
                result.extend(mask.iter().copied());
            }

            OpKind::VCompress {
                dst,
                src,
                mask,
                zeroing,
                ..
            }
            | OpKind::VExpand {
                dst,
                src,
                mask,
                zeroing,
                ..
            } => {
                result.push(*src);
                if let Some(mask) = mask {
                    result.push(*mask);
                }
                if !zeroing {
                    result.push(*dst);
                }
            }

            OpKind::X86NarrowInt {
                dst,
                src,
                mask,
                zeroing,
                ..
            } => {
                result.push(*src);
                if let Some(mask) = mask {
                    result.push(*mask);
                }
                if !zeroing {
                    result.push(*dst);
                }
            }

            OpKind::VCvtFP32ToBF16 {
                dst,
                src1,
                src2,
                mask,
                zeroing,
                ..
            } => {
                result.push(*src1);
                if let Some(s2) = src2 {
                    result.push(*s2);
                }
                result.extend(mask.iter().copied());
                if mask.is_some() && !zeroing {
                    result.push(*dst);
                }
            }

            OpKind::VFP16Arith {
                dst,
                src1,
                src2,
                mask,
                zeroing,
                ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.extend(mask.iter().copied());
                if mask.is_some() && !zeroing {
                    result.push(*dst);
                }
            }

            OpKind::VMinMax { src1, src2, .. }
            | OpKind::VMpsadbw { src1, src2, .. }
            | OpKind::VSadBytes { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::X86Aes { src1, src2, .. } => {
                result.push(*src1);
                result.extend(src2.iter().copied());
            }

            OpKind::X86DotProduct { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::X86Sha512Msg1 { dst, src } | OpKind::X86Sha512Msg2 { dst, src } => {
                result.push(*dst);
                result.push(*src);
            }

            OpKind::X86Sha512Rounds2 { dst, state, wk } => {
                result.push(*dst);
                result.push(*state);
                result.push(*wk);
            }

            OpKind::X86Sm3Msg1 {
                dst, src1, src2, ..
            }
            | OpKind::X86Sm3Msg2 {
                dst, src1, src2, ..
            } => {
                result.push(*dst);
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::X86Sm3Rounds2 {
                dst, state, words, ..
            } => {
                result.push(*dst);
                result.push(*state);
                result.push(*words);
            }

            OpKind::X86Sm4 { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::X86Convert16ToFp32 { src, .. } => {
                result.push(*src);
            }

            OpKind::X86PackedShiftImm { src, .. } => {
                result.push(*src);
            }

            OpKind::X86PackedShift { src, count, .. } => {
                result.push(*src);
                result.push(*count);
            }
            OpKind::X86PackedShiftVariable {
                src, count, mask, ..
            } => {
                result.push(*src);
                result.push(*count);
                if let Some(mask) = mask {
                    result.push(*mask);
                }
            }

            OpKind::X86PackedRotate {
                src, count, mask, ..
            } => {
                result.push(*src);
                if let Some(count) = count {
                    result.push(*count);
                }
                if let Some(mask) = mask {
                    result.push(*mask);
                }
            }

            OpKind::X86TernaryLogic {
                src1,
                src2,
                src3,
                mask,
                ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.push(*src3);
                if let Some(mask) = mask {
                    result.push(*mask);
                }
            }

            OpKind::X86PackedFunnelShift {
                src,
                fill,
                count,
                mask,
                ..
            } => {
                result.push(*src);
                result.push(*fill);
                if let Some(count) = count {
                    result.push(*count);
                }
                if let Some(mask) = mask {
                    result.push(*mask);
                }
            }

            OpKind::X86MultiShiftQB {
                control,
                source,
                mask,
                ..
            } => {
                result.push(*control);
                result.push(*source);
                if let Some(mask) = mask {
                    result.push(*mask);
                }
            }

            OpKind::VCvtFpToIntSat { src, .. } => {
                result.push(*src);
            }

            OpKind::VDotProductExt {
                acc, src1, src2, ..
            } => {
                result.push(*acc);
                result.push(*src1);
                result.push(*src2);
            }

            // Carry-less multiply: src1/src2 are SrcOperands; the `_acc` forms
            // also read the existing dst/dst_hi (XOR target).
            OpKind::ClMul {
                src1,
                src2,
                dst,
                dst_hi,
                acc,
                ..
            } => {
                if let SrcOperand::Reg(r) = src1 {
                    result.push(*r);
                }
                if let SrcOperand::Reg(r) = src2 {
                    result.push(*r);
                }
                if *acc {
                    result.push(*dst);
                    if let Some(hi) = dst_hi {
                        result.push(*hi);
                    }
                }
            }

            OpKind::Crc32C { crc, data, .. } => {
                result.push(*crc);
                result.push(*data);
            }

            // Wide complex multiply: reads both halves of the Rss and Rtt pairs.
            OpKind::CmpyW128Sat {
                rss_lo,
                rss_hi,
                rtt_lo,
                rtt_hi,
                ..
            } => {
                result.push(*rss_lo);
                result.push(*rss_hi);
                result.push(*rtt_lo);
                result.push(*rtt_hi);
            }

            // Register-amount saturating shift: src and amount are SrcOperands.
            OpKind::SatOrigShl { src, amount, .. } => {
                if let SrcOperand::Reg(r) = src {
                    result.push(*r);
                }
                if let SrcOperand::Reg(r) = amount {
                    result.push(*r);
                }
            }

            OpKind::X86String {
                kind,
                rep,
                accumulator,
                src_index,
                dst_index,
                count,
                src_segment,
                ..
            } => {
                match kind {
                    X86StringKind::Movs => result.extend([*src_index, *dst_index]),
                    X86StringKind::Stos => result.extend([*accumulator, *dst_index]),
                    X86StringKind::Lods => result.extend([*accumulator, *src_index]),
                    X86StringKind::Scas => result.extend([*accumulator, *dst_index]),
                    X86StringKind::Cmps => result.extend([*src_index, *dst_index]),
                }
                if *rep != X86RepMode::None {
                    result.push(*count);
                }
                if let Some(segment) = src_segment {
                    result.push(*segment);
                }
            }
        }

        result
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::ops::{
        OpKind, X86CacheControlKind, X86CountKind, X86X87ControlKind, X86X87DataKind,
    };
    use crate::smir::ir::types::{
        Avx10FP16Op, Condition, FunctionId, OpId, VLaneOp, VecCmpCond, VecElementType, X86AesOp,
        X86NarrowMode,
    };

    fn make_op(id: u16, kind: OpKind) -> SmirOp {
        SmirOp::new(OpId(id), 0x1000, kind)
    }

    fn string_compare(rep: X86RepMode) -> OpKind {
        OpKind::X86String {
            kind: X86StringKind::Cmps,
            rep,
            accumulator: VReg::virt(0),
            src_index: VReg::virt(1),
            dst_index: VReg::virt(2),
            count: VReg::virt(3),
            src_segment: None,
            width: MemWidth::B1,
            address_width: OpWidth::W64,
        }
    }

    #[test]
    fn x86_string_compare_flag_metadata_handles_zero_count_rep() {
        let plain = string_compare(X86RepMode::None);
        assert_eq!(plain.flags_written(), FlagSet::ALL_X86);
        assert_eq!(plain.flags_must_write(), FlagSet::ALL_X86);

        for rep in [X86RepMode::Repe, X86RepMode::Repne] {
            let repeated = string_compare(rep);
            assert_eq!(repeated.flags_written(), FlagSet::ALL_X86);
            assert_eq!(repeated.flags_must_write(), FlagSet::EMPTY);
        }
    }

    #[test]
    fn x86_count_metadata_tracks_results_sources_and_dead_flags() {
        let dst = VReg::virt(0);
        let src = VReg::virt(1);
        let popcnt = OpKind::X86Count {
            dst,
            src,
            width: OpWidth::W32,
            kind: X86CountKind::Popcnt,
            flags: FlagUpdate::All,
        };
        assert_eq!(popcnt.dests(), vec![dst]);
        assert_eq!(popcnt.source_vregs(), vec![src]);
        assert_eq!(popcnt.flags_written(), FlagSet::ALL_X86);
        assert_eq!(popcnt.flags_must_write(), FlagSet::ALL_X86);
        assert!(op_fully_defines(&popcnt));

        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(0, popcnt));
        block.set_terminator(Terminator::Return { values: vec![] });
        assert_eq!(dead_flag_elimination(&mut block), 1);
        assert!(matches!(
            block.ops[0].kind,
            OpKind::X86Count {
                flags: FlagUpdate::None,
                ..
            }
        ));

        let defined = FlagSet::CF.union(FlagSet::ZF);
        let arch_dst = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let arch_src = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let lzcnt = OpKind::X86Count {
            dst: arch_dst,
            src: arch_src,
            width: OpWidth::W16,
            kind: X86CountKind::Lzcnt,
            flags: FlagUpdate::Specific(defined),
        };
        assert_eq!(lzcnt.flags_written(), defined);
        assert_eq!(lzcnt.flags_must_write(), defined);
        assert!(!op_fully_defines(&lzcnt));
    }

    #[test]
    fn dead_code_elimination_preserves_volatile_x86_timestamp_read() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::X86ReadTsc {
                dst_lo: VReg::virt(0),
                dst_hi: VReg::virt(1),
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![] });

        assert_eq!(dead_code_elimination(&mut block), 0);
        assert!(matches!(block.ops[0].kind, OpKind::X86ReadTsc { .. }));
    }

    #[test]
    fn test_dead_flag_elimination() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);

        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);
        let v2 = VReg::virt(2);

        // Add with flags that are never used
        block.push_op(make_op(
            0,
            OpKind::Add {
                dst: v0,
                src1: v1,
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ));

        // Another add with flags
        block.push_op(make_op(
            1,
            OpKind::Add {
                dst: v2,
                src1: v0,
                src2: SrcOperand::Imm(2),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ));

        block.set_terminator(Terminator::Return { values: vec![v2] });

        let eliminated = dead_flag_elimination(&mut block);

        // Both flag updates should be eliminated since no flags are read
        assert_eq!(eliminated, 2);

        // Check flags are now None
        for op in &block.ops {
            if let OpKind::Add { flags, .. } = &op.kind {
                assert_eq!(*flags, FlagUpdate::None);
            }
        }
    }

    #[test]
    fn mov_from_arm_nzcv_keeps_prior_flag_update_live() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        let cmp_result = VReg::virt(0);
        let src1 = VReg::virt(1);
        let cmp_nzcv = VReg::virt(2);
        let nzcv = VReg::Arch(ArchReg::Arm(ArmReg::Nzcv));

        block.push_op(make_op(
            0,
            OpKind::Sub {
                dst: cmp_result,
                src1,
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::Mov {
                dst: cmp_nzcv,
                src: SrcOperand::Reg(nzcv),
                width: OpWidth::W32,
            },
        ));
        block.set_terminator(Terminator::Return {
            values: vec![cmp_nzcv],
        });

        let eliminated = dead_flag_elimination(&mut block);
        assert_eq!(eliminated, 0);
        let OpKind::Sub { flags, .. } = &block.ops[0].kind else {
            panic!("expected compare op");
        };
        assert_eq!(*flags, FlagUpdate::All);

        let removed = dead_code_elimination(&mut block);
        assert_eq!(removed, 0);
    }

    #[test]
    fn accumulating_vcmp_to_q_reports_dst_as_source() {
        let dst = VReg::virt(0);
        let src1 = VReg::virt(1);
        let src2 = VReg::virt(2);
        let make_vcmp = |accumulate| OpKind::VCmpToQ {
            dst,
            src1,
            src2,
            cond: VecCmpCond::Eq,
            elem: VecElementType::I8,
            lanes: 16,
            accumulate,
        };

        let overwrite_sources = make_vcmp(None).source_vregs();
        assert_eq!(overwrite_sources, vec![src1, src2]);

        let accumulate_sources = make_vcmp(Some(VLaneOp::Or)).source_vregs();
        assert_eq!(accumulate_sources, vec![src1, src2, dst]);
    }

    #[test]
    fn optimize_function_preserves_cond_compare_flags_for_nzcv_select() {
        use crate::smir::ir::FunctionBuilder;

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        let cond = builder.alloc_vreg();
        let cmp_result = builder.alloc_vreg();
        let cmp_nzcv = builder.alloc_vreg();
        let final_nzcv = builder.alloc_vreg();
        let nzcv = VReg::Arch(ArchReg::Arm(ArmReg::Nzcv));

        builder.push_op(
            0x1000,
            OpKind::TestCondition {
                dst: cond,
                cond: Condition::Eq,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Sub {
                dst: cmp_result,
                src1: VReg::Arch(ArchReg::Arm(ArmReg::X(1))),
                src2: SrcOperand::Reg(VReg::Arch(ArchReg::Arm(ArmReg::X(2)))),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: cmp_nzcv,
                src: SrcOperand::Reg(nzcv),
                width: OpWidth::W32,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Select {
                dst: final_nzcv,
                cond,
                src_true: cmp_nzcv,
                src_false: VReg::Imm(0x4000_0000),
                width: OpWidth::W32,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: nzcv,
                src: SrcOperand::Reg(final_nzcv),
                width: OpWidth::W32,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut func = builder.finish();
        optimize_function(&mut func, OptLevel::O2);

        assert!(
            func.blocks[0].ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Sub {
                    flags: FlagUpdate::All,
                    ..
                }
            )),
            "conditional compare must keep the flag-producing compare op"
        );
    }

    // Regression for issue #23: a non-LOCK memory XADD lifts to a flag-free,
    // store-feeding Add, then the Store, then the source writeback, then a
    // flag-producing Add that commits the arithmetic flags only AFTER the store
    // has retired. The optimizer must preserve that ordering: it may neither sink
    // the flag-producing Add before the Store (which would re-expose flags on a
    // faulting store) nor drop it while its flags are live. This optimizes a real
    // lifted XADD with all flags live-out (Return frontier) and asserts the
    // flag-producing Add survives and stays after the Store.
    #[test]
    fn issue_23_optimizer_keeps_xadd_flag_add_after_store() {
        use crate::smir::ir::FunctionBuilder;
        use crate::smir::ir::types::SourceArch;
        use crate::smir::lift::x86_64::X86_64Lifter;
        use crate::smir::lift::{LiftContext, SmirLifter};

        // xadd dword ptr [rax], ecx (0F C1 08): a non-LOCK memory XADD.
        let mut lifter = X86_64Lifter::new();
        let mut lctx = LiftContext::new(SourceArch::X86_64);
        let result = lifter
            .lift_insn(0x1000, &[0x0F, 0xC1, 0x08], &mut lctx)
            .unwrap();

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        for op in result.ops {
            builder.push_op(op.guest_pc, op.kind);
        }
        // A Return frontier exit makes every architectural flag live-out, so the
        // flag-producing Add must be kept.
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut func = builder.finish();

        optimize_function(&mut func, OptLevel::O2);

        let ops = &func.blocks[0].ops;
        let store_pos = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::Store { .. }))
            .expect("memory XADD must keep its store");
        let flag_add_positions: Vec<usize> = ops
            .iter()
            .enumerate()
            .filter(|(_, op)| {
                matches!(
                    op.kind,
                    OpKind::Add {
                        flags: FlagUpdate::All,
                        ..
                    }
                )
            })
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            flag_add_positions.len(),
            1,
            "exactly one flag-producing Add must survive (not fused/duplicated)"
        );
        assert!(
            flag_add_positions[0] > store_pos,
            "the flag-producing Add must remain AFTER the store so a faulting \
             store cannot commit flags (store at {store_pos}, flag add at {})",
            flag_add_positions[0],
        );
    }

    #[test]
    fn optimizer_keeps_generic_memory_rmw_flag_commits_after_store() {
        use crate::smir::ir::FunctionBuilder;
        use crate::smir::ir::types::SourceArch;
        use crate::smir::lift::x86_64::X86_64Lifter;
        use crate::smir::lift::{LiftContext, SmirLifter};

        for (name, bytes) in [
            ("add", &[0x01, 0x08][..]),
            ("adc immediate", &[0x83, 0x10, 0x01][..]),
            ("shift", &[0xC1, 0x20, 0x01][..]),
            ("rcr", &[0x48, 0xD3, 0x18][..]),
            ("neg", &[0xF7, 0x18][..]),
            ("inc", &[0x48, 0xFF, 0x00][..]),
        ] {
            let mut lifter = X86_64Lifter::new();
            let mut lctx = LiftContext::new(SourceArch::X86_64);
            let result = lifter.lift_insn(0x1000, bytes, &mut lctx).unwrap();
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            for op in result.ops {
                builder.push_op(op.guest_pc, op.kind);
            }
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut func = builder.finish();
            optimize_function(&mut func, OptLevel::O2);

            let ops = &func.blocks[0].ops;
            let store = ops
                .iter()
                .position(|op| matches!(op.kind, OpKind::Store { .. }))
                .unwrap_or_else(|| panic!("{name}: store removed"));
            assert!(
                ops[..store]
                    .iter()
                    .all(|op| op.kind.flags_written().is_empty()),
                "{name}: optimizer exposed flags before store: {ops:?}",
            );
            assert!(
                ops[store + 1..]
                    .iter()
                    .any(|op| !op.kind.flags_written().is_empty()),
                "{name}: optimizer removed post-store flag commit: {ops:?}",
            );
        }
    }

    #[test]
    fn optimizer_keeps_locked_memory_rmw_flag_commits_after_atomic_write() {
        use crate::smir::ir::FunctionBuilder;
        use crate::smir::ir::types::SourceArch;
        use crate::smir::lift::x86_64::X86_64Lifter;
        use crate::smir::lift::{LiftContext, SmirLifter};

        for (name, bytes) in [
            ("lock add", &[0xF0, 0x01, 0x08][..]),
            ("lock adc", &[0xF0, 0x83, 0x10, 0x01][..]),
            ("lock inc", &[0xF0, 0x48, 0xFF, 0x00][..]),
            ("lock neg", &[0xF0, 0xF7, 0x18][..]),
        ] {
            let mut lifter = X86_64Lifter::new();
            let mut lctx = LiftContext::new(SourceArch::X86_64);
            let result = lifter.lift_insn(0x1000, bytes, &mut lctx).unwrap();
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            for op in result.ops {
                builder.push_op(op.guest_pc, op.kind);
            }
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut func = builder.finish();
            optimize_function(&mut func, OptLevel::O2);

            let ops = &func.blocks[0].ops;
            let atomic = ops
                .iter()
                .position(|op| matches!(op.kind, OpKind::AtomicRmw { .. }))
                .unwrap_or_else(|| panic!("{name}: atomic write removed"));
            assert!(
                ops[..atomic]
                    .iter()
                    .all(|op| op.kind.flags_written().is_empty()),
                "{name}: optimizer exposed flags before atomic write: {ops:?}",
            );
            assert!(
                ops[atomic + 1..]
                    .iter()
                    .any(|op| !op.kind.flags_written().is_empty()),
                "{name}: optimizer removed post-atomic flag commit: {ops:?}",
            );
        }
    }

    #[test]
    fn optimizer_preserves_vex_scalar_merge_zeroing_and_load_fault_boundary() {
        use crate::smir::ir::types::{
            FpRoundMode, ShiftOp, SourceArch, VecCmpCond, VecUnaryOp, VecWidth, X86Reg,
        };
        use crate::smir::ir::{FunctionBuilder, SmirFunction};
        use crate::smir::lift::x86_64::X86_64Lifter;
        use crate::smir::lift::{LiftContext, SmirLifter};

        fn optimized(bytes: &[u8]) -> SmirFunction {
            let mut lifter = X86_64Lifter::new();
            let mut lctx = LiftContext::new(SourceArch::X86_64);
            let result = lifter.lift_insn(0x1000, bytes, &mut lctx).unwrap();
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut func = builder.finish();
            func.blocks[0].ops = result.ops;
            optimize_function(&mut func, OptLevel::O2);
            func
        }

        let arithmetic = optimized(&[0xC5, 0xF2, 0x58, 0xC2]);
        let ops = &arithmetic.blocks[0].ops;
        let last_upper_extract = ops
            .iter()
            .rposition(|op| {
                matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                        lane: 3,
                        ..
                    }
                )
            })
            .expect("VADDSS must retain merge-source lane extraction");
        let clear = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBroadcast {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        ..
                    }
                )
            })
            .expect("VADDSS must retain VEX upper-state clearing");
        assert!(
            last_upper_extract < clear,
            "alias-safe merge must precede clear"
        );
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                lane: 0,
                ..
            }
        )));

        let memory = optimized(&[0xC5, 0xFB, 0x10, 0x00]);
        let ops = &memory.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B8,
                        ..
                    }
                )
            })
            .expect("faulting VMOVSD load must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBroadcast {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        ..
                    }
                )
            })
            .expect("VMOVSD destination write must survive optimization");
        assert!(
            load < destination_write,
            "destination changed before load fault boundary"
        );

        let movq = optimized(&[0xC4, 0xE1, 0xF9, 0x6E, 0x00]);
        let ops = &movq.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B8,
                        ..
                    }
                )
            })
            .expect("faulting VMOVQ load must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBroadcast {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I64,
                        ..
                    }
                )
            })
            .expect("VMOVQ destination clear must survive optimization");
        assert!(
            load < destination_write,
            "VMOVQ changed its destination before the load fault boundary"
        );
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                elem: VecElementType::I64,
                lane: 0,
                ..
            }
        )));

        let scalar_vec_movq = optimized(&[0xC5, 0xFA, 0x7E, 0x00]);
        let ops = &scalar_vec_movq.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B8,
                        ..
                    }
                )
            })
            .expect("faulting scalar-vector VMOVQ load must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBroadcast {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I64,
                        ..
                    }
                )
            })
            .expect("scalar-vector VMOVQ destination clear must survive optimization");
        assert!(
            load < destination_write,
            "scalar-vector VMOVQ changed its destination before the load fault boundary"
        );

        let alias = optimized(&[0xC5, 0xFA, 0x7E, 0xC0]);
        let ops = &alias.blocks[0].ops;
        let extract = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I64,
                        lane: 0,
                        ..
                    }
                )
            })
            .expect("same-register VMOVQ source extraction must survive optimization");
        let clear = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBroadcast {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I64,
                        ..
                    }
                )
            })
            .expect("same-register VMOVQ destination clear must survive optimization");
        assert!(extract < clear, "VMOVQ alias extraction must precede clear");

        let packed_compare = optimized(&[0xC5, 0xF5, 0x74, 0x00]);
        let ops = &packed_compare.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("faulting VPCMPEQB source load must survive optimization");
        let compare = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VCmp {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                        cond: VecCmpCond::Eq,
                        elem: VecElementType::I8,
                        ..
                    }
                )
            })
            .expect("VPCMPEQB architectural compare write must survive optimization");
        assert!(
            load < compare,
            "VPCMPEQB changed its destination before the source load fault boundary"
        );

        let legacy_compare = optimized(&[0x66, 0x0F, 0x66, 0xC0]);
        let ops = &legacy_compare.blocks[0].ops;
        let compare = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VCmp {
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        cond: VecCmpCond::Gt,
                        elem: VecElementType::I32,
                        ..
                    }
                )
            })
            .expect("same-register PCMPGTD source compare must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I32,
                        ..
                    }
                )
            })
            .expect("legacy PCMPGTD destination merge must survive optimization");
        assert!(
            compare < destination_write,
            "legacy packed compare must capture aliased inputs before destination writes"
        );

        let evex_compare = optimized(&[0x62, 0xF1, 0x75, 0x09, 0x76, 0x10]);
        let ops = &evex_compare.blocks[0].ops;
        let first_pred_load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                )
            })
            .expect("masked EVEX VPCMPEQD predicated source loads must survive optimization");
        let k_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::And {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::K(2))),
                        src2: SrcOperand::Reg(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                        flags: FlagUpdate::None,
                        ..
                    }
                )
            })
            .expect("EVEX VPCMPEQD masked k-destination write must survive optimization");
        assert!(
            first_pred_load < k_write,
            "EVEX compare committed its k destination before masked memory accesses"
        );
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            4,
            "EVEX.128 VPCMPEQD requires one fault-suppressible load per lane"
        );

        let vex_unpack = optimized(&[0xC5, 0xF5, 0x60, 0x00]);
        let ops = &vex_unpack.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("faulting VPUNPCKLBW source load must survive optimization");
        let shuffle = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VShuffle {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                        elem: VecElementType::I8,
                        lanes: 32,
                        ..
                    }
                )
            })
            .expect("VPUNPCKLBW architectural shuffle write must survive optimization");
        assert!(
            load < shuffle,
            "VPUNPCKLBW changed its destination before the memory fault boundary"
        );

        let evex_unpack = optimized(&[0x62, 0xF1, 0xF5, 0x49, 0x6D, 0x00]);
        let ops = &evex_unpack.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V512,
                        ..
                    }
                )
            })
            .expect("E4NF VPUNPCKHQDQ complete source load must survive optimization");
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: VecElementType::I64,
                        ..
                    }
                )
            })
            .expect("masked EVEX unpack destination writes must survive optimization");
        assert!(
            load < destination_write,
            "EVEX unpack committed its destination before the complete E4NF memory access"
        );

        let vex_pack = optimized(&[0xC5, 0xF5, 0x63, 0x00]);
        let ops = &vex_pack.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("faulting VPACKSSWB source load must survive optimization");
        let pack = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VPackSat {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                        src_elem: VecElementType::I16,
                        src_lanes: 16,
                        block_lanes: 8,
                        ..
                    }
                )
            })
            .expect("VPACKSSWB architectural pack write must survive optimization");
        assert!(
            load < pack,
            "VPACKSSWB changed its destination before the memory fault boundary"
        );

        let evex_pack = optimized(&[0x62, 0xF1, 0x75, 0x49, 0x6B, 0x00]);
        let ops = &evex_pack.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16,
            "masked EVEX.512 VPACKSSDW requires one fault-suppressible load per r/m dword"
        );
        let last_load = ops
            .iter()
            .rposition(|op| {
                matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                )
            })
            .unwrap();
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: VecElementType::I16,
                        ..
                    }
                )
            })
            .expect("masked EVEX pack destination writes must survive optimization");
        assert!(
            last_load < destination_write,
            "EVEX pack committed its destination before predicated memory accesses"
        );

        let evex_pack_broadcast = optimized(&[0x62, 0xF1, 0x75, 0x59, 0x6B, 0x00]);
        let ops = &evex_pack_broadcast.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            1,
            "masked EVEX VPACKSSDW broadcast must retain one conditional scalar read"
        );
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem: VecElementType::I32,
                lanes: 16,
                ..
            }
        )));

        let vex_pshufb = optimized(&[0xC4, 0xE2, 0x75, 0x00, 0x00]);
        let ops = &vex_pshufb.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("faulting VPSHUFB control load must survive optimization");
        let shuffle = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VByteShuffle {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                        lanes: 32,
                        block_lanes: 16,
                        ..
                    }
                )
            })
            .expect("VPSHUFB architectural shuffle write must survive optimization");
        assert!(
            load < shuffle,
            "VPSHUFB changed its destination before the memory fault boundary"
        );

        let legacy_pshufb = optimized(&[0x66, 0x0F, 0x38, 0x00, 0x00]);
        let ops = &legacy_pshufb.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy PSHUFB alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PSHUFB aligned control load must survive optimization");
        assert!(
            alignment < load,
            "legacy PSHUFB loaded memory before its mandatory alignment check"
        );

        let evex_pshufb = optimized(&[0x62, 0xF2, 0x75, 0x49, 0x00, 0x00]);
        let ops = &evex_pshufb.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B1,
                        ..
                    }
                ))
                .count(),
            64,
            "masked EVEX.512 VPSHUFB requires one conditional control-byte load per output"
        );
        let last_load = ops
            .iter()
            .rposition(|op| {
                matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B1,
                        ..
                    }
                )
            })
            .unwrap();
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: VecElementType::I8,
                        ..
                    }
                )
            })
            .expect("masked EVEX VPSHUFB destination writes must survive optimization");
        assert!(
            last_load < destination_write,
            "EVEX VPSHUFB committed its destination before predicated control-byte accesses"
        );

        let legacy_horizontal = optimized(&[0x66, 0x0F, 0x38, 0x03, 0x00]);
        let ops = &legacy_horizontal.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy PHADDSW alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PHADDSW source load must survive optimization");
        let horizontal = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VHorizontalBin {
                        elem: VecElementType::I16,
                        saturating: true,
                        subtract: false,
                        ..
                    }
                )
            })
            .expect("legacy PHADDSW computation must survive optimization");
        assert!(alignment < load && load < horizontal);

        let vex_horizontal = optimized(&[0xC4, 0xE2, 0x75, 0x06, 0x00]);
        let ops = &vex_horizontal.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("faulting VPHSUBD source load must survive optimization");
        let horizontal = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VHorizontalBin {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                        elem: VecElementType::I32,
                        subtract: true,
                        saturating: false,
                        ..
                    }
                )
            })
            .expect("VPHSUBD architectural write must survive optimization");
        assert!(
            load < horizontal,
            "VPHSUBD changed its destination before the memory fault boundary"
        );

        let legacy_maddubs = optimized(&[0x66, 0x0F, 0x38, 0x04, 0x00]);
        let ops = &legacy_maddubs.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy PMADDUBSW alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PMADDUBSW source load must survive optimization");
        let dot = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VDotProduct {
                        acc_elem: VecElementType::I16,
                        saturate: true,
                        ..
                    }
                )
            })
            .expect("legacy PMADDUBSW computation must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I16,
                        ..
                    }
                )
            })
            .expect("legacy PMADDUBSW destination merge must survive optimization");
        assert!(alignment < load && load < dot && dot < destination_write);

        let vex_maddubs = optimized(&[0xC4, 0xE2, 0x75, 0x04, 0x00]);
        let ops = &vex_maddubs.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VEX VPMADDUBSW source load must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VDotProduct {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                        acc_elem: VecElementType::I16,
                        ..
                    }
                )
            })
            .expect("VEX VPMADDUBSW architectural write must survive optimization");
        assert!(load < destination_write);

        let evex_maddubs = optimized(&[0x62, 0xF2, 0x75, 0x49, 0x04, 0x00]);
        let ops = &evex_maddubs.blocks[0].ops;
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
            "E4NF EVEX.512 VPMADDUBSW must not predicate its memory read"
        );
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V512,
                        ..
                    }
                )
            })
            .expect("E4NF EVEX.512 VPMADDUBSW full source load must survive optimization");
        let dot = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VDotProduct {
                        acc_elem: VecElementType::I16,
                        width: VecWidth::V512,
                        ..
                    }
                )
            })
            .expect("masked EVEX VPMADDUBSW computation must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: VecElementType::I16,
                        ..
                    }
                )
            })
            .expect("masked EVEX VPMADDUBSW destination write must survive optimization");
        assert!(load < dot && dot < destination_write);

        let legacy_psign = optimized(&[0x66, 0x0F, 0x38, 0x09, 0x00]);
        let ops = &legacy_psign.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy PSIGNW alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PSIGNW source load must survive optimization");
        let negation = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VUnary {
                        elem: VecElementType::I16,
                        op: VecUnaryOp::Neg,
                        ..
                    }
                )
            })
            .expect("legacy PSIGNW wrapping negation must survive optimization");
        let sign_select = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBitSelect {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PSIGNW sign selection must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I16,
                        ..
                    }
                )
            })
            .expect("legacy PSIGNW destination merge must survive optimization");
        assert!(alignment < load && load < negation && negation < sign_select);
        assert!(sign_select < destination_write);

        let vex_psign = optimized(&[0xC4, 0xE2, 0x75, 0x0A, 0x00]);
        let ops = &vex_psign.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VEX VPSIGND source load must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VAndNot {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VEX VPSIGND architectural write must survive optimization");
        assert!(load < destination_write);

        let legacy_mulhrsw = optimized(&[0x66, 0x0F, 0x38, 0x0B, 0x00]);
        let ops = &legacy_mulhrsw.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy PMULHRSW alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PMULHRSW load must survive optimization");
        let multiply = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VMulShiftSat {
                        lanes: 8,
                        round: true,
                        out_shift: 15,
                        ..
                    }
                )
            })
            .expect("legacy PMULHRSW rounded multiply must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I16,
                        ..
                    }
                )
            })
            .expect("legacy PMULHRSW destination merge must survive optimization");
        assert!(alignment < load && load < multiply && multiply < destination_write);

        let evex_mulhrsw = optimized(&[0x62, 0xF2, 0x75, 0x49, 0x0B, 0x00]);
        let ops = &evex_mulhrsw.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B2,
                        ..
                    }
                ))
                .count(),
            32
        );
        let last_load = ops
            .iter()
            .rposition(|op| {
                matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B2,
                        ..
                    }
                )
            })
            .unwrap();
        let multiply = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VMulShiftSat {
                        lanes: 32,
                        round: true,
                        out_shift: 15,
                        ..
                    }
                )
            })
            .expect("EVEX VPMULHRSW rounded multiply must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: VecElementType::I16,
                        ..
                    }
                )
            })
            .expect("EVEX VPMULHRSW destination write must survive optimization");
        assert!(last_load < multiply && multiply < destination_write);

        let legacy_pabs = optimized(&[0x66, 0x0F, 0x38, 0x1D, 0x00]);
        let ops = &legacy_pabs.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy PABSW alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PABSW load must survive optimization");
        let absolute = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VUnary {
                        elem: VecElementType::I16,
                        op: VecUnaryOp::Abs,
                        ..
                    }
                )
            })
            .expect("legacy PABSW absolute value must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I16,
                        ..
                    }
                )
            })
            .expect("legacy PABSW destination merge must survive optimization");
        assert!(alignment < load && load < absolute && absolute < destination_write);

        let evex_pabs = optimized(&[0x62, 0xF2, 0x7D, 0x49, 0x1C, 0x00]);
        let ops = &evex_pabs.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B1,
                        ..
                    }
                ))
                .count(),
            64
        );
        let last_load = ops
            .iter()
            .rposition(|op| {
                matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B1,
                        ..
                    }
                )
            })
            .unwrap();
        let absolute = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VUnary {
                        elem: VecElementType::I8,
                        lanes: 64,
                        op: VecUnaryOp::Abs,
                        ..
                    }
                )
            })
            .expect("EVEX VPABSB absolute value must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: VecElementType::I8,
                        ..
                    }
                )
            })
            .expect("EVEX VPABSB destination write must survive optimization");
        assert!(last_load < absolute && absolute < destination_write);

        let broadcast_pabs = optimized(&[0x62, 0xF2, 0x7D, 0x59, 0x1E, 0x00]);
        let ops = &broadcast_pabs.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            1
        );
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem: VecElementType::I32,
                lanes: 16,
                ..
            }
        )));

        let legacy_palignr = optimized(&[0x66, 0x0F, 0x3A, 0x0F, 0x00, 0x01]);
        let ops = &legacy_palignr.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy PALIGNR alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PALIGNR source load must survive optimization");
        let shuffle = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VShuffle {
                        elem: VecElementType::I8,
                        lanes: 16,
                        ..
                    }
                )
            })
            .expect("legacy PALIGNR shuffle must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I8,
                        ..
                    }
                )
            })
            .expect("legacy PALIGNR destination merge must survive optimization");
        assert!(alignment < load && load < shuffle && shuffle < destination_write);

        let evex_palignr = optimized(&[0x62, 0xF3, 0x75, 0x49, 0x0F, 0x00, 0x01]);
        let ops = &evex_palignr.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B1,
                        ..
                    }
                ))
                .count(),
            60
        );
        let last_load = ops
            .iter()
            .rposition(|op| {
                matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B1,
                        ..
                    }
                )
            })
            .unwrap();
        let shuffle = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VShuffle {
                        elem: VecElementType::I8,
                        lanes: 64,
                        ..
                    }
                )
            })
            .expect("EVEX VPALIGNR shuffle must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: VecElementType::I8,
                        ..
                    }
                )
            })
            .expect("EVEX VPALIGNR destination write must survive optimization");
        assert!(last_load < shuffle && shuffle < destination_write);

        let high_only_palignr = optimized(&[0x62, 0xF3, 0x75, 0x49, 0x0F, 0x00, 0x10]);
        assert!(
            !high_only_palignr.blocks[0]
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );

        let legacy_pmovsxbq = optimized(&[0x66, 0x0F, 0x38, 0x22, 0x00]);
        let ops = &legacy_pmovsxbq.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B1,
                        ..
                    }
                ))
                .count(),
            2,
            "legacy PMOVSXBQ must retain its exact two-byte fault surface"
        );
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. })),
            "legacy packed extension has no aligned-memory requirement"
        );
        let last_load = ops
            .iter()
            .rposition(|op| {
                matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B1,
                        ..
                    }
                )
            })
            .unwrap();
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I64,
                        ..
                    }
                )
            })
            .expect("legacy PMOVSXBQ destination merge must survive optimization");
        assert!(
            last_load < destination_write,
            "packed-extension destination write crossed its source fault boundary"
        );

        for (opcode, destination_elem, expected_loads) in [
            (0x20, VecElementType::I16, 32usize),
            (0x22, VecElementType::I64, 8usize),
        ] {
            let evex_pmov = optimized(&[0x62, 0xF2, 0x7D, 0x49, opcode, 0x00]);
            let ops = &evex_pmov.blocks[0].ops;
            assert_eq!(
                ops.iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::PredLoad {
                            width: MemWidth::B1,
                            ..
                        }
                    ))
                    .count(),
                expected_loads,
                "EVEX packed extension lost per-source-element predication"
            );
            assert!(!ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Load { .. } | OpKind::X86CheckAlignment { .. }
            )));
            let last_load = ops
                .iter()
                .rposition(|op| {
                    matches!(
                        op.kind,
                        OpKind::PredLoad {
                            width: MemWidth::B1,
                            ..
                        }
                    )
                })
                .unwrap();
            let destination_write = ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::VInsertLane {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                            elem,
                            ..
                        } if elem == destination_elem
                    )
                })
                .expect("EVEX packed-extension destination merge must survive optimization");
            assert!(
                last_load < destination_write,
                "EVEX packed-extension write crossed a conditional source fault boundary"
            );
        }

        let legacy_pminsb = optimized(&[0x66, 0x0F, 0x38, 0x38, 0x00]);
        let ops = &legacy_pminsb.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy PMINSB alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PMINSB load must survive optimization");
        let compare = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VCmp {
                        elem: VecElementType::I8,
                        ..
                    }
                )
            })
            .expect("legacy PMINSB comparison must survive optimization");
        let select = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBitSelect {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PMINSB selection must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I8,
                        ..
                    }
                )
            })
            .expect("legacy PMINSB destination merge must survive optimization");
        assert!(
            alignment < load && load < compare && compare < select && select < destination_write
        );

        for (bytes, elem, mem_width, expected_loads) in [
            (
                &[0x62, 0xF1, 0x75, 0x49, 0xDA, 0x00][..],
                VecElementType::I8,
                MemWidth::B1,
                64usize,
            ),
            (
                &[0x62, 0xF1, 0x75, 0x49, 0xEA, 0x00][..],
                VecElementType::I16,
                MemWidth::B2,
                32usize,
            ),
            (
                &[0x62, 0xF2, 0x75, 0x49, 0x38, 0x00][..],
                VecElementType::I8,
                MemWidth::B1,
                64usize,
            ),
            (
                &[0x62, 0xF2, 0xF5, 0x59, 0x3F, 0x00][..],
                VecElementType::I64,
                MemWidth::B8,
                8usize,
            ),
        ] {
            let evex_minmax = optimized(bytes);
            let ops = &evex_minmax.blocks[0].ops;
            assert_eq!(
                ops.iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::PredLoad { width, .. } if width == mem_width
                    ))
                    .count(),
                expected_loads,
                "EVEX packed min/max lost elementwise fault suppression"
            );
            assert!(!ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Load { .. } | OpKind::VLoad { .. } | OpKind::X86CheckAlignment { .. }
            )));
            let last_load = ops
                .iter()
                .rposition(|op| {
                    matches!(
                        op.kind,
                        OpKind::PredLoad { width, .. } if width == mem_width
                    )
                })
                .unwrap();
            let compare = ops
                .iter()
                .position(
                    |op| matches!(op.kind, OpKind::VCmp { elem: actual, .. } if actual == elem),
                )
                .unwrap();
            let select = ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::VBitSelect {
                            width: VecWidth::V512,
                            ..
                        }
                    )
                })
                .unwrap();
            let destination_write = ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::VInsertLane {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                            elem: actual,
                            ..
                        } if actual == elem
                    )
                })
                .unwrap();
            assert!(last_load < compare && compare < select && select < destination_write);
        }

        let legacy_ptest = optimized(&[0x66, 0x0F, 0x38, 0x17, 0x00]);
        let ops = &legacy_ptest.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy PTEST alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PTEST load must survive optimization");
        let read_flags = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::ReadFlags { .. }))
            .expect("legacy PTEST preserved-flag capture must survive optimization");
        let write_flags = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::WriteFlags { .. }))
            .expect("legacy PTEST flag commit must survive optimization");
        assert!(alignment < load && load < read_flags && read_flags < write_flags);
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        elem: VecElementType::I64,
                        ..
                    }
                ))
                .count(),
            4
        );

        let vex_ptest = optimized(&[0xC4, 0xE2, 0x7D, 0x17, 0x00]);
        let ops = &vex_ptest.blocks[0].ops;
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VPTEST.256 load must survive optimization");
        let read_flags = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::ReadFlags { .. }))
            .unwrap();
        let write_flags = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::WriteFlags { .. }))
            .unwrap();
        assert!(load < read_flags && read_flags < write_flags);
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        elem: VecElementType::I64,
                        ..
                    }
                ))
                .count(),
            8
        );
        assert!(
            ops[..load]
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );

        let legacy_blend = optimized(&[0x66, 0x0F, 0x38, 0x10, 0x10]);
        let ops = &legacy_blend.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy PBLENDVB alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PBLENDVB source load must survive optimization");
        let mask_compare = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VCmp {
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I8,
                        cond: VecCmpCond::Lt,
                        ..
                    }
                )
            })
            .expect("legacy PBLENDVB implicit mask must survive optimization");
        let select = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBitSelect {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PBLENDVB selection must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                        elem: VecElementType::I8,
                        ..
                    }
                )
            })
            .expect("legacy PBLENDVB destination merge must survive optimization");
        assert!(
            alignment < load
                && load < mask_compare
                && mask_compare < select
                && select < destination_write
        );

        let vex_blend = optimized(&[0xC4, 0xE3, 0x65, 0x4A, 0x10, 0x40]);
        let ops = &vex_blend.blocks[0].ops;
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VBLENDVPS memory source must survive optimization");
        let mask_compare = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VCmp {
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(4))),
                        elem: VecElementType::I32,
                        cond: VecCmpCond::Lt,
                        ..
                    }
                )
            })
            .expect("VBLENDVPS explicit mask must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBitSelect {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VBLENDVPS destination write must survive optimization");
        assert!(load < mask_compare && mask_compare < destination_write);
        assert!(ops.iter().all(|op| op.kind.flags_written().is_empty()));

        let legacy_pmuldq = optimized(&[0x66, 0x0F, 0x38, 0x28, 0x00]);
        let ops = &legacy_pmuldq.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .unwrap();
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .unwrap();
        let multiply = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::MulS {
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                        ..
                    }
                )
            })
            .unwrap();
        let write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I64,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(alignment < load && load < multiply && multiply < write);

        let evex_pmuldq = optimized(&[0x62, 0xF2, 0xF5, 0x49, 0x28, 0x00]);
        let ops = &evex_pmuldq.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B8,
                        ..
                    }
                ))
                .count(),
            8
        );
        let last_load = ops
            .iter()
            .rposition(|op| {
                matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B8,
                        ..
                    }
                )
            })
            .unwrap();
        let multiply = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::MulS {
                        width: OpWidth::W64,
                        ..
                    }
                )
            })
            .unwrap();
        let write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: VecElementType::I64,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(last_load < multiply && multiply < write);

        for (name, bytes, width, alignment, dst) in [
            (
                "legacy MOVNTDQA",
                &[0x66, 0x0F, 0x38, 0x2A, 0x00][..],
                VecWidth::V128,
                16,
                X86Reg::Xmm(0),
            ),
            (
                "VEX.256 VMOVNTDQA",
                &[0xC4, 0xE2, 0x7D, 0x2A, 0x00][..],
                VecWidth::V256,
                32,
                X86Reg::Ymm(0),
            ),
            (
                "EVEX.512 VMOVNTDQA",
                &[0x62, 0xE2, 0x7D, 0x48, 0x2A, 0x00][..],
                VecWidth::V512,
                64,
                X86Reg::Zmm(16),
            ),
        ] {
            let function = optimized(bytes);
            let ops = &function.blocks[0].ops;
            let alignment_check = ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::X86CheckAlignment {
                            alignment: actual,
                            ..
                        } if actual == alignment
                    )
                })
                .unwrap_or_else(|| panic!("{name}: mandatory alignment check was removed"));
            let load = ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::VLoad {
                            width: actual,
                            ..
                        } if actual == width
                    )
                })
                .unwrap_or_else(|| panic!("{name}: memory load was removed"));
            let destination_write = ops
                .iter()
                .position(|op| match op.kind {
                    OpKind::VMov {
                        dst: VReg::Arch(ArchReg::X86(actual)),
                        width: actual_width,
                        ..
                    } => actual == dst && actual_width == width,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(actual)),
                        ..
                    } => actual == dst,
                    _ => false,
                })
                .unwrap_or_else(|| panic!("{name}: architectural destination write was removed"));
            assert!(
                alignment_check < load && load < destination_write,
                "{name}: optimizer violated check-before-load-before-write ordering: {ops:?}"
            );
        }

        let legacy_phminposuw = optimized(&[0x66, 0x0F, 0x38, 0x41, 0x00]);
        let ops = &legacy_phminposuw.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy PHMINPOSUW alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PHMINPOSUW source load must survive optimization");
        let read_flags = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::ReadFlags { .. }))
            .expect("PHMINPOSUW flag preservation capture must survive optimization");
        let first_compare = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Cmp {
                        width: OpWidth::W16,
                        ..
                    }
                )
            })
            .expect("PHMINPOSUW minimum comparisons must survive optimization");
        let write_flags = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::WriteFlags { .. }))
            .expect("PHMINPOSUW flag restoration must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I64,
                        ..
                    }
                )
            })
            .expect("legacy PHMINPOSUW destination write must survive optimization");
        assert!(
            alignment < load
                && load < read_flags
                && read_flags < first_compare
                && first_compare < write_flags
                && write_flags < destination_write
        );
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::SetCC {
                        cond: Condition::Ult,
                        ..
                    }
                ))
                .count(),
            7
        );

        let vex_phminposuw = optimized(&[0xC4, 0xE2, 0x79, 0x41, 0x00]);
        let ops = &vex_phminposuw.blocks[0].ops;
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("VEX VPHMINPOSUW unaligned source load must survive optimization");
        let read_flags = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::ReadFlags { .. }))
            .unwrap();
        let write_flags = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::WriteFlags { .. }))
            .unwrap();
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VMov {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("VEX VPHMINPOSUW zeroing destination write must survive optimization");
        assert!(load < read_flags && read_flags < write_flags && write_flags < destination_write);

        for (name, bytes, width, products, legacy_alignment, dst) in [
            (
                "legacy PCLMULQDQ",
                &[0x66, 0x0F, 0x3A, 0x44, 0x00, 0x11][..],
                VecWidth::V128,
                1usize,
                true,
                X86Reg::Xmm(0),
            ),
            (
                "VEX.256 VPCLMULQDQ",
                &[0xC4, 0xE3, 0x75, 0x44, 0x00, 0x11][..],
                VecWidth::V256,
                2,
                false,
                X86Reg::Ymm(0),
            ),
            (
                "EVEX.512 VPCLMULQDQ",
                &[0x62, 0xF3, 0x75, 0x48, 0x44, 0x00, 0x11][..],
                VecWidth::V512,
                4,
                false,
                X86Reg::Zmm(0),
            ),
        ] {
            let function = optimized(bytes);
            let ops = &function.blocks[0].ops;
            let alignment = ops
                .iter()
                .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }));
            assert_eq!(alignment.is_some(), legacy_alignment, "{name}");
            let load = ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::VLoad {
                            width: actual,
                            ..
                        } if actual == width
                    )
                })
                .unwrap_or_else(|| panic!("{name}: full source load was removed"));
            let first_product = ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::ClMul {
                            elem_bits: 64,
                            lanes: 1,
                            acc: false,
                            ..
                        }
                    )
                })
                .unwrap_or_else(|| panic!("{name}: carry-less products were removed"));
            let last_product = ops
                .iter()
                .rposition(|op| {
                    matches!(
                        op.kind,
                        OpKind::ClMul {
                            elem_bits: 64,
                            lanes: 1,
                            acc: false,
                            ..
                        }
                    )
                })
                .unwrap();
            assert_eq!(
                ops.iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::ClMul {
                            elem_bits: 64,
                            lanes: 1,
                            acc: false,
                            ..
                        }
                    ))
                    .count(),
                products,
                "{name}"
            );
            let destination_write = ops
                .iter()
                .position(|op| match op.kind {
                    OpKind::VMov {
                        dst: VReg::Arch(ArchReg::X86(actual)),
                        width: actual_width,
                        ..
                    } => actual == dst && actual_width == width,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(actual)),
                        ..
                    } => actual == dst,
                    _ => false,
                })
                .unwrap_or_else(|| panic!("{name}: architectural result write was removed"));
            if let Some(alignment) = alignment {
                assert!(alignment < load, "{name}");
            }
            assert!(
                load < first_product
                    && first_product <= last_product
                    && last_product < destination_write,
                "{name}: optimizer violated load/product/write ordering: {ops:?}"
            );
            assert!(
                !ops.iter()
                    .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
                "{name}: PCLMULQDQ must not acquire memory fault suppression"
            );
            assert!(ops.iter().all(|op| op.kind.flags_written().is_empty()));
        }

        let crc_memory = optimized(&[0xF2, 0x4C, 0x0F, 0x38, 0xF1, 0x00]);
        let ops = &crc_memory.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B8,
                        ..
                    }
                )
            })
            .expect("CRC32 qword memory read must survive optimization");
        let crc = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Crc32C {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::R8)),
                        crc: VReg::Arch(ArchReg::X86(X86Reg::R8)),
                        data_width: OpWidth::W64,
                        ..
                    }
                )
            })
            .expect("CRC32 architectural result must survive optimization");
        assert!(load < crc);
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        assert!(ops.iter().all(|op| op.kind.flags_written().is_empty()));

        let crc_high_byte = optimized(&[0xF2, 0x0F, 0x38, 0xF0, 0xD5]);
        let ops = &crc_high_byte.blocks[0].ops;
        let extraction = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Shr {
                        src: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
                        amount: SrcOperand::Imm(8),
                        flags: FlagUpdate::None,
                        ..
                    }
                )
            })
            .expect("CRC32 CH extraction must survive optimization");
        let crc = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Crc32C {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
                        data_width: OpWidth::W8,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(extraction < crc);

        let crc_alias = optimized(&[0xF2, 0x4D, 0x0F, 0x38, 0xF1, 0xC0]);
        assert!(crc_alias.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Crc32C {
                dst: VReg::Arch(ArchReg::X86(X86Reg::R8)),
                crc: VReg::Arch(ArchReg::X86(X86Reg::R8)),
                data: VReg::Arch(ArchReg::X86(X86Reg::R8)),
                data_width: OpWidth::W64,
            }
        )));

        let legacy_blend_imm = optimized(&[0x66, 0x0F, 0x3A, 0x0C, 0x00, 0xA5]);
        let ops = &legacy_blend_imm.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy BLENDPS alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy BLENDPS memory source must survive optimization");
        let selection = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        elem: VecElementType::I32,
                        ..
                    }
                )
            })
            .expect("legacy BLENDPS lane selection must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I32,
                        ..
                    }
                )
            })
            .expect("legacy BLENDPS destination merge must survive optimization");
        assert!(alignment < load && load < selection && selection < destination_write);

        let vex_blend_imm = optimized(&[0xC4, 0xE3, 0x65, 0x0C, 0x08, 0xA5]);
        let ops = &vex_blend_imm.blocks[0].ops;
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VEX VBLENDPS unaligned source load must survive optimization");
        let first_selection = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        elem: VecElementType::I32,
                        ..
                    }
                )
            })
            .unwrap();
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VMov {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VEX VBLENDPS destination write must survive optimization");
        assert!(load < first_selection && first_selection < destination_write);
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        elem: VecElementType::I32,
                        ..
                    }
                ))
                .count(),
            8
        );
        assert!(ops.iter().all(|op| op.kind.flags_written().is_empty()));

        let legacy_insert = optimized(&[0x66, 0x44, 0x0F, 0x3A, 0x22, 0x08, 0x03]);
        let ops = &legacy_insert.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B4,
                        ..
                    }
                )
            })
            .expect("faulting PINSRD scalar load must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                        elem: VecElementType::I32,
                        ..
                    }
                )
            })
            .expect("PINSRD architectural merge must survive optimization");
        assert!(load < destination_write);

        let vector_insert = optimized(&[0xC4, 0x63, 0x29, 0x22, 0x48, 0x14, 0x03]);
        let ops = &vector_insert.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B4,
                        ..
                    }
                )
            })
            .expect("faulting VPINSRD scalar load must survive optimization");
        let first_merge_read = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
                        elem: VecElementType::I32,
                        ..
                    }
                )
            })
            .expect("VPINSRD merge-source reads must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VMov {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("VPINSRD destination write must survive optimization");
        assert!(load < first_merge_read && first_merge_read < destination_write);

        let extract = optimized(&[0x66, 0x44, 0x0F, 0x3A, 0x15, 0x48, 0x22, 0x0F]);
        let ops = &extract.blocks[0].ops;
        let extraction = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                        lane: 7,
                        elem: VecElementType::I16,
                        ..
                    }
                )
            })
            .expect("PEXTRW source lane extraction must survive optimization");
        let store = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Store {
                        width: MemWidth::B2,
                        ..
                    }
                )
            })
            .expect("PEXTRW scalar store must survive optimization");
        assert!(extraction < store);

        let mpsadbw = optimized(&[0x66, 0x44, 0x0F, 0x3A, 0x42, 0x08, 0x07]);
        let ops = &mpsadbw.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy MPSADBW alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy MPSADBW source load must survive optimization");
        let sad = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VMpsadbw {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy MPSADBW operation must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                        elem: VecElementType::I16,
                        ..
                    }
                )
            })
            .expect("legacy MPSADBW destination merge must survive optimization");
        assert!(alignment < load && load < sad && sad < destination_write);

        let vex_mpsadbw = optimized(&[0xC4, 0x63, 0x25, 0x42, 0x08, 0x38]);
        let ops = &vex_mpsadbw.blocks[0].ops;
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VEX VMPSADBW unaligned load must survive optimization");
        let sad = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VMpsadbw {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VEX VMPSADBW destination operation must survive optimization");
        assert!(load < sad);

        let psadbw = optimized(&[0x66, 0x44, 0x0F, 0xF6, 0x08]);
        let ops = &psadbw.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy PSADBW alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PSADBW source load must survive optimization");
        let sad = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VSadBytes {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy PSADBW operation must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                        elem: VecElementType::I64,
                        ..
                    }
                )
            })
            .expect("legacy PSADBW destination merge must survive optimization");
        assert!(alignment < load && load < sad && sad < destination_write);

        let evex_psadbw = optimized(&[0x62, 0xE1, 0x5D, 0x40, 0xF6, 0x18]);
        let ops = &evex_psadbw.blocks[0].ops;
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V512,
                        ..
                    }
                )
            })
            .expect("EVEX VPSADBW unaligned load must survive optimization");
        let sad = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VSadBytes {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                        width: VecWidth::V512,
                        ..
                    }
                )
            })
            .expect("EVEX VPSADBW destination operation must survive optimization");
        assert!(load < sad);

        let dpps = optimized(&[0x66, 0x44, 0x0F, 0x3A, 0x40, 0x08, 0xF1]);
        let ops = &dpps.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy DPPS alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy DPPS source load must survive optimization");
        let dot = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::X86DotProduct {
                        elem: VecElementType::F32,
                        width: VecWidth::V128,
                        imm: 0xF1,
                        ..
                    }
                )
            })
            .expect("legacy DPPS MXCSR side effect must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                        elem: VecElementType::F32,
                        ..
                    }
                )
            })
            .expect("legacy DPPS destination merge must survive optimization");
        assert!(alignment < load && load < dot && dot < destination_write);

        let vdpps = optimized(&[0xC4, 0x63, 0x25, 0x40, 0x08, 0xFF]);
        let ops = &vdpps.blocks[0].ops;
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VEX VDPPS unaligned load must survive optimization");
        let dot = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::X86DotProduct {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                        elem: VecElementType::F32,
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VEX VDPPS destination and MXCSR operation must survive optimization");
        assert!(load < dot);

        for (bytes, expected_sources) in [
            (
                &[0xC4, 0x42, 0x7F, 0xCC, 0xCA][..],
                vec![
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
                ],
            ),
            (
                &[0xC4, 0x42, 0x7F, 0xCD, 0xCA][..],
                vec![
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(10))),
                ],
            ),
            (
                &[0xC4, 0x42, 0x27, 0xCB, 0xCA][..],
                vec![
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(11))),
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
                ],
            ),
        ] {
            let sha = optimized(bytes);
            let operation = sha.blocks[0]
                .ops
                .iter()
                .find(|op| {
                    matches!(
                        op.kind,
                        OpKind::X86Sha512Msg1 { .. }
                            | OpKind::X86Sha512Msg2 { .. }
                            | OpKind::X86Sha512Rounds2 { .. }
                    )
                })
                .expect("SHA-512 operation must survive optimization");
            assert_eq!(operation.kind.source_vregs(), expected_sources);
        }

        for (bytes, rounds) in [
            (&[0xC4, 0x62, 0x20, 0xDA, 0x08][..], false),
            (&[0xC4, 0x63, 0x21, 0xDE, 0x08, 0x3F][..], true),
        ] {
            let sm3 = optimized(bytes);
            let ops = &sm3.blocks[0].ops;
            let load = ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::VLoad {
                            width: VecWidth::V128,
                            ..
                        }
                    )
                })
                .expect("SM3 memory source load must survive optimization");
            let operation = ops
                .iter()
                .position(|op| {
                    if rounds {
                        matches!(op.kind, OpKind::X86Sm3Rounds2 { .. })
                    } else {
                        matches!(op.kind, OpKind::X86Sm3Msg1 { .. })
                    }
                })
                .expect("SM3 operation must survive optimization");
            assert!(load < operation);
        }

        for bytes in [
            &[0xC4, 0x62, 0x26, 0xDA, 0x08][..],
            &[0xC4, 0x62, 0x27, 0xDA, 0x08][..],
        ] {
            let sm4 = optimized(bytes);
            let ops = &sm4.blocks[0].ops;
            let load = ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::VLoad {
                            width: VecWidth::V256,
                            ..
                        }
                    )
                })
                .expect("SM4 memory source load must survive optimization");
            let operation = ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::X86Sm4 {
                            width: VecWidth::V256,
                            ..
                        }
                    )
                })
                .expect("SM4 operation must survive optimization");
            assert!(load < operation);
        }

        let round = optimized(&[0x66, 0x44, 0x0F, 0x3A, 0x08, 0x08, 0x00]);
        let ops = &round.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .expect("legacy ROUNDPS alignment check must survive optimization");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("legacy ROUNDPS source load must survive optimization");
        let rounding = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::X86Round {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                        elem: VecElementType::F32,
                        lanes: 4,
                        ..
                    }
                )
            })
            .expect("ROUNDPS MXCSR side effect and destination must survive optimization");
        assert!(alignment < load && load < rounding);

        let vex_round = optimized(&[0xC4, 0x63, 0x21, 0x0B, 0x08, 0x04]);
        let ops = &vex_round.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B8,
                        ..
                    }
                )
            })
            .expect("VROUNDSD scalar load must survive optimization");
        let rounding = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::X86Round {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                        merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(11))),
                        mode: FpRoundMode::Dynamic,
                        ..
                    }
                )
            })
            .expect("VROUNDSD merge and MXCSR side effect must survive optimization");
        assert!(load < rounding);

        let evex = optimized(&[0x62, 0xF1, 0x7E, 0x09, 0x58, 0x10]);
        let ops = &evex.blocks[0].ops;
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::And {
                src1: VReg::Arch(ArchReg::X86(X86Reg::K(1))),
                ..
            }
        )));
        let pred_load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                )
            })
            .expect("masked EVEX memory source must retain conditional load");
        assert!(!ops.iter().any(|op| matches!(op.kind, OpKind::Load { .. })));
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBroadcast {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                        ..
                    }
                )
            })
            .expect("masked EVEX arithmetic must retain destination clear/write");
        assert!(pred_load < destination_write);
        assert!(
            ops.iter()
                .any(|op| matches!(op.kind, OpKind::Select { .. }))
        );

        let legacy_sqrt = optimized(&[0x0F, 0x51, 0x00]);
        let ops = &legacy_sqrt.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("faulting packed SQRTPS load must survive optimization");
        let first_destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        ..
                    }
                )
            })
            .expect("legacy SQRTPS XMM merge must survive optimization");
        assert!(
            load < first_destination_write,
            "legacy SQRTPS changed its destination before the load fault boundary"
        );
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VUnary {
                dst: VReg::Virtual(_),
                op: VecUnaryOp::FSqrt,
                ..
            }
        )));

        let evex_sqrt = optimized(&[0x62, 0xF1, 0x7E, 0x09, 0x51, 0x10]);
        let ops = &evex_sqrt.blocks[0].ops;
        let pred_load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                )
            })
            .expect("masked EVEX VSQRTSS conditional load must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBroadcast {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                        ..
                    }
                )
            })
            .expect("masked EVEX VSQRTSS destination clear/write must survive optimization");
        assert!(pred_load < destination_write);
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VUnary {
                op: VecUnaryOp::FSqrt,
                ..
            }
        )));
        assert!(
            ops.iter()
                .any(|op| matches!(op.kind, OpKind::Select { .. }))
        );

        let legacy_min = optimized(&[0xF3, 0x0F, 0x5D, 0x00]);
        let ops = &legacy_min.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B4,
                        ..
                    }
                )
            })
            .expect("faulting MINSS load must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        ..
                    }
                )
            })
            .expect("MINSS destination merge must survive optimization");
        assert!(load < destination_write);
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VX86MinMax {
                min: true,
                lanes: 1,
                ..
            }
        )));

        let evex_min = optimized(&[0x62, 0xF1, 0x7E, 0x09, 0x5D, 0x10]);
        let ops = &evex_min.blocks[0].ops;
        let pred_load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                )
            })
            .expect("masked EVEX VMINSS conditional load must survive optimization");
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBroadcast {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                        ..
                    }
                )
            })
            .expect("masked EVEX VMINSS destination write must survive optimization");
        assert!(pred_load < destination_write);
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VX86MinMax {
                min: true,
                lanes: 1,
                ..
            }
        )));
        assert!(
            ops.iter()
                .any(|op| matches!(op.kind, OpKind::Select { .. }))
        );

        let comi = optimized(&[0x66, 0x0F, 0x2F, 0x00]);
        let ops = &comi.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B8,
                        ..
                    }
                )
            })
            .expect("faulting COMISD load must survive optimization");
        let compare = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::X86FpCompare {
                        elem: VecElementType::F64,
                        signaling: true,
                        ..
                    }
                )
            })
            .expect("COMISD flag producer must survive optimization");
        assert!(load < compare);
        assert_eq!(ops[compare].kind.flags_written(), FlagSet::ALL_X86);

        let fp_to_int = optimized(&[0xF2, 0x48, 0x0F, 0x2D, 0x00]);
        let ops = &fp_to_int.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B8,
                        ..
                    }
                )
            })
            .expect("faulting CVTSD2SI load must survive optimization");
        let conversion = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::X86FpToInt {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                        elem: VecElementType::F64,
                        int_width: OpWidth::W64,
                        truncate: false,
                        ..
                    }
                )
            })
            .expect("CVTSD2SI conversion must survive optimization");
        assert!(load < conversion);
        assert!(ops[conversion].kind.flags_written().is_empty());

        let int_to_fp = optimized(&[0xF2, 0x48, 0x0F, 0x2A, 0x08]);
        let ops = &int_to_fp.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B8,
                        sign: SignExtend::Sign,
                        ..
                    }
                )
            })
            .expect("faulting CVTSI2SD load must survive optimization");
        let conversion = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::X86IntToFp {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                        merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                        elem: VecElementType::F64,
                        int_width: OpWidth::W64,
                        zero_upper: false,
                        ..
                    }
                )
            })
            .expect("CVTSI2SD conversion must survive optimization");
        assert!(load < conversion);
        assert!(ops[conversion].kind.flags_written().is_empty());

        let fp_convert = optimized(&[0xF2, 0x0F, 0x5A, 0x00]);
        let ops = &fp_convert.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B8,
                        ..
                    }
                )
            })
            .expect("faulting CVTSD2SS load must survive optimization");
        let conversion = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::X86FpConvert {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        from: VecElementType::F64,
                        to: VecElementType::F32,
                        zero_upper: false,
                        ..
                    }
                )
            })
            .expect("CVTSD2SS conversion must survive optimization");
        assert!(load < conversion);
        assert!(ops[conversion].kind.flags_written().is_empty());

        let packed_fp_convert = optimized(&[0x66, 0x0F, 0x5A, 0x00]);
        let ops = &packed_fp_convert.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("faulting CVTPD2PS load must survive optimization");
        let conversion = ops
            .iter()
            .position(|op| {
                matches!(
                    op,
                    SmirOp {
                        kind: OpKind::X86PackedFpConvert {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                            from: VecElementType::F64,
                            to: VecElementType::F32,
                            lanes: 2,
                            dst_width: VecWidth::V128,
                            zero_upper: false,
                            ..
                        },
                        x86_hint: Some(X86OpHint::SseOp { .. }),
                        ..
                    }
                )
            })
            .expect("CVTPD2PS conversion must survive optimization");
        assert!(load < conversion);
        assert!(ops[conversion].kind.flags_written().is_empty());

        let evex_packed_fp_convert = optimized(&[0x62, 0xF1, 0x7C, 0x4B, 0x5A, 0x00]);
        let ops = &evex_packed_fp_convert.blocks[0].ops;
        let conversion = ops
            .iter()
            .position(|op| {
                matches!(
                    op,
                    SmirOp {
                        kind: OpKind::X86PackedFpConvert {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
                            lanes: 8,
                            ..
                        },
                        x86_hint: Some(X86OpHint::EvexOp { .. }),
                        ..
                    }
                )
            })
            .expect("masked EVEX packed conversion removed");
        assert_eq!(
            ops[..conversion]
                .iter()
                .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                .count(),
            8,
            "per-lane fault-suppressing loads must precede conversion"
        );

        for (name, bytes, load) in [
            ("LDMXCSR", &[0x0F, 0xAE, 0x10][..], true),
            ("VSTMXCSR", &[0xC5, 0xF8, 0xAE, 0x18][..], false),
        ] {
            let function = optimized(bytes);
            assert!(
                function.blocks[0].ops.iter().any(|op| {
                    (load && matches!(op.kind, OpKind::X86LoadMxcsr { .. }))
                        || (!load && matches!(op.kind, OpKind::X86StoreMxcsr { .. }))
                }),
                "{name}: architectural MXCSR operation removed"
            );
        }

        let cldemote = optimized(&[0x0F, 0x1C, 0x00]);
        let cldemote = cldemote.blocks[0]
            .ops
            .iter()
            .find(|op| {
                matches!(
                    op.kind,
                    OpKind::X86CacheControl {
                        kind: X86CacheControlKind::Cldemote,
                        ..
                    }
                )
            })
            .expect("CLDEMOTE hint removed by DCE");
        assert!(!cldemote.kind.reads_memory());
        assert!(cldemote.kind.has_side_effects());

        for (name, bytes, expected) in [
            ("FNINIT", &[0xDB, 0xE3][..], X86X87ControlKind::Init),
            (
                "FNCLEX",
                &[0xDB, 0xE2][..],
                X86X87ControlKind::ClearExceptions,
            ),
            (
                "FLDCW",
                &[0xD9, 0x28][..],
                X86X87ControlKind::LoadControlWord,
            ),
            (
                "FNSTCW",
                &[0xD9, 0x38][..],
                X86X87ControlKind::StoreControlWord,
            ),
            (
                "FNSTSW",
                &[0xDD, 0x38][..],
                X86X87ControlKind::StoreStatusWord,
            ),
            (
                "FLDENV m28byte",
                &[0xD9, 0x20][..],
                X86X87ControlKind::LoadEnvironment(crate::smir::ir::ops::X86X87EnvWidth::W32),
            ),
            (
                "FNSTENV m14byte",
                &[0x66, 0xD9, 0x30][..],
                X86X87ControlKind::StoreEnvironment(crate::smir::ir::ops::X86X87EnvWidth::W16),
            ),
            (
                "FRSTOR m108byte",
                &[0xDD, 0x20][..],
                X86X87ControlKind::RestoreState(crate::smir::ir::ops::X86X87EnvWidth::W32),
            ),
            (
                "FNSAVE m94byte",
                &[0x66, 0xDD, 0x30][..],
                X86X87ControlKind::SaveState(crate::smir::ir::ops::X86X87EnvWidth::W16),
            ),
        ] {
            let function = optimized(bytes);
            assert!(
                function.blocks[0].ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::X86X87Control { kind, .. } if kind == expected
                )),
                "{name}: x87 environment operation removed"
            );
        }

        for (name, bytes, expected) in [
            ("FLD m32fp", &[0xD9, 0x00][..], X86X87DataKind::LoadSingle),
            ("FLD m64fp", &[0xDD, 0x00][..], X86X87DataKind::LoadDouble),
            ("FILD m64int", &[0xDF, 0x28][..], X86X87DataKind::LoadInt64),
            ("FBLD m80bcd", &[0xDF, 0x20][..], X86X87DataKind::LoadBcd),
            (
                "FISTP m64int",
                &[0xDF, 0x38][..],
                X86X87DataKind::StoreInteger {
                    width: crate::smir::ir::ops::X86X87IntWidth::I64,
                    pop: true,
                    truncate: false,
                },
            ),
            (
                "FSTP m64fp",
                &[0xDD, 0x18][..],
                X86X87DataKind::StoreFloat {
                    width: crate::smir::ir::ops::X86X87FloatWidth::F64,
                    pop: true,
                },
            ),
            ("FBSTP m80bcd", &[0xDF, 0x30][..], X86X87DataKind::StoreBcd),
            ("FLD m80fp", &[0xDB, 0x28][..], X86X87DataKind::LoadExtended),
            (
                "FSTP m80fp",
                &[0xDB, 0x38][..],
                X86X87DataKind::StorePopExtended,
            ),
            ("FLD ST(3)", &[0xD9, 0xC3][..], X86X87DataKind::LoadRegister),
            ("FXCH ST(1)", &[0xD9, 0xC9][..], X86X87DataKind::Exchange),
            ("FFREE ST(2)", &[0xDD, 0xC2][..], X86X87DataKind::Free),
            ("FCHS", &[0xD9, 0xE0][..], X86X87DataKind::ChangeSign),
            ("FINCSTP", &[0xD9, 0xF7][..], X86X87DataKind::IncrementTop),
            (
                "FLDPI",
                &[0xD9, 0xEB][..],
                X86X87DataKind::LoadConstant(crate::smir::ir::ops::X86X87Constant::Pi),
            ),
            (
                "FCMOVE ST(2)",
                &[0xDA, 0xCA][..],
                X86X87DataKind::ConditionalMove(Condition::Eq),
            ),
            ("FXAM", &[0xD9, 0xE5][..], X86X87DataKind::Examine),
            ("FTST", &[0xD9, 0xE4][..], X86X87DataKind::TestZero),
            ("FRNDINT", &[0xD9, 0xFC][..], X86X87DataKind::RoundInteger),
            ("FXTRACT", &[0xD9, 0xF4][..], X86X87DataKind::Extract),
            (
                "FPREM1",
                &[0xD9, 0xF5][..],
                X86X87DataKind::Remainder { nearest: true },
            ),
            (
                "FPREM",
                &[0xD9, 0xF8][..],
                X86X87DataKind::Remainder { nearest: false },
            ),
            ("FSCALE", &[0xD9, 0xFD][..], X86X87DataKind::Scale),
            ("FSQRT", &[0xD9, 0xFA][..], X86X87DataKind::SquareRoot),
            (
                "FADD m64fp",
                &[0xDC, 0x00][..],
                X86X87DataKind::AddSubtract {
                    source: crate::smir::ir::ops::X86X87ArithmeticSource::Double,
                    destination: crate::smir::ir::ops::X86X87ArithmeticDestination::St0,
                    pop: false,
                    subtract: false,
                    reverse: false,
                },
            ),
            (
                "FSUB ST(3),ST(0)",
                &[0xDC, 0xEB][..],
                X86X87DataKind::AddSubtract {
                    source: crate::smir::ir::ops::X86X87ArithmeticSource::Register,
                    destination: crate::smir::ir::ops::X86X87ArithmeticDestination::StI,
                    pop: false,
                    subtract: true,
                    reverse: false,
                },
            ),
            (
                "FSUBRP ST(1),ST(0)",
                &[0xDE, 0xE1][..],
                X86X87DataKind::AddSubtract {
                    source: crate::smir::ir::ops::X86X87ArithmeticSource::Register,
                    destination: crate::smir::ir::ops::X86X87ArithmeticDestination::StI,
                    pop: true,
                    subtract: true,
                    reverse: true,
                },
            ),
            (
                "FISUBR m32int",
                &[0xDA, 0x28][..],
                X86X87DataKind::AddSubtract {
                    source: crate::smir::ir::ops::X86X87ArithmeticSource::Int32,
                    destination: crate::smir::ir::ops::X86X87ArithmeticDestination::St0,
                    pop: false,
                    subtract: true,
                    reverse: true,
                },
            ),
            (
                "FDIV m64fp",
                &[0xDC, 0x30][..],
                X86X87DataKind::Divide {
                    source: crate::smir::ir::ops::X86X87ArithmeticSource::Double,
                    destination: crate::smir::ir::ops::X86X87ArithmeticDestination::St0,
                    pop: false,
                    reverse: false,
                },
            ),
            (
                "FDIVP ST(1),ST(0)",
                &[0xDE, 0xF9][..],
                X86X87DataKind::Divide {
                    source: crate::smir::ir::ops::X86X87ArithmeticSource::Register,
                    destination: crate::smir::ir::ops::X86X87ArithmeticDestination::StI,
                    pop: true,
                    reverse: false,
                },
            ),
            (
                "FIDIVR m32int",
                &[0xDA, 0x38][..],
                X86X87DataKind::Divide {
                    source: crate::smir::ir::ops::X86X87ArithmeticSource::Int32,
                    destination: crate::smir::ir::ops::X86X87ArithmeticDestination::St0,
                    pop: false,
                    reverse: true,
                },
            ),
            (
                "FMUL m64fp",
                &[0xDC, 0x08][..],
                X86X87DataKind::Multiply {
                    source: crate::smir::ir::ops::X86X87ArithmeticSource::Double,
                    destination: crate::smir::ir::ops::X86X87ArithmeticDestination::St0,
                    pop: false,
                },
            ),
            (
                "FMULP ST(1),ST(0)",
                &[0xDE, 0xC9][..],
                X86X87DataKind::Multiply {
                    source: crate::smir::ir::ops::X86X87ArithmeticSource::Register,
                    destination: crate::smir::ir::ops::X86X87ArithmeticDestination::StI,
                    pop: true,
                },
            ),
            (
                "FIMUL m32int",
                &[0xDA, 0x08][..],
                X86X87DataKind::Multiply {
                    source: crate::smir::ir::ops::X86X87ArithmeticSource::Int32,
                    destination: crate::smir::ir::ops::X86X87ArithmeticDestination::St0,
                    pop: false,
                },
            ),
            (
                "FCOM m32fp",
                &[0xD8, 0x10][..],
                X86X87DataKind::Compare {
                    source: crate::smir::ir::ops::X86X87CompareSource::Single,
                    unordered: false,
                    pop: 0,
                    eflags: false,
                },
            ),
            (
                "FUCOMIP ST(1)",
                &[0xDF, 0xE9][..],
                X86X87DataKind::Compare {
                    source: crate::smir::ir::ops::X86X87CompareSource::Register,
                    unordered: true,
                    pop: 1,
                    eflags: true,
                },
            ),
            (
                "FICOMP m32int",
                &[0xDA, 0x18][..],
                X86X87DataKind::Compare {
                    source: crate::smir::ir::ops::X86X87CompareSource::Int32,
                    unordered: false,
                    pop: 1,
                    eflags: false,
                },
            ),
        ] {
            let function = optimized(bytes);
            assert!(
                function.blocks[0].ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::X86X87Data { kind, .. } if kind == expected
                )),
                "{name}: x87 data operation removed"
            );
        }

        let fcomi = optimized(&[0xDB, 0xF1]);
        let fcomi = fcomi.blocks[0]
            .ops
            .iter()
            .find(|op| {
                matches!(
                    op.kind,
                    OpKind::X86X87Data {
                        kind: X86X87DataKind::Compare { eflags: true, .. },
                        ..
                    }
                )
            })
            .expect("FCOMI removed");
        assert_eq!(fcomi.kind.flags_written(), FlagSet::ALL_X86);
        assert_eq!(
            fcomi.kind.flags_must_write(),
            FlagSet::OF.union(FlagSet::SF).union(FlagSet::AF)
        );
        let fcmovbe = optimized(&[0xDA, 0xD1]);
        let fcmovbe = fcmovbe.blocks[0]
            .ops
            .iter()
            .find(|op| {
                matches!(
                    op.kind,
                    OpKind::X86X87Data {
                        kind: X86X87DataKind::ConditionalMove(Condition::Ule),
                        ..
                    }
                )
            })
            .expect("FCMOVBE removed");
        assert_eq!(fcmovbe.kind.flags_read(), FlagSet::CF.union(FlagSet::ZF));

        for (name, bytes, save) in [
            ("FXSAVE64", &[0x48, 0x0F, 0xAE, 0x00][..], true),
            ("FXRSTOR64", &[0x48, 0x0F, 0xAE, 0x08][..], false),
        ] {
            let function = optimized(bytes);
            assert!(
                function.blocks[0].ops.iter().any(|op| {
                    (save && matches!(op.kind, OpKind::X86FxSave { .. }))
                        || (!save && matches!(op.kind, OpKind::X86FxRstor { .. }))
                }),
                "{name}: state operation removed"
            );
        }

        for (name, bytes, get) in [
            ("XGETBV", &[0x0F, 0x01, 0xD0][..], true),
            ("XSETBV", &[0x0F, 0x01, 0xD1][..], false),
        ] {
            let function = optimized(bytes);
            assert!(
                function.blocks[0].ops.iter().any(|op| {
                    (get && matches!(op.kind, OpKind::X86XGetBv { .. }))
                        || (!get && matches!(op.kind, OpKind::X86XSetBv { .. }))
                }),
                "{name}: XCR operation removed"
            );
        }

        for (name, bytes, save) in [
            ("XSAVE64", &[0x48, 0x0F, 0xAE, 0x23][..], true),
            ("XSAVEOPT64", &[0x48, 0x0F, 0xAE, 0x33][..], true),
            ("XRSTOR64", &[0x48, 0x0F, 0xAE, 0x2B][..], false),
            ("XSAVEC64", &[0x48, 0x0F, 0xC7, 0x23][..], true),
            ("XSAVES64", &[0x48, 0x0F, 0xC7, 0x2B][..], true),
            ("XRSTORS64", &[0x48, 0x0F, 0xC7, 0x1B][..], false),
        ] {
            let function = optimized(bytes);
            assert!(
                function.blocks[0].ops.iter().any(|op| {
                    (save && matches!(op.kind, OpKind::X86XSave { .. }))
                        || (!save && matches!(op.kind, OpKind::X86XRstor { .. }))
                }),
                "{name}: extended-state operation removed"
            );
        }

        for (name, bytes, predicate) in [
            ("CMPXCHG16B", &[0xF0, 0x48, 0x0F, 0xC7, 0x0E][..], 0u8),
            ("RDRAND", &[0x48, 0x0F, 0xC7, 0xF0][..], 1),
            ("RDSEED", &[0x48, 0x0F, 0xC7, 0xF8][..], 2),
        ] {
            let function = optimized(bytes);
            let op = function.blocks[0]
                .ops
                .iter()
                .find(|op| match predicate {
                    0 => matches!(op.kind, OpKind::X86Cmpxchg8b16b { .. }),
                    1 => matches!(op.kind, OpKind::X86Random { seed: false, .. }),
                    _ => matches!(op.kind, OpKind::X86Random { seed: true, .. }),
                })
                .unwrap_or_else(|| panic!("{name}: Group-9 operation removed"));
            assert_eq!(
                op.kind.flags_written(),
                if predicate == 0 {
                    FlagSet::ZF
                } else {
                    FlagSet::ALL_X86
                }
            );
            assert_eq!(op.kind.flags_must_write(), op.kind.flags_written());
        }

        {
            let name = "ADDPS";
            let function = optimized(&[0x0F, 0x58, 0x00]);
            let ops = &function.blocks[0].ops;
            let load = ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::VLoad {
                            width: VecWidth::V128,
                            ..
                        }
                    )
                })
                .unwrap_or_else(|| panic!("{name}: faulting VLoad removed"));
            let first_destination_write = ops
                .iter()
                .position(|op| {
                    matches!(
                        op,
                        SmirOp {
                            kind: OpKind::VAdd {
                                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                                ..
                            },
                            x86_hint: Some(X86OpHint::SseOp { .. }),
                            ..
                        }
                    )
                })
                .unwrap_or_else(|| panic!("{name}: hinted destination write removed"));
            assert!(
                load < first_destination_write,
                "{name}: write before fault boundary"
            );
        }

        {
            let name = "PADDSB";
            let function = optimized(&[0x66, 0x0F, 0xEC, 0x00]);
            let ops = &function.blocks[0].ops;
            let load = ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::VLoad {
                            width: VecWidth::V128,
                            ..
                        }
                    )
                })
                .unwrap_or_else(|| panic!("{name}: faulting VLoad removed"));
            let saturated_write = ops
                .iter()
                .position(|op| {
                    matches!(
                        op,
                        SmirOp {
                            kind: OpKind::VAddSubSat {
                                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                                elem: VecElementType::I8,
                                subtract: false,
                                signed: true,
                                ..
                            },
                            x86_hint: Some(X86OpHint::SseOp { .. }),
                            ..
                        }
                    )
                })
                .unwrap_or_else(|| panic!("{name}: saturated destination write removed"));
            assert!(
                load < saturated_write,
                "{name}: write before fault boundary"
            );
        }

        for (name, bytes, vector_load) in [
            ("VBCSTNEBF162PS", &[0xC4, 0x62, 0x7A, 0xB1, 0x08][..], false),
            ("VCVTNEEBF162PS", &[0xC4, 0x62, 0x7E, 0xB0, 0x08][..], true),
        ] {
            let function = optimized(bytes);
            let ops = &function.blocks[0].ops;
            let load = ops
                .iter()
                .position(|op| {
                    if vector_load {
                        matches!(
                            op.kind,
                            OpKind::VLoad {
                                width: VecWidth::V256,
                                ..
                            }
                        )
                    } else {
                        matches!(
                            op.kind,
                            OpKind::Load {
                                width: MemWidth::B2,
                                ..
                            }
                        )
                    }
                })
                .unwrap_or_else(|| panic!("{name}: faulting source load removed"));
            let conversion = ops
                .iter()
                .position(|op| matches!(op.kind, OpKind::X86Convert16ToFp32 { .. }))
                .unwrap_or_else(|| panic!("{name}: conversion removed"));
            assert!(load < conversion, "{name}: write before fault boundary");
        }

        let packed_shift = optimized(&[0xC4, 0xC1, 0x35, 0x73, 0xDA, 0x01]);
        assert!(packed_shift.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86PackedShiftImm {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Ymm(10))),
                width: VecWidth::V256,
                shift: ShiftOp::Lsr,
                amount: 1,
                byte_lane: true,
                ..
            }
        )));

        let legacy_shift = optimized(&[0x66, 0x0F, 0x73, 0xF8, 0x01]);
        assert!(legacy_shift.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86PackedShiftImm {
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                width: VecWidth::V128,
                shift: ShiftOp::Lsl,
                amount: 1,
                byte_lane: true,
                ..
            }
        )));

        let e4nf_shift = optimized(&[0x62, 0xF1, 0x7D, 0x49, 0x71, 0x10, 0x03]);
        let ops = &e4nf_shift.blocks[0].ops;
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V512,
                        ..
                    }
                )
            })
            .expect("E4NF immediate word-shift load must survive optimization");
        let shift = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86PackedShiftImm { .. }))
            .expect("EVEX immediate word shift must survive optimization");
        let write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: VecElementType::I16,
                        ..
                    }
                )
            })
            .expect("EVEX immediate word-shift destination write must survive optimization");
        assert!(load < shift && shift < write);

        let e4_shift = optimized(&[0x62, 0xF1, 0x7D, 0x49, 0x72, 0x10, 0x03]);
        assert_eq!(
            e4_shift.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16,
        );

        let packed_shift_count = optimized(&[0xC4, 0x41, 0x35, 0xD2, 0xC2]);
        assert!(packed_shift_count.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86PackedShift {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(8))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                width: VecWidth::V256,
                elem: VecElementType::I32,
                shift: ShiftOp::Lsr,
                ..
            }
        )));

        let packed_shift_variable = optimized(&[0x62, 0xF2, 0xED, 0x08, 0x10, 0xCB]);
        assert!(
            packed_shift_variable.blocks[0]
                .ops
                .iter()
                .any(|op| matches!(
                    op.kind,
                    OpKind::X86PackedShiftVariable {
                        src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                        count: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                        elem: VecElementType::I16,
                        shift: ShiftOp::Lsr,
                        ..
                    }
                ))
        );

        let packed_rotate = optimized(&[0x62, 0xF1, 0x75, 0x08, 0x72, 0xCA, 0x07]);
        assert!(packed_rotate.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86PackedRotate {
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                count: None,
                amount: 7,
                width: VecWidth::V128,
                elem: VecElementType::I32,
                left: true,
                ..
            }
        )));

        let masked_rotate = optimized(&[0x62, 0xF2, 0x4D, 0x5A, 0x14, 0x68, 0x01]);
        let rotate_ops = &masked_rotate.blocks[0].ops;
        assert_eq!(
            rotate_ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16,
        );
        assert!(rotate_ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86PackedRotate {
                count: Some(_),
                width: VecWidth::V512,
                elem: VecElementType::I32,
                left: false,
                ..
            }
        )));

        let ternary = optimized(&[0x62, 0xF3, 0x6D, 0x08, 0x25, 0xCB, 0x96]);
        assert!(ternary.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86TernaryLogic {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                src3: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                imm: 0x96,
                width: VecWidth::V128,
                ..
            }
        )));

        let masked_ternary = optimized(&[0x62, 0xC3, 0x6D, 0x57, 0x25, 0x4D, 0x7F, 0xE4]);
        let ternary_ops = &masked_ternary.blocks[0].ops;
        assert_eq!(
            ternary_ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16,
        );
        assert!(
            ternary_ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86TernaryLogic { imm: 0xE4, .. }))
        );

        let funnel = optimized(&[0x62, 0xF3, 0xED, 0x08, 0x70, 0xCB, 0x07]);
        assert!(funnel.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86PackedFunnelShift {
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                fill: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                count: None,
                amount: 7,
                elem: VecElementType::I16,
                left: true,
                ..
            }
        )));

        let variable_funnel = optimized(&[0x62, 0xF2, 0xED, 0x08, 0x73, 0xCB]);
        assert!(variable_funnel.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86PackedFunnelShift {
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                fill: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                count: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(3)))),
                elem: VecElementType::I64,
                left: false,
                ..
            }
        )));

        let multishift = optimized(&[0x62, 0xF2, 0xED, 0x08, 0x83, 0xCB]);
        assert!(multishift.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86MultiShiftQB {
                control: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                source: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                width: VecWidth::V128,
                ..
            }
        )));

        let e4nf_multishift = optimized(&[0x62, 0x62, 0x8D, 0xC1, 0x83, 0x78, 0x01]);
        assert!(e4nf_multishift.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V512,
                ..
            }
        )));
        assert!(
            !e4nf_multishift.blocks[0]
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );

        let vector_align = optimized(&[0x62, 0xF3, 0x6D, 0x08, 0x03, 0xCB, 0x01]);
        assert_eq!(
            vector_align.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        elem: VecElementType::I32,
                        ..
                    }
                ))
                .count(),
            4,
        );
        let e4nf_align = optimized(&[0x62, 0xC3, 0x6D, 0x47, 0x03, 0x4D, 0x01, 0x1F]);
        assert!(e4nf_align.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V512,
                ..
            }
        )));
        assert!(
            !e4nf_align.blocks[0]
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );

        for bytes in [
            &[0x66, 0x45, 0x0F, 0xF7, 0xC1][..],
            &[0xC4, 0x41, 0x79, 0xF7, 0xC1][..],
        ] {
            let maskmov = optimized(bytes);
            let ops = &maskmov.blocks[0].ops;
            assert_eq!(
                ops.iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::PredStore {
                            width: MemWidth::B1,
                            ..
                        }
                    ))
                    .count(),
                16,
                "MASKMOVDQU byte stores removed for {bytes:02X?}",
            );
            assert!(ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(8))),
                    lane: 15,
                    elem: VecElementType::I8,
                    ..
                }
            )));
            assert!(ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                    lane: 15,
                    elem: VecElementType::I8,
                    ..
                }
            )));
        }

        let addr32_maskmov = optimized(&[0x67, 0xC4, 0x41, 0x79, 0xF7, 0xC1]);
        let ops = &addr32_maskmov.blocks[0].ops;
        let truncated = ops
            .iter()
            .find_map(|op| match op.kind {
                OpKind::And {
                    dst,
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Rdi)),
                    src2: SrcOperand::Imm(0xFFFF_FFFF),
                    width: OpWidth::W64,
                    ..
                } => Some(dst),
                _ => None,
            })
            .expect("optimizer removed MASKMOVDQU EDI truncation");
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredStore {
                addr: Address::BaseOffset {
                    base,
                    offset: 15,
                    ..
                },
                ..
            } if base == truncated
        )));

        for (bytes, loads, stores) in [
            (&[0xC4, 0xE2, 0x75, 0x2C, 0x17][..], 8usize, 0usize),
            (&[0xC4, 0xE2, 0xF1, 0x8E, 0x17][..], 0, 2),
        ] {
            let masked_memory = optimized(bytes);
            let ops = &masked_memory.blocks[0].ops;
            assert_eq!(
                ops.iter()
                    .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                    .count(),
                loads,
            );
            assert_eq!(
                ops.iter()
                    .filter(|op| matches!(op.kind, OpKind::PredStore { .. }))
                    .count(),
                stores,
            );
            assert!(ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1) | X86Reg::Ymm(1))),
                    ..
                }
            )));
        }

        let vex_gather = optimized(&[0xC4, 0xE2, 0x75, 0x90, 0x1C, 0x90]);
        let ops = &vex_gather.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            8,
        );
        let first_load = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::PredLoad { .. }))
            .expect("VPGATHERDD loads removed");
        let first_commit = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                        ..
                    }
                )
            })
            .expect("VPGATHERDD destination commits removed");
        assert!(first_load < first_commit);
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                        ..
                    }
                ))
                .count(),
            8,
            "restart mask updates must survive optimization",
        );

        let evex_gather = optimized(&[0x62, 0xE2, 0x7D, 0x43, 0x92, 0x14, 0x88]);
        let ops = &evex_gather.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16,
        );
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::And {
                dst: VReg::Arch(ArchReg::X86(X86Reg::K(3))),
                flags: FlagUpdate::None,
                ..
            }
        )));
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                sign: SignExtend::Sign,
                ..
            }
        )));

        let evex_scatter = optimized(&[0x62, 0xF2, 0x7D, 0x09, 0xA0, 0x0C, 0x90]);
        let ops = &evex_scatter.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredStore {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            4,
        );
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::And {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::K(1))),
                        flags: FlagUpdate::None,
                        ..
                    }
                ))
                .count(),
            5,
            "scatter mask normalization and per-lane restart updates removed",
        );
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                sign: SignExtend::Sign,
                ..
            }
        )));

        let evex_aes = optimized(&[0x62, 0xE2, 0x5D, 0x20, 0xDE, 0x68, 0x02]);
        let ops = &evex_aes.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        addr: Address::BaseOffset { offset: 64, .. },
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("EVEX VAESDEC full-tuple load removed");
        let aes = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::X86Aes {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(21))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(20))),
                        op: X86AesOp::Dec,
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("EVEX VAESDEC computation removed");
        assert!(load < aes, "VAESDEC moved before its memory fault boundary");

        let evex_fma = optimized(&[0x62, 0xF2, 0x65, 0xD9, 0xA6, 0x10]);
        let ops = &evex_fma.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16,
        );
        let first_fma = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::VFma { .. }))
            .expect("EVEX FMA computation removed");
        let last_load = ops
            .iter()
            .rposition(|op| matches!(op.kind, OpKind::PredLoad { .. }))
            .expect("EVEX FMA masked broadcast loads removed");
        assert!(last_load < first_fma, "FMA moved before its fault boundary");
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(op.kind, OpKind::Select { .. }))
                .count(),
            16,
        );

        let horizontal = optimized(&[0xC5, 0xFF, 0x7C, 0x50, 0x20]);
        let ops = &horizontal.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VHADDPS source load removed");
        let arithmetic = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::FAdd { .. }))
            .expect("VHADDPS arithmetic removed");
        assert!(load < arithmetic);
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                width: VecWidth::V256,
                ..
            }
        )));

        let legacy_horizontal = optimized(&[0x66, 0x0F, 0x7D, 0x00]);
        assert!(
            legacy_horizontal.blocks[0]
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        );

        let reciprocal = optimized(&[0xC5, 0xFC, 0x53, 0x50, 0x20]);
        let ops = &reciprocal.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("VRCPPS source load removed");
        let estimate = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VUnary {
                        op: VecUnaryOp::FRecipEstimate,
                        lanes: 8,
                        ..
                    }
                )
            })
            .expect("VRCPPS estimate removed");
        assert!(load < estimate, "VRCPPS moved before its fault boundary");
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                width: VecWidth::V256,
                ..
            }
        )));

        let legacy_reciprocal = optimized(&[0x0F, 0x52, 0x00]);
        assert!(
            legacy_reciprocal.blocks[0]
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        );

        let masked_shift_count = optimized(&[0x62, 0xF1, 0xF5, 0x49, 0xF3, 0x40, 0x04]);
        let ops = &masked_shift_count.blocks[0].ops;
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .expect("E4NF packed shift Mem128 load must survive optimization");
        let shift = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::X86PackedShift {
                        width: VecWidth::V512,
                        elem: VecElementType::I64,
                        shift: ShiftOp::Lsl,
                        ..
                    }
                )
            })
            .expect("packed shift-by-count computation must survive optimization");
        let write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: VecElementType::I64,
                        ..
                    }
                )
            })
            .expect("masked packed shift destination write must survive optimization");
        assert!(load < shift && shift < write);

        let packed_shuffle = optimized(&[0xC4, 0x41, 0x7D, 0x70, 0xCA, 0x1B]);
        assert!(packed_shuffle.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VShuffle {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(10))),
                elem: VecElementType::I32,
                lanes: 8,
                ..
            }
        )));

        let masked_shuffle = optimized(&[0x62, 0xE1, 0x7D, 0x4B, 0x70, 0x08, 0x1B]);
        let masked_ops = &masked_shuffle.blocks[0].ops;
        let load = masked_ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V512,
                        ..
                    }
                )
            })
            .expect("masked VPSHUFD E4NF load removed");
        let shuffle = masked_ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VShuffle {
                        elem: VecElementType::I32,
                        lanes: 16,
                        ..
                    }
                )
            })
            .expect("masked VPSHUFD shuffle removed");
        assert!(
            load < shuffle,
            "masked VPSHUFD reordered before its E4NF load"
        );
        assert_eq!(
            masked_ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::Select { .. }))
                .count(),
            16
        );

        let two_source_shuffle = optimized(&[0xC4, 0x41, 0x2C, 0xC6, 0xCB, 0xE4]);
        assert!(two_source_shuffle.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VShuffle {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(10))),
                src2: Some(VReg::Arch(ArchReg::X86(X86Reg::Ymm(11)))),
                elem: VecElementType::F32,
                lanes: 8,
                ..
            }
        )));

        let duplicate_move = optimized(&[0xC4, 0x41, 0x7E, 0x12, 0xCA]);
        assert!(duplicate_move.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VShuffle {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(10))),
                src2: None,
                elem: VecElementType::F32,
                lanes: 8,
                ..
            }
        )));

        let masked_sat = optimized(&[0x62, 0xF1, 0x7D, 0xC9, 0xEC, 0xD1]);
        assert!(masked_sat.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VAddSubSat {
                elem: VecElementType::I8,
                lanes: 64,
                subtract: false,
                signed: true,
                ..
            }
        )));
        assert_eq!(
            masked_sat.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::Select {
                        width: OpWidth::W8,
                        ..
                    }
                ))
                .count(),
            64,
        );

        let movups = optimized(&[0x0F, 0x10, 0x00]);
        assert!(movups.blocks[0].ops.iter().any(|op| matches!(
            op,
            SmirOp {
                kind: OpKind::VLoad {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    width: VecWidth::V128,
                    ..
                },
                x86_hint: Some(X86OpHint::SseMov { .. }),
                ..
            }
        )));
    }

    // Regression for issue #108: OpKind::SatN ORs the Hexagon USR:OVF sticky bit
    // as a side effect, but that write is invisible to dests(). DCE must therefore
    // keep a SatN that can set OVF (set_ovf == true) even when its data result is
    // dead — yet may still drop one that cannot (set_ovf == false). The SatN
    // writes a virtual temp that is never read (so its data result is dead and not
    // kept alive by the frontier), isolating the decision to the side effect.
    #[test]
    fn issue_108_dce_keeps_satn_with_ovf_side_effect() {
        use crate::smir::ir::FunctionBuilder;

        fn satn_count_after_opt(set_ovf: bool) -> usize {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            let tmp = builder.alloc_vreg();
            let dead = builder.alloc_vreg();
            builder.push_op(
                0x1000,
                OpKind::Mov {
                    dst: tmp,
                    src: SrcOperand::Imm(0x8000),
                    width: OpWidth::W64,
                },
            );
            builder.push_op(
                0x1004,
                OpKind::SatN {
                    dst: dead,
                    src: SrcOperand::Reg(tmp),
                    sat_bits: 16,
                    signed: true,
                    set_ovf,
                    width: OpWidth::W64,
                },
            );
            builder.set_terminator(Terminator::Trap {
                kind: crate::smir::ir::TrapKind::Halt,
            });
            let mut func = builder.finish();
            optimize_function(&mut func, OptLevel::O2);
            func.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::SatN { .. }))
                .count()
        }

        assert_eq!(
            satn_count_after_opt(true),
            1,
            "a SatN that can set USR:OVF must survive DCE even with a dead data result",
        );
        assert_eq!(
            satn_count_after_opt(false),
            0,
            "a SatN with set_ovf=false and a dead data result has no side effect and is removable",
        );
    }

    // Regression for issue #112: PredStore writes memory (writes_memory() == true),
    // so redundant-load elimination must invalidate its cached loads across one.
    // A `Load X; PredStore X; Load X` sequence must keep BOTH loads — forwarding the
    // second from the first would read stale memory if the PredStore committed.
    #[test]
    fn issue_112_redundant_load_elim_invalidates_on_pred_store() {
        use crate::smir::ir::FunctionBuilder;
        use crate::smir::ir::types::SignExtend;

        let r0 = VReg::Arch(ArchReg::Hexagon(HexagonReg::R(0)));
        let r1 = VReg::Arch(ArchReg::Hexagon(HexagonReg::R(1)));
        let r2 = VReg::Arch(ArchReg::Hexagon(HexagonReg::R(2)));
        let r3 = VReg::Arch(ArchReg::Hexagon(HexagonReg::R(3)));
        let p0 = VReg::Arch(ArchReg::Hexagon(HexagonReg::P(0)));

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: r1,
                addr: Address::Direct(r0),
                width: MemWidth::B4,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1004,
            OpKind::PredStore {
                src: SrcOperand::Reg(r2),
                cond: p0,
                addr: Address::Direct(r0),
                width: MemWidth::B4,
            },
        );
        builder.push_op(
            0x1008,
            OpKind::Load {
                dst: r3,
                addr: Address::Direct(r0),
                width: MemWidth::B4,
                sign: SignExtend::Zero,
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: crate::smir::ir::TrapKind::Halt,
        });
        let mut func = builder.finish();

        let eliminated = redundant_load_elimination(&mut func);
        assert_eq!(
            eliminated, 0,
            "a PredStore must prevent the following load from being forwarded",
        );
        let load_count = func.blocks[0]
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Load { .. }))
            .count();
        assert_eq!(
            load_count, 2,
            "both loads must survive across a PredStore (none rewritten to a Mov)",
        );
    }

    #[test]
    fn test_constant_propagation() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);

        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);
        let v2 = VReg::virt(2);

        // mov v0, 10
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: v0,
                src: SrcOperand::Imm(10),
                width: OpWidth::W64,
            },
        ));

        // mov v1, v0 (should propagate to mov v1, 10)
        block.push_op(make_op(
            1,
            OpKind::Mov {
                dst: v1,
                src: SrcOperand::Reg(v0),
                width: OpWidth::W64,
            },
        ));

        // add v2, v1, v0 (v0 should be replaced with 10)
        block.push_op(make_op(
            2,
            OpKind::Add {
                dst: v2,
                src1: v1,
                src2: SrcOperand::Reg(v0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        block.set_terminator(Terminator::Return { values: vec![v2] });

        let propagated = constant_propagation(&mut block);

        assert!(propagated >= 2);

        // Check that v0 in add was replaced with immediate
        if let OpKind::Add { src2, .. } = &block.ops[2].kind {
            assert!(matches!(src2, SrcOperand::Imm(10)));
        }
    }

    #[test]
    fn test_constant_folding() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);

        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);

        // and v0, v1, 0 -> mov v0, 0
        block.push_op(make_op(
            0,
            OpKind::And {
                dst: v0,
                src1: v1,
                src2: SrcOperand::Imm(0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        block.set_terminator(Terminator::Return { values: vec![v0] });

        let folded = constant_folding(&mut block);

        assert_eq!(folded, 1);

        // Check it was folded to a mov
        if let OpKind::Mov { src, .. } = &block.ops[0].kind {
            assert!(matches!(src, SrcOperand::Imm(0)));
        } else {
            panic!("Expected Mov operation");
        }
    }

    #[test]
    fn folds_evex_ternary_projections_and_zero_reduced_immediates() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        let dst = VReg::virt(0);
        let src1 = VReg::virt(1);
        let src2 = VReg::virt(2);
        let src3 = VReg::virt(3);
        block.push_op(make_op(
            0,
            OpKind::X86TernaryLogic {
                dst,
                src1,
                src2,
                src3,
                mask: None,
                imm: 0xAA,
                width: VecWidth::V512,
                elem: VecElementType::I32,
                zeroing: false,
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::X86PackedRotate {
                dst,
                src: src1,
                count: None,
                mask: None,
                amount: 64,
                width: VecWidth::V512,
                elem: VecElementType::I32,
                left: true,
                zeroing: false,
            },
        ));
        block.push_op(make_op(
            2,
            OpKind::X86PackedFunnelShift {
                dst,
                src: src2,
                fill: src3,
                count: None,
                mask: None,
                amount: 128,
                width: VecWidth::V512,
                elem: VecElementType::I64,
                left: false,
                zeroing: false,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        assert_eq!(constant_folding(&mut block), 3);
        assert!(matches!(
            block.ops[0].kind,
            OpKind::VMov { src, width: VecWidth::V512, .. } if src == src3
        ));
        assert!(matches!(
            block.ops[1].kind,
            OpKind::VMov { src, width: VecWidth::V512, .. } if src == src1
        ));
        assert!(matches!(
            block.ops[2].kind,
            OpKind::VMov { src, width: VecWidth::V512, .. } if src == src2
        ));
    }

    #[test]
    fn test_xor_same_register_fold() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);

        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);

        // xor v0, v1, v1 -> mov v0, 0
        block.push_op(make_op(
            0,
            OpKind::Xor {
                dst: v0,
                src1: v1,
                src2: SrcOperand::Reg(v1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        block.set_terminator(Terminator::Return { values: vec![v0] });

        let folded = constant_folding(&mut block);

        assert_eq!(folded, 1);

        if let OpKind::Mov { src, .. } = &block.ops[0].kind {
            assert!(matches!(src, SrcOperand::Imm(0)));
        } else {
            panic!("Expected Mov operation");
        }
    }

    #[test]
    fn test_dead_code_elimination() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);

        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);
        let v2 = VReg::virt(2);

        // mov v0, 10 (unused)
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: v0,
                src: SrcOperand::Imm(10),
                width: OpWidth::W64,
            },
        ));

        // mov v1, 20 (used)
        block.push_op(make_op(
            1,
            OpKind::Mov {
                dst: v1,
                src: SrcOperand::Imm(20),
                width: OpWidth::W64,
            },
        ));

        // mov v2, 30 (unused)
        block.push_op(make_op(
            2,
            OpKind::Mov {
                dst: v2,
                src: SrcOperand::Imm(30),
                width: OpWidth::W64,
            },
        ));

        block.set_terminator(Terminator::Return { values: vec![v1] });

        let eliminated = dead_code_elimination(&mut block);

        assert_eq!(eliminated, 2);
        assert_eq!(block.ops.len(), 1);

        // Only v1 should remain
        if let OpKind::Mov { dst, .. } = &block.ops[0].kind {
            assert_eq!(*dst, v1);
        }
    }

    #[test]
    fn test_strength_reduction_mul() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);

        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);

        // mul v0, v1, 8 -> shl v0, v1, 3
        block.push_op(make_op(
            0,
            OpKind::MulU {
                dst_lo: v0,
                dst_hi: None,
                src1: v1,
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        block.set_terminator(Terminator::Return { values: vec![v0] });

        let reductions = strength_reduction(&mut block);

        assert_eq!(reductions, 1);

        if let OpKind::Shl { amount, .. } = &block.ops[0].kind {
            assert!(matches!(amount, SrcOperand::Imm(3)));
        } else {
            panic!("Expected Shl operation");
        }
    }

    #[test]
    fn test_strength_reduction_div() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);

        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);

        // div v0, v1, 16 -> shr v0, v1, 4
        block.push_op(make_op(
            0,
            OpKind::DivU {
                quot: v0,
                rem: None,
                src1: v1,
                src2: SrcOperand::Imm(16),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        block.set_terminator(Terminator::Return { values: vec![v0] });

        let reductions = strength_reduction(&mut block);

        assert_eq!(reductions, 1);

        if let OpKind::Shr { amount, .. } = &block.ops[0].kind {
            assert!(matches!(amount, SrcOperand::Imm(4)));
        } else {
            panic!("Expected Shr operation");
        }
    }

    #[test]
    fn optimize_function_preserves_faulting_load_after_mul_zero_fold() {
        use crate::smir::ir::FunctionBuilder;

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        let load_tmp = builder.alloc_vreg();
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Rax));

        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: load_tmp,
                addr: Address::Absolute(0x2000),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1003,
            OpKind::MulS {
                dst_lo: dst,
                dst_hi: None,
                src1: load_tmp,
                src2: SrcOperand::Imm(0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![dst] });

        let mut func = builder.finish();
        optimize_function(&mut func, OptLevel::O2);
        let block = &func.blocks[0];

        assert!(block.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load { dst, .. } if dst == load_tmp
        )));
        assert!(block.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Mov {
                dst: mov_dst,
                src: SrcOperand::Imm(0),
                ..
            } if mov_dst == dst
        )));
        assert!(
            !block
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::MulS { .. }))
        );
    }

    #[test]
    fn test_optimize_function() {
        use crate::smir::ir::FunctionBuilder;

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);

        let v0 = builder.alloc_vreg();
        let v1 = builder.alloc_vreg();
        let v2 = builder.alloc_vreg();
        let v3 = builder.alloc_vreg();

        // mov v0, 10
        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: v0,
                src: SrcOperand::Imm(10),
                width: OpWidth::W64,
            },
        );

        // add v1, v0, 5 (with flags)
        builder.push_op(
            0x1004,
            OpKind::Add {
                dst: v1,
                src1: v0,
                src2: SrcOperand::Imm(5),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        );

        // mov v2, 100 (dead)
        builder.push_op(
            0x1008,
            OpKind::Mov {
                dst: v2,
                src: SrcOperand::Imm(100),
                width: OpWidth::W64,
            },
        );

        // and v3, v1, 0 -> should fold to mov v3, 0
        builder.push_op(
            0x100c,
            OpKind::And {
                dst: v3,
                src1: v1,
                src2: SrcOperand::Imm(0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );

        builder.set_terminator(Terminator::Return { values: vec![v3] });

        let mut func = builder.finish();

        let stats = optimize_function(&mut func, OptLevel::O2);

        // Should have optimizations applied
        assert!(stats.total() > 0);
    }

    #[test]
    fn test_opt_stats() {
        let mut stats1 = OptStats::new();
        stats1.dead_flags_eliminated = 5;
        stats1.constants_propagated = 3;

        let mut stats2 = OptStats::new();
        stats2.dead_ops_eliminated = 2;
        stats2.expressions_folded = 1;

        stats1.merge(&stats2);

        assert_eq!(stats1.dead_flags_eliminated, 5);
        assert_eq!(stats1.constants_propagated, 3);
        assert_eq!(stats1.dead_ops_eliminated, 2);
        assert_eq!(stats1.expressions_folded, 1);
        assert_eq!(stats1.total(), 11);
    }

    #[test]
    fn test_copy_propagation() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));

        // mov v0, rbx     (W64 copy)
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: v0,
                src: SrcOperand::Reg(rbx),
                width: OpWidth::W64,
            },
        ));
        // add v1, rax, v0  -> v0 rewritten to rbx
        block.push_op(make_op(
            1,
            OpKind::Add {
                dst: v1,
                src1: rax,
                src2: SrcOperand::Reg(v0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![v1] });

        let n = copy_propagation(&mut block);
        assert_eq!(n, 1);
        if let OpKind::Add { src2, .. } = &block.ops[1].kind {
            assert!(matches!(src2, SrcOperand::Reg(r) if *r == rbx));
        } else {
            panic!("expected Add");
        }
    }

    #[test]
    fn test_copy_propagation_w32_not_recorded() {
        // A 32-bit copy must NOT be propagated into a 64-bit-equality use.
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: v0,
                src: SrcOperand::Reg(rbx),
                width: OpWidth::W32, // zero-extends; v0 != rbx in 64 bits
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::Add {
                dst: v1,
                src1: rax,
                src2: SrcOperand::Reg(v0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![v1] });
        let n = copy_propagation(&mut block);
        assert_eq!(n, 0); // not propagated
    }

    #[test]
    fn vfma_accumulator_definition_survives_dead_code_elimination() {
        let scalar = VReg::virt(0);
        let accumulator = VReg::virt(1);
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(i64::from(2.0f32.to_bits())),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::VBroadcast {
                dst: accumulator,
                scalar,
                elem: VecElementType::F32,
                lanes: 8,
            },
        ));
        block.push_op(make_op(
            2,
            OpKind::VFma {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                acc: accumulator,
                elem: VecElementType::F32,
                lanes: 8,
                negate_product: false,
                negate_acc: false,
            },
        ));
        block.set_terminator(Terminator::Return {
            values: vec![VReg::Arch(ArchReg::X86(X86Reg::Ymm(2)))],
        });

        dead_code_elimination(&mut block);
        assert_eq!(block.ops.len(), 3, "VFma accumulator producer was removed");
        assert!(matches!(
            block.ops[1].kind,
            OpKind::VBroadcast { dst, .. } if dst == accumulator
        ));
    }

    #[test]
    fn vpermute_table_and_index_definitions_survive_dead_code_elimination() {
        let scalar = VReg::virt(0);
        let table1 = VReg::virt(1);
        let table2 = VReg::virt(2);
        let indices = VReg::virt(3);
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(3),
                width: OpWidth::W64,
            },
        ));
        for (id, vector) in [(1, table1), (2, table2), (3, indices)] {
            block.push_op(make_op(
                id,
                OpKind::VBroadcast {
                    dst: vector,
                    scalar,
                    elem: VecElementType::I8,
                    lanes: 16,
                },
            ));
        }
        block.push_op(make_op(
            4,
            OpKind::VPermute {
                dst,
                src1: table1,
                src2: Some(table2),
                indices,
                elem: VecElementType::I8,
                width: VecWidth::V128,
                overwrite_table: false,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        dead_code_elimination(&mut block);
        assert_eq!(block.ops.len(), 5, "VPermute source producer was removed");
        for source in [table1, table2, indices] {
            assert!(block.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VBroadcast { dst, .. } if dst == source
            )));
        }
    }

    #[test]
    fn x86_permute_bytes_words_inputs_and_merge_destination_survive_dce() {
        let scalar = VReg::virt(0);
        let dst = VReg::virt(1);
        let table1 = VReg::virt(2);
        let table2 = VReg::virt(3);
        let mask = VReg::virt(4);
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(3),
                width: OpWidth::W64,
            },
        ));
        for (id, vector) in [(1, dst), (2, table1), (3, table2)] {
            block.push_op(make_op(
                id,
                OpKind::VBroadcast {
                    dst: vector,
                    scalar,
                    elem: VecElementType::I8,
                    lanes: 16,
                },
            ));
        }
        block.push_op(make_op(
            4,
            OpKind::Mov {
                dst: mask,
                src: SrcOperand::Imm(0x55),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            5,
            OpKind::X86PermuteBytesWords {
                dst,
                table1,
                table2: Some(table2),
                indices: dst,
                mask: Some(mask),
                elem: VecElementType::I8,
                width: VecWidth::V128,
                overwrite_table: false,
                zeroing: false,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        dead_code_elimination(&mut block);
        assert_eq!(block.ops.len(), 6);
        for source in [dst, table1, table2] {
            assert!(block.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VBroadcast { dst, .. } if dst == source
            )));
        }
        assert!(block.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Mov { dst, .. } if dst == mask
        )));
    }

    #[test]
    fn vpopcnt_source_definition_survives_dead_code_elimination() {
        let scalar = VReg::virt(0);
        let source = VReg::virt(1);
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(0x55),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::VBroadcast {
                dst: source,
                scalar,
                elem: VecElementType::I8,
                lanes: 16,
            },
        ));
        block.push_op(make_op(
            2,
            OpKind::VPopcnt {
                dst,
                src: source,
                mask: None,
                elem: VecElementType::I8,
                width: VecWidth::V128,
                zeroing: false,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        dead_code_elimination(&mut block);
        assert_eq!(block.ops.len(), 3, "VPopcnt source producer was removed");
        assert!(block.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast { dst, .. } if dst == source
        )));
    }

    #[test]
    fn vconflict_source_definition_survives_dead_code_elimination() {
        let scalar = VReg::virt(0);
        let source = VReg::virt(1);
        let mask = VReg::virt(2);
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(1),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::VBroadcast {
                dst: source,
                scalar,
                elem: VecElementType::I32,
                lanes: 4,
            },
        ));
        block.push_op(make_op(
            2,
            OpKind::Mov {
                dst: mask,
                src: SrcOperand::Imm(1),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            3,
            OpKind::VConflict {
                dst,
                src: source,
                mask: Some(mask),
                elem: VecElementType::I32,
                width: VecWidth::V128,
                zeroing: false,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        dead_code_elimination(&mut block);
        assert_eq!(block.ops.len(), 4, "VConflict input producer was removed");
    }

    #[test]
    fn vleadingzeros_source_definition_survives_dead_code_elimination() {
        let scalar = VReg::virt(0);
        let source = VReg::virt(1);
        let mask = VReg::virt(2);
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(1),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::VBroadcast {
                dst: source,
                scalar,
                elem: VecElementType::I32,
                lanes: 4,
            },
        ));
        block.push_op(make_op(
            2,
            OpKind::Mov {
                dst: mask,
                src: SrcOperand::Imm(1),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            3,
            OpKind::VLeadingZeros {
                dst,
                src: source,
                mask: Some(mask),
                elem: VecElementType::I32,
                width: VecWidth::V128,
                zeroing: false,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        dead_code_elimination(&mut block);
        assert_eq!(
            block.ops.len(),
            4,
            "VLeadingZeros input producer was removed"
        );
    }

    #[test]
    fn vmultiplyadd52_input_definitions_survive_dead_code_elimination() {
        let scalar = VReg::virt(0);
        let acc = VReg::virt(1);
        let src1 = VReg::virt(2);
        let src2 = VReg::virt(3);
        let mask = VReg::virt(4);
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(3),
                width: OpWidth::W64,
            },
        ));
        for (id, vector) in [(1, acc), (2, src1), (3, src2)] {
            block.push_op(make_op(
                id,
                OpKind::VBroadcast {
                    dst: vector,
                    scalar,
                    elem: VecElementType::I64,
                    lanes: 2,
                },
            ));
        }
        block.push_op(make_op(
            4,
            OpKind::Mov {
                dst: mask,
                src: SrcOperand::Imm(1),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            5,
            OpKind::VMultiplyAdd52 {
                dst,
                acc,
                src1,
                src2,
                mask: Some(mask),
                width: VecWidth::V128,
                high: false,
                zeroing: false,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        dead_code_elimination(&mut block);
        assert_eq!(
            block.ops.len(),
            6,
            "VMultiplyAdd52 input producer was removed"
        );
    }

    #[test]
    fn vdotproductext_input_definitions_survive_dead_code_elimination() {
        let scalar = VReg::virt(0);
        let acc = VReg::virt(1);
        let src1 = VReg::virt(2);
        let src2 = VReg::virt(3);
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(3),
                width: OpWidth::W64,
            },
        ));
        for (id, vector) in [(1, acc), (2, src1), (3, src2)] {
            block.push_op(make_op(
                id,
                OpKind::VBroadcast {
                    dst: vector,
                    scalar,
                    elem: VecElementType::I32,
                    lanes: 4,
                },
            ));
        }
        block.push_op(make_op(
            4,
            OpKind::VDotProductExt {
                dst,
                acc,
                src1,
                src2,
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I32,
                width: VecWidth::V128,
                src1_signed: true,
                src2_signed: false,
                saturate: true,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        dead_code_elimination(&mut block);
        assert_eq!(
            block.ops.len(),
            5,
            "VDotProductExt input producer was removed"
        );
    }

    #[test]
    fn bf16_input_definitions_survive_dead_code_elimination() {
        let scalar = VReg::virt(0);
        let acc = VReg::virt(1);
        let src1 = VReg::virt(2);
        let src2 = VReg::virt(3);
        let dot = VReg::virt(4);
        let mask = VReg::virt(5);
        let convert_mask = VReg::virt(6);
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(3),
                width: OpWidth::W64,
            },
        ));
        for (id, vector) in [(1, acc), (2, src1), (3, src2)] {
            block.push_op(make_op(
                id,
                OpKind::VBroadcast {
                    dst: vector,
                    scalar,
                    elem: VecElementType::I32,
                    lanes: 4,
                },
            ));
        }
        block.push_op(make_op(
            4,
            OpKind::Mov {
                dst: mask,
                src: SrcOperand::Imm(1),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            5,
            OpKind::VDotProductBF16 {
                dst: dot,
                acc,
                src1,
                src2,
                mask: Some(mask),
                width: VecWidth::V128,
                zeroing: false,
            },
        ));
        block.push_op(make_op(
            6,
            OpKind::Mov {
                dst: convert_mask,
                src: SrcOperand::Imm(0x55),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            7,
            OpKind::VCvtFP32ToBF16 {
                dst,
                src1: dot,
                src2: Some(src2),
                mask: Some(convert_mask),
                width: VecWidth::V128,
                zeroing: false,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        dead_code_elimination(&mut block);
        assert_eq!(block.ops.len(), 8, "BF16 input producer was removed");
    }

    #[test]
    fn fp16_mask_and_input_definitions_survive_dead_code_elimination() {
        let scalar = VReg::virt(0);
        let src1 = VReg::virt(1);
        let src2 = VReg::virt(2);
        let mask = VReg::virt(3);
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(0x3c00),
                width: OpWidth::W64,
            },
        ));
        for (id, vector) in [(1, src1), (2, src2)] {
            block.push_op(make_op(
                id,
                OpKind::VBroadcast {
                    dst: vector,
                    scalar,
                    elem: VecElementType::I16,
                    lanes: 8,
                },
            ));
        }
        block.push_op(make_op(
            3,
            OpKind::Mov {
                dst: mask,
                src: SrcOperand::Imm(0x55),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            4,
            OpKind::VFP16Arith {
                dst,
                src1,
                src2,
                mask: Some(mask),
                op: Avx10FP16Op::Add,
                width: VecWidth::V128,
                zeroing: false,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        dead_code_elimination(&mut block);
        assert_eq!(block.ops.len(), 5, "FP16 input producer was removed");
    }

    #[test]
    fn vshufflebitqm_input_definitions_survive_dead_code_elimination() {
        let scalar = VReg::virt(0);
        let src = VReg::virt(1);
        let indices = VReg::virt(2);
        let dst = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(3),
                width: OpWidth::W64,
            },
        ));
        for (id, vector) in [(1, src), (2, indices)] {
            block.push_op(make_op(
                id,
                OpKind::VBroadcast {
                    dst: vector,
                    scalar,
                    elem: VecElementType::I64,
                    lanes: 2,
                },
            ));
        }
        block.push_op(make_op(
            3,
            OpKind::VShuffleBitQM {
                dst,
                src,
                indices,
                mask: None,
                width: VecWidth::V128,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        dead_code_elimination(&mut block);
        assert_eq!(
            block.ops.len(),
            4,
            "VShuffleBitQM input producer was removed"
        );
    }

    #[test]
    fn vcompress_vexpand_inputs_and_merge_destinations_survive_dead_code_elimination() {
        let scalar = VReg::virt(0);
        let src = VReg::virt(1);
        let packed = VReg::virt(2);
        let mask = VReg::virt(3);
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(3),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::VBroadcast {
                dst: src,
                scalar,
                elem: VecElementType::I32,
                lanes: 4,
            },
        ));
        block.push_op(make_op(
            2,
            OpKind::VBroadcast {
                dst: packed,
                scalar,
                elem: VecElementType::I32,
                lanes: 4,
            },
        ));
        block.push_op(make_op(
            3,
            OpKind::Mov {
                dst: mask,
                src: SrcOperand::Imm(5),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            4,
            OpKind::VCompress {
                dst: packed,
                src,
                mask: Some(mask),
                elem: VecElementType::I32,
                width: VecWidth::V128,
                zeroing: false,
            },
        ));
        block.push_op(make_op(
            5,
            OpKind::VBroadcast {
                dst,
                scalar,
                elem: VecElementType::I32,
                lanes: 4,
            },
        ));
        block.push_op(make_op(
            6,
            OpKind::VExpand {
                dst,
                src: packed,
                mask: Some(mask),
                elem: VecElementType::I32,
                width: VecWidth::V128,
                zeroing: false,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        dead_code_elimination(&mut block);
        assert_eq!(
            block.ops.len(),
            7,
            "compress/expand input producer was removed"
        );
    }

    #[test]
    fn x86_narrow_inputs_and_merge_destination_survive_dead_code_elimination() {
        let scalar = VReg::virt(0);
        let src = VReg::virt(1);
        let mask = VReg::virt(2);
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(3),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::VBroadcast {
                dst: src,
                scalar,
                elem: VecElementType::I32,
                lanes: 4,
            },
        ));
        block.push_op(make_op(
            2,
            OpKind::Mov {
                dst: mask,
                src: SrcOperand::Imm(5),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            3,
            OpKind::VBroadcast {
                dst,
                scalar,
                elem: VecElementType::I8,
                lanes: 4,
            },
        ));
        block.push_op(make_op(
            4,
            OpKind::X86NarrowInt {
                dst,
                src,
                mask: Some(mask),
                src_elem: VecElementType::I32,
                dst_elem: VecElementType::I8,
                width: VecWidth::V128,
                mode: X86NarrowMode::SignedSaturate,
                zeroing: false,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        dead_code_elimination(&mut block);
        assert_eq!(block.ops.len(), 5, "narrowing input producer was removed");
    }

    #[test]
    fn x86_aes_sources_survive_dead_code_elimination() {
        let scalar = VReg::virt(0);
        let state = VReg::virt(1);
        let key = VReg::virt(2);
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(0x5A),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::VBroadcast {
                dst: state,
                scalar,
                elem: VecElementType::I64,
                lanes: 2,
            },
        ));
        block.push_op(make_op(
            2,
            OpKind::VBroadcast {
                dst: key,
                scalar,
                elem: VecElementType::I64,
                lanes: 2,
            },
        ));
        block.push_op(make_op(
            3,
            OpKind::X86Aes {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src1: state,
                src2: Some(key),
                width: VecWidth::V128,
                op: X86AesOp::Enc,
                imm: 0,
            },
        ));
        block.set_terminator(Terminator::Return {
            values: vec![VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)))],
        });

        dead_code_elimination(&mut block);
        assert_eq!(block.ops.len(), 4, "AES source producer was removed");
        assert!(block.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast { dst, .. } if dst == state
        )));
        assert!(block.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast { dst, .. } if dst == key
        )));
    }

    #[test]
    fn test_branch_folding_same_target_and_unreachable() {
        use crate::smir::ir::types::FunctionId;
        let b0 = BlockId(0);
        let b1 = BlockId(1);
        let b2 = BlockId(2);
        let mut func = SmirFunction::new(FunctionId(0), b0, 0x1000);

        // b0: cond-branch to b1 either way (same target) -> folds to Branch b1.
        let mut blk0 = SmirBlock::new(b0, 0x1000);
        blk0.set_terminator(Terminator::CondBranch {
            cond: VReg::virt(0),
            true_target: b1,
            false_target: b1,
        });
        func.add_block(blk0);

        // b1: reachable, returns.
        let mut blk1 = SmirBlock::new(b1, 0x1010);
        blk1.set_terminator(Terminator::Return { values: vec![] });
        func.add_block(blk1);

        // b2: unreachable -> removed.
        let mut blk2 = SmirBlock::new(b2, 0x1020);
        blk2.set_terminator(Terminator::Return { values: vec![] });
        func.add_block(blk2);

        let n = branch_folding(&mut func);
        assert!(n >= 2); // 1 fold + 1 unreachable removed
        assert!(matches!(func.blocks[0].terminator, Terminator::Branch { target } if target == b1));
        assert!(func.blocks.iter().all(|b| b.id != b2));
    }
}
