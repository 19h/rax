//! VEX-encoded AVX-512 opmask operation execution.

use crate::smir::interpret::SmirInterpreter;
use crate::smir::ir::context::SmirContext;
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86OpmaskBinaryKind, X86OpmaskMoveDestination, X86OpmaskMoveSource,
    X86OpmaskOp, X86OpmaskShiftKind, X86OpmaskTestKind,
};
use crate::smir::ir::types::{OpWidth, SignExtend};

impl SmirInterpreter {
    pub(crate) fn execute_op_opmask(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let OpKind::X86Opmask(opmask) = &op.kind else {
            return self.execute_op_meta(ctx, memory, op);
        };

        let width = opmask.width();
        debug_assert!(matches!(
            width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        ));
        let mask = width.mask();

        match opmask {
            X86OpmaskOp::MoveToMask { dst, src, .. } => {
                let value = match src {
                    X86OpmaskMoveSource::Mask(src) | X86OpmaskMoveSource::Gpr(src) => {
                        ctx.read_vreg(*src)
                    }
                    X86OpmaskMoveSource::Memory(addr) => {
                        let effective_addr = self.compute_address(ctx, addr);
                        self.load_memory(
                            memory,
                            effective_addr,
                            width.to_mem_width(),
                            SignExtend::Zero,
                        )?
                    }
                };
                // Every K destination is zero-extended to MAX_KL=64 bits.
                ctx.write_vreg(*dst, value & mask);
            }
            X86OpmaskOp::MoveFromMask { dst, src, .. } => {
                let value = ctx.read_vreg(*src) & mask;
                match dst {
                    // Byte, word, and dword KMOV-to-GPR forms write a 32-bit
                    // destination; qword writes 64 bits. Both zero-extend.
                    X86OpmaskMoveDestination::Gpr(dst) => ctx.write_vreg(*dst, value),
                    X86OpmaskMoveDestination::Memory(addr) => {
                        let effective_addr = self.compute_address(ctx, addr);
                        self.store_memory(memory, effective_addr, value, width.to_mem_width())?;
                    }
                }
            }
            X86OpmaskOp::Not { dst, src, .. } => {
                ctx.write_vreg(*dst, !ctx.read_vreg(*src) & mask);
            }
            X86OpmaskOp::Binary {
                kind,
                dst,
                src1,
                src2,
                ..
            } => {
                let lhs = ctx.read_vreg(*src1) & mask;
                let rhs = ctx.read_vreg(*src2) & mask;
                let value = match kind {
                    X86OpmaskBinaryKind::Add => lhs.wrapping_add(rhs),
                    X86OpmaskBinaryKind::And => lhs & rhs,
                    X86OpmaskBinaryKind::AndNot => !lhs & rhs,
                    X86OpmaskBinaryKind::Or => lhs | rhs,
                    X86OpmaskBinaryKind::Xnor => !(lhs ^ rhs),
                    X86OpmaskBinaryKind::Xor => lhs ^ rhs,
                };
                ctx.write_vreg(*dst, value & mask);
            }
            X86OpmaskOp::Unpack {
                dst, src1, src2, ..
            } => {
                debug_assert!(matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64));
                let half_bits = width.bits() / 2;
                let half_mask = (1u64 << half_bits) - 1;
                let high = (ctx.read_vreg(*src1) & half_mask) << half_bits;
                let low = ctx.read_vreg(*src2) & half_mask;
                ctx.write_vreg(*dst, (high | low) & mask);
            }
            X86OpmaskOp::Shift {
                kind,
                dst,
                src,
                count,
                ..
            } => {
                let value = ctx.read_vreg(*src) & mask;
                let shifted = if u32::from(*count) >= width.bits() {
                    0
                } else {
                    match kind {
                        X86OpmaskShiftKind::Left => value << count,
                        X86OpmaskShiftKind::Right => value >> count,
                    }
                };
                ctx.write_vreg(*dst, shifted & mask);
            }
            X86OpmaskOp::Test {
                kind, src1, src2, ..
            } => {
                let lhs = ctx.read_vreg(*src1) & mask;
                let rhs = ctx.read_vreg(*src2) & mask;
                let (zf, cf) = match kind {
                    X86OpmaskTestKind::And => (lhs & rhs == 0, ((!lhs) & rhs & mask) == 0),
                    X86OpmaskTestKind::Or => {
                        let value = (lhs | rhs) & mask;
                        (value == 0, value == mask)
                    }
                };
                ctx.flags.materialize_all();
                ctx.flags.materialized.cf = cf;
                ctx.flags.materialized.zf = zf;
                ctx.flags.materialized.of = false;
                ctx.flags.materialized.sf = false;
                ctx.flags.materialized.af = false;
                ctx.flags.materialized.pf = false;
            }
        }

        Ok(())
    }
}
