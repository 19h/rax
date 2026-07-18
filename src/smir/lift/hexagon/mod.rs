//! Hexagon instruction lifter.
//!
//! This module lifts Hexagon machine code to SMIR. Since Hexagon's DecodedInsn
//! is already in an IR-like format, this is a relatively straightforward mapping.

use std::collections::HashSet;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{HexDfOp, HexFpOp, HexFpRecipKind, OpKind, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
};
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

// Re-use the existing Hexagon decoder types
use crate::isa::hexagon::decode::{
    AddrMode, CmpKind, DecodedInsn, ExtendKind, MemOpKind, MemOpSrc, MemSign,
    MemWidth as HexMemWidth, ShiftKind,
};
// Direct opcode-level decoding for the ~900 scalar ops that decode to
// `DecodedInsn::Unknown` (handled only by the sem layer in cpu.rs). The lifter
// re-decodes such words via `decode_word` and emits SMIR for the regular
// scalar register ops; see `lift_unknown_op`.
use crate::isa::hexagon::opcode::{DecodedOp, Opcode, decode_word};

// ---- module tree (auto-split) ----
mod emit;
pub use emit::*;
mod helpers;
pub use helpers::*;
mod lift;
pub use lift::*;
#[cfg(test)]
mod tests;


// ============================================================================
// Hexagon Lifter
// ============================================================================

/// A histogram opcode lifted earlier in the current packet, awaiting the
/// same-packet `.tmp` vmem load that supplies its 128-byte input. The histogram
/// instruction word is decoded BEFORE its producing `.tmp` load (the assembler
/// emits it first), and the histogram opcode itself has no register operand for
/// its input — the data comes from the per-packet `.tmp` scratch (qemu's
/// `tmp_VRegs[0]`). We therefore defer emitting the `VHist` op until we see the
/// `.tmp` load, whose effective address we splice into `input` so the interp can
/// re-read the same 128 bytes from guest memory.
#[derive(Clone)]
struct PendingHist {
    mask_q: VReg,
    use_q: bool,
    imm_match: Option<u8>,
    sat: bool,
    kind: u8,
}

/// Hexagon instruction lifter
pub struct HexagonLifter {
    /// ISA version for feature detection
    isa: crate::config::HexagonIsa,
    /// A histogram opcode awaiting its same-packet `.tmp` load (see PendingHist).
    pending_hist: Option<PendingHist>,
    /// GPR producers of the CURRENT packet, in execution (lift) order — the
    /// lowest GPR newly written by each instruction. Mirrors the interpreter's
    /// per-packet `producers` list (cpu.rs `record_producer`) so new-value
    /// stores (`Nt8`) and new-value compound compare-jumps (`Ns8`) can resolve
    /// their `.new` source register at lift time. Reset at every packet
    /// boundary (see `prev_word_ended_packet`).
    packet_producers: Vec<u8>,
    /// `true` once the most recently lifted word ended its packet (parse bits
    /// `0b11` or `0b00`). The NEXT `lift_insn` clears `packet_producers` before
    /// processing — so producers never leak across packets within a block.
    prev_word_ended_packet: bool,
    /// Guest address of the FIRST instruction of the current packet (the packet
    /// PC). Hexagon PC-relative branches are computed relative to the packet
    /// start, not the branching instruction's own address — so a new-value
    /// compound compare-jump (which is NOT the first word of its packet) needs
    /// this. Updated at every packet boundary alongside `packet_producers`.
    packet_start_pc: GuestAddr,
}


impl SmirLifter for HexagonLifter {
    fn source_arch(&self) -> SourceArch {
        SourceArch::Hexagon
    }

    fn lift_insn(
        &mut self,
        addr: GuestAddr,
        bytes: &[u8],
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.len() < 4 {
            return Err(LiftError::Incomplete {
                addr,
                have: bytes.len(),
                need: 4,
            });
        }

        let word = u32::from_le_bytes(bytes[..4].try_into().unwrap());

        // PACKET-PRODUCER TRACKING (for new-value `.new` resolution): the GPR
        // producers list is per-packet. If the previous word ended its packet,
        // start a fresh producers list before lifting this (first-of-packet)
        // instruction. The parse bits live in word[15:14] (`0b11`/`0b00` = end).
        if self.prev_word_ended_packet {
            self.packet_producers.clear();
            self.packet_start_pc = addr;
        }
        let parse = (word >> 14) & 0x3;
        // `0b00` is the duplex (two 16-bit sub-insns) end-of-packet marker;
        // `0b11` is a single-word end-of-packet (or a lone instruction).
        self.prev_word_ended_packet = parse == 0b11 || parse == 0b00;

        // Use the existing Hexagon decoder
        let decoded = crate::isa::hexagon::decode::decode(word, ctx.extended_imm, self.isa);

        let insn = decoded.insn;
        ctx.guest_pc = addr;

        // A pending histogram (set by a previous instruction) must be consumed by
        // the very next `.tmp` vmem load in the same packet. If this instruction
        // does NOT consume it, drop the stale entry so it can never leak into an
        // unrelated later instruction/block.
        let had_pending = self.pending_hist.is_some();

        let result = self.lift_insn_inner(&insn, addr, ctx);

        if had_pending && self.pending_hist.is_some() {
            self.pending_hist = None;
        }

        let (ops, control_flow) = result?;

        // Record this instruction's GPR producer for same-packet new-value
        // resolution: the LOWEST Hexagon R register newly written by the ops it
        // just emitted (mirrors the interpreter's `record_producer`, which pushes
        // the lowest GPR with a fresh in-flight write). A pair write (even/odd)
        // contributes only the even register; instructions that write no GPR
        // (stores, pure predicate ops, control flow) contribute nothing.
        let mut produced: Option<u8> = None;
        for op in &ops {
            for dst in op.kind.dests() {
                if let VReg::Arch(ArchReg::Hexagon(HexagonReg::R(n))) = dst {
                    produced = Some(produced.map_or(n, |cur| cur.min(n)));
                }
            }
        }
        if let Some(n) = produced {
            self.packet_producers.push(n);
        }

        let mut branch_targets = Vec::new();
        match &control_flow {
            ControlFlow::Branch { target } => {
                branch_targets.push(*target);
            }
            ControlFlow::CondBranch {
                target,
                fallthrough,
                ..
            } => {
                branch_targets.push(*target);
                branch_targets.push(*fallthrough);
            }
            ControlFlow::Call {
                target: CallTarget::GuestAddr(target),
            } => {
                branch_targets.push(*target);
            }
            _ => {}
        }

        Ok(LiftResult {
            ops,
            bytes_consumed: 4,
            control_flow,
            branch_targets,
        })
    }

    fn lift_block(
        &mut self,
        addr: GuestAddr,
        mem: &dyn MemoryReader,
        ctx: &mut LiftContext,
    ) -> Result<SmirBlock, LiftError> {
        let block_id = ctx.get_or_create_block(addr);
        let mut all_ops = Vec::new();
        let mut current_addr = addr;

        loop {
            // Fetch instruction bytes
            let bytes = mem
                .read(current_addr, 4)
                .map_err(|e| LiftError::MemoryError {
                    addr: current_addr,
                    error: e,
                })?;

            // Lift the instruction
            let result = self.lift_insn(current_addr, &bytes, ctx)?;
            all_ops.extend(result.ops);
            current_addr += result.bytes_consumed as u64;

            // Check if block ends
            if result.control_flow.ends_block() {
                let terminator = match result.control_flow {
                    ControlFlow::Fallthrough | ControlFlow::NextInsn => unreachable!(),
                    ControlFlow::Branch { target } | ControlFlow::DirectBranch(target) => {
                        Terminator::Branch {
                            target: ctx.get_or_create_block(target),
                        }
                    }
                    ControlFlow::CondBranch {
                        cond: _,
                        target,
                        fallthrough,
                    } => {
                        // Need a condition vreg - use the last op if it's a SetCC
                        let cond_vreg = ctx.alloc_vreg();
                        Terminator::CondBranch {
                            cond: cond_vreg,
                            true_target: ctx.get_or_create_block(target),
                            false_target: ctx.get_or_create_block(fallthrough),
                        }
                    }
                    ControlFlow::CondBranchReg {
                        cond,
                        taken,
                        not_taken,
                    } => Terminator::CondBranch {
                        cond,
                        true_target: ctx.get_or_create_block(taken),
                        false_target: ctx.get_or_create_block(not_taken),
                    },
                    ControlFlow::IndirectBranch { target } => Terminator::IndirectBranch {
                        target,
                        possible_targets: vec![],
                    },
                    ControlFlow::IndirectBranchMem { addr } => Terminator::IndirectBranchMem {
                        addr,
                        possible_targets: vec![],
                    },
                    ControlFlow::Call { target } => Terminator::Call {
                        target,
                        args: vec![],
                        continuation: ctx.get_or_create_block(current_addr),
                    },
                    ControlFlow::Return => Terminator::Return { values: vec![] },
                    ControlFlow::Trap { kind } => Terminator::Trap { kind },
                    ControlFlow::Syscall => Terminator::Trap {
                        kind: TrapKind::SystemCall,
                    },
                };

                return Ok(SmirBlock {
                    id: block_id,
                    guest_pc: addr,
                    phis: vec![],
                    ops: all_ops,
                    terminator,
                    exec_count: 0,
                });
            }
        }
    }

    fn lift_function(
        &mut self,
        entry: GuestAddr,
        mem: &dyn MemoryReader,
        ctx: &mut LiftContext,
    ) -> Result<SmirFunction, LiftError> {
        let func_id = FunctionId(ctx.known_functions.len() as u32);
        ctx.known_functions.insert(entry, func_id);

        let mut blocks = Vec::new();
        let mut worklist = vec![entry];
        let mut visited = HashSet::new();
        let mut min_addr = entry;
        let mut max_addr = entry;

        while let Some(addr) = worklist.pop() {
            if visited.contains(&addr) {
                continue;
            }
            visited.insert(addr);

            let block = self.lift_block(addr, mem, ctx)?;

            // Track address range
            if block.guest_pc < min_addr {
                min_addr = block.guest_pc;
            }
            let block_end = block.guest_pc + (block.ops.len() * 4) as u64;
            if block_end > max_addr {
                max_addr = block_end;
            }

            // Add branch targets to worklist
            for succ in block.successors() {
                if let Some(&succ_addr) = ctx
                    .block_cache
                    .iter()
                    .find(|(_, id)| **id == succ)
                    .map(|(addr, _)| addr)
                {
                    if !visited.contains(&succ_addr) {
                        worklist.push(succ_addr);
                    }
                }
            }

            blocks.push(block);
        }

        Ok(SmirFunction {
            id: func_id,
            entry: ctx.get_or_create_block(entry),
            blocks,
            locals: vec![],
            guest_range: (min_addr, max_addr),
            calling_convention: CallingConv::HexagonStd,
            attrs: FunctionAttrs::default(),
            x86_instruction_bytes: std::collections::HashMap::new(),
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

