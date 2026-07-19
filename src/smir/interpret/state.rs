//! Register, flag, and memory state access

use crate::smir::interpret::*;
use std::cmp::Ordering;
use std::collections::HashMap;

use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, VecValue};
use crate::smir::ir::flags::{FlagSet, FlagUpdate, LazyFlagOp, LazyFlags};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{
    HexFpOp, HexFpRecipKind, OpKind, RvVectorState, SmirOp, X86AdxKind, X86BlsKind,
    X86CacheControlKind, X86CountKind, X86OpHint, X86ThreeDNowKind, X86X87ArithmeticDestination,
    X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant, X86X87ControlKind, X86X87DataKind,
    X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth, X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};

impl SmirInterpreter {
    /// Write a width-tagged operation result to a destination, applying x86
    /// sub-register write semantics for architectural GPRs: an 8-bit or 16-bit
    /// write MERGES into the existing register (the upper bits are preserved),
    /// a 32-bit write zero-extends (the caller already masked `value` to 32
    /// bits, so a full store clears the upper 32), and 64-bit is a full store.
    /// Virtual (SSA temp) and non-x86 destinations are written as-is. Without
    /// this, an 8/16-bit ALU result would zero-extend the whole register, which
    /// the smir_alu differential test against KVM flagged.
    #[inline]
    pub(crate) fn write_gpr(ctx: &mut SmirContext, dst: VReg, value: u64, width: OpWidth) {
        if let VReg::Arch(ArchReg::X86(_)) = dst {
            let merged = match width {
                OpWidth::W8 => (ctx.read_vreg(dst) & !0xFFu64) | (value & 0xFF),
                OpWidth::W16 => (ctx.read_vreg(dst) & !0xFFFFu64) | (value & 0xFFFF),
                _ => value,
            };
            ctx.write_vreg(dst, merged);
        } else {
            ctx.write_vreg(dst, value);
        }
    }

    pub(crate) fn write_x86_partial(ctx: &mut SmirContext, dst: VReg, value: u64, width: OpWidth) {
        if let VReg::Arch(ArchReg::X86(_)) = dst {
            if matches!(width, OpWidth::W8 | OpWidth::W16) {
                let mask = width.mask();
                let prev = ctx.read_vreg(dst);
                ctx.write_vreg(dst, (prev & !mask) | (value & mask));
                return;
            }
        }
        ctx.write_vreg(dst, value & width.mask());
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    /// Read source operand
    pub(crate) fn read_src_operand(&self, ctx: &SmirContext, src: &SrcOperand) -> u64 {
        match src {
            SrcOperand::Reg(r) => ctx.read_vreg(*r),
            SrcOperand::Imm(i) | SrcOperand::Imm64(i) => *i as u64,
            SrcOperand::Shifted { reg, shift, amount } => {
                let val = ctx.read_vreg(*reg);
                match shift {
                    ShiftOp::Lsl => val << amount,
                    ShiftOp::Lsr => val >> amount,
                    ShiftOp::Asr => ((val as i64) >> amount) as u64,
                    ShiftOp::Ror => val.rotate_right(*amount as u32),
                    ShiftOp::Rrx => {
                        // This needs the carry flag, simplified here
                        val >> 1
                    }
                }
            }
            SrcOperand::Extended { reg, extend, shift } => {
                let val = ctx.read_vreg(*reg);
                let extended = match extend {
                    ExtendOp::Uxtb => val & 0xFF,
                    ExtendOp::Uxth => val & 0xFFFF,
                    ExtendOp::Uxtw => val & 0xFFFF_FFFF,
                    ExtendOp::Uxtx => val,
                    ExtendOp::Sxtb => ((val as i8) as i64) as u64,
                    ExtendOp::Sxth => ((val as i16) as i64) as u64,
                    ExtendOp::Sxtw => ((val as i32) as i64) as u64,
                    ExtendOp::Sxtx => val,
                };
                extended << shift
            }
        }
    }

    /// Compute effective address
    pub(crate) fn compute_address(&self, ctx: &SmirContext, addr: &Address) -> GuestAddr {
        match addr {
            Address::Direct(r) => ctx.read_vreg(*r),
            Address::BaseOffset { base, offset, .. } => {
                ctx.read_vreg(*base).wrapping_add(*offset as u64)
            }
            Address::BaseIndexScale {
                base,
                index,
                scale,
                disp,
                ..
            } => {
                let base_val = base.map(|b| ctx.read_vreg(b)).unwrap_or(0);
                let index_val = ctx.read_vreg(*index);
                base_val
                    .wrapping_add(index_val.wrapping_mul(*scale as u64))
                    .wrapping_add(*disp as i64 as u64)
            }
            Address::PcRel { offset, base, .. } => {
                let base_pc = base.unwrap_or(ctx.pc);
                base_pc.wrapping_add(*offset as u64)
            }
            Address::GpRel { offset } => {
                let gp = match &ctx.arch_regs {
                    ArchRegState::Hexagon(hex) => hex.gp as u64,
                    _ => 0,
                };
                gp.wrapping_add(*offset as i64 as u64)
            }
            Address::Absolute(a) => *a,
            Address::SegmentRel {
                segment,
                base,
                index,
                scale,
                disp,
            } => {
                // [segment_base + base + index*scale + disp]. The segment base
                // lives in the FsBase/GsBase architectural register.
                let seg = ctx.read_vreg(*segment);
                let base_val = base.map(|b| ctx.read_vreg(b)).unwrap_or(0);
                let index_val = index.map(|i| ctx.read_vreg(i)).unwrap_or(0);
                seg.wrapping_add(base_val)
                    .wrapping_add(index_val.wrapping_mul(*scale as u64))
                    .wrapping_add(*disp as u64)
            }
        }
    }

    /// Compute an x86-64 effective address under a 32-bit address-size
    /// override. Offset components wrap modulo 2^32 and are zero-extended;
    /// FS/GS is then added as a full 64-bit segment base.
    pub(crate) fn compute_x86_addr32(&self, ctx: &SmirContext, addr: &Address) -> GuestAddr {
        match addr {
            Address::SegmentRel {
                segment,
                base,
                index,
                scale,
                disp,
            } => {
                let base = base.map(|reg| ctx.read_vreg(reg) as u32).unwrap_or(0);
                let index = index.map(|reg| ctx.read_vreg(reg) as u32).unwrap_or(0);
                let offset = base
                    .wrapping_add(index.wrapping_mul(u32::from(*scale)))
                    .wrapping_add(*disp as u32);
                ctx.read_vreg(*segment).wrapping_add(u64::from(offset))
            }
            _ => u64::from(self.compute_address(ctx, addr) as u32),
        }
    }

    /// Load from memory
    pub(crate) fn load_memory(
        &self,
        memory: &mut dyn SmirMemory,
        addr: GuestAddr,
        width: MemWidth,
        sign: SignExtend,
    ) -> Result<u64, MemoryError> {
        let mut buf = [0u8; 8];
        let size = width.bytes() as usize;
        memory.read(addr, &mut buf[..size])?;

        let raw = u64::from_le_bytes(buf);

        Ok(match sign {
            SignExtend::Zero => {
                if size >= 8 {
                    raw
                } else {
                    raw & ((1u64 << (size * 8)) - 1)
                }
            }
            SignExtend::Sign => {
                if size >= 8 {
                    raw
                } else {
                    let shift = 64 - size * 8;
                    ((raw as i64) << shift >> shift) as u64
                }
            }
        })
    }

    /// Store to memory
    pub(crate) fn store_memory(
        &self,
        memory: &mut dyn SmirMemory,
        addr: GuestAddr,
        value: u64,
        width: MemWidth,
    ) -> Result<(), MemoryError> {
        let bytes = value.to_le_bytes();
        let size = width.bytes() as usize;
        memory.write(addr, &bytes[..size])
    }
}
