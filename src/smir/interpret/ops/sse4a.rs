//! Canonical AMD SSE4A operation execution.

use crate::smir::interpret::*;
use crate::smir::ir::context::{ExitReason, SmirContext};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{OpKind, SmirOp, X86Sse4aBitfieldKind};
use crate::smir::ir::types::{ArchReg, MemWidth, VReg, X86Reg};

impl SmirInterpreter {
    pub(crate) fn execute_op_sse4a(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        match &op.kind {
            OpKind::X86Sse4aBitfield {
                dst,
                source,
                kind,
                length,
                index,
            } => {
                let mut destination = Self::read_vec(ctx, *dst);
                let source = Self::read_vec(ctx, *source);
                let controls = match (*length, *index) {
                    (Some(length @ 0..=63), Some(index @ 0..=63)) => Some((length, index)),
                    (None, None) => {
                        let raw = match kind {
                            X86Sse4aBitfieldKind::Extract => source[0],
                            X86Sse4aBitfieldKind::Insert => source[1],
                        };
                        Some(((raw & 0x3F) as u8, ((raw >> 8) & 0x3F) as u8))
                    }
                    _ => None,
                };
                let Some((length, index)) = controls else {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                };
                let mask = if length == 0 {
                    u64::MAX
                } else {
                    u64::MAX >> (64 - length)
                };
                destination[0] = match kind {
                    X86Sse4aBitfieldKind::Extract => {
                        destination[0].wrapping_shr(u32::from(index)) & mask
                    }
                    X86Sse4aBitfieldKind::Insert => {
                        let shifted_mask = mask.wrapping_shl(u32::from(index));
                        (destination[0] & !shifted_mask)
                            | ((source[0] & mask).wrapping_shl(u32::from(index)))
                    }
                };
                Self::write_vec(ctx, *dst, destination);
            }

            OpKind::X86Sse4aMovntStore { src, addr, width } => {
                let source_valid = matches!(src, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))));
                let size = match width {
                    MemWidth::B4 => 4,
                    MemWidth::B8 => 8,
                    _ => 0,
                };
                if !source_valid || size == 0 {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let effective_addr = self.compute_address(ctx, addr);
                let low = Self::read_vec(ctx, *src)[0].to_le_bytes();
                memory.write(effective_addr, &low[..size])?;
            }

            _ => return self.execute_op_unary(ctx, memory, op),
        }

        Ok(())
    }
}
