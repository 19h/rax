//! Reachable control-flow discovery and exact guest byte extents.

use super::EbpfLifter;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{FunctionId, GuestAddr, OpWidth};
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
};
use crate::smir::lift::{ControlFlow, LiftContext, LiftError, MemoryReader, SmirLifter};
use std::collections::HashSet;

impl EbpfLifter {
    /// Lift a program into distinct functions. Call targets are discovered
    /// separately from intraprocedural successors so each guest_range describes
    /// one function. Registered entries supply possible indirect-call targets.
    pub fn lift_program(
        &mut self,
        entry: GuestAddr,
        mem: &dyn MemoryReader,
        ctx: &mut LiftContext,
    ) -> Result<Vec<SmirFunction>, LiftError> {
        let mut pending = vec![entry];
        let mut visited = HashSet::new();
        let mut functions = Vec::new();
        while let Some(address) = pending.pop() {
            if !visited.insert(address) {
                continue;
            }
            let function = self.lift_reachable_function(address, mem, ctx)?;
            for block in &function.blocks {
                match &block.terminator {
                    Terminator::Call {
                        target: CallTarget::GuestAddr(target),
                        ..
                    } => pending.push(*target),
                    Terminator::Call {
                        target: CallTarget::Indirect(_),
                        ..
                    } => pending.extend(self.function_registry.values().copied()),
                    _ => {}
                }
            }
            functions.push(function);
        }
        Ok(functions)
    }

    pub(super) fn lift_block_with_end(
        &mut self,
        addr: GuestAddr,
        mem: &dyn MemoryReader,
        ctx: &mut LiftContext,
    ) -> Result<(SmirBlock, GuestAddr), LiftError> {
        let block_id = ctx.get_or_create_block(addr);
        let mut all_ops = Vec::new();
        let mut current_addr = addr;

        loop {
            // Try reading 16 bytes for potential LDDW; fall back to 8 bytes at end of buffer
            let bytes = match mem.read(current_addr, 16) {
                Ok(b) => b,
                Err(_) => mem
                    .read(current_addr, 8)
                    .map_err(|e| LiftError::MemoryError {
                        addr: current_addr,
                        error: e,
                    })?,
            };

            let result = self.lift_insn(current_addr, &bytes, ctx)?;
            all_ops.extend(result.ops);
            current_addr = current_addr
                .checked_add(result.bytes_consumed as u64)
                .ok_or_else(|| LiftError::Internal("eBPF guest byte extent overflow".into()))?;

            if result.control_flow.ends_block() {
                let terminator = match result.control_flow {
                    ControlFlow::Fallthrough | ControlFlow::NextInsn => unreachable!(),
                    ControlFlow::Branch { target } | ControlFlow::DirectBranch(target) => {
                        Terminator::Branch {
                            target: ctx.get_or_create_block(target),
                        }
                    }
                    ControlFlow::CondBranch {
                        cond,
                        target,
                        fallthrough,
                    } => {
                        let cond_vreg = ctx.alloc_vreg();
                        all_ops.push(SmirOp::new(
                            ctx.next_op_id(),
                            current_addr - result.bytes_consumed as u64,
                            OpKind::SetCC {
                                dst: cond_vreg,
                                cond,
                                width: OpWidth::W8,
                            },
                        ));
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

                return Ok((
                    SmirBlock {
                        id: block_id,
                        guest_pc: addr,
                        phis: vec![],
                        ops: all_ops,
                        terminator,
                        exec_count: 0,
                    },
                    current_addr,
                ));
            }
        }
    }

    pub(super) fn lift_reachable_function(
        &mut self,
        entry: GuestAddr,
        mem: &dyn MemoryReader,
        ctx: &mut LiftContext,
    ) -> Result<SmirFunction, LiftError> {
        let proposed_id = FunctionId(ctx.known_functions.len() as u32);
        let func_id = *ctx.known_functions.entry(entry).or_insert(proposed_id);

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

            let (block, block_end) = self.lift_block_with_end(addr, mem, ctx)?;
            if block.guest_pc < min_addr {
                min_addr = block.guest_pc;
            }
            if block_end > max_addr {
                max_addr = block_end;
            }

            if matches!(
                block.terminator,
                Terminator::Trap {
                    kind: TrapKind::SystemCall
                }
            ) {
                worklist.push(block_end);
            }

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
            calling_convention: CallingConv::GuestPreserveAll,
            attrs: FunctionAttrs::default(),
            x86_instruction_bytes: std::collections::HashMap::new(),
        })
    }
}
