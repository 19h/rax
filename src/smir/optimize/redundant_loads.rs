//! Redundant-load tracking with explicit memory and packet barriers.
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{Address, MemWidth, OpWidth, SignExtend, SrcOperand, VReg};
use crate::smir::ir::{SmirBlock, SmirFunction};
use std::collections::HashMap;

/// Eliminate redundant loads.
///
/// When a value is loaded from memory and the same address is loaded again
/// (without an intervening store), replace the second load with a move.
/// This transform is disabled unless
/// [`FunctionAttrs::allow_redundant_load_elimination`](crate::smir::ir::FunctionAttrs::allow_redundant_load_elimination)
/// explicitly proves that ordinary loads cannot fault or perform volatile/MMIO
/// reads. The load extension mode is part of the identity key.
///
/// Returns the number of redundant loads eliminated.
pub fn redundant_load_elimination(func: &mut SmirFunction) -> usize {
    if !func.attrs.allow_redundant_load_elimination {
        return 0;
    }

    let mut eliminated = 0;

    for block in &mut func.blocks {
        eliminated += redundant_load_elimination_block(block);
    }

    eliminated
}

fn redundant_load_elimination_block(block: &mut SmirBlock) -> usize {
    // Track what's currently in registers from memory
    // Key: (base_vreg, offset, width, extension), Value: loaded VReg.
    let mut mem_to_reg: HashMap<(Option<VReg>, i64, MemWidth, SignExtend), VReg> = HashMap::new();
    let mut eliminated = 0;

    let mut new_ops = Vec::new();

    for op in &block.ops {
        match &op.kind {
            OpKind::Load {
                dst,
                addr,
                width,
                sign,
            } => {
                // Only loads from a key-able address (Direct/BaseOffset/Absolute)
                // are candidates. Complex addresses (BaseIndexScale, PcRel) are
                // NOT tracked — a single sentinel key would make distinct
                // addresses (e.g. [rsi+rdx-16] vs [rsi+rdx-8]) collide and
                // wrongly forward one load's value to the other.
                if let Some(key) = address_key(addr, *width, *sign) {
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
            | OpKind::HexagonPacketBegin
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
fn address_key(
    addr: &Address,
    width: MemWidth,
    sign: SignExtend,
) -> Option<(Option<VReg>, i64, MemWidth, SignExtend)> {
    match addr {
        Address::Direct(r) => Some((Some(*r), 0, width, sign)),
        Address::BaseOffset { base, offset, .. } => Some((Some(*base), *offset, width, sign)),
        Address::Absolute(a) => Some((None, *a as i64, width, sign)),
        _ => None,
    }
}
