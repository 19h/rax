//! Interpreter core loop, block/function execution

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
    /// Create a new interpreter
    pub fn new() -> Self {
        SmirInterpreter {
            block_cache: HashMap::new(),
            func_cache: HashMap::new(),
            max_insns_per_run: 10000,
            block_addrs: HashMap::new(),
        }
    }


    /// Set the maximum instructions per run
    pub fn set_max_insns(&mut self, max: u64) {
        self.max_insns_per_run = max;
    }


    /// Add a block to the cache
    pub fn add_block(&mut self, addr: GuestAddr, block: SmirBlock) {
        self.block_addrs.insert(block.id, addr);
        self.block_cache.insert(addr, block);
    }


    /// Add a function to the cache
    pub fn add_function(&mut self, func: SmirFunction) {
        let addr = func.guest_range.0;
        for block in &func.blocks {
            self.block_addrs.insert(block.id, block.guest_pc);
        }
        self.func_cache.insert(addr, func);
    }


    /// Run until exit condition
    pub fn run(&mut self, ctx: &mut SmirContext, memory: &mut dyn SmirMemory) -> ExitReason {
        let limit = ctx.insn_count + self.max_insns_per_run;

        loop {
            // Check instruction limit
            if ctx.insn_count >= limit {
                return ExitReason::InsnLimit;
            }

            // Check for pending exit
            if let Some(reason) = ctx.exit_reason.take() {
                return reason;
            }

            // Check breakpoints
            if ctx.debug.has_breakpoint(ctx.pc) {
                return ExitReason::Breakpoint { addr: ctx.pc };
            }

            // Get block from cache
            let block = match self.block_cache.get(&ctx.pc) {
                Some(b) => b.clone(),
                None => {
                    return ExitReason::BlockNotFound { addr: ctx.pc };
                }
            };

            // Execute block
            match self.execute_block(ctx, memory, &block) {
                BlockResult::Continue(next_pc) => {
                    ctx.pc = next_pc;
                }
                BlockResult::Exit(reason) => {
                    return reason;
                }
            }

            // Single-step mode
            if ctx.debug.single_step {
                return ExitReason::SingleStep;
            }
        }
    }


    /// Execute a single block
    pub fn execute_block(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        block: &SmirBlock,
    ) -> BlockResult {
        // Execute each operation
        for op in &block.ops {
            if let Err(e) = self.execute_op(ctx, memory, op) {
                return BlockResult::Exit(ExitReason::MemoryFault {
                    addr: match e {
                        MemoryError::PageFault { addr, .. } => addr,
                        MemoryError::AccessViolation { addr, .. } => addr,
                        MemoryError::Alignment { addr, .. } => addr,
                        MemoryError::Mmio { addr, .. } => addr,
                        MemoryError::OutOfBounds { addr } => addr,
                        MemoryError::ExclusiveFailed => ctx.pc,
                    },
                    write: match e {
                        MemoryError::PageFault { write, .. }
                        | MemoryError::AccessViolation { write, .. } => write,
                        // These error variants do not carry an access direction;
                        // recover it from the faulting operation's memory-effect
                        // metadata rather than misreporting every store as a read.
                        _ => op.kind.writes_memory(),
                    },
                });
            }
            ctx.insn_count += 1;
            if let Some(reason) = ctx.exit_reason.take() {
                return BlockResult::Exit(reason);
            }
        }

        // Execute terminator
        self.execute_terminator(ctx, memory, &block.terminator)
    }


    /// OR the Hexagon USR sticky overflow/saturation bit (USR:0) into the
    /// context's USR register, preserving all other bits. Used by saturating
    /// ops whose `fSATN`/`fSATUN` semantics set `fSET_OVF` when a clamp
    /// clobbered the value.
    #[inline]
    pub(crate) fn set_hex_ovf(ctx: &mut SmirContext) {
        let usr = ctx.read_arch_reg(ArchReg::Hexagon(HexagonReg::Usr));
        ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::Usr), usr | 1);
    }


    /// Execute a single operation
    pub(crate) fn execute_op(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let x86_hint = op.x86_hint;
        match &op.kind {
            // ==================================================================
            // INTEGER ARITHMETIC
            // ==================================================================
            OpKind::Add {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = a.wrapping_add(b) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.set_lazy_add(a, b, result, *width);
                }
            }

            OpKind::Sub {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = a.wrapping_sub(b) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.set_lazy_sub(a, b, result, *width);
                }
            }

            OpKind::Adc {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let cf = if ctx.flags.get_cf() { 1u64 } else { 0 };
                let result = a.wrapping_add(b).wrapping_add(cf) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    // Original operands + carry-in: CF/AF/OF must account for the
                    // carry (folding cf into `b` loses the carry-out).
                    ctx.flags.set_lazy_adc(a, b, cf, result, *width);
                }
            }

            OpKind::Sbb {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let cf = if ctx.flags.get_cf() { 1u64 } else { 0 };
                let result = a.wrapping_sub(b).wrapping_sub(cf) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.set_lazy_sbb(a, b, cf, result, *width);
                }
            }

            OpKind::Neg {
                dst,
                src,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src);
                let result = (0u64.wrapping_sub(a)) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Neg,
                        result,
                        left: a,
                        right: 0,
                        width: *width,
                        high: 0,
                    });
                }
            }

            OpKind::Inc {
                dst,
                src,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src);
                let result = a.wrapping_add(1) & width.mask();
                Self::write_x86_partial(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags::inc(a, result, *width));
                }
            }

            OpKind::Dec {
                dst,
                src,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src);
                let result = a.wrapping_sub(1) & width.mask();
                Self::write_x86_partial(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags::dec(a, result, *width));
                }
            }

            OpKind::Cmp { src1, src2, width } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = a.wrapping_sub(b) & width.mask();

                ctx.flags.set_lazy_sub(a, b, result, *width);
            }

            OpKind::MulU {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1) & width.mask();
                let b = self.read_src_operand(ctx, src2) & width.mask();

                let (result_lo, result_hi) = match width {
                    OpWidth::W8 => {
                        let r = (a as u16) * (b as u16);
                        ((r & 0xFF) as u64, ((r >> 8) & 0xFF) as u64)
                    }
                    OpWidth::W16 => {
                        let r = (a as u32) * (b as u32);
                        ((r & 0xFFFF) as u64, ((r >> 16) & 0xFFFF) as u64)
                    }
                    OpWidth::W32 => {
                        let r = (a as u64) * (b as u64);
                        (r & 0xFFFF_FFFF, (r >> 32) & 0xFFFF_FFFF)
                    }
                    OpWidth::W64 => {
                        let r = (a as u128) * (b as u128);
                        (r as u64, (r >> 64) as u64)
                    }
                    OpWidth::W128 => {
                        // 128-bit multiply not supported
                        (a.wrapping_mul(b), 0)
                    }
                };

                if *width == OpWidth::W8 {
                    // 8-bit MUL: the full 16-bit product lives in AX (AH:AL);
                    // DX is untouched. Merge the 16-bit product into AX.
                    Self::write_gpr(ctx, *dst_lo, result_lo | (result_hi << 8), OpWidth::W16);
                } else {
                    Self::write_gpr(ctx, *dst_lo, result_lo, *width);
                    if let Some(hi) = dst_hi {
                        Self::write_gpr(ctx, *hi, result_hi, *width);
                    }
                }

                ctx.flags.set_lazy_with_update(
                    LazyFlags {
                        op: LazyFlagOp::Mul,
                        result: result_lo,
                        left: a,
                        right: b,
                        width: *width,
                        high: result_hi,
                    },
                    *flags,
                );
            }

            OpKind::MulS {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = self.sign_extend(ctx.read_vreg(*src1), *width);
                let b = self.sign_extend(self.read_src_operand(ctx, src2), *width);

                let (result_lo, result_hi) = match width {
                    OpWidth::W8 => {
                        let r = (a as i8 as i16) * (b as i8 as i16);
                        ((r as u16 & 0xFF) as u64, (((r as u16) >> 8) & 0xFF) as u64)
                    }
                    OpWidth::W16 => {
                        let r = (a as i16 as i32) * (b as i16 as i32);
                        (
                            (r as u32 & 0xFFFF) as u64,
                            (((r as u32) >> 16) & 0xFFFF) as u64,
                        )
                    }
                    OpWidth::W32 => {
                        let r = (a as i32 as i64) * (b as i32 as i64);
                        ((r as u64 & 0xFFFF_FFFF), ((r as u64) >> 32) & 0xFFFF_FFFF)
                    }
                    OpWidth::W64 => {
                        let r = (a as i64 as i128) * (b as i64 as i128);
                        (r as u64, (r >> 64) as u64)
                    }
                    OpWidth::W128 => ((a as i64).wrapping_mul(b as i64) as u64, 0),
                };

                if *width == OpWidth::W8 {
                    // 8-bit IMUL: the full 16-bit product lives in AX (AH:AL);
                    // DX is untouched.
                    Self::write_gpr(ctx, *dst_lo, result_lo | (result_hi << 8), OpWidth::W16);
                } else {
                    Self::write_gpr(ctx, *dst_lo, result_lo, *width);
                    if let Some(hi) = dst_hi {
                        Self::write_gpr(ctx, *hi, result_hi, *width);
                    }
                }

                ctx.flags.set_lazy_with_update(
                    LazyFlags {
                        // Signed: CF/OF iff the product isn't the sign-extension
                        // of the low half (distinct from unsigned Mul's high!=0).
                        op: LazyFlagOp::Imul,
                        result: result_lo,
                        left: a as u64,
                        right: b as u64,
                        width: *width,
                        high: result_hi,
                    },
                    *flags,
                );
            }

            OpKind::MulAdd {
                dst,
                acc,
                src1,
                src2,
                width,
            } => {
                let a = ctx.read_vreg(*src1) & width.mask();
                let b = ctx.read_vreg(*src2) & width.mask();
                let c = ctx.read_vreg(*acc) & width.mask();
                let result = c.wrapping_add(a.wrapping_mul(b)) & width.mask();
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::MulSub {
                dst,
                acc,
                src1,
                src2,
                width,
            } => {
                let a = ctx.read_vreg(*src1) & width.mask();
                let b = ctx.read_vreg(*src2) & width.mask();
                let c = ctx.read_vreg(*acc) & width.mask();
                let result = c.wrapping_sub(a.wrapping_mul(b)) & width.mask();
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::DivU {
                quot,
                rem,
                src1,
                src2,
                width,
                ..
            } => {
                let mask = width.mask();
                let b = (self.read_src_operand(ctx, src2) & mask) as u128;
                if b == 0 {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                // x86 DIV divides the double-width RDX:RAX (AX for 8-bit) by the
                // operand; non-x86 contexts have no high half (single-width div).
                let lo = ctx.read_vreg(*src1) & mask;
                let is_x86 = matches!(ctx.arch_regs, ArchRegState::X86_64(_));
                let dividend: u128 = if !is_x86 {
                    lo as u128
                } else if *width == OpWidth::W8 {
                    (ctx.read_arch_reg(ArchReg::X86(X86Reg::Rax)) & 0xFFFF) as u128
                } else {
                    let hi = ctx.read_arch_reg(ArchReg::X86(X86Reg::Rdx)) & mask;
                    ((hi as u128) << width.bits()) | (lo as u128)
                };
                let q = dividend / b;
                let r = dividend % b;
                if q > mask as u128 {
                    // Quotient overflow -> #DE.
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let (q, r) = (q as u64, r as u64);
                if is_x86 && *width == OpWidth::W8 {
                    // 8-bit: quotient -> AL, remainder -> AH.
                    let rax = ctx.read_arch_reg(ArchReg::X86(X86Reg::Rax));
                    let new = (rax & !0xFFFF) | ((r & 0xFF) << 8) | (q & 0xFF);
                    ctx.write_arch_reg(ArchReg::X86(X86Reg::Rax), new);
                } else {
                    Self::write_gpr(ctx, *quot, q, *width);
                    if let Some(rem_reg) = rem {
                        Self::write_gpr(ctx, *rem_reg, r, *width);
                    }
                }
            }

            OpKind::DivS {
                quot,
                rem,
                src1,
                src2,
                width,
                ..
            } => {
                let mask = width.mask();
                let bits = width.bits();
                let b = self.sign_extend(self.read_src_operand(ctx, src2), *width) as i64 as i128;
                if b == 0 {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let is_x86 = matches!(ctx.arch_regs, ArchRegState::X86_64(_));
                // Signed double-width dividend: RDX:RAX (AX for 8-bit) on x86.
                let dividend: i128 = if !is_x86 {
                    self.sign_extend(ctx.read_vreg(*src1), *width) as i64 as i128
                } else if *width == OpWidth::W8 {
                    ((ctx.read_arch_reg(ArchReg::X86(X86Reg::Rax)) & 0xFFFF) as u16) as i16 as i128
                } else {
                    let lo = ctx.read_vreg(*src1) & mask;
                    let hi = ctx.read_arch_reg(ArchReg::X86(X86Reg::Rdx)) & mask;
                    let combined = ((hi as u128) << bits) | (lo as u128);
                    Self::sext128(combined, bits * 2)
                };
                let q = dividend.wrapping_div(b);
                let r = dividend.wrapping_rem(b);
                // x86 IDIV raises #DE when the quotient does not fit. Non-x86
                // users of DivS are single-width operations and keep the
                // wrapping MIN / -1 result.
                if is_x86 {
                    let qmax = (1i128 << (bits - 1)) - 1;
                    let qmin = -(1i128 << (bits - 1));
                    if q < qmin || q > qmax {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                }
                let (q, r) = ((q as u64) & mask, (r as u64) & mask);
                if is_x86 && *width == OpWidth::W8 {
                    let rax = ctx.read_arch_reg(ArchReg::X86(X86Reg::Rax));
                    let new = (rax & !0xFFFF) | ((r & 0xFF) << 8) | (q & 0xFF);
                    ctx.write_arch_reg(ArchReg::X86(X86Reg::Rax), new);
                } else {
                    Self::write_gpr(ctx, *quot, q, *width);
                    if let Some(rem_reg) = rem {
                        Self::write_gpr(ctx, *rem_reg, r, *width);
                    }
                }
            }

            // ==================================================================
            // BITWISE LOGICAL
            // ==================================================================
            OpKind::And {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = (a & b) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                ctx.flags
                    .set_lazy_with_update(LazyFlags::logic(result, *width), *flags);
            }

            OpKind::Or {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = (a | b) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                ctx.flags
                    .set_lazy_with_update(LazyFlags::logic(result, *width), *flags);
            }

            OpKind::Xor {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = (a ^ b) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                ctx.flags
                    .set_lazy_with_update(LazyFlags::logic(result, *width), *flags);
            }

            OpKind::Not { dst, src, width } => {
                let a = ctx.read_vreg(*src);
                let result = (!a) & width.mask();
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::Test { src1, src2, width } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = (a & b) & width.mask();

                ctx.flags.set_lazy_logic(result, *width);
            }

            OpKind::AndNot {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = (a & !b) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                let lazy = if matches!(flags, FlagUpdate::Specific(_)) {
                    LazyFlags::andn(result, *width)
                } else {
                    LazyFlags::logic(result, *width)
                };
                ctx.flags.set_lazy_with_update(lazy, *flags);
            }

            // ==================================================================
            // SHIFTS AND ROTATES
            // ==================================================================
            OpKind::Shl {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let count_mask = Self::scalar_shift_count_mask(ctx.source_arch, *width);
                let amt = self.read_src_operand(ctx, amount) & count_mask;
                let result = if amt >= width.bits() as u64 {
                    0
                } else {
                    (val << amt) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                if amt != 0 {
                    ctx.flags.set_lazy_with_update(
                        LazyFlags {
                            op: LazyFlagOp::Shl,
                            result,
                            left: val,
                            right: amt,
                            width: *width,
                            high: 0,
                        },
                        *flags,
                    );
                }
            }

            OpKind::Shr {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let count_mask = Self::scalar_shift_count_mask(ctx.source_arch, *width);
                let amt = self.read_src_operand(ctx, amount) & count_mask;
                let result = if amt >= width.bits() as u64 {
                    0
                } else {
                    (val >> amt) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                if amt != 0 {
                    ctx.flags.set_lazy_with_update(
                        LazyFlags {
                            op: LazyFlagOp::Shr,
                            result,
                            left: val,
                            right: amt,
                            width: *width,
                            high: 0,
                        },
                        *flags,
                    );
                }
            }

            OpKind::Sar {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                // Mask to the operand width BEFORE sign-extending, or stale upper
                // register bits leak into both the shifted-out bits and the sign.
                let val = self.sign_extend(ctx.read_vreg(*src) & width.mask(), *width);
                let count_mask = Self::scalar_shift_count_mask(ctx.source_arch, *width);
                let amt = self.read_src_operand(ctx, amount) & count_mask;
                let result = if amt >= width.bits() as u64 {
                    if (val as i64) < 0 { width.mask() } else { 0 }
                } else {
                    ((val as i64 >> amt) as u64) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                // A masked shift count of 0 leaves all status flags unchanged.
                if amt != 0 {
                    ctx.flags.set_lazy_with_update(
                        LazyFlags {
                            op: LazyFlagOp::Sar,
                            result,
                            left: val as u64,
                            right: amt,
                            width: *width,
                            high: 0,
                        },
                        *flags,
                    );
                }
            }

            OpKind::Shld {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let left = ctx.read_vreg(*dst) & width.mask();
                let right = ctx.read_vreg(*src) & width.mask();
                let bits = width.bits() as u64;
                let mask = if bits == 64 { 0x3F } else { 0x1F };
                let amt = self.read_src_operand(ctx, amount) & mask;
                let defined = amt != 0 && amt <= bits;
                let result = if !defined {
                    left
                } else {
                    ((left << amt) | (right >> (bits - amt))) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                // The deterministic no-op cases (zero or a masked subword count above
                // the operand width) preserve flags; otherwise CF is the last bit out
                // of the destination's top.
                if defined && flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Shld,
                        result,
                        left,
                        right: amt,
                        width: *width,
                        high: 0,
                    });
                }
            }

            OpKind::Shrd {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let left = ctx.read_vreg(*dst) & width.mask();
                let right = ctx.read_vreg(*src) & width.mask();
                let bits = width.bits() as u64;
                let mask = if bits == 64 { 0x3F } else { 0x1F };
                let amt = self.read_src_operand(ctx, amount) & mask;
                let defined = amt != 0 && amt <= bits;
                let result = if !defined {
                    left
                } else {
                    ((left >> amt) | (right << (bits - amt))) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                // The deterministic no-op cases (zero or a masked subword count above
                // the operand width) preserve flags; otherwise CF is the last bit out
                // of the destination's bottom.
                if defined && flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Shrd,
                        result,
                        left,
                        right: amt,
                        width: *width,
                        high: 0,
                    });
                }
            }

            OpKind::X86NddDoubleShift {
                dst,
                base,
                fill,
                amount,
                width,
                left,
                flags,
            } => {
                let base = ctx.read_vreg(*base) & width.mask();
                let fill = ctx.read_vreg(*fill) & width.mask();
                let bits = width.bits() as u64;
                let count_mask = if bits == 64 { 0x3F } else { 0x1F };
                let amt = self.read_src_operand(ctx, amount) & count_mask;
                let defined = amt != 0 && amt <= bits;
                let result = if !defined {
                    base
                } else if *left {
                    ((base << amt) | (fill >> (bits - amt))) & width.mask()
                } else {
                    ((base >> amt) | (fill << (bits - amt))) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);
                if defined && flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags {
                        op: if *left {
                            LazyFlagOp::Shld
                        } else {
                            LazyFlagOp::Shrd
                        },
                        result,
                        left: base,
                        right: amt,
                        width: *width,
                        high: 0,
                    });
                }
            }

            OpKind::Rol {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let bits = width.bits() as u64;
                // x86 masks the count to 5 bits (6 for 64-bit); the rotation
                // amount is that masked count mod the width.
                let cmask = if bits == 64 { 0x3F } else { 0x1F };
                let masked = self.read_src_operand(ctx, amount) & cmask;
                let amt = masked % bits;
                let result = if amt == 0 {
                    val
                } else {
                    ((val << amt) | (val >> (bits - amt))) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                // CF/OF update iff the MASKED count != 0 — even when the rotation
                // amount (masked mod width) is 0, e.g. ROL r16 by 16. `right`
                // carries the masked count so OF keys on masked==1.
                if masked != 0 && flags.updates_any() {
                    ctx.flags.materialize_all();
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Rotate,
                        result,
                        left: val,
                        right: masked,
                        width: *width,
                        high: 0,
                    });
                }
            }

            OpKind::Ror {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let bits = width.bits() as u64;
                let cmask = if bits == 64 { 0x3F } else { 0x1F };
                let masked = self.read_src_operand(ctx, amount) & cmask;
                let amt = masked % bits;
                let result = if amt == 0 {
                    val
                } else {
                    ((val >> amt) | (val << (bits - amt))) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                // CF/OF update iff the MASKED count != 0 (see Rol).
                if masked != 0 && flags.updates_any() {
                    ctx.flags.materialize_all();
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Ror,
                        result,
                        left: val,
                        right: masked,
                        width: *width,
                        high: 0,
                    });
                }
            }

            OpKind::ArmRegShift {
                dst,
                src,
                amount,
                shift,
                width,
                flags,
            } => {
                use crate::isa::arm::aarch32::cpu::shift_c;
                use crate::isa::arm::decoder::ShiftType;

                debug_assert_eq!(*width, OpWidth::W32);
                let value = ctx.read_vreg(*src) as u32;
                let count = (self.read_src_operand(ctx, amount) & 0xff) as u32;
                ctx.flags.materialize_all();
                let carry_in = ctx.flags.materialized.cf;
                let shift_type = match shift {
                    crate::smir::ir::types::ShiftOp::Lsl => ShiftType::LSL,
                    crate::smir::ir::types::ShiftOp::Lsr => ShiftType::LSR,
                    crate::smir::ir::types::ShiftOp::Asr => ShiftType::ASR,
                    crate::smir::ir::types::ShiftOp::Ror => ShiftType::ROR,
                    crate::smir::ir::types::ShiftOp::Rrx => ShiftType::RRX,
                };
                let (result, carry) = shift_c(value, shift_type, count, carry_in);
                Self::write_gpr(ctx, *dst, u64::from(result), OpWidth::W32);

                let updated = flags.as_set();
                if updated.contains(FlagSet::SF) {
                    ctx.flags.materialized.sf = result & 0x8000_0000 != 0;
                }
                if updated.contains(FlagSet::ZF) {
                    ctx.flags.materialized.zf = result == 0;
                }
                if updated.contains(FlagSet::CF) {
                    ctx.flags.materialized.cf = carry;
                }
            }

            OpKind::ArmDpRegShift {
                kind,
                dst,
                rn,
                rm,
                rs,
                shift,
                flags,
            } => {
                use crate::isa::arm::aarch32::cpu::{add_with_carry, shift_c};
                use crate::isa::arm::decoder::ShiftType;
                use crate::smir::ir::ops::ArmDpRegShiftKind;

                ctx.flags.materialize_all();
                let carry_in = ctx.flags.materialized.cf;
                let value = ctx.read_vreg(*rm) as u32;
                let count = (ctx.read_vreg(*rs) & 0xff) as u32;
                let shift_type = match shift {
                    crate::smir::ir::types::ShiftOp::Lsl => ShiftType::LSL,
                    crate::smir::ir::types::ShiftOp::Lsr => ShiftType::LSR,
                    crate::smir::ir::types::ShiftOp::Asr => ShiftType::ASR,
                    crate::smir::ir::types::ShiftOp::Ror => ShiftType::ROR,
                    crate::smir::ir::types::ShiftOp::Rrx => ShiftType::RRX,
                };
                let (shifted, shifter_carry) = shift_c(value, shift_type, count, carry_in);
                let lhs = rn.map(|reg| ctx.read_vreg(reg) as u32).unwrap_or(0);

                let (result, arithmetic_flags) = match kind {
                    ArmDpRegShiftKind::And | ArmDpRegShiftKind::Tst => (lhs & shifted, None),
                    ArmDpRegShiftKind::Eor | ArmDpRegShiftKind::Teq => (lhs ^ shifted, None),
                    ArmDpRegShiftKind::Orr => (lhs | shifted, None),
                    ArmDpRegShiftKind::Mov => (shifted, None),
                    ArmDpRegShiftKind::Bic => (lhs & !shifted, None),
                    ArmDpRegShiftKind::Mvn => (!shifted, None),
                    ArmDpRegShiftKind::Sub | ArmDpRegShiftKind::Cmp => {
                        let (result, carry, overflow) = add_with_carry(lhs, !shifted, 1);
                        (result, Some((carry, overflow)))
                    }
                    ArmDpRegShiftKind::Rsb => {
                        let (result, carry, overflow) = add_with_carry(shifted, !lhs, 1);
                        (result, Some((carry, overflow)))
                    }
                    ArmDpRegShiftKind::Add | ArmDpRegShiftKind::Cmn => {
                        let (result, carry, overflow) = add_with_carry(lhs, shifted, 0);
                        (result, Some((carry, overflow)))
                    }
                    ArmDpRegShiftKind::Adc => {
                        let (result, carry, overflow) =
                            add_with_carry(lhs, shifted, u32::from(carry_in));
                        (result, Some((carry, overflow)))
                    }
                    ArmDpRegShiftKind::Sbc => {
                        let (result, carry, overflow) =
                            add_with_carry(lhs, !shifted, u32::from(carry_in));
                        (result, Some((carry, overflow)))
                    }
                    ArmDpRegShiftKind::Rsc => {
                        let (result, carry, overflow) =
                            add_with_carry(shifted, !lhs, u32::from(carry_in));
                        (result, Some((carry, overflow)))
                    }
                };

                if let Some(dst) = dst {
                    Self::write_gpr(ctx, *dst, u64::from(result), OpWidth::W32);
                }

                let updated = flags.as_set();
                if updated.contains(FlagSet::SF) {
                    ctx.flags.materialized.sf = result & 0x8000_0000 != 0;
                }
                if updated.contains(FlagSet::ZF) {
                    ctx.flags.materialized.zf = result == 0;
                }
                if updated.contains(FlagSet::CF) {
                    ctx.flags.materialized.cf = arithmetic_flags
                        .map(|(carry, _)| carry)
                        .unwrap_or(shifter_carry);
                }
                if updated.contains(FlagSet::OF) {
                    debug_assert!(arithmetic_flags.is_some());
                    if let Some((_, overflow)) = arithmetic_flags {
                        ctx.flags.materialized.of = overflow;
                    }
                }
            }

            OpKind::Rcl {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let count = self.read_src_operand(ctx, amount);
                let bits = width.bits() as u64;
                let count_mask = if bits == 64 { 0x3F } else { 0x1F };
                let masked = count & count_mask;
                ctx.flags.materialize_all();
                let (result, carry, effective) =
                    Self::x86_rcl(val, count, ctx.flags.materialized.cf, *width);

                Self::write_gpr(ctx, *dst, result, *width);

                if effective != 0 && flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Rcl,
                        result,
                        left: val,
                        right: masked,
                        width: *width,
                        high: u64::from(carry),
                    });
                }
            }

            OpKind::Rcr {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let count = self.read_src_operand(ctx, amount);
                let bits = width.bits() as u64;
                let count_mask = if bits == 64 { 0x3F } else { 0x1F };
                let masked = count & count_mask;
                ctx.flags.materialize_all();
                let (result, carry, effective) =
                    Self::x86_rcr(val, count, ctx.flags.materialized.cf, *width);

                Self::write_gpr(ctx, *dst, result, *width);

                if effective != 0 && flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Rcr,
                        result,
                        left: val,
                        right: masked,
                        width: *width,
                        high: u64::from(carry),
                    });
                }
            }

            // Hexagon bidirectional register-amount shift (S2_{asl,asr,lsr,lsl}
            // _r_r and the pair forms via a W64 temp). The count is the sign-
            // extension of the low 7 bits of `amount` to [-64, 63]; a negative
            // count reverses the shift direction. All arithmetic is performed in
            // i128/u128 with the spec's two-step `>> (n-1) >> 1` / `<< (n-1) << 1`
            // idiom so a `|count| == 64` shift never triggers Rust shift overflow.
            OpKind::BidirShift {
                dst,
                src,
                amount,
                kind,
                width,
            } => {
                let bits = width.bits();
                let raw = self.read_src_operand(ctx, src) & width.mask();
                // sxtn7(amount): sign-extend the low 7 bits to [-64, 63].
                let cnt = {
                    let low7 = (self.read_src_operand(ctx, amount) & 0x7f) as i64;
                    ((low7 << 57) >> 57) as i64
                };
                let result: u64 = match kind {
                    // arithmetic left (asl): + shifts left, - shifts (arith)right.
                    0 => {
                        let s = Self::sext128(raw as u128, bits);
                        let r = if cnt < 0 {
                            let n = (-cnt) as u32 - 1;
                            (s >> n) >> 1
                        } else {
                            s << (cnt as u32)
                        };
                        r as u64 & width.mask()
                    }
                    // arithmetic right (asr): + shifts (arith)right, - shifts left.
                    1 => {
                        let s = Self::sext128(raw as u128, bits);
                        let r = if cnt < 0 {
                            let n = (-cnt) as u32 - 1;
                            (s << n) << 1
                        } else {
                            s >> (cnt as u32)
                        };
                        r as u64 & width.mask()
                    }
                    // logical left (lsl): + shifts left, - shifts (logical)right.
                    2 => {
                        let u = raw as u128;
                        let r = if cnt < 0 {
                            let n = (-cnt) as u32 - 1;
                            (u >> n) >> 1
                        } else {
                            u << (cnt as u32)
                        };
                        r as u64 & width.mask()
                    }
                    // logical right (lsr): + shifts (logical)right, - shifts left.
                    _ => {
                        let u = raw as u128;
                        let r = if cnt < 0 {
                            let n = (-cnt) as u32 - 1;
                            (u << n) << 1
                        } else {
                            u >> (cnt as u32)
                        };
                        r as u64 & width.mask()
                    }
                };
                Self::write_gpr(ctx, *dst, result, *width);
            }

            // Hexagon saturating clamp (`fSATN`/`fSATUN`) with the USR:OVF sticky
            // overflow bit. The source temp is read and sign-extended from the
            // operation `width` (the lifter feeds an already-sign-extended wide
            // value), clamped to a `sat_bits` signed/unsigned range, and the
            // (truncated) result stored. When the value was actually clamped and
            // `set_ovf` is set, USR bit 0 is OR-ed in (sticky, other bits kept).
            OpKind::SatN {
                dst,
                src,
                sat_bits,
                signed,
                set_ovf,
                width,
            } => {
                // Read the source and sign-extend from `width` to a full i64 so
                // the clamp compares signed magnitudes correctly.
                let raw = self.read_src_operand(ctx, src);
                let val = Self::sext128(raw as u128, width.bits()) as i64;
                let n = *sat_bits as u32;
                let (lo, hi) = if *signed {
                    (-(1i64 << (n - 1)), (1i64 << (n - 1)) - 1)
                } else {
                    (0i64, (1i64 << n) - 1)
                };
                let (clamped, ovf) = if val < lo {
                    (lo, true)
                } else if val > hi {
                    (hi, true)
                } else {
                    (val, false)
                };
                if ovf && *set_ovf {
                    Self::set_hex_ovf(ctx);
                }
                // Store the clamped value's low `width` bits (two's-complement
                // low bits for a negative signed-clamp result).
                Self::write_gpr(ctx, *dst, (clamped as u64) & width.mask(), *width);
            }

            // Carry-less (GF(2)) polynomial multiply — Hexagon
            // `pmpyw`/`vpmpyh` (+ `_acc`) and x86 PCLMULQDQ.
            OpKind::ClMul {
                dst,
                dst_hi,
                src1,
                src2,
                elem_bits,
                lanes,
                acc,
            } => {
                // Carry-less product of two `bits`-wide operands: XOR-accumulate
                // of the shifted partial products (no carries; sign irrelevant).
                #[inline]
                pub(crate) fn clmul(a: u64, b: u64, bits: u32) -> u128 {
                    let mut prod: u128 = 0;
                    for k in 0..bits {
                        if (b >> k) & 1 == 1 {
                            prod ^= u128::from(a) << k;
                        }
                    }
                    prod
                }
                let a = self.read_src_operand(ctx, src1);
                let b = self.read_src_operand(ctx, src2);
                let bits = *elem_bits as u32;
                let elem_mask = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let result_mask = if *lanes == 1 {
                    elem_mask
                } else {
                    u64::from(u32::MAX)
                };
                let (mut lo, mut hi): (u64, u64) = if *lanes == 1 {
                    // One product split at the element boundary: 32x32 for
                    // Hexagon pmpyw, 64x64 for x86 PCLMULQDQ.
                    let p = clmul(a & elem_mask, b & elem_mask, bits);
                    (
                        (p & u128::from(elem_mask)) as u64,
                        ((p >> bits) & u128::from(elem_mask)) as u64,
                    )
                } else {
                    // vpmpyh: two 16x16 -> 32-bit products, interleaved:
                    //   lo.h0=p0.lo, lo.h1=p1.lo, hi.h0=p0.hi, hi.h1=p1.hi.
                    let x0 = a & 0xffff;
                    let x1 = (a >> 16) & 0xffff;
                    let y0 = b & 0xffff;
                    let y1 = (b >> 16) & 0xffff;
                    let p0 = (clmul(x0, y0, bits) & 0xffff_ffff) as u64;
                    let p1 = (clmul(x1, y1, bits) & 0xffff_ffff) as u64;
                    let lo = (p0 & 0xffff) | ((p1 & 0xffff) << 16);
                    let hi = ((p0 >> 16) & 0xffff) | (((p1 >> 16) & 0xffff) << 16);
                    (lo, hi)
                };
                if *acc {
                    lo ^= ctx.read_vreg(*dst) & result_mask;
                    if let Some(h) = dst_hi {
                        hi ^= ctx.read_vreg(*h) & result_mask;
                    }
                }
                let width = if bits == 64 && *lanes == 1 {
                    OpWidth::W64
                } else {
                    OpWidth::W32
                };
                Self::write_gpr(ctx, *dst, lo & result_mask, width);
                if let Some(h) = dst_hi {
                    Self::write_gpr(ctx, *h, hi & result_mask, width);
                }
            }

            OpKind::Crc32C {
                dst,
                crc,
                data,
                data_width,
            } => {
                // Reflected Castagnoli recurrence. Register byte 0 is consumed
                // first, matching x86's little-endian source interpretation.
                const POLY_REFLECTED: u32 = 0x82F6_3B78;
                let mut value = ctx.read_vreg(*crc) as u32;
                let input = ctx.read_vreg(*data);
                for byte in 0..(data_width.bits() / 8) {
                    value ^= ((input >> (byte * 8)) & 0xFF) as u32;
                    for _ in 0..8 {
                        value = (value >> 1) ^ (POLY_REFLECTED & 0u32.wrapping_sub(value & 1));
                    }
                }
                // Both r32 and r64 instruction forms architecturally clear the
                // destination's high 32 bits.
                Self::write_gpr(ctx, *dst, u64::from(value), OpWidth::W64);
            }

            // `M7_wcmpy*` — 32x32 wide complex multiply with an i128 accumulator,
            // `:<<1` scale (>>31), optional `:rnd`, and signed-32 saturation.
            OpKind::CmpyW128Sat {
                dst,
                rss_lo,
                rss_hi,
                rtt_lo,
                rtt_hi,
                w0,
                w1,
                w2,
                w3,
                add,
                rnd,
            } => {
                // Reconstruct the two register pairs (even = low word, odd = high
                // word) and select a signed 32-bit word from each.
                let rss = (ctx.read_vreg(*rss_lo) & 0xffff_ffff)
                    | ((ctx.read_vreg(*rss_hi) & 0xffff_ffff) << 32);
                let rtt = (ctx.read_vreg(*rtt_lo) & 0xffff_ffff)
                    | ((ctx.read_vreg(*rtt_hi) & 0xffff_ffff) << 32);
                #[inline]
                pub(crate) fn word(src: u64, n: u8) -> i128 {
                    ((src >> (n as u32 * 32)) as u32 as i32) as i128
                }
                let term0 = word(rss, *w0) * word(rtt, *w1);
                let term1 = word(rss, *w2) * word(rtt, *w3);
                let mut accv: i128 = if *add { term0 + term1 } else { term0 - term1 };
                if *rnd {
                    accv += 0x4000_0000i128;
                }
                let shifted = accv >> 31; // arithmetic shift of the signed accumulator
                // Saturate to signed 32 bits with the sticky USR:OVF bit.
                let lo = i32::MIN as i128;
                let hi = i32::MAX as i128;
                let (clamped, ovf) = if shifted < lo {
                    (lo, true)
                } else if shifted > hi {
                    (hi, true)
                } else {
                    (shifted, false)
                };
                if ovf {
                    Self::set_hex_ovf(ctx);
                }
                Self::write_gpr(
                    ctx,
                    *dst,
                    (clamped as i64 as u64) & 0xffff_ffff,
                    OpWidth::W32,
                );
            }

            // `S2_asl_r_r_sat` / `S2_asr_r_r_sat` — register-amount saturating
            // shift implementing `fSAT_ORIG_SHL` (port of sem/shift.rs).
            OpKind::SatOrigShl {
                dst,
                src,
                amount,
                right,
                width,
            } => {
                let src_v = self.read_src_operand(ctx, src) as u32;
                // shamt = fSXTN(7,32, amount): sign-extend the low 7 bits to i32.
                let raw = self.read_src_operand(ctx, amount) as u32;
                let sh = ((raw as i32) << 25) >> 25;
                let orig_i = src_v as i32 as i64;

                // fSAT_ORIG_SHL(a, orig): saturate `a` to s32 honoring orig's
                // sign. NOTE: the sem's `ctx.sat_n(a, 32)` ALSO sets USR:OVF
                // whenever it clamps (a < INT_MIN or a > INT_MAX), independent of
                // the sign-flip / special cases below — so OVF is set on any
                // clamp, then again (idempotently) on a sign flip / orig>0&&a==0.
                #[inline]
                pub(crate) fn sat_orig_shl(ctx: &mut SmirContext, a: i64, orig: u32) -> u32 {
                    let orig_s = orig as i32;
                    // sat_n(a, 32): clamp to [INT_MIN, INT_MAX], setting OVF on clamp.
                    let sat = if a < i32::MIN as i64 {
                        SmirInterpreter::set_hex_ovf(ctx);
                        i32::MIN
                    } else if a > i32::MAX as i64 {
                        SmirInterpreter::set_hex_ovf(ctx);
                        i32::MAX
                    } else {
                        a as i32
                    };
                    if (sat ^ orig_s) < 0 {
                        // sign flipped -> saturate toward ORIG's extreme
                        let v = if orig_s < 0 { i32::MIN } else { i32::MAX };
                        SmirInterpreter::set_hex_ovf(ctx);
                        v as u32
                    } else if orig_s > 0 && a == 0 {
                        SmirInterpreter::set_hex_ovf(ctx);
                        i32::MAX as u32
                    } else {
                        sat as u32
                    }
                }

                let result: u32 = if !*right {
                    // asl_r_r_sat: positive count = left (saturating).
                    if sh < 0 {
                        // fBIDIR_ASHIFTL with negative amount -> arithmetic right.
                        (((orig_i >> ((-sh) - 1)) >> 1) as i64) as u32
                    } else {
                        let a = orig_i << sh;
                        sat_orig_shl(ctx, a, src_v)
                    }
                } else {
                    // asr_r_r_sat: negative count = left (saturating).
                    if sh < 0 {
                        let a = (orig_i << ((-sh) - 1)) << 1;
                        sat_orig_shl(ctx, a, src_v)
                    } else {
                        ((orig_i >> sh) as i64) as u32
                    }
                };
                Self::write_gpr(ctx, *dst, (result as u64) & width.mask(), *width);
            }

            // ==================================================================
            // BIT MANIPULATION
            // ==================================================================
            OpKind::Bt { src, index, width } => {
                let val = ctx.read_vreg(*src);
                let idx = self.read_src_operand(ctx, index) & (width.bits() as u64 - 1);

                ctx.flags.lazy = Some(LazyFlags {
                    op: LazyFlagOp::Bt,
                    result: 0,
                    left: val,
                    right: idx,
                    width: *width,
                    high: 0,
                });
            }

            OpKind::Bts {
                dst,
                src,
                index,
                width,
            } => {
                let val = ctx.read_vreg(*src);
                let idx = self.read_src_operand(ctx, index) & (width.bits() as u64 - 1);
                let result = val | (1u64 << idx);

                Self::write_gpr(ctx, *dst, result & width.mask(), *width);

                ctx.flags.lazy = Some(LazyFlags {
                    op: LazyFlagOp::Bt,
                    result: 0,
                    left: val,
                    right: idx,
                    width: *width,
                    high: 0,
                });
            }

            OpKind::Btr {
                dst,
                src,
                index,
                width,
            } => {
                let val = ctx.read_vreg(*src);
                let idx = self.read_src_operand(ctx, index) & (width.bits() as u64 - 1);
                let result = val & !(1u64 << idx);

                Self::write_gpr(ctx, *dst, result & width.mask(), *width);

                ctx.flags.lazy = Some(LazyFlags {
                    op: LazyFlagOp::Bt,
                    result: 0,
                    left: val,
                    right: idx,
                    width: *width,
                    high: 0,
                });
            }

            OpKind::Btc {
                dst,
                src,
                index,
                width,
            } => {
                let val = ctx.read_vreg(*src);
                let idx = self.read_src_operand(ctx, index) & (width.bits() as u64 - 1);
                let result = val ^ (1u64 << idx);

                Self::write_gpr(ctx, *dst, result & width.mask(), *width);

                ctx.flags.lazy = Some(LazyFlags {
                    op: LazyFlagOp::Bt,
                    result: 0,
                    left: val,
                    right: idx,
                    width: *width,
                    high: 0,
                });
            }

            OpKind::Bsf {
                dst,
                src,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let result = if val == 0 {
                    0 // ZF will be set
                } else {
                    val.trailing_zeros() as u64
                };

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    // BSF defines only ZF; retain the emulator's deterministic
                    // values for architecturally undefined status flags.
                    ctx.flags.materialize_all();
                    ctx.flags.materialized.zf = val == 0;
                }
            }

            OpKind::Bsr {
                dst,
                src,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let result = if val == 0 {
                    0 // ZF will be set
                } else {
                    (width.bits() - 1 - val.leading_zeros()) as u64
                };

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    // BSR has the same ZF-only architectural flag contract as
                    // BSF. Preserve every other materialized status flag.
                    ctx.flags.materialize_all();
                    ctx.flags.materialized.zf = val == 0;
                }
            }

            OpKind::Bextr {
                dst,
                src,
                control,
                width,
                flags,
            } => {
                let src = ctx.read_vreg(*src) & width.mask();
                let control = ctx.read_vreg(*control);
                let start = (control & 0xff) as u32;
                let len = ((control >> 8) & 0xff) as u32;
                let bits = width.bits();
                let result = if start >= bits || len == 0 {
                    0
                } else {
                    let shifted = src >> start;
                    if len >= bits {
                        shifted
                    } else {
                        shifted & ((1u64 << len) - 1)
                    }
                };
                let result = result & width.mask();
                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.set_lazy_bextr(result, *width);
                }
            }

            OpKind::Bzhi {
                dst,
                src,
                index,
                width,
                flags,
            } => {
                let src = ctx.read_vreg(*src) & width.mask();
                let index = (ctx.read_vreg(*index) & 0xff) as u32;
                let bits = width.bits();
                let result = if index >= bits {
                    src
                } else {
                    src & ((1u64 << index) - 1)
                };
                let result = result & width.mask();
                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.set_lazy_bzhi(u64::from(index), result, *width);
                }
            }

            OpKind::X86Bls {
                dst,
                src,
                width,
                kind,
                flags,
            } => {
                let src = ctx.read_vreg(*src) & width.mask();
                let result = match kind {
                    X86BlsKind::Blsr => src & src.wrapping_sub(1),
                    X86BlsKind::Blsmsk => src ^ src.wrapping_sub(1),
                    X86BlsKind::Blsi => src.wrapping_neg() & src,
                } & width.mask();
                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    match kind {
                        X86BlsKind::Blsr => ctx.flags.set_lazy_blsr(src, result, *width),
                        X86BlsKind::Blsmsk => ctx.flags.set_lazy_blsmsk(src, result, *width),
                        X86BlsKind::Blsi => ctx.flags.set_lazy_blsi(src, result, *width),
                    }
                }
            }

            OpKind::X86Adx {
                dst,
                src1,
                src2,
                width,
                kind,
                flags,
            } => {
                let left = ctx.read_vreg(*src1) & width.mask();
                let right = ctx.read_vreg(*src2) & width.mask();
                let carry_in = match kind {
                    X86AdxKind::Adcx => ctx.flags.get_cf(),
                    X86AdxKind::Adox => ctx.flags.get_of(),
                };
                let full = u128::from(left) + u128::from(right) + u128::from(carry_in);
                let result = (full as u64) & width.mask();
                let carry_out = full > u128::from(width.mask());
                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.materialize_all();
                    match kind {
                        X86AdxKind::Adcx => ctx.flags.materialized.cf = carry_out,
                        X86AdxKind::Adox => ctx.flags.materialized.of = carry_out,
                    }
                }
            }

            OpKind::Pdep {
                dst,
                src,
                mask,
                width,
            } => {
                let src = ctx.read_vreg(*src) & width.mask();
                let mask = ctx.read_vreg(*mask) & width.mask();
                let mut result = 0u64;
                let mut src_bit = 0u32;
                for bit in 0..width.bits() {
                    if ((mask >> bit) & 1) != 0 {
                        if ((src >> src_bit) & 1) != 0 {
                            result |= 1u64 << bit;
                        }
                        src_bit += 1;
                    }
                }
                Self::write_gpr(ctx, *dst, result & width.mask(), *width);
            }

            OpKind::Pext {
                dst,
                src,
                mask,
                width,
            } => {
                let src = ctx.read_vreg(*src) & width.mask();
                let mask = ctx.read_vreg(*mask) & width.mask();
                let mut result = 0u64;
                let mut dst_bit = 0u32;
                for bit in 0..width.bits() {
                    if ((mask >> bit) & 1) != 0 {
                        if ((src >> bit) & 1) != 0 {
                            result |= 1u64 << dst_bit;
                        }
                        dst_bit += 1;
                    }
                }
                Self::write_gpr(ctx, *dst, result & width.mask(), *width);
            }

            OpKind::Clz { dst, src, width } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let extra_bits = 64 - width.bits();
                let result = (val.leading_zeros() - extra_bits) as u64;
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::Ctz { dst, src, width } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let result = if val == 0 {
                    width.bits() as u64
                } else {
                    val.trailing_zeros() as u64
                };
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::Popcnt { dst, src, width } => {
                let val = ctx.read_vreg(*src) & width.mask();
                Self::write_gpr(ctx, *dst, val.count_ones() as u64, *width);
            }

            OpKind::X86Count {
                dst,
                src,
                width,
                kind,
                flags,
            } => {
                // Read before writing so architectural source/destination
                // aliasing remains exact for all three legacy forms.
                let val = ctx.read_vreg(*src) & width.mask();
                let result = match kind {
                    X86CountKind::Popcnt => val.count_ones() as u64,
                    X86CountKind::Tzcnt => {
                        if val == 0 {
                            width.bits() as u64
                        } else {
                            val.trailing_zeros() as u64
                        }
                    }
                    X86CountKind::Lzcnt => {
                        let extra_bits = 64 - width.bits();
                        (val.leading_zeros() - extra_bits) as u64
                    }
                };
                Self::write_gpr(ctx, *dst, result, *width);

                let requested = flags.as_set();
                if !requested.is_empty() {
                    ctx.flags.materialize_all();
                    ctx.flags.lazy = None;
                    match kind {
                        X86CountKind::Popcnt => {
                            if requested.contains(FlagSet::CF) {
                                ctx.flags.materialized.cf = false;
                            }
                            if requested.contains(FlagSet::ZF) {
                                ctx.flags.materialized.zf = val == 0;
                            }
                            if requested.contains(FlagSet::SF) {
                                ctx.flags.materialized.sf = false;
                            }
                            if requested.contains(FlagSet::OF) {
                                ctx.flags.materialized.of = false;
                            }
                            if requested.contains(FlagSet::PF) {
                                ctx.flags.materialized.pf = false;
                            }
                            if requested.contains(FlagSet::AF) {
                                ctx.flags.materialized.af = false;
                            }
                        }
                        X86CountKind::Tzcnt | X86CountKind::Lzcnt => {
                            if requested.contains(FlagSet::CF) {
                                ctx.flags.materialized.cf = val == 0;
                            }
                            if requested.contains(FlagSet::ZF) {
                                ctx.flags.materialized.zf = result == 0;
                            }
                        }
                    }
                }
            }

            OpKind::Bswap { dst, src, width } => {
                let val = ctx.read_vreg(*src);
                let result = match width {
                    OpWidth::W16 => (val as u16).swap_bytes() as u64,
                    OpWidth::W32 => (val as u32).swap_bytes() as u64,
                    OpWidth::W64 => val.swap_bytes(),
                    _ => val,
                };
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::Rbit { dst, src, width } => {
                let val = ctx.read_vreg(*src);
                let result = match width {
                    OpWidth::W32 => (val as u32).reverse_bits() as u64,
                    OpWidth::W64 => val.reverse_bits(),
                    _ => val,
                };
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::Bfx {
                dst,
                src,
                lsb,
                width_bits,
                sign_extend,
                op_width,
            } => {
                let val = ctx.read_vreg(*src);
                let mask = (1u64 << *width_bits) - 1;
                let extracted = (val >> *lsb) & mask;

                let result = if *sign_extend && (*width_bits > 0) {
                    let sign_bit = 1u64 << (*width_bits - 1);
                    if (extracted & sign_bit) != 0 {
                        extracted | !mask
                    } else {
                        extracted
                    }
                } else {
                    extracted
                };

                ctx.write_vreg(*dst, result & op_width.mask());
            }

            OpKind::Bfi {
                dst,
                dst_in,
                src,
                lsb,
                width_bits,
                op_width,
            } => {
                let dest_val = ctx.read_vreg(*dst_in);
                let src_val = ctx.read_vreg(*src);
                let mask = ((1u64 << *width_bits) - 1) << *lsb;
                let result = (dest_val & !mask) | ((src_val << *lsb) & mask);
                ctx.write_vreg(*dst, result & op_width.mask());
            }

            // ==================================================================
            // DATA MOVEMENT
            // ==================================================================
            OpKind::Mov { dst, src, width } => {
                let val = self.read_src_operand(ctx, src);
                Self::write_x86_partial(ctx, *dst, val, *width);
            }

            OpKind::CMove {
                dst,
                src,
                cond,
                width,
            } => {
                if ctx.flags.eval_condition(*cond) {
                    let val = ctx.read_vreg(*src) & width.mask();
                    Self::write_x86_partial(ctx, *dst, val, *width);
                }
            }

            OpKind::Select {
                dst,
                cond,
                src_true,
                src_false,
                width,
            } => {
                let cond_val = ctx.read_vreg(*cond);
                let result = if cond_val != 0 {
                    ctx.read_vreg(*src_true)
                } else {
                    ctx.read_vreg(*src_false)
                };
                Self::write_x86_partial(ctx, *dst, result, *width);
            }

            OpKind::ZeroExtend {
                dst,
                src,
                from_width,
                to_width,
            } => {
                let raw = ctx.read_vreg(*src);
                let val = if matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)) {
                    (raw >> 8) & from_width.mask()
                } else {
                    raw & from_width.mask()
                };
                Self::write_x86_partial(ctx, *dst, val, *to_width);
            }

            OpKind::SignExtend {
                dst,
                src,
                from_width,
                to_width,
            } => {
                let raw = ctx.read_vreg(*src);
                let val = if matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)) {
                    (raw >> 8) & from_width.mask()
                } else {
                    raw & from_width.mask()
                };
                let sign_bit = from_width.sign_bit();
                let extended = if (val & sign_bit) != 0 {
                    val | !from_width.mask()
                } else {
                    val
                };
                Self::write_x86_partial(ctx, *dst, extended, *to_width);
            }

            OpKind::Cwd { dst, src, width } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let sign_bit = width.sign_bit();
                let result = if (val & sign_bit) != 0 {
                    width.mask()
                } else {
                    0
                };
                Self::write_x86_partial(ctx, *dst, result, *width);
            }

            OpKind::Truncate {
                dst,
                src,
                from_width: _,
                to_width,
            } => {
                let val = ctx.read_vreg(*src);
                ctx.write_vreg(*dst, val & to_width.mask());
            }

            OpKind::Lea { dst, addr } => {
                let effective_addr = self.compute_address(ctx, addr);
                ctx.write_vreg(*dst, effective_addr);
            }

            OpKind::Xchg { reg1, reg2, width } => {
                let v1 = ctx.read_vreg(*reg1) & width.mask();
                let v2 = ctx.read_vreg(*reg2) & width.mask();
                Self::write_x86_partial(ctx, *reg1, v2, *width);
                Self::write_x86_partial(ctx, *reg2, v1, *width);
            }

            // ==================================================================
            // MEMORY OPERATIONS
            // ==================================================================
            OpKind::Load {
                dst,
                addr,
                width,
                sign,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                let val = self.load_memory(memory, effective_addr, *width, *sign)?;
                let op_width = width.to_op_width().unwrap_or(OpWidth::W64);
                if *sign == SignExtend::Zero {
                    Self::write_x86_partial(ctx, *dst, val, op_width);
                } else {
                    ctx.write_vreg(*dst, val);
                }
            }

            OpKind::Store { src, addr, width } => {
                let effective_addr = self.compute_address(ctx, addr);
                let val = ctx.read_vreg(*src);
                self.store_memory(memory, effective_addr, val, *width)?;
            }

            // Predicated load (Hexagon `if (Pu) Rd = memX(...)`). COMMITS only
            // when `cond`'s bit 0 is set: then `dst = load(EA)`. When the
            // predicate is FALSE the load CANCELS — `dst` is left UNCHANGED and
            // NO memory access is performed (so a false predicate never faults,
            // matching the sem's `return Ok(None)`).
            OpKind::PredLoad {
                dst,
                cond,
                addr,
                width,
                signed,
            } => {
                if (ctx.read_vreg(*cond) & 1) != 0 {
                    let effective_addr = self.compute_address(ctx, addr);
                    let val = self.load_memory(memory, effective_addr, *width, *signed)?;
                    let op_width = width.to_op_width().unwrap_or(OpWidth::W64);
                    if *signed == SignExtend::Zero {
                        Self::write_x86_partial(ctx, *dst, val, op_width);
                    } else {
                        ctx.write_vreg(*dst, val);
                    }
                }
            }

            // Predicated store (Hexagon `if (Pu) memX(...) = Rt`). COMMITS only
            // when `cond`'s bit 0 is set: then `store(EA, src)`. When the
            // predicate is FALSE the store CANCELS — NO memory access is
            // performed.
            OpKind::PredStore {
                src,
                cond,
                addr,
                width,
            } => {
                if (ctx.read_vreg(*cond) & 1) != 0 {
                    let effective_addr = self.compute_address(ctx, addr);
                    let val = self.read_src_operand(ctx, src);
                    self.store_memory(memory, effective_addr, val, *width)?;
                }
            }

            OpKind::RepStos {
                dst,
                src,
                count,
                width,
            } => {
                let mut addr = ctx.read_vreg(*dst);
                let mut remaining = ctx.read_vreg(*count);
                let val = ctx.read_vreg(*src);
                let stride = width.bytes() as u64;

                while remaining > 0 {
                    self.store_memory(memory, addr, val, *width)?;
                    addr = addr.wrapping_add(stride);
                    remaining -= 1;
                }

                ctx.write_vreg(*dst, addr);
                ctx.write_vreg(*count, remaining);
            }

            OpKind::RepMovs {
                dst,
                src,
                count,
                width,
            } => {
                let mut dst_addr = ctx.read_vreg(*dst);
                let mut src_addr = ctx.read_vreg(*src);
                let mut remaining = ctx.read_vreg(*count);
                let stride = width.bytes() as u64;
                let forward = !ctx.flags.materialized.df;

                while remaining > 0 {
                    let val = self.load_memory(memory, src_addr, *width, SignExtend::Zero)?;
                    self.store_memory(memory, dst_addr, val, *width)?;
                    if forward {
                        dst_addr = dst_addr.wrapping_add(stride);
                        src_addr = src_addr.wrapping_add(stride);
                    } else {
                        dst_addr = dst_addr.wrapping_sub(stride);
                        src_addr = src_addr.wrapping_sub(stride);
                    }
                    remaining -= 1;
                }

                ctx.write_vreg(*dst, dst_addr);
                ctx.write_vreg(*src, src_addr);
                ctx.write_vreg(*count, remaining);
            }

            OpKind::X86String {
                kind,
                rep,
                accumulator,
                src_index,
                dst_index,
                count,
                src_segment,
                width,
                address_width,
            } => {
                let addr_mask = match address_width {
                    OpWidth::W32 => u32::MAX as u64,
                    OpWidth::W64 => u64::MAX,
                    _ => {
                        return Err(MemoryError::OutOfBounds { addr: ctx.pc });
                    }
                };
                let op_width = width.to_op_width().unwrap_or(OpWidth::W64);
                let stride = width.bytes() as u64;
                ctx.flags.materialize_all();
                let forward = !ctx.flags.materialized.df;
                let segment_base = src_segment.map_or(0, |reg| ctx.read_vreg(reg));
                let repeated = *rep != crate::smir::ir::ops::X86RepMode::None;
                let mut remaining = if repeated {
                    ctx.read_vreg(*count) & addr_mask
                } else {
                    1
                };

                if repeated && remaining == 0 {
                    ctx.write_vreg(*count, 0);
                    if *address_width == OpWidth::W32 {
                        if *kind == crate::smir::ir::ops::X86StringKind::Movs {
                            ctx.write_vreg(*src_index, ctx.read_vreg(*src_index) & addr_mask);
                        }
                        if matches!(
                            kind,
                            crate::smir::ir::ops::X86StringKind::Movs
                                | crate::smir::ir::ops::X86StringKind::Stos
                        ) {
                            ctx.write_vreg(*dst_index, ctx.read_vreg(*dst_index) & addr_mask);
                        }
                    }
                }

                while remaining != 0 {
                    let src_off = ctx.read_vreg(*src_index) & addr_mask;
                    let dst_off = ctx.read_vreg(*dst_index) & addr_mask;
                    let src_addr = segment_base.wrapping_add(src_off);
                    let mut compared = false;

                    match kind {
                        crate::smir::ir::ops::X86StringKind::Movs => {
                            let value =
                                self.load_memory(memory, src_addr, *width, SignExtend::Zero)?;
                            self.store_memory(memory, dst_off, value, *width)?;
                        }
                        crate::smir::ir::ops::X86StringKind::Stos => {
                            self.store_memory(
                                memory,
                                dst_off,
                                ctx.read_vreg(*accumulator),
                                *width,
                            )?;
                        }
                        crate::smir::ir::ops::X86StringKind::Lods => {
                            let value =
                                self.load_memory(memory, src_addr, *width, SignExtend::Zero)?;
                            Self::write_gpr(ctx, *accumulator, value, op_width);
                        }
                        crate::smir::ir::ops::X86StringKind::Scas => {
                            let rhs =
                                self.load_memory(memory, dst_off, *width, SignExtend::Zero)?;
                            let lhs = ctx.read_vreg(*accumulator) & op_width.mask();
                            let result = lhs.wrapping_sub(rhs) & op_width.mask();
                            ctx.flags.set_lazy_sub(lhs, rhs, result, op_width);
                            compared = true;
                        }
                        crate::smir::ir::ops::X86StringKind::Cmps => {
                            let lhs =
                                self.load_memory(memory, src_addr, *width, SignExtend::Zero)?;
                            let rhs =
                                self.load_memory(memory, dst_off, *width, SignExtend::Zero)?;
                            let result = lhs.wrapping_sub(rhs) & op_width.mask();
                            ctx.flags.set_lazy_sub(lhs, rhs, result, op_width);
                            compared = true;
                        }
                    }

                    let advance = |value: u64| {
                        if forward {
                            value.wrapping_add(stride) & addr_mask
                        } else {
                            value.wrapping_sub(stride) & addr_mask
                        }
                    };
                    if matches!(
                        kind,
                        crate::smir::ir::ops::X86StringKind::Movs
                            | crate::smir::ir::ops::X86StringKind::Lods
                            | crate::smir::ir::ops::X86StringKind::Cmps
                    ) {
                        ctx.write_vreg(*src_index, advance(src_off));
                    }
                    if matches!(
                        kind,
                        crate::smir::ir::ops::X86StringKind::Movs
                            | crate::smir::ir::ops::X86StringKind::Stos
                            | crate::smir::ir::ops::X86StringKind::Scas
                            | crate::smir::ir::ops::X86StringKind::Cmps
                    ) {
                        ctx.write_vreg(*dst_index, advance(dst_off));
                    }

                    if repeated {
                        remaining = remaining.wrapping_sub(1) & addr_mask;
                        ctx.write_vreg(*count, remaining);
                    } else {
                        remaining = 0;
                    }

                    if compared && repeated {
                        let zf = ctx.flags.get_zf();
                        if (*rep == crate::smir::ir::ops::X86RepMode::Repe && !zf)
                            || (*rep == crate::smir::ir::ops::X86RepMode::Repne && zf)
                        {
                            break;
                        }
                    }
                }
            }

            OpKind::Leave => {
                let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
                let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
                let frame = ctx.read_vreg(rbp);
                let val = self.load_memory(memory, frame, MemWidth::B8, SignExtend::Zero)?;
                ctx.write_vreg(rsp, frame.wrapping_add(8));
                ctx.write_vreg(rbp, val);
            }

            OpKind::IoIn { dst, .. } => {
                ctx.write_vreg(*dst, 0);
            }

            OpKind::IoOut { .. } => {}

            OpKind::LoadPair {
                dst1,
                dst2,
                addr,
                width,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                let val1 = self.load_memory(memory, effective_addr, *width, SignExtend::Zero)?;
                let val2 = self.load_memory(
                    memory,
                    effective_addr + width.bytes() as u64,
                    *width,
                    SignExtend::Zero,
                )?;
                ctx.write_vreg(*dst1, val1);
                ctx.write_vreg(*dst2, val2);
            }

            OpKind::StorePair {
                src1,
                src2,
                addr,
                width,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                let val1 = ctx.read_vreg(*src1);
                let val2 = ctx.read_vreg(*src2);
                self.store_memory(memory, effective_addr, val1, *width)?;
                self.store_memory(memory, effective_addr + width.bytes() as u64, val2, *width)?;
            }

            OpKind::AtomicLoad {
                dst,
                addr,
                width,
                order,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                let val = memory.atomic_load(effective_addr, *width, *order)?;
                ctx.write_vreg(*dst, val);
            }

            OpKind::AtomicStore {
                src,
                addr,
                width,
                order,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                let val = ctx.read_vreg(*src);
                memory.atomic_store(effective_addr, val, *width, *order)?;
            }

            OpKind::AtomicRmw {
                dst,
                addr,
                src,
                op,
                width,
                order,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                let operand = ctx.read_vreg(*src);
                let old = memory.atomic_rmw(effective_addr, *op, operand, *width, *order)?;
                ctx.write_vreg(*dst, old);
            }

            OpKind::Cas {
                dst,
                success,
                addr,
                expected,
                new_val,
                width,
                order,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                let exp = ctx.read_vreg(*expected);
                let new = ctx.read_vreg(*new_val);
                memory.probe(effective_addr, width.bytes() as usize, true)?;
                let (old, succ) = memory.compare_and_swap(
                    effective_addr,
                    exp,
                    new,
                    *width,
                    *order,
                    order.cas_failure(),
                )?;
                ctx.write_vreg(*dst, old);
                ctx.write_vreg(*success, if succ { 1 } else { 0 });
            }

            OpKind::CasPair {
                dst_lo,
                dst_hi,
                success,
                addr,
                expected_lo,
                expected_hi,
                new_lo,
                new_hi,
                order,
                failure_order,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                if effective_addr & 0xf != 0 {
                    return Err(MemoryError::Alignment {
                        addr: effective_addr,
                        required: 16,
                    });
                }
                memory.probe(effective_addr, 16, true)?;
                let expected = [ctx.read_vreg(*expected_lo), ctx.read_vreg(*expected_hi)];
                let new = [ctx.read_vreg(*new_lo), ctx.read_vreg(*new_hi)];
                let (old, succ) = memory.compare_and_swap_pair(
                    effective_addr,
                    expected,
                    new,
                    *order,
                    *failure_order,
                )?;
                ctx.write_vreg(*dst_lo, old[0]);
                ctx.write_vreg(*dst_hi, old[1]);
                ctx.write_vreg(*success, u64::from(succ));
            }

            OpKind::AtomicCmpXadd {
                dst_old,
                addr,
                cmp,
                add,
                cond,
                width,
                order,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                let op_width = width.to_op_width().unwrap_or(OpWidth::W64);
                let mask = op_width.mask();
                let cmp_val = ctx.read_vreg(*cmp) & mask;
                let add_val = ctx.read_vreg(*add) & mask;
                let mut old = memory.atomic_load(effective_addr, *width, MemoryOrder::Acquire)?;

                loop {
                    let old_m = old & mask;
                    let cmp_result = old_m.wrapping_sub(cmp_val) & mask;
                    ctx.flags.set_lazy_sub(old_m, cmp_val, cmp_result, op_width);
                    let should_add = ctx.flags.eval_condition(*cond);
                    let new_val = if should_add {
                        old_m.wrapping_add(add_val) & mask
                    } else {
                        old_m
                    };

                    let (seen, succ) = memory.compare_and_swap(
                        effective_addr,
                        old_m,
                        new_val,
                        *width,
                        *order,
                        MemoryOrder::Relaxed,
                    )?;
                    if succ {
                        Self::write_gpr(ctx, *dst_old, old_m, op_width);
                        break;
                    }
                    old = seen;
                }
            }

            OpKind::LoadExclusive { dst, addr, width } => {
                let effective_addr = self.compute_address(ctx, addr);
                let val = memory.load_exclusive(effective_addr, *width)?;
                ctx.exclusive_monitor
                    .mark_exclusive(effective_addr, *width, val);
                ctx.write_vreg(*dst, val);
            }

            OpKind::StoreExclusive {
                status,
                src,
                addr,
                width,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                let val = ctx.read_vreg(*src);
                let success = memory.store_exclusive(effective_addr, val, *width)?;
                ctx.write_vreg(*status, if success { 0 } else { 1 });
                ctx.exclusive_monitor.clear();
            }

            OpKind::ClearExclusive => {
                ctx.exclusive_monitor.clear();
                memory.clear_exclusive();
            }

            OpKind::Prefetch { addr, write } => {
                let effective_addr = self.compute_address(ctx, addr);
                memory.prefetch(effective_addr, *write);
            }

            OpKind::X86CacheControl { addr, kind } => {
                let effective_addr = self.compute_address(ctx, addr);
                // Intel specifies no memory-address exceptions for CLDEMOTE;
                // it is permitted to be ignored even when the line is absent.
                // Flush/writeback operations perform an architectural address
                // access and therefore retain the explicit probe.
                if *kind != X86CacheControlKind::Cldemote {
                    memory.probe(effective_addr, 1, false)?;
                }
                memory.prefetch(effective_addr, matches!(kind, X86CacheControlKind::Clwb));
            }

            OpKind::Fence { kind } => {
                memory.fence(*kind);
            }

            // ==================================================================
            // FLOATING POINT
            // ==================================================================
            OpKind::FAdd {
                dst,
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                self.write_fp(ctx, *dst, a + b, *precision);
            }

            OpKind::FSub {
                dst,
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                self.write_fp(ctx, *dst, a - b, *precision);
            }

            OpKind::FMul {
                dst,
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                self.write_fp(ctx, *dst, a * b, *precision);
            }

            OpKind::FDiv {
                dst,
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                self.write_fp(ctx, *dst, a / b, *precision);
            }

            OpKind::FFma {
                dst,
                src1,
                src2,
                src3,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                let c = self.read_fp(ctx, *src3, *precision);
                self.write_fp(ctx, *dst, a.mul_add(b, c), *precision);
            }

            OpKind::FAbs {
                dst,
                src,
                precision,
            } => {
                let a = self.read_fp(ctx, *src, *precision);
                self.write_fp(ctx, *dst, a.abs(), *precision);
            }

            OpKind::FNeg {
                dst,
                src,
                precision,
            } => {
                let a = self.read_fp(ctx, *src, *precision);
                self.write_fp(ctx, *dst, -a, *precision);
            }

            OpKind::FSqrt {
                dst,
                src,
                precision,
            } => {
                let a = self.read_fp(ctx, *src, *precision);
                self.write_fp(ctx, *dst, a.sqrt(), *precision);
            }

            OpKind::FMin {
                dst,
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                self.write_fp(ctx, *dst, a.min(b), *precision);
            }

            OpKind::FMax {
                dst,
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                self.write_fp(ctx, *dst, a.max(b), *precision);
            }

            OpKind::FCmp {
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                // Set flags based on comparison
                let result = if a < b {
                    u64::MAX
                } else if a > b {
                    1
                } else {
                    0
                };
                ctx.flags.set_lazy_sub(
                    if a >= b { 1 } else { 0 },
                    if a <= b { 1 } else { 0 },
                    result,
                    OpWidth::W64,
                );
            }

            OpKind::X86FpCompare {
                src1, src2, elem, ..
            } => {
                let elem_bits = elem.bytes() * 8;
                let a_bits = Self::get_lane(&Self::read_vec(ctx, *src1), 0, elem_bits);
                let b_bits = Self::get_lane(&Self::read_vec(ctx, *src2), 0, elem_bits);
                let ordering = match elem {
                    VecElementType::F16 => Self::x86_fp16_to_f32(a_bits as u16)
                        .partial_cmp(&Self::x86_fp16_to_f32(b_bits as u16)),
                    VecElementType::F32 => {
                        f32::from_bits(a_bits as u32).partial_cmp(&f32::from_bits(b_bits as u32))
                    }
                    VecElementType::F64 => {
                        f64::from_bits(a_bits).partial_cmp(&f64::from_bits(b_bits))
                    }
                    _ => None,
                };
                ctx.flags.materialize_all();
                ctx.flags.lazy = None;
                ctx.flags.materialized.of = false;
                ctx.flags.materialized.sf = false;
                ctx.flags.materialized.af = false;
                match ordering {
                    None => {
                        ctx.flags.materialized.zf = true;
                        ctx.flags.materialized.pf = true;
                        ctx.flags.materialized.cf = true;
                    }
                    Some(std::cmp::Ordering::Less) => {
                        ctx.flags.materialized.zf = false;
                        ctx.flags.materialized.pf = false;
                        ctx.flags.materialized.cf = true;
                    }
                    Some(std::cmp::Ordering::Equal) => {
                        ctx.flags.materialized.zf = true;
                        ctx.flags.materialized.pf = false;
                        ctx.flags.materialized.cf = false;
                    }
                    Some(std::cmp::Ordering::Greater) => {
                        ctx.flags.materialized.zf = false;
                        ctx.flags.materialized.pf = false;
                        ctx.flags.materialized.cf = false;
                    }
                }
            }

            OpKind::X86VectorFpCompare {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                predicate,
                scalar,
                mask_destination,
                zero_upper,
                suppress_exceptions,
            } => {
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let format = match elem {
                    VecElementType::F16 => X86_SIMD_F16,
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let signaling = matches!(
                    *predicate & 0x1F,
                    1 | 2 | 5 | 6 | 9 | 10 | 13 | 14 | 16 | 19 | 20 | 23 | 24 | 27 | 28 | 31
                );
                let mut status = 0u32;
                let mut mask_result = 0u64;
                let mut vector_result = if *scalar {
                    first
                } else {
                    Self::read_vec(ctx, *dst)
                };
                if *zero_upper && !*mask_destination {
                    vector_result[(width.bytes() / 8) as usize..].fill(0);
                }
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        continue;
                    }
                    let first_raw = Self::get_lane(&first, lane, elem.bytes() * 8);
                    let second_raw = Self::get_lane(&second, lane, elem.bytes() * 8);
                    let first_value = Self::x86_simd_fp_apply_daz(first_raw, format, mxcsr);
                    let second_value = Self::x86_simd_fp_apply_daz(second_raw, format, mxcsr);
                    status |= first_value.status | second_value.status;
                    let first_nan = Self::x86_simd_fp_is_nan(first_value.bits, format);
                    let second_nan = Self::x86_simd_fp_is_nan(second_value.bits, format);
                    if Self::x86_simd_fp_is_snan(first_value.bits, format)
                        || Self::x86_simd_fp_is_snan(second_value.bits, format)
                        || (signaling && (first_nan || second_nan))
                    {
                        status |= 1;
                    }
                    let relation = if first_nan || second_nan {
                        3
                    } else {
                        let ordering = match elem {
                            VecElementType::F16 => Self::x86_fp16_to_f32(first_value.bits as u16)
                                .partial_cmp(&Self::x86_fp16_to_f32(second_value.bits as u16)),
                            VecElementType::F32 => f32::from_bits(first_value.bits as u32)
                                .partial_cmp(&f32::from_bits(second_value.bits as u32)),
                            VecElementType::F64 => f64::from_bits(first_value.bits)
                                .partial_cmp(&f64::from_bits(second_value.bits)),
                            _ => unreachable!(),
                        };
                        match ordering {
                            Some(std::cmp::Ordering::Greater) => 0,
                            Some(std::cmp::Ordering::Less) => 1,
                            Some(std::cmp::Ordering::Equal) => 2,
                            None => 3,
                        }
                    };
                    const TRUTH_TABLES: [u8; 16] = [
                        0b0100, 0b0010, 0b0110, 0b1000, 0b1011, 0b1101, 0b1001, 0b0111, 0b1100,
                        0b1010, 0b1110, 0b0000, 0b0011, 0b0101, 0b0001, 0b1111,
                    ];
                    let is_true =
                        TRUTH_TABLES[usize::from(*predicate & 0x0F)] & (1u8 << relation) != 0;
                    if *mask_destination {
                        if is_true {
                            mask_result |= 1u64 << lane;
                        }
                    } else {
                        Self::set_lane(
                            &mut vector_result,
                            lane,
                            elem.bytes() * 8,
                            if is_true {
                                if *elem == VecElementType::F32 {
                                    u64::from(u32::MAX)
                                } else {
                                    u64::MAX
                                }
                            } else {
                                0
                            },
                        );
                    }
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                if *mask_destination {
                    ctx.write_vreg(*dst, mask_result);
                } else {
                    Self::write_vec(ctx, *dst, vector_result);
                }
            }

            OpKind::X86GetExponent {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F16 => X86_SIMD_F16,
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, elem_bits);
                    let converted = Self::x86_simd_get_exponent(source_bits, format, mxcsr);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86GetMantissa {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F16 => X86_SIMD_F16,
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, elem_bits);
                    let converted = Self::x86_simd_get_mantissa(source_bits, format, mxcsr, *imm);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86RoundScale {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F16 => X86_SIMD_F16,
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, elem_bits);
                    let converted = Self::x86_simd_round_scale(source_bits, format, mxcsr, *imm);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Reduce {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F16 => X86_SIMD_F16,
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, elem_bits);
                    let converted = Self::x86_simd_reduce(source_bits, format, mxcsr, *imm);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Range {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *imm > 0x0F
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if *scalar { first } else { old };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let converted = Self::x86_simd_range(
                        Self::get_lane(&first, lane, elem_bits),
                        Self::get_lane(&second, lane, elem_bits),
                        format,
                        mxcsr,
                        *imm,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86FixupImm {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let first = Self::read_vec(ctx, *src1);
                let table = Self::read_vec(ctx, *src2);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if *scalar { first } else { old };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let converted = Self::x86_simd_fixup_imm(
                        Self::get_lane(&old, lane, elem_bits),
                        Self::get_lane(&first, lane, elem_bits),
                        Self::get_lane(&table, lane, elem_bits),
                        format,
                        mxcsr,
                        *imm,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                // VFIXUPIMM treats MXCSR exception masks as set: reporting can
                // update the IE/ZE sticky flags but never raises #XM.
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Exp2 {
                dst,
                src,
                mask,
                elem,
                width,
                lanes,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *width != VecWidth::V512
                    || *lanes != width.lanes(*elem) as u8
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = old;
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        }
                        continue;
                    }
                    let converted =
                        Self::x86_simd_exp2(Self::get_lane(&source, lane, elem_bits), format);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Recip14 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
            }
            | OpKind::X86Rsqrt14 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
            } => {
                let rsqrt = matches!(op.kind, OpKind::X86Rsqrt14 { .. });
                let format = match elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar
                        && (!matches!(width, VecWidth::V128 | VecWidth::V256 | VecWidth::V512)
                            || *lanes != width.lanes(*elem) as u8))
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let bits = Self::get_lane(&source, lane, elem_bits);
                    let converted = if rsqrt {
                        Self::x86_simd_rsqrt14(bits, format, mxcsr)
                    } else {
                        Self::x86_simd_recip14(bits, format, mxcsr)
                    };
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86RecipFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing,
            }
            | OpKind::X86RsqrtFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing,
            } => {
                let rsqrt = matches!(op.kind, OpKind::X86RsqrtFp16 { .. });
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar
                        && (!matches!(width, VecWidth::V128 | VecWidth::V256 | VecWidth::V512)
                            || *lanes != width.lanes(VecElementType::F16) as u8))
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        let inactive = if *mask_zeroing {
                            0
                        } else {
                            Self::get_lane(&old, lane, 16)
                        };
                        Self::set_lane(&mut result, lane, 16, inactive);
                        continue;
                    }
                    let bits = Self::get_lane(&source, lane, 16) as u16;
                    Self::set_lane(
                        &mut result,
                        lane,
                        16,
                        u64::from(Self::x86_fp16_approx(bits, rsqrt)),
                    );
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Recip28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar
                        && (*width != VecWidth::V512 || *lanes != width.lanes(*elem) as u8))
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let converted =
                        Self::x86_simd_recip28(Self::get_lane(&source, lane, elem_bits), format);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Rsqrt28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar
                        && (*width != VecWidth::V512 || *lanes != width.lanes(*elem) as u8))
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let converted =
                        Self::x86_simd_rsqrt28(Self::get_lane(&source, lane, elem_bits), format);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86ScaleF {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                round,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F16 => X86_SIMD_F16,
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                    || (*mask_zeroing && mask.is_none())
                    || (*suppress_exceptions != (*round != FpRoundMode::Dynamic))
                    || matches!(round, FpRoundMode::RoundNearestTiesAway)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if *scalar { first } else { old };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let first_bits = Self::get_lane(&first, lane, elem_bits);
                    let second_bits = Self::get_lane(&second, lane, elem_bits);
                    let converted = Self::x86_simd_scale_f(
                        first_bits,
                        second_bits,
                        format,
                        mode,
                        mxcsr,
                        *scalar,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86CheckAlignment { addr, alignment } => {
                debug_assert!(alignment.is_power_of_two());
                let effective_addr = self.compute_address(ctx, addr);
                if effective_addr & (u64::from(*alignment) - 1) != 0 {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                }
            }

            OpKind::FConvert { dst, src, from, to } => {
                let a = self.read_fp(ctx, *src, *from);
                self.write_fp(ctx, *dst, a, *to);
            }

            OpKind::HexFp {
                dst,
                src1,
                src2,
                op,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = ctx.read_vreg(*src2);
                let r = hex_fp_eval(*op, a, b);
                ctx.write_vreg(*dst, r);
            }

            OpKind::HexFp3 {
                dst,
                src1,
                src2,
                src3,
                negate_product,
                lib,
            } => {
                let a = ctx.read_vreg(*src1) as u32;
                let b = ctx.read_vreg(*src2) as u32;
                let c = ctx.read_vreg(*src3) as u32;
                let r = if *lib {
                    // `:lib` form: exact-core fma + Hexagon post-fixups.
                    hex_sf_fma_lib(a, b, c, *negate_product)
                } else {
                    hex_sf_fma(a, b, c, *negate_product)
                };
                ctx.write_vreg(*dst, r as u64);
            }

            OpKind::HexFpRecip {
                dst,
                pred,
                src1,
                src2,
                kind,
            } => {
                let rs = ctx.read_vreg(*src1) as u32;
                let rt = ctx.read_vreg(*src2) as u32;
                let (rd, pe) = hex_fp_recip_eval(*kind, rs, rt);
                ctx.write_vreg(*dst, rd as u64);
                if let Some(p) = pred {
                    // The seed ops (sfrecipa/sfinvsqrta) write the FULL Hexagon
                    // predicate byte Pe; the harness compares the whole byte.
                    ctx.write_vreg(*p, pe as u64);
                }
            }

            OpKind::HexFpDf {
                dst,
                src1,
                src2,
                src3,
                op,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = ctx.read_vreg(*src2);
                let r = match op {
                    crate::smir::ir::ops::HexDfOp::DfMpyHh => {
                        let acc = ctx.read_vreg(*src3);
                        hr_df_mpyhh(a, b, acc)
                    }
                    crate::smir::ir::ops::HexDfOp::DfMpyFix => hr_df_mpyfix(a, b),
                };
                ctx.write_vreg(*dst, r);
            }

            OpKind::HexFpScFma {
                dst,
                src1,
                src2,
                src3,
                scale,
            } => {
                let rs = ctx.read_vreg(*src1) as u32;
                let rt = ctx.read_vreg(*src2) as u32;
                let rx = ctx.read_vreg(*src3) as u32;
                let pu = ctx.read_vreg(*scale) as u8;
                let r = hex_sf_fma_scale(rs, rt, rx, pu);
                ctx.write_vreg(*dst, r as u64);
            }

            OpKind::HexCabacDecBin {
                dst,
                pred,
                src1,
                src2,
            } => {
                let rss = ctx.read_vreg(*src1);
                let rtt = ctx.read_vreg(*src2);
                let (rdd, p0) = hex_cabac_decbin(rss, rtt);
                ctx.write_vreg(*dst, rdd);
                ctx.write_vreg(*pred, p0 as u64);
            }

            OpKind::HexTlbMatch { dst, src1, src2 } => {
                let rss = ctx.read_vreg(*src1);
                let rt = ctx.read_vreg(*src2) as u32;
                let p = hex_tlbmatch(rss, rt);
                ctx.write_vreg(*dst, p as u64);
            }

            OpKind::RvFp {
                dst,
                fcsr_dst,
                src1,
                src2,
                src3,
                fcsr_src,
                op: fp_op,
                rm_field,
                xlen,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = ctx.read_vreg(*src2);
                let c = ctx.read_vreg(*src3);
                let fcsr = ctx.read_vreg(*fcsr_src) as u32;
                let malformed = !matches!(*xlen, 32 | 64)
                    || *rm_field > 7
                    || (*xlen == 32 && crate::isa::riscv::float::fp_requires_rv64(*fp_op));
                // Bit-exact against the qemu-verified RISC-V interpreter.
                let evaluated = if malformed {
                    None
                } else {
                    crate::isa::riscv::float::eval_scalar_fp(*fp_op, *rm_field, fcsr, a, b, c)
                };
                match evaluated {
                    Some((res, new_fcsr)) => {
                        let res =
                            if *xlen == 32 && crate::isa::riscv::float::fp_writes_int_dst(*fp_op) {
                                res & 0xffff_ffff
                            } else {
                                res
                            };
                        ctx.write_vreg(*dst, res);
                        ctx.write_vreg(*fcsr_dst, new_fcsr as u64);
                    }
                    None => {
                        // Illegal rounding-mode fields trap with no architectural
                        // state change.
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                    }
                }
            }

            OpKind::RvIntCrypto {
                dst,
                src1,
                src2,
                op,
                imm,
                xlen,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = ctx.read_vreg(*src2);
                if let Some(res) =
                    crate::isa::riscv::crypto::eval_int_crypto(*op, a, b, *imm, *xlen as u32)
                {
                    ctx.write_vreg(*dst, res);
                }
            }

            OpKind::RvVector {
                insn, xlen, state, ..
            } => {
                exec_rv_vector(ctx, memory, *insn, *xlen, op.guest_pc, state);
            }

            OpKind::IntToFp {
                dst,
                src,
                int_width,
                fp_precision,
                signed,
            } => {
                let val = ctx.read_vreg(*src) & int_width.mask();
                let f = if *signed {
                    self.sign_extend(val, *int_width) as i64 as f64
                } else {
                    val as f64
                };
                self.write_fp(ctx, *dst, f, *fp_precision);
            }

            OpKind::FpToInt {
                dst,
                src,
                fp_precision,
                int_width,
                signed,
                round,
            } => {
                let f = self.read_fp(ctx, *src, *fp_precision);
                let rounded = self.round_fp_value(ctx, f, *round);
                let val = if *signed {
                    (rounded as i64) as u64
                } else {
                    rounded as u64
                };
                ctx.write_vreg(*dst, val & int_width.mask());
            }

            OpKind::X86FpToInt {
                dst,
                src,
                elem,
                int_width,
                signed,
                truncate,
                round,
                ..
            } => {
                let bits = if matches!(src, VReg::Virtual(_)) {
                    ctx.read_vreg(*src)
                } else {
                    Self::get_lane(&Self::read_vec(ctx, *src), 0, elem.bytes() * 8)
                };
                let value = match elem {
                    VecElementType::F16 => Self::x86_fp16_to_f32(bits as u16) as f64,
                    VecElementType::F32 => f32::from_bits(bits as u32) as f64,
                    VecElementType::F64 => f64::from_bits(bits),
                    _ => f64::NAN,
                };
                let rounded = if *truncate {
                    value.trunc()
                } else {
                    self.round_fp_value(ctx, value, *round)
                };
                let indefinite = if *signed {
                    match int_width {
                        OpWidth::W32 => 0x8000_0000,
                        OpWidth::W64 => 0x8000_0000_0000_0000,
                        _ => 0,
                    }
                } else {
                    int_width.mask()
                };
                let valid = if *signed {
                    match int_width {
                        OpWidth::W32 => {
                            rounded.is_finite()
                                && rounded >= i32::MIN as f64
                                && rounded <= i32::MAX as f64
                        }
                        OpWidth::W64 => {
                            rounded.is_finite()
                                && rounded >= -9_223_372_036_854_775_808.0
                                && rounded < 9_223_372_036_854_775_808.0
                        }
                        _ => false,
                    }
                } else {
                    match int_width {
                        OpWidth::W32 => {
                            rounded.is_finite() && rounded >= 0.0 && rounded <= 4_294_967_295.0
                        }
                        OpWidth::W64 => {
                            rounded.is_finite()
                                && rounded >= 0.0
                                && rounded < 18_446_744_073_709_551_616.0
                        }
                        _ => false,
                    }
                };
                let result = if valid {
                    let converted = if *signed {
                        rounded as i64 as u64
                    } else {
                        rounded as u64
                    };
                    converted & int_width.mask()
                } else {
                    indefinite
                };
                Self::write_gpr(ctx, *dst, result, *int_width);
            }

            OpKind::X86IntToFp {
                dst,
                merge,
                src,
                elem,
                int_width,
                signed,
                round,
                zero_upper,
                ..
            } => {
                let raw = ctx.read_vreg(*src) & int_width.mask();
                let value = if *signed {
                    self.sign_extend(raw, *int_width) as i64 as i128
                } else {
                    raw as i128
                };
                let scalar_bits = self.x86_int_to_fp_bits(ctx, value, *elem, *round);
                let mut result = Self::read_vec(ctx, *merge);
                Self::set_lane(&mut result, 0, elem.bytes() * 8, scalar_bits);
                if *zero_upper {
                    result[2..].fill(0);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86FpConvert {
                dst,
                merge,
                src,
                mask,
                from,
                to,
                mask_zeroing,
                round,
                suppress_exceptions,
                zero_upper,
            } => {
                let mut result = Self::read_vec(ctx, *merge);
                if *zero_upper {
                    result[2..].fill(0);
                }
                let active = mask.map_or(true, |reg| ctx.read_vreg(reg) & 1 != 0);
                if !active {
                    let scalar_bits = if *mask_zeroing {
                        0
                    } else {
                        Self::get_lane(&Self::read_vec(ctx, *dst), 0, to.bytes() * 8)
                    };
                    Self::set_lane(&mut result, 0, to.bytes() * 8, scalar_bits);
                    Self::write_vec(ctx, *dst, result);
                    return Ok(());
                }

                let source = if matches!(src, VReg::Virtual(_)) {
                    ctx.read_vreg(*src)
                } else {
                    Self::get_lane(&Self::read_vec(ctx, *src), 0, from.bytes() * 8)
                };
                let (from_format, to_format) = match (*from, *to) {
                    (VecElementType::F16, VecElementType::F32) => (X86_SIMD_F16, X86_SIMD_F32),
                    (VecElementType::F16, VecElementType::F64) => (X86_SIMD_F16, X86_SIMD_F64),
                    (VecElementType::F32, VecElementType::F16) => (X86_SIMD_F32, X86_SIMD_F16),
                    (VecElementType::F32, VecElementType::F64) => (X86_SIMD_F32, X86_SIMD_F64),
                    (VecElementType::F64, VecElementType::F16) => (X86_SIMD_F64, X86_SIMD_F16),
                    (VecElementType::F64, VecElementType::F32) => (X86_SIMD_F64, X86_SIMD_F32),
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                let converted = Self::x86_simd_fp_convert_precision(
                    source,
                    from_format,
                    to_format,
                    mode,
                    mxcsr,
                    true,
                );
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= converted.status;
                    }
                    if Self::x86_simd_fp_unmasked(converted.status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::set_lane(&mut result, 0, to.bytes() * 8, converted.bits);
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedFpConvert {
                dst,
                src,
                mask,
                from,
                to,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
                report_fp16_denormal,
            } => {
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = old;
                if *zero_upper {
                    result[(dst_width.bytes() / 8) as usize..].fill(0);
                }
                result[..(dst_width.bytes() / 8) as usize].fill(0);
                let mask_bits = mask.map(|reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                let (from_format, to_format) = match (*from, *to) {
                    (VecElementType::F16, VecElementType::F32) => (X86_SIMD_F16, X86_SIMD_F32),
                    (VecElementType::F16, VecElementType::F64) => (X86_SIMD_F16, X86_SIMD_F64),
                    (VecElementType::F32, VecElementType::F16) => (X86_SIMD_F32, X86_SIMD_F16),
                    (VecElementType::F32, VecElementType::F64) => (X86_SIMD_F32, X86_SIMD_F64),
                    (VecElementType::F64, VecElementType::F16) => (X86_SIMD_F64, X86_SIMD_F16),
                    (VecElementType::F64, VecElementType::F32) => (X86_SIMD_F64, X86_SIMD_F32),
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let mut status = 0;
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        if !*mask_zeroing {
                            let preserved = Self::get_lane(&old, lane, to.bytes() * 8);
                            Self::set_lane(&mut result, lane, to.bytes() * 8, preserved);
                        }
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, from.bytes() * 8);
                    let converted = Self::x86_simd_fp_convert_precision(
                        source_bits,
                        from_format,
                        to_format,
                        mode,
                        if *to == VecElementType::F16 {
                            // VCVTPD2PH/VCVTPS2PHX produce FP16 denormals
                            // regardless of MXCSR.FTZ.
                            mxcsr & !(1 << 15)
                        } else {
                            mxcsr
                        },
                        *report_fp16_denormal,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, to.bytes() * 8, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedIntToFp {
                dst,
                src,
                mask,
                int_elem,
                fp_elem,
                signed,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
                ..
            } => {
                let format = match fp_elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if !matches!(int_elem, VecElementType::I32 | VecElementType::I64) {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = old;
                if *zero_upper {
                    result[(dst_width.bytes() / 8) as usize..].fill(0);
                }
                result[..(dst_width.bytes() / 8) as usize].fill(0);
                let mask_bits = mask.map(|reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                if mode == FpRoundMode::RoundNearestTiesAway {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let int_bits = int_elem.bytes() * 8;
                let int_mask = if int_bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << int_bits) - 1
                };
                let fp_bits = fp_elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        if !*mask_zeroing {
                            let preserved = Self::get_lane(&old, lane, fp_bits);
                            Self::set_lane(&mut result, lane, fp_bits, preserved);
                        }
                        continue;
                    }
                    let raw = Self::get_lane(&source, lane, int_bits) & int_mask;
                    let value = if *signed {
                        let shift = 128 - int_bits;
                        (i128::from(raw) << shift) >> shift
                    } else {
                        i128::from(raw)
                    };
                    let negative = value < 0;
                    let magnitude = if negative {
                        (-value) as u128
                    } else {
                        value as u128
                    };
                    let converted = Self::x86_simd_fp_round_exact(
                        negative, magnitude, 0, false, format, mode, mxcsr,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, fp_bits, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedFpToInt {
                dst,
                src,
                mask,
                fp_elem,
                int_elem,
                signed,
                truncate,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
                ..
            } => {
                let format = match fp_elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if !matches!(int_elem, VecElementType::I32 | VecElementType::I64) {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = old;
                if *zero_upper {
                    result[(dst_width.bytes() / 8) as usize..].fill(0);
                }
                result[..(dst_width.bytes() / 8) as usize].fill(0);
                let mask_bits = mask.map(|reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                if mode == FpRoundMode::RoundNearestTiesAway
                    || (*truncate && mode != FpRoundMode::RoundTowardZero)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let fp_bits = fp_elem.bytes() * 8;
                let int_bits = int_elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        if !*mask_zeroing {
                            let preserved = Self::get_lane(&old, lane, int_bits);
                            Self::set_lane(&mut result, lane, int_bits, preserved);
                        }
                        continue;
                    }
                    let mut source_bits = Self::get_lane(&source, lane, fp_bits);
                    // Intel specifies only Invalid and Precision for these
                    // conversions. DAZ still substitutes a signed zero, but a
                    // preserved denormal does not independently raise DE.
                    if mxcsr & (1 << 6) != 0 && Self::x86_simd_fp_is_denormal(source_bits, format) {
                        let (sign, _, _, _) = Self::x86_simd_fp_masks(format);
                        source_bits &= sign;
                    }
                    let converted =
                        Self::x86_simd_fp_to_int(source_bits, format, int_bits, *signed, mode);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, int_bits, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedIntToFp16 {
                dst,
                src,
                mask,
                int_elem,
                signed,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
                ..
            } => {
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = old;
                if *zero_upper {
                    result[(dst_width.bytes() / 8) as usize..].fill(0);
                }
                result[..(dst_width.bytes() / 8) as usize].fill(0);
                let mask_bits = mask.map(|reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                if mode == FpRoundMode::RoundNearestTiesAway {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let elem_bits = int_elem.bytes() * 8;
                let elem_mask = if elem_bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << elem_bits) - 1
                };
                let mut status = 0;
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        if !*mask_zeroing {
                            Self::set_lane(&mut result, lane, 16, Self::get_lane(&old, lane, 16));
                        }
                        continue;
                    }
                    let raw = Self::get_lane(&source, lane, elem_bits) & elem_mask;
                    let value = if *signed {
                        let shift = 128 - elem_bits;
                        (i128::from(raw) << shift) >> shift
                    } else {
                        i128::from(raw)
                    };
                    let negative = value < 0;
                    let magnitude = if negative {
                        (-value) as u128
                    } else {
                        value as u128
                    };
                    let converted = Self::x86_simd_fp_round_exact(
                        negative,
                        magnitude,
                        0,
                        false,
                        X86_SIMD_F16,
                        mode,
                        mxcsr,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, 16, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedFp16ToInt {
                dst,
                src,
                mask,
                int_elem,
                signed,
                truncate,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
                ..
            } => {
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = old;
                if *zero_upper {
                    result[(dst_width.bytes() / 8) as usize..].fill(0);
                }
                result[..(dst_width.bytes() / 8) as usize].fill(0);
                let mask_bits = mask.map(|reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                if mode == FpRoundMode::RoundNearestTiesAway
                    || (*truncate && mode != FpRoundMode::RoundTowardZero)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let elem_bits = int_elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        if !*mask_zeroing {
                            let preserved = Self::get_lane(&old, lane, elem_bits);
                            Self::set_lane(&mut result, lane, elem_bits, preserved);
                        }
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, 16);
                    let converted = Self::x86_simd_fp_to_int(
                        source_bits,
                        X86_SIMD_F16,
                        elem_bits,
                        *signed,
                        mode,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedFpConvertStore {
                addr,
                src,
                mask,
                lanes,
                round,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                let source = Self::read_vec(ctx, *src);
                let mask_bits = mask.map(|reg| ctx.read_vreg(reg));

                // E11 fault suppression is per destination element. Probe all
                // active lanes before conversion so a late address fault cannot
                // leave either partial stores or premature MXCSR status updates.
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        continue;
                    }
                    memory.probe(effective_addr.wrapping_add(u64::from(lane) * 2), 2, true)?;
                }

                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                let mut converted_lanes = [0u16; 16];
                let mut status = 0;
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, 32);
                    let converted = Self::x86_simd_fp_convert_precision(
                        source_bits,
                        X86_SIMD_F32,
                        X86_SIMD_F16,
                        mode,
                        // VCVTPS2PH produces FP16 denormals even when FTZ is set.
                        mxcsr & !(1 << 15),
                        false,
                    );
                    status |= converted.status;
                    converted_lanes[usize::from(lane)] = converted.bits as u16;
                }
                if Self::x86_simd_fp_unmasked(status, mxcsr) {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                    return Ok(());
                }

                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        continue;
                    }
                    memory.write(
                        effective_addr.wrapping_add(u64::from(lane) * 2),
                        &converted_lanes[usize::from(lane)].to_le_bytes(),
                    )?;
                }
                if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                    x86.mxcsr |= status;
                }
            }

            OpKind::FRound {
                dst,
                src,
                precision,
                mode,
            } => {
                let a = self.read_fp(ctx, *src, *precision);
                self.write_fp(ctx, *dst, self.round_fp_value(ctx, a, *mode), *precision);
            }

            OpKind::X86Round {
                dst,
                merge,
                src,
                elem,
                width,
                lanes,
                scalar_source,
                zero_upper,
                mode,
                suppress_precision,
            } => {
                let source = if *scalar_source && matches!(src, VReg::Virtual(_)) {
                    let mut value = [0u64; 16];
                    value[0] = ctx.read_vreg(*src);
                    value
                } else {
                    Self::read_vec(ctx, *src)
                };
                let old = Self::read_vec(ctx, *dst);
                let mut result = if u32::from(*lanes) * elem.bytes() < width.bytes() {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *zero_upper {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let daz = mxcsr & (1 << 6) != 0;
                let mut status = 0u32;
                for lane in 0..*lanes {
                    let raw = Self::get_lane(&source, lane, elem.bytes() * 8);
                    let rounded = match elem {
                        VecElementType::F32 => {
                            let mut bits = raw as u32;
                            let exponent = bits & 0x7F80_0000;
                            let fraction = bits & 0x007F_FFFF;
                            if exponent == 0 && fraction != 0 && daz {
                                bits &= 0x8000_0000;
                            }
                            let exponent = bits & 0x7F80_0000;
                            let fraction = bits & 0x007F_FFFF;
                            if exponent == 0x7F80_0000 && fraction != 0 {
                                if fraction & 0x0040_0000 == 0 {
                                    status |= 1;
                                    bits |= 0x0040_0000;
                                }
                                u64::from(bits)
                            } else {
                                let value = f32::from_bits(bits);
                                let rounded =
                                    self.round_fp_value(ctx, f64::from(value), *mode) as f32;
                                let rounded_bits = rounded.to_bits();
                                if rounded_bits != bits && !*suppress_precision {
                                    status |= 1 << 5;
                                }
                                u64::from(rounded_bits)
                            }
                        }
                        VecElementType::F64 => {
                            let mut bits = raw;
                            let exponent = bits & 0x7FF0_0000_0000_0000;
                            let fraction = bits & 0x000F_FFFF_FFFF_FFFF;
                            if exponent == 0 && fraction != 0 && daz {
                                bits &= 0x8000_0000_0000_0000;
                            }
                            let exponent = bits & 0x7FF0_0000_0000_0000;
                            let fraction = bits & 0x000F_FFFF_FFFF_FFFF;
                            if exponent == 0x7FF0_0000_0000_0000 && fraction != 0 {
                                if fraction & 0x0008_0000_0000_0000 == 0 {
                                    status |= 1;
                                    bits |= 0x0008_0000_0000_0000;
                                }
                                bits
                            } else {
                                let value = f64::from_bits(bits);
                                let rounded = self.round_fp_value(ctx, value, *mode);
                                let rounded_bits = rounded.to_bits();
                                if rounded_bits != bits && !*suppress_precision {
                                    status |= 1 << 5;
                                }
                                rounded_bits
                            }
                        }
                        _ => {
                            ctx.request_exit(ExitReason::Undefined {
                                addr: ctx.pc,
                                opcode: 0,
                            });
                            return Ok(());
                        }
                    };
                    Self::set_lane(&mut result, lane, elem.bytes() * 8, rounded);
                }
                if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                    x86.mxcsr |= status;
                }
                let invalid_unmasked = status & 1 != 0 && mxcsr & (1 << 7) == 0;
                let precision_unmasked = status & (1 << 5) != 0 && mxcsr & (1 << 12) == 0;
                if invalid_unmasked || precision_unmasked {
                    ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                } else {
                    Self::write_vec(ctx, *dst, result);
                }
            }

            OpKind::X86DotProduct {
                dst,
                src1,
                src2,
                elem,
                width,
                imm,
            } => {
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = match (mxcsr >> 13) & 3 {
                    0 => FpRoundMode::RoundNearest,
                    1 => FpRoundMode::RoundDown,
                    2 => FpRoundMode::RoundUp,
                    _ => FpRoundMode::RoundTowardZero,
                };
                let (format, lanes_per_group, groups) = match (*elem, *width) {
                    (VecElementType::F32, VecWidth::V128) => (X86_SIMD_F32, 4u8, 1u8),
                    (VecElementType::F32, VecWidth::V256) => (X86_SIMD_F32, 4, 2),
                    (VecElementType::F64, VecWidth::V128) => (X86_SIMD_F64, 2, 1),
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let commit_stage = |ctx: &mut SmirContext, stage_status: u32| -> bool {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= stage_status;
                    }
                    if Self::x86_simd_fp_unmasked(stage_status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        true
                    } else {
                        false
                    }
                };

                let mut products = vec![[0u64; 4]; groups as usize];
                let mut stage_status = 0u32;
                for group in 0..groups {
                    for lane in 0..lanes_per_group {
                        if imm & (1 << (lane + 4)) == 0 {
                            continue;
                        }
                        let vector_lane = group * lanes_per_group + lane;
                        let a = Self::get_lane(&first, vector_lane, elem.bytes() * 8);
                        let b = Self::get_lane(&second, vector_lane, elem.bytes() * 8);
                        let product = Self::x86_simd_fp_mul(a, b, format, mode, mxcsr);
                        products[group as usize][lane as usize] = product.bits;
                        stage_status |= product.status;
                    }
                }
                if commit_stage(ctx, stage_status) {
                    return Ok(());
                }

                let mut totals = vec![0u64; groups as usize];
                if *elem == VecElementType::F64 {
                    stage_status = 0;
                    for group in 0..groups {
                        let sum = Self::x86_simd_fp_add(
                            products[group as usize][0],
                            products[group as usize][1],
                            format,
                            mode,
                            mxcsr,
                        );
                        totals[group as usize] = sum.bits;
                        stage_status |= sum.status;
                    }
                    if commit_stage(ctx, stage_status) {
                        return Ok(());
                    }
                } else {
                    let mut low = vec![0u64; groups as usize];
                    let mut high = vec![0u64; groups as usize];
                    stage_status = 0;
                    for group in 0..groups {
                        let sum = Self::x86_simd_fp_add(
                            products[group as usize][0],
                            products[group as usize][1],
                            format,
                            mode,
                            mxcsr,
                        );
                        low[group as usize] = sum.bits;
                        stage_status |= sum.status;
                    }
                    if commit_stage(ctx, stage_status) {
                        return Ok(());
                    }
                    stage_status = 0;
                    for group in 0..groups {
                        let sum = Self::x86_simd_fp_add(
                            products[group as usize][2],
                            products[group as usize][3],
                            format,
                            mode,
                            mxcsr,
                        );
                        high[group as usize] = sum.bits;
                        stage_status |= sum.status;
                    }
                    if commit_stage(ctx, stage_status) {
                        return Ok(());
                    }
                    stage_status = 0;
                    for group in 0..groups {
                        let sum = Self::x86_simd_fp_add(
                            low[group as usize],
                            high[group as usize],
                            format,
                            mode,
                            mxcsr,
                        );
                        totals[group as usize] = sum.bits;
                        stage_status |= sum.status;
                    }
                    if commit_stage(ctx, stage_status) {
                        return Ok(());
                    }
                }

                let mut result = [0u64; 16];
                for group in 0..groups {
                    for lane in 0..lanes_per_group {
                        if imm & (1 << lane) != 0 {
                            Self::set_lane(
                                &mut result,
                                group * lanes_per_group + lane,
                                elem.bytes() * 8,
                                totals[group as usize],
                            );
                        }
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            // ==================================================================
            // SIMD / VECTOR (simplified)
            // ==================================================================
            OpKind::VAdd {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                match elem {
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| a + b);
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| a + b);
                    }
                    _ => {
                        self.vec_binary_op(ctx, *dst, *src1, *src2, *elem, *lanes, |a, b| {
                            a.wrapping_add(b)
                        });
                    }
                }
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VSub {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                match elem {
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| a - b);
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| a - b);
                    }
                    _ => {
                        self.vec_binary_op(ctx, *dst, *src1, *src2, *elem, *lanes, |a, b| {
                            a.wrapping_sub(b)
                        });
                    }
                }
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VMax {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                match elem {
                    // VMax is architectural vector FMAX: NaN-PROPAGATING (a lone
                    // quiet NaN wins), distinct from the numeric VFMinMaxNm. Rust's
                    // `a.max(b)` is numeric (drops a lone NaN), so propagate
                    // explicitly. (#159)
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| {
                            if a.is_nan() {
                                a
                            } else if b.is_nan() {
                                b
                            } else {
                                a.max(b)
                            }
                        });
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| {
                            if a.is_nan() {
                                a
                            } else if b.is_nan() {
                                b
                            } else {
                                a.max(b)
                            }
                        });
                    }
                    _ => {
                        self.vec_binary_op(ctx, *dst, *src1, *src2, *elem, *lanes, |a, b| a.max(b));
                    }
                }
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VX86MinMax {
                dst,
                src1,
                src2,
                elem,
                lanes,
                min,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                match elem {
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| {
                            if (*min && a < b) || (!*min && a > b) {
                                a
                            } else {
                                b
                            }
                        });
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| {
                            if (*min && a < b) || (!*min && a > b) {
                                a
                            } else {
                                b
                            }
                        });
                    }
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                    }
                }
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VAddSubSat {
                dst,
                src1,
                src2,
                elem,
                lanes,
                subtract,
                signed,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                let lhs = Self::read_vec(ctx, *src1);
                let rhs = Self::read_vec(ctx, *src2);
                let bits = elem.bytes() * 8;
                let mask = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let a = Self::get_lane(&lhs, lane, bits);
                    let b = Self::get_lane(&rhs, lane, bits);
                    let value = if *signed {
                        let shift = 64 - bits;
                        let a = ((a << shift) as i64 >> shift) as i128;
                        let b = ((b << shift) as i64 >> shift) as i128;
                        let raw = if *subtract { a - b } else { a + b };
                        let min = -(1i128 << (bits - 1));
                        let max = (1i128 << (bits - 1)) - 1;
                        raw.clamp(min, max) as u64 & mask
                    } else if *subtract {
                        a.saturating_sub(b)
                    } else {
                        (u128::from(a) + u128::from(b)).min(u128::from(mask)) as u64
                    };
                    Self::set_lane(&mut result, lane, bits, value);
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VMul {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                match elem {
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| a * b);
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| a * b);
                    }
                    _ => {
                        self.vec_binary_op(ctx, *dst, *src1, *src2, *elem, *lanes, |a, b| {
                            a.wrapping_mul(b)
                        });
                    }
                }
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VDiv {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                match elem {
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| a / b);
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| a / b);
                    }
                    _ => {
                        // Integer vector divide is not a NEON op; guard against
                        // division-by-zero in case a malformed op reaches here.
                        self.vec_binary_op(ctx, *dst, *src1, *src2, *elem, *lanes, |a, b| {
                            if b == 0 { 0 } else { a.wrapping_div(b) }
                        });
                    }
                }
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VUnary {
                dst,
                src,
                elem,
                lanes,
                op,
            } => match (op, elem) {
                (VecUnaryOp::FAbs, VecElementType::F32) => {
                    self.vec_unary_op_f32(ctx, *dst, *src, *lanes, |a| a.abs());
                }
                (VecUnaryOp::FAbs, VecElementType::F64) => {
                    self.vec_unary_op_f64(ctx, *dst, *src, *lanes, |a| a.abs());
                }
                (VecUnaryOp::FNeg, VecElementType::F32) => {
                    self.vec_unary_op_f32(ctx, *dst, *src, *lanes, |a| -a);
                }
                (VecUnaryOp::FNeg, VecElementType::F64) => {
                    self.vec_unary_op_f64(ctx, *dst, *src, *lanes, |a| -a);
                }
                (VecUnaryOp::FSqrt, VecElementType::F32) => {
                    self.vec_unary_op_f32(ctx, *dst, *src, *lanes, |a| a.sqrt());
                }
                (VecUnaryOp::FSqrt, VecElementType::F64) => {
                    self.vec_unary_op_f64(ctx, *dst, *src, *lanes, |a| a.sqrt());
                }
                (VecUnaryOp::FRecipEstimate | VecUnaryOp::FRsqrtEstimate, VecElementType::F32) => {
                    let input = Self::read_vec(ctx, *src);
                    let mut result = [0u64; 16];
                    for lane in 0..*lanes {
                        let raw = Self::get_lane(&input, lane, 32) as u32;
                        let sign = raw & 0x8000_0000;
                        let exponent = raw & 0x7F80_0000;
                        let fraction = raw & 0x007F_FFFF;
                        let estimate = if exponent == 0 {
                            // Zero and denormal inputs are both treated as signed zero.
                            sign | 0x7F80_0000
                        } else if exponent == 0x7F80_0000 && fraction != 0 {
                            // Preserve the NaN payload/sign and quiet signaling NaNs.
                            raw | 0x0040_0000
                        } else if *op == VecUnaryOp::FRsqrtEstimate && sign != 0 {
                            // Negative finite values and -infinity produce FP indefinite;
                            // -0 was handled by the exponent==0 case above.
                            0xFFC0_0000
                        } else if exponent == 0x7F80_0000 {
                            // Reciprocal(+/-inf) is signed zero; rsqrt(+inf) is +zero.
                            if *op == VecUnaryOp::FRecipEstimate {
                                sign
                            } else {
                                0
                            }
                        } else {
                            let value = f32::from_bits(raw);
                            let exact = if *op == VecUnaryOp::FRecipEstimate {
                                1.0 / value
                            } else {
                                1.0 / value.sqrt()
                            };
                            let bits = exact.to_bits();
                            // Architectural estimate results that are tiny are flushed.
                            if bits & 0x7F80_0000 == 0 {
                                bits & 0x8000_0000
                            } else {
                                bits
                            }
                        };
                        Self::set_lane(&mut result, lane, 32, u64::from(estimate));
                    }
                    Self::write_vec(ctx, *dst, result);
                }
                (VecUnaryOp::FRecipEstimate | VecUnaryOp::FRsqrtEstimate, VecElementType::F64) => {
                    unreachable!("x86 reciprocal estimate instructions are F32-only")
                }
                (VecUnaryOp::Neg, _) => {
                    let bits = elem.bytes() * 8;
                    self.vec_unary_op(ctx, *dst, *src, *elem, *lanes, |a| {
                        let neg = (a as i64).wrapping_neg() as u64;
                        if bits >= 64 {
                            neg
                        } else {
                            neg & ((1u64 << bits) - 1)
                        }
                    });
                }
                (VecUnaryOp::Abs, _) => {
                    let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                    let bits = elem.bytes() * 8;
                    self.vec_unary_op(ctx, *dst, *src, *elem, *lanes, |a| {
                        // Sign-extend the lane, take abs, re-truncate.
                        let shift = 64 - bits;
                        let signed = ((a << shift) as i64) >> shift;
                        let abs = signed.unsigned_abs();
                        if bits >= 64 {
                            abs
                        } else {
                            abs & ((1u64 << bits) - 1)
                        }
                    });
                    Self::restore_legacy_xmm_upper(ctx, *dst, old);
                }
                (VecUnaryOp::Clz, _) => {
                    let bits = elem.bytes() * 8;
                    self.vec_unary_op(ctx, *dst, *src, *elem, *lanes, |a| {
                        let v = if bits >= 64 {
                            a
                        } else {
                            a & ((1u64 << bits) - 1)
                        };
                        if v == 0 {
                            bits as u64
                        } else {
                            (v.leading_zeros() - (64 - bits)) as u64
                        }
                    });
                }
                (VecUnaryOp::Cls, _) => {
                    let bits = elem.bytes() * 8;
                    self.vec_unary_op(ctx, *dst, *src, *elem, *lanes, |a| {
                        // Count consecutive bits below the MSB equal to it.
                        let sign = (a >> (bits - 1)) & 1;
                        let mut count = 0u64;
                        for i in (0..bits - 1).rev() {
                            if (a >> i) & 1 == sign {
                                count += 1;
                            } else {
                                break;
                            }
                        }
                        count
                    });
                }
                (VecUnaryOp::Rbit, _) => {
                    self.vec_unary_op(ctx, *dst, *src, VecElementType::I8, *lanes, |a| {
                        (a as u8).reverse_bits() as u64
                    });
                }
                (VecUnaryOp::Cnt, _) => {
                    self.vec_unary_op(ctx, *dst, *src, VecElementType::I8, *lanes, |a| {
                        (a as u8).count_ones() as u64
                    });
                }
                (VecUnaryOp::Not, _) => {
                    self.vec_unary_op(ctx, *dst, *src, VecElementType::I8, *lanes, |a| {
                        !(a as u8) as u64
                    });
                }
                (VecUnaryOp::Rev16 | VecUnaryOp::Rev32 | VecUnaryOp::Rev64, _) => {
                    // Reverse the order of `elem`-sized elements within each
                    // container (16/32/64 bits). This reorders lanes, so it
                    // can't use the per-lane vec_unary_op helper.
                    let container_bits = match op {
                        VecUnaryOp::Rev16 => 16u32,
                        VecUnaryOp::Rev32 => 32,
                        _ => 64,
                    };
                    let elem_bits = elem.bytes() * 8;
                    let per = (container_bits / elem_bits).max(1);
                    let a = Self::read_vec(ctx, *src);
                    let mut result = [0u64; 16];
                    for lane in 0..u32::from(*lanes) {
                        let container = lane / per;
                        let within = lane % per;
                        let dst_lane = container * per + (per - 1 - within);
                        let v = Self::get_lane(&a, lane as u8, elem_bits);
                        Self::set_lane(&mut result, dst_lane as u8, elem_bits, v);
                    }
                    Self::write_vec(ctx, *dst, result);
                }
                _ => {
                    // FP-only ops with an integer element (or vice versa) should
                    // not be produced; leave dst unchanged defensively.
                }
            },

            OpKind::VReduce {
                dst,
                src,
                elem,
                lanes,
                op,
            } => {
                let a = Self::read_vec(ctx, *src);
                let bits = elem.bytes() * 8;
                let mask = if bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let lane = |i: u8| Self::get_lane(&a, i, bits) & mask;
                let sext = |v: u64| {
                    let shift = 64 - bits;
                    ((v << shift) as i64) >> shift
                };
                let n = *lanes;
                let value = match op {
                    VecReduceOp::Add => {
                        let mut acc = 0u64;
                        for i in 0..n {
                            acc = acc.wrapping_add(lane(i));
                        }
                        acc & mask
                    }
                    VecReduceOp::SMax => {
                        let mut acc = sext(lane(0));
                        for i in 1..n {
                            acc = acc.max(sext(lane(i)));
                        }
                        acc as u64 & mask
                    }
                    VecReduceOp::SMin => {
                        let mut acc = sext(lane(0));
                        for i in 1..n {
                            acc = acc.min(sext(lane(i)));
                        }
                        acc as u64 & mask
                    }
                    VecReduceOp::UMax => {
                        let mut acc = lane(0);
                        for i in 1..n {
                            acc = acc.max(lane(i));
                        }
                        acc
                    }
                    VecReduceOp::UMin => {
                        let mut acc = lane(0);
                        for i in 1..n {
                            acc = acc.min(lane(i));
                        }
                        acc
                    }
                    // FP reductions. NaN-quiet (FMaxNm/FMinNm) use Rust min/max
                    // (maxNum/minNum); NaN-propagating (FMax/FMin) yield NaN if
                    // any lane is NaN.
                    VecReduceOp::FMax
                    | VecReduceOp::FMin
                    | VecReduceOp::FMaxNm
                    | VecReduceOp::FMinNm => {
                        let nm = matches!(op, VecReduceOp::FMaxNm | VecReduceOp::FMinNm);
                        let is_min = matches!(op, VecReduceOp::FMin | VecReduceOp::FMinNm);
                        if bits == 32 {
                            let lf = |i: u8| f32::from_bits(Self::get_lane(&a, i, 32) as u32);
                            let mut acc = lf(0);
                            for i in 1..n {
                                let x = lf(i);
                                acc = if !nm && (acc.is_nan() || x.is_nan()) {
                                    f32::NAN
                                } else if is_min {
                                    acc.min(x)
                                } else {
                                    acc.max(x)
                                };
                            }
                            acc.to_bits() as u64
                        } else {
                            let lf = |i: u8| f64::from_bits(Self::get_lane(&a, i, 64));
                            let mut acc = lf(0);
                            for i in 1..n {
                                let x = lf(i);
                                acc = if !nm && (acc.is_nan() || x.is_nan()) {
                                    f64::NAN
                                } else if is_min {
                                    acc.min(x)
                                } else {
                                    acc.max(x)
                                };
                            }
                            acc.to_bits()
                        }
                    }
                    // Widening add: sum sign/zero-extended lanes; result is 2x
                    // the element width.
                    VecReduceOp::SAddLong => {
                        let mut acc = 0i128;
                        for i in 0..n {
                            acc += i128::from(sext(lane(i)));
                        }
                        acc as u64
                    }
                    VecReduceOp::UAddLong => {
                        let mut acc = 0u128;
                        for i in 0..n {
                            acc += u128::from(lane(i));
                        }
                        acc as u64
                    }
                };
                // Widening reductions write a result 2x the element width.
                let result_bits = if matches!(op, VecReduceOp::SAddLong | VecReduceOp::UAddLong) {
                    (bits * 2).min(64)
                } else {
                    bits
                };
                let rmask = if result_bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << result_bits) - 1
                };
                let mut result = [0u64; 16];
                Self::set_lane(&mut result, 0, result_bits, value & rmask);
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VFMinMaxNm {
                dst,
                src1,
                src2,
                elem,
                lanes,
                min,
            } => {
                // IEEE maxNum/minNum: Rust f32/f64 max/min return the numeric
                // operand when one is NaN, matching FMAXNM/FMINNM.
                match elem {
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| {
                            if *min { a.min(b) } else { a.max(b) }
                        });
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| {
                            if *min { a.min(b) } else { a.max(b) }
                        });
                    }
                    _ => {
                        // FMAXNM/FMINNM are FP-only; ignore otherwise.
                    }
                }
            }

            OpKind::VPermute2 {
                dst,
                src1,
                src2,
                elem,
                lanes,
                kind,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let bits = elem.bytes() * 8;
                let n = *lanes as usize;
                let half = n / 2;
                let geta = |i: usize| Self::get_lane(&a, i as u8, bits);
                let getb = |i: usize| Self::get_lane(&b, i as u8, bits);
                let mut result = [0u64; 16];
                for d in 0..n {
                    let v = match kind {
                        VecPermuteKind::Zip1 => {
                            if d % 2 == 0 {
                                geta(d / 2)
                            } else {
                                getb(d / 2)
                            }
                        }
                        VecPermuteKind::Zip2 => {
                            if d % 2 == 0 {
                                geta(half + d / 2)
                            } else {
                                getb(half + d / 2)
                            }
                        }
                        VecPermuteKind::Uzp1 => {
                            let idx = 2 * d;
                            if idx < n { geta(idx) } else { getb(idx - n) }
                        }
                        VecPermuteKind::Uzp2 => {
                            let idx = 2 * d + 1;
                            if idx < n { geta(idx) } else { getb(idx - n) }
                        }
                        VecPermuteKind::Trn1 => {
                            if d % 2 == 0 {
                                geta(d)
                            } else {
                                getb(d - 1)
                            }
                        }
                        VecPermuteKind::Trn2 => {
                            if d % 2 == 0 {
                                geta(d + 1)
                            } else {
                                getb(d)
                            }
                        }
                    };
                    Self::set_lane(&mut result, d as u8, bits, v);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VTableLookup {
                dst,
                table,
                num_tables,
                index,
                lanes,
                is_tbx,
            } => {
                // Build the byte table from `num_tables` consecutive registers
                // (table, table+1, ... mod 32).
                let base = match table {
                    VReg::Arch(ArchReg::Arm(ArmReg::V(n))) => u32::from(*n),
                    _ => 0,
                };
                let mut tbl = [0u8; 64];
                for t in 0..u32::from(*num_tables) {
                    let reg = VReg::Arch(ArchReg::Arm(ArmReg::V(((base + t) % 32) as u8)));
                    let rv = Self::read_vec(ctx, reg);
                    for byte in 0..16u8 {
                        tbl[(t * 16 + u32::from(byte)) as usize] =
                            Self::get_lane(&rv, byte, 8) as u8;
                    }
                }
                let table_size = usize::from(*num_tables) * 16;
                let idx_v = Self::read_vec(ctx, *index);
                let mut out = [0u8; 16];
                if *is_tbx {
                    let cur = Self::read_vec(ctx, *dst);
                    for byte in 0..16u8 {
                        out[byte as usize] = Self::get_lane(&cur, byte, 8) as u8;
                    }
                }
                let n = *lanes as usize;
                for byte in 0..n {
                    let idx = Self::get_lane(&idx_v, byte as u8, 8) as usize;
                    if idx < table_size {
                        out[byte] = tbl[idx];
                    } else if !*is_tbx {
                        out[byte] = 0;
                    }
                }
                // Q==0 (8 lanes) zeroes the upper 64 bits.
                if n == 8 {
                    for byte in &mut out[8..16] {
                        *byte = 0;
                    }
                }
                let mut result = [0u64; 16];
                for byte in 0..16u8 {
                    Self::set_lane(&mut result, byte, 8, u64::from(out[byte as usize]));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VAnd {
                dst,
                src1,
                src2,
                width,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                let word_count = (width.bytes() / 8) as usize;
                for i in 0..word_count {
                    result[i] = a[i] & b[i];
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VAndNot {
                dst,
                src1,
                src2,
                width,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                let word_count = (width.bytes() / 8) as usize;
                for i in 0..word_count {
                    result[i] = !a[i] & b[i];
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VOr {
                dst,
                src1,
                src2,
                width,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                let word_count = (width.bytes() / 8) as usize;
                for i in 0..word_count {
                    result[i] = a[i] | b[i];
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VXor {
                dst,
                src1,
                src2,
                width,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                let word_count = (width.bytes() / 8) as usize;
                for i in 0..word_count {
                    result[i] = a[i] ^ b[i];
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VBitSelect {
                dst,
                mask,
                src_true,
                src_false,
                width,
            } => {
                let m = Self::read_vec(ctx, *mask);
                let t = Self::read_vec(ctx, *src_true);
                let f = Self::read_vec(ctx, *src_false);
                let mut result = [0u64; 16];
                let word_count = (width.bytes() / 8) as usize;
                for i in 0..word_count {
                    result[i] = (t[i] & m[i]) | (f[i] & !m[i]);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VLane {
                dst,
                src1,
                src2,
                elem,
                lanes,
                op,
                signed,
                set_ovf,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let elem_bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                let mut ovf = false;
                for lane in 0..*lanes {
                    let av = Self::get_lane(&a, lane, elem_bits);
                    let bv = Self::get_lane(&b, lane, elem_bits);
                    let rv = Self::apply_lane_op(*op, av, bv, elem_bits, *signed);
                    // For the saturating VLane opcodes whose sem uses
                    // `ctx.sat_n`/`ctx.satu_n` (e.g. `vsubuwsat`), flag USR:OVF
                    // on any lane whose add/sub clamped out of the target range.
                    if *set_ovf {
                        ovf |= Self::lane_sat_clamped(*op, av, bv, elem_bits, *signed);
                    }
                    Self::set_lane(&mut result, lane, elem_bits, rv);
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
                if *set_ovf && ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VWidenMul {
                dst_lo,
                dst_hi,
                src1,
                src2,
                src_elem,
                signed1,
                signed2,
                acc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let nbits = src_elem.bytes() * 8;
                let wbits = nbits * 2;
                let wide_lanes = (1024 / nbits as usize) / 2; // wide lanes per output vector
                let mut lo = if *acc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut hi = if *acc {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                // Sign- or zero-extend an `nbits` zero-extended lane value to i64.
                let ext = |v: u64, signed: bool| -> i64 {
                    if signed {
                        let shift = 64 - nbits;
                        ((v << shift) as i64) >> shift
                    } else {
                        v as i64
                    }
                };
                for i in 0..wide_lanes {
                    let even = i as u8 * 2;
                    let odd = even + 1;
                    let pe = ext(Self::get_lane(&a, even, nbits), *signed1)
                        .wrapping_mul(ext(Self::get_lane(&b, even, nbits), *signed2));
                    let po = ext(Self::get_lane(&a, odd, nbits), *signed1)
                        .wrapping_mul(ext(Self::get_lane(&b, odd, nbits), *signed2));
                    let ae = if *acc {
                        Self::get_lane(&lo, i as u8, wbits) as i64
                    } else {
                        0
                    };
                    let ao = if *acc {
                        Self::get_lane(&hi, i as u8, wbits) as i64
                    } else {
                        0
                    };
                    Self::set_lane(&mut lo, i as u8, wbits, ae.wrapping_add(pe) as u64);
                    Self::set_lane(&mut hi, i as u8, wbits, ao.wrapping_add(po) as u64);
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VWidenAddSub {
                dst_lo,
                dst_hi,
                src1,
                src2,
                src_elem,
                signed1,
                signed2,
                sub,
                acc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let nbits = src_elem.bytes() * 8;
                let wbits = nbits * 2;
                let wide_lanes = (1024 / nbits as usize) / 2; // wide lanes per output vector
                let mut lo = if *acc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut hi = if *acc {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                // Sign- or zero-extend an `nbits` zero-extended lane value to i64.
                let ext = |v: u64, signed: bool| -> i64 {
                    if signed {
                        let shift = 64 - nbits;
                        ((v << shift) as i64) >> shift
                    } else {
                        v as i64
                    }
                };
                let combine = |x: i64, y: i64| -> i64 {
                    if *sub {
                        x.wrapping_sub(y)
                    } else {
                        x.wrapping_add(y)
                    }
                };
                for i in 0..wide_lanes {
                    let even = i as u8 * 2;
                    let odd = even + 1;
                    let re = combine(
                        ext(Self::get_lane(&a, even, nbits), *signed1),
                        ext(Self::get_lane(&b, even, nbits), *signed2),
                    );
                    let ro = combine(
                        ext(Self::get_lane(&a, odd, nbits), *signed1),
                        ext(Self::get_lane(&b, odd, nbits), *signed2),
                    );
                    let ae = if *acc {
                        // sign-extend the existing wide lane so accumulate wraps signed
                        let v = Self::get_lane(&lo, i as u8, wbits);
                        let s = 64 - wbits;
                        ((v << s) as i64) >> s
                    } else {
                        0
                    };
                    let ao = if *acc {
                        let v = Self::get_lane(&hi, i as u8, wbits);
                        let s = 64 - wbits;
                        ((v << s) as i64) >> s
                    } else {
                        0
                    };
                    Self::set_lane(&mut lo, i as u8, wbits, ae.wrapping_add(re) as u64);
                    Self::set_lane(&mut hi, i as u8, wbits, ao.wrapping_add(ro) as u64);
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VLaneUnary {
                dst,
                src,
                elem,
                lanes,
                op,
                signed,
            } => {
                let a = Self::read_vec(ctx, *src);
                let elem_bits = elem.bytes() * 8;
                let mask: u64 = if elem_bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << elem_bits) - 1
                };
                // Sign-extend a zero-extended `elem_bits` lane value to i64.
                let sx = |v: u64| -> i64 {
                    if elem_bits >= 64 {
                        v as i64
                    } else {
                        let shift = 64 - elem_bits;
                        ((v << shift) as i64) >> shift
                    }
                };
                let smax: i64 = if elem_bits >= 64 {
                    i64::MAX
                } else {
                    (1i64 << (elem_bits - 1)) - 1
                };
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let av = Self::get_lane(&a, lane, elem_bits);
                    let rv: u64 = match op {
                        // Not
                        0 => !av,
                        // Abs (wrapping: MIN -> MIN)
                        1 => (sx(av).wrapping_abs()) as u64,
                        // AbsSat: clamp |a| to the signed max (MIN -> MAX)
                        2 => {
                            let s = sx(av);
                            // wrapping_abs of MIN stays MIN (negative); clamp via i128
                            ((s as i128).abs().min(smax as i128)) as u64
                        }
                        // Clz within the elem-wide lane
                        3 => {
                            let v = av & mask;
                            (v << (64 - elem_bits)).leading_zeros().min(elem_bits) as u64
                        }
                        // Popcount of the elem-wide lane
                        4 => (av & mask).count_ones() as u64,
                        // NormAmt: max(clz(a), clz(!a)) - 1 within the lane
                        5 => {
                            let v = (av & mask) << (64 - elem_bits);
                            let nv = (!av & mask) << (64 - elem_bits);
                            let n = v
                                .leading_zeros()
                                .min(elem_bits)
                                .max(nv.leading_zeros().min(elem_bits));
                            (n - 1) as u64
                        }
                        // Neg (two's complement)
                        6 => sx(av).wrapping_neg() as u64,
                        // Clb: count leading sign bits = max(clz, clo) capped at
                        // the element width, on the left-justified lane value.
                        7 => {
                            let lj = (av & mask) << (64 - elem_bits);
                            let zeros = lj.leading_zeros().min(elem_bits);
                            let ones = lj.leading_ones().min(elem_bits);
                            zeros.max(ones) as u64
                        }
                        _ => av,
                    };
                    let _ = signed;
                    Self::set_lane(&mut result, lane, elem_bits, rv & mask);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VNavg {
                dst,
                src1,
                src2,
                elem,
                lanes,
                signed,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let elem_bits = elem.bytes() * 8;
                let mask: u64 = if elem_bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << elem_bits) - 1
                };
                let ext = |v: u64| -> i64 {
                    if *signed {
                        if elem_bits >= 64 {
                            v as i64
                        } else {
                            let shift = 64 - elem_bits;
                            ((v << shift) as i64) >> shift
                        }
                    } else {
                        (v & mask) as i64
                    }
                };
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let av = ext(Self::get_lane(&a, lane, elem_bits));
                    let bv = ext(Self::get_lane(&b, lane, elem_bits));
                    let r = (av.wrapping_sub(bv)) >> 1; // arithmetic, like sem `>> 1`
                    Self::set_lane(&mut result, lane, elem_bits, (r as u64) & mask);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VShiftAcc {
                dst,
                src,
                amount,
                shift,
                elem,
                lanes,
            } => {
                let amt = match amount {
                    SrcOperand::Imm(val) => *val as u32,
                    SrcOperand::Reg(reg) => ctx.read_vreg(*reg) as u32,
                    _ => 0,
                };
                let elem_bits = elem.bytes() * 8;
                let mask = if elem_bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << elem_bits) - 1
                };
                let sh = amt % elem_bits;
                let src_val = Self::read_vec(ctx, *src);
                let mut result = Self::read_vec(ctx, *dst);
                for lane in 0..*lanes {
                    let val = Self::get_lane(&src_val, lane, elem_bits);
                    let shifted = match shift {
                        ShiftOp::Lsl => (val << sh) & mask,
                        ShiftOp::Lsr => (val >> sh) & mask,
                        ShiftOp::Asr => {
                            let sv = if elem_bits >= 64 {
                                val as i64
                            } else {
                                let s = 64 - elem_bits;
                                ((val << s) as i64) >> s
                            };
                            ((sv >> sh) as u64) & mask
                        }
                        _ => val & mask,
                    };
                    let prev = Self::get_lane(&result, lane, elem_bits);
                    Self::set_lane(
                        &mut result,
                        lane,
                        elem_bits,
                        prev.wrapping_add(shifted) & mask,
                    );
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VLut16 {
                dst_lo,
                dst_hi,
                src_idx,
                table,
                sel,
                nomatch,
                oracc,
            } => {
                let vu = Self::read_vec(ctx, *src_idx);
                let vv = Self::read_vec(ctx, *table);
                let sel_v = match sel {
                    SrcOperand::Imm(v) => *v as u32,
                    SrcOperand::Reg(r) => ctx.read_vreg(*r) as u32,
                    _ => 0,
                };
                let matchval = (sel_v & 0xF) as u8;
                let oh = ((sel_v >> 1) & 0x1) as u8;
                let mut lo = if *oracc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut hi = if *oracc {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                let look = |idx: u8| -> u16 {
                    if *nomatch {
                        let k = ((idx & 0x0F) | (matchval << 4)) as usize;
                        Self::get_lane(&vv, ((k % 32) * 2) as u8 + oh, 16) as u16
                    } else if (idx & 0xF0) == (matchval << 4) {
                        let k = idx as usize;
                        Self::get_lane(&vv, ((k % 32) * 2) as u8 + oh, 16) as u16
                    } else {
                        0
                    }
                };
                for i in 0..64u8 {
                    let v_lo = look(Self::get_lane(&vu, i * 2, 8) as u8);
                    let v_hi = look(Self::get_lane(&vu, i * 2 + 1, 8) as u8);
                    if *oracc {
                        let plo = Self::get_lane(&lo, i, 16) as u16;
                        let phi = Self::get_lane(&hi, i, 16) as u16;
                        Self::set_lane(&mut lo, i, 16, (plo | v_lo) as u64);
                        Self::set_lane(&mut hi, i, 16, (phi | v_hi) as u64);
                    } else {
                        Self::set_lane(&mut lo, i, 16, v_lo as u64);
                        Self::set_lane(&mut hi, i, 16, v_hi as u64);
                    }
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VLut {
                dst,
                src_idx,
                table,
                sel,
                nomatch,
                oracc,
            } => {
                let vu = Self::read_vec(ctx, *src_idx);
                let vv = Self::read_vec(ctx, *table);
                let sel_v = match sel {
                    SrcOperand::Imm(v) => *v as u32,
                    SrcOperand::Reg(r) => ctx.read_vreg(*r) as u32,
                    _ => 0,
                };
                let matchval = (sel_v & 0x7) as u8;
                let oh = ((sel_v >> 1) & 0x1) as u8;
                let mut out = if *oracc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                for i in 0..128u8 {
                    let idx = Self::get_lane(&vu, i, 8) as u8;
                    let val: u8 = if *nomatch {
                        let lut_idx = ((idx & 0x1f) | (matchval << 5)) as usize;
                        Self::get_lane(&vv, ((lut_idx % 64) * 2) as u8 + oh, 8) as u8
                    } else if (idx & 0xe0) == (matchval << 5) {
                        let lut_idx = idx as usize;
                        Self::get_lane(&vv, ((lut_idx % 64) * 2) as u8 + oh, 8) as u8
                    } else {
                        0
                    };
                    if *oracc {
                        let prev = Self::get_lane(&out, i, 8) as u8;
                        Self::set_lane(&mut out, i, 8, (prev | val) as u64);
                    } else {
                        Self::set_lane(&mut out, i, 8, val as u64);
                    }
                }
                Self::write_vec(ctx, *dst, out);
            }

            OpKind::VDelta {
                dst,
                src,
                control,
                ascending,
            } => {
                let mut cur = Self::read_vec(ctx, *src);
                let ctrl = Self::read_vec(ctx, *control);
                let mut offsets = [1u8, 2, 4, 8, 16, 32, 64];
                if !*ascending {
                    offsets.reverse();
                }
                for &offset in offsets.iter() {
                    let off = offset as usize;
                    let prev = cur;
                    for k in 0..128usize {
                        let cb = Self::get_lane(&ctrl, k as u8, 8);
                        let src_k = if cb & (off as u64) != 0 {
                            (k ^ off) as u8
                        } else {
                            k as u8
                        };
                        Self::set_lane(&mut cur, k as u8, 8, Self::get_lane(&prev, src_k, 8));
                    }
                }
                Self::write_vec(ctx, *dst, cur);
            }

            OpKind::VShuffVdd {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                amount,
            } => {
                let mut lo = Self::read_vec(ctx, *src_lo);
                let mut hi = Self::read_vec(ctx, *src_hi);
                let rt = match amount {
                    SrcOperand::Imm(v) => *v as usize,
                    SrcOperand::Reg(r) => ctx.read_vreg(*r) as usize,
                    _ => 0,
                };
                let mut offset = 1usize;
                while offset < 128 {
                    if rt & offset != 0 {
                        for k in 0..128usize {
                            if k & offset == 0 {
                                let a = Self::get_lane(&hi, k as u8, 8);
                                let b = Self::get_lane(&lo, (k + offset) as u8, 8);
                                Self::set_lane(&mut hi, k as u8, 8, b);
                                Self::set_lane(&mut lo, (k + offset) as u8, 8, a);
                            }
                        }
                    }
                    offset <<= 1;
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VDealB4W { dst, src1, src2 } => {
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                for i in 0..32u8 {
                    Self::set_lane(&mut result, i, 8, Self::get_lane(&v, i * 4, 8));
                    Self::set_lane(&mut result, 32 + i, 8, Self::get_lane(&v, i * 4 + 2, 8));
                    Self::set_lane(&mut result, 64 + i, 8, Self::get_lane(&u, i * 4, 8));
                    Self::set_lane(&mut result, 96 + i, 8, Self::get_lane(&u, i * 4 + 2, 8));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VAlign {
                dst,
                src1,
                src2,
                amount,
                left,
            } => {
                let amt = match amount {
                    SrcOperand::Imm(v) => *v as usize,
                    SrcOperand::Reg(r) => ctx.read_vreg(*r) as usize,
                    _ => 0,
                };
                let shift = if *left { 128 - (amt & 127) } else { amt & 127 };
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                for i in 0..128u8 {
                    let j = i as usize + shift;
                    let byte = if j < 128 {
                        Self::get_lane(&v, j as u8, 8)
                    } else {
                        Self::get_lane(&u, (j - 128) as u8, 8)
                    };
                    Self::set_lane(&mut result, i, 8, byte);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VShuffle2 {
                dst,
                src,
                elem,
                deal,
            } => {
                let s = Self::read_vec(ctx, *src);
                let nbits = elem.bytes() * 8;
                let total = (1024 / nbits) as u8;
                let half = total / 2;
                let mut result = [0u64; 16];
                for i in 0..half {
                    if *deal {
                        Self::set_lane(&mut result, i, nbits, Self::get_lane(&s, i * 2, nbits));
                        Self::set_lane(
                            &mut result,
                            i + half,
                            nbits,
                            Self::get_lane(&s, i * 2 + 1, nbits),
                        );
                    } else {
                        Self::set_lane(&mut result, i * 2, nbits, Self::get_lane(&s, i, nbits));
                        Self::set_lane(
                            &mut result,
                            i * 2 + 1,
                            nbits,
                            Self::get_lane(&s, i + half, nbits),
                        );
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VShuffleEO {
                dst,
                src1,
                src2,
                elem,
                odd,
            } => {
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let nbits = elem.bytes() * 8;
                let total = (1024 / nbits) as u8;
                let half = total / 2;
                let parity = if *odd { 1 } else { 0 };
                let mut result = [0u64; 16];
                for i in 0..half {
                    let sel = i * 2 + parity;
                    Self::set_lane(&mut result, i * 2, nbits, Self::get_lane(&v, sel, nbits));
                    Self::set_lane(
                        &mut result,
                        i * 2 + 1,
                        nbits,
                        Self::get_lane(&u, sel, nbits),
                    );
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VPack {
                dst,
                src1,
                src2,
                elem,
                odd,
            } => {
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let nbits = elem.bytes() * 8;
                let total = (1024 / nbits) as u8;
                let half = total / 2;
                let parity = if *odd { 1 } else { 0 };
                let mut result = [0u64; 16];
                for i in 0..half {
                    let sel = i * 2 + parity;
                    Self::set_lane(&mut result, i, nbits, Self::get_lane(&v, sel, nbits));
                    Self::set_lane(&mut result, i + half, nbits, Self::get_lane(&u, sel, nbits));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VPackSat {
                dst,
                src1,
                src2,
                src_elem,
                to_unsigned,
                src_lanes,
                block_lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let wbits = src_elem.bytes() * 8;
                let nbits = wbits / 2;
                let (lo_b, hi_b) = if *to_unsigned {
                    (0i64, ((1i64 << nbits) - 1))
                } else {
                    (-(1i64 << (nbits - 1)), (1i64 << (nbits - 1)) - 1)
                };
                let sat = |raw: u64| -> u64 {
                    let sh = 64 - wbits;
                    let sv = ((raw << sh) as i64) >> sh; // sign-extend wide source
                    sv.clamp(lo_b, hi_b) as u64
                };
                let mut result = [0u64; 16];
                debug_assert!(*block_lanes != 0 && *src_lanes % *block_lanes == 0);
                for block_base in (0..*src_lanes).step_by(*block_lanes as usize) {
                    let output_base = block_base * 2;
                    for i in 0..*block_lanes {
                        let source_lane = block_base + i;
                        Self::set_lane(
                            &mut result,
                            output_base + i,
                            nbits,
                            sat(Self::get_lane(&v, source_lane, wbits)),
                        );
                        Self::set_lane(
                            &mut result,
                            output_base + *block_lanes + i,
                            nbits,
                            sat(Self::get_lane(&u, source_lane, wbits)),
                        );
                    }
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VWidenExt {
                dst_lo,
                dst_hi,
                src,
                src_elem,
                signed,
                interleave,
            } => {
                let s = Self::read_vec(ctx, *src);
                let nbits = src_elem.bytes() * 8;
                let wbits = nbits * 2;
                let wide_lanes = (1024 / wbits) as u8; // wide lanes per output vector
                let ext = |raw: u64| -> u64 {
                    if *signed {
                        let sh = 64 - nbits;
                        (((raw << sh) as i64) >> sh) as u64
                    } else {
                        raw
                    }
                };
                let mut lo = [0u64; 16];
                let mut hi = [0u64; 16];
                for i in 0..wide_lanes {
                    let (lo_idx, hi_idx) = if *interleave {
                        (i * 2, i * 2 + 1)
                    } else {
                        (i, i + wide_lanes)
                    };
                    Self::set_lane(&mut lo, i, wbits, ext(Self::get_lane(&s, lo_idx, nbits)));
                    Self::set_lane(&mut hi, i, wbits, ext(Self::get_lane(&s, hi_idx, nbits)));
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VCmpToQ {
                dst,
                src1,
                src2,
                cond,
                elem,
                lanes,
                accumulate,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let nbits = elem.bytes() * 8;
                let ebytes = elem.bytes() as usize;
                let sext = |v: u64| -> i64 {
                    let sh = 64 - nbits;
                    ((v << sh) as i64) >> sh
                };
                let mut q = [0u64; 16];
                for lane in 0..*lanes {
                    let av = Self::get_lane(&a, lane, nbits);
                    let bv = Self::get_lane(&b, lane, nbits);
                    let t = match cond {
                        VecCmpCond::Eq => av == bv,
                        VecCmpCond::Ne => av != bv,
                        VecCmpCond::Gt => sext(av) > sext(bv),
                        VecCmpCond::Ge => sext(av) >= sext(bv),
                        VecCmpCond::Lt => sext(av) < sext(bv),
                        VecCmpCond::Le => sext(av) <= sext(bv),
                        VecCmpCond::Gtu => av > bv,
                        VecCmpCond::Geu => av >= bv,
                        VecCmpCond::Ltu => av < bv,
                        VecCmpCond::Leu => av <= bv,
                    };
                    if t {
                        for byte in 0..ebytes {
                            let bit = lane as usize * ebytes + byte;
                            q[bit >> 6] |= 1u64 << (bit & 63);
                        }
                    }
                }
                // Accumulating compares combine the new mask into the existing Q.
                if let Some(combine) = accumulate {
                    let prev = Self::read_vec(ctx, *dst);
                    for w in 0..2 {
                        q[w] = match combine {
                            VLaneOp::And => prev[w] & q[w],
                            VLaneOp::Or => prev[w] | q[w],
                            VLaneOp::Xor => prev[w] ^ q[w],
                            _ => q[w],
                        };
                    }
                }
                Self::write_vec(ctx, *dst, q);
            }

            OpKind::VQFromVAndR {
                dst,
                src1,
                src2,
                oracc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                // vandvrt_acc OR-accumulates into the existing dst Q; otherwise
                // overwrite (start from a clean Q).
                let mut q = if *oracc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                for byte in 0..128usize {
                    let av = Self::get_lane(&a, byte as u8, 8);
                    let bv = Self::get_lane(&b, byte as u8, 8);
                    if (av & bv) != 0 {
                        q[byte >> 6] |= 1u64 << (byte & 63);
                    }
                }
                Self::write_vec(ctx, *dst, q);
            }

            OpKind::VMaskZero {
                dst,
                mask_q,
                src,
                negate,
                oracc,
            } => {
                let m = Self::read_vec(ctx, *mask_q);
                let s = Self::read_vec(ctx, *src);
                // vandqrt_acc OR-accumulates the gated bytes into the existing
                // dst; the plain forms overwrite (unselected bytes -> 0).
                let mut result = if *oracc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                for byte in 0..128usize {
                    let bit = (m[byte >> 6] >> (byte & 63)) & 1 != 0;
                    if bit ^ *negate {
                        let sv = Self::get_lane(&s, byte as u8, 8);
                        if *oracc {
                            let prev = Self::get_lane(&result, byte as u8, 8);
                            Self::set_lane(&mut result, byte as u8, 8, prev | sv);
                        } else {
                            Self::set_lane(&mut result, byte as u8, 8, sv);
                        }
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VLaneCond {
                dst,
                src,
                mask_q,
                elem,
                lanes,
                sub,
                negate,
            } => {
                let x = Self::read_vec(ctx, *dst);
                let u = Self::read_vec(ctx, *src);
                let m = Self::read_vec(ctx, *mask_q);
                let elem_bits = elem.bytes() * 8;
                let ebytes = elem.bytes() as usize;
                let mut result = x;
                for lane in 0..*lanes {
                    let a = Self::get_lane(&x, lane, elem_bits);
                    let b = Self::get_lane(&u, lane, elem_bits);
                    let r = if *sub {
                        a.wrapping_sub(b)
                    } else {
                        a.wrapping_add(b)
                    };
                    let rb = r.to_le_bytes();
                    let base = lane as usize * ebytes;
                    // Per-byte select: each Q bit covering this lane's bytes
                    // chooses op-result vs unchanged dst (fCONDMASK{8,16,32}).
                    for byte in 0..ebytes {
                        let bidx = base + byte;
                        let qb = (m[bidx >> 6] >> (bidx & 63)) & 1 != 0;
                        if qb ^ *negate {
                            Self::set_lane(&mut result, bidx as u8, 8, rb[byte] as u64);
                        }
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VCarry {
                dst,
                src1,
                src2,
                q_inout,
                sub,
                has_cin,
                cin0,
                has_cout,
                sat,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let qin = if *has_cin {
                    Self::read_vec(ctx, *q_inout)
                } else {
                    [0u64; 16]
                };
                let mut out = [0u64; 16];
                let mut qout = [0u64; 16];
                // vaddcarrysat (sat=true) is the only carry form that saturates;
                // its sem (hvx_carry.rs) clamps via `ctx.sat_n(s, 32)`, setting
                // USR:OVF on any clamped lane.
                let mut ovf = false;
                for i in 0..32usize {
                    let av = Self::get_lane(&a, i as u8, 32) as u32;
                    let bv0 = Self::get_lane(&b, i as u8, 32) as u32;
                    let bv = if *sub { !bv0 } else { bv0 };
                    let cin = if *has_cin {
                        let bit = i * 4;
                        ((qin[bit >> 6] >> (bit & 63)) & 1) as u32
                    } else {
                        *cin0 as u32
                    };
                    if *sat {
                        // vaddcarrysat: signed sat_32 of Vu + Vv + cin (no
                        // carry-out). `sub` is never set for the sat form.
                        let s = av as i32 as i64 + bv0 as i32 as i64 + cin as i64;
                        if s < i32::MIN as i64 || s > i32::MAX as i64 {
                            ovf = true;
                        }
                        let clamped = s.clamp(i32::MIN as i64, i32::MAX as i64) as u32;
                        Self::set_lane(&mut out, i as u8, 32, clamped as u64);
                    } else {
                        let full = av as u64 + bv as u64 + cin as u64;
                        Self::set_lane(&mut out, i as u8, 32, full & 0xffff_ffff);
                        let carry = (full >> 32) != 0;
                        if *has_cout {
                            for byte in 0..4 {
                                let bit = i * 4 + byte;
                                if carry {
                                    qout[bit >> 6] |= 1u64 << (bit & 63);
                                }
                            }
                        }
                    }
                }
                Self::write_vec(ctx, *dst, out);
                if *has_cout {
                    Self::write_vec(ctx, *q_inout, qout);
                }
                if *sat && ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VSwap {
                dst_lo,
                dst_hi,
                mask_q,
                src1,
                src2,
            } => {
                let m = Self::read_vec(ctx, *mask_q);
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let mut lo = [0u64; 16];
                let mut hi = [0u64; 16];
                for byte in 0..128usize {
                    let qb = (m[byte >> 6] >> (byte & 63)) & 1 != 0;
                    let uv = Self::get_lane(&u, byte as u8, 8);
                    let vv = Self::get_lane(&v, byte as u8, 8);
                    if qb {
                        Self::set_lane(&mut lo, byte as u8, 8, uv);
                        Self::set_lane(&mut hi, byte as u8, 8, vv);
                    } else {
                        Self::set_lane(&mut lo, byte as u8, 8, vv);
                        Self::set_lane(&mut hi, byte as u8, 8, uv);
                    }
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            // HVX vshufoeb/vshufoeh: even shuffle -> dst_lo, odd shuffle -> dst_hi.
            // out_lo[2i]=src2[2i], out_lo[2i+1]=src1[2i]; out_hi uses sub-lane 2i+1.
            OpKind::VShuffleEOPair {
                dst_lo,
                dst_hi,
                src1,
                src2,
                elem,
            } => {
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let nbits = elem.bytes() * 8;
                let total = (1024 / nbits) as u8;
                let half = total / 2;
                let mut lo = [0u64; 16];
                let mut hi = [0u64; 16];
                for i in 0..half {
                    let e = i * 2;
                    let o = i * 2 + 1;
                    Self::set_lane(&mut lo, i * 2, nbits, Self::get_lane(&v, e, nbits));
                    Self::set_lane(&mut lo, i * 2 + 1, nbits, Self::get_lane(&u, e, nbits));
                    Self::set_lane(&mut hi, i * 2, nbits, Self::get_lane(&v, o, nbits));
                    Self::set_lane(&mut hi, i * 2 + 1, nbits, Self::get_lane(&u, o, nbits));
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            // HVX in-place dual-register byte shuffle/deal: swap Vy.b[k] <-> Vx.b[k+offset].
            OpKind::VShuffleDeal {
                dst_y,
                dst_x,
                amount,
                deal,
            } => {
                let mut vy = Self::read_vec(ctx, *dst_y);
                let mut vx = Self::read_vec(ctx, *dst_x);
                let rt = match amount {
                    SrcOperand::Imm(v) => *v as usize,
                    SrcOperand::Reg(r) => ctx.read_vreg(*r) as usize,
                    _ => 0,
                };
                // shuffle: offset ascending 1..64; deal: descending 64..1.
                let offsets: [usize; 7] = if *deal {
                    [64, 32, 16, 8, 4, 2, 1]
                } else {
                    [1, 2, 4, 8, 16, 32, 64]
                };
                for &offset in offsets.iter() {
                    if rt & offset != 0 {
                        for k in 0..128usize {
                            if k & offset == 0 {
                                let a = Self::get_lane(&vy, k as u8, 8);
                                let b = Self::get_lane(&vx, (k + offset) as u8, 8);
                                Self::set_lane(&mut vy, k as u8, 8, b);
                                Self::set_lane(&mut vx, (k + offset) as u8, 8, a);
                            }
                        }
                    }
                }
                Self::write_vec(ctx, *dst_y, vy);
                Self::write_vec(ctx, *dst_x, vx);
            }

            // HVX vdealvdd: deal-direction byte swap network over a pair (lo=Vv, hi=Vu).
            OpKind::VDealVdd {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                amount,
            } => {
                let mut lo = Self::read_vec(ctx, *src_lo);
                let mut hi = Self::read_vec(ctx, *src_hi);
                let rt = match amount {
                    SrcOperand::Imm(v) => *v as usize,
                    SrcOperand::Reg(r) => ctx.read_vreg(*r) as usize,
                    _ => 0,
                };
                let mut offset = 64usize;
                while offset > 0 {
                    if rt & offset != 0 {
                        for k in 0..128usize {
                            if k & offset == 0 {
                                let a = Self::get_lane(&hi, k as u8, 8);
                                let b = Self::get_lane(&lo, (k + offset) as u8, 8);
                                Self::set_lane(&mut hi, k as u8, 8, b);
                                Self::set_lane(&mut lo, (k + offset) as u8, 8, a);
                            }
                        }
                    }
                    offset >>= 1;
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            // HVX vunpackob/oh: Vxx.<2w>[i] |= ZE(Vu.<w>[i]) << nbits (sequential split).
            OpKind::VUnpackOAcc {
                dst_lo,
                dst_hi,
                src,
                src_elem,
            } => {
                let s = Self::read_vec(ctx, *src);
                let nbits = src_elem.bytes() * 8;
                let wbits = nbits * 2;
                let total = (1024 / nbits as usize); // narrow lanes total
                let half = (total / 2) as u8;
                let mut lo = Self::read_vec(ctx, *dst_lo);
                let mut hi = Self::read_vec(ctx, *dst_hi);
                for i in 0..total as u8 {
                    let add = Self::get_lane(&s, i, nbits) << nbits;
                    if i < half {
                        let cur = Self::get_lane(&lo, i, wbits);
                        Self::set_lane(&mut lo, i, wbits, cur | add);
                    } else {
                        let cur = Self::get_lane(&hi, i - half, wbits);
                        Self::set_lane(&mut hi, i - half, wbits, cur | add);
                    }
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            // HVX vinsertwr: Vx.w[0] = Rt (other words preserved).
            OpKind::VInsertWordR { dst, scalar } => {
                let mut v = Self::read_vec(ctx, *dst);
                let rt = ctx.read_vreg(*scalar) as u32 as u64;
                Self::set_lane(&mut v, 0, 32, rt);
                Self::write_vec(ctx, *dst, v);
            }

            // HVX extractw: Rd = Vu.uw[(Rs & 127) >> 2].
            OpKind::VExtractWord { dst, src, sel } => {
                let v = Self::read_vec(ctx, *src);
                let rs = ctx.read_vreg(*sel) as u32;
                let idx = ((rs & 127) >> 2) as u8;
                let word = Self::get_lane(&v, idx, 32);
                ctx.write_vreg(*dst, word & 0xffff_ffff);
            }

            // HVX vlut4: Vd.h[i] = Rtt.h[(Vu.uh[i] >> 14) & 3].
            OpKind::VLut4 { dst, src, table } => {
                let u = Self::read_vec(ctx, *src);
                let rtt = ctx.read_vreg(*table);
                let mut out = [0u64; 16];
                for i in 0..64u8 {
                    let sel = (Self::get_lane(&u, i, 16) >> 14) & 3;
                    let entry = (rtt >> (sel * 16)) & 0xffff;
                    Self::set_lane(&mut out, i, 16, entry);
                }
                Self::write_vec(ctx, *dst, out);
            }

            // HVX vrotr: Vd.uw[i] = rotate_right(Vu.uw[i], Vv.uw[i] & 0x1f).
            OpKind::VRotr { dst, src, amount } => {
                let u = Self::read_vec(ctx, *src);
                let v = Self::read_vec(ctx, *amount);
                let mut out = [0u64; 16];
                for i in 0..32u8 {
                    let amt = (Self::get_lane(&v, i, 32) & 0x1f) as u32;
                    let val = Self::get_lane(&u, i, 32) as u32;
                    Self::set_lane(&mut out, i, 32, val.rotate_right(amt) as u64);
                }
                Self::write_vec(ctx, *dst, out);
            }

            // HVX vaddububb_sat/vsubububb_sat: Vd.ub = sat_u8(Vu.ub +/- Vv.b).
            OpKind::VAddSubMixedSat {
                dst,
                src1,
                src2,
                sub,
            } => {
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let mut out = [0u64; 16];
                // vaddububb_sat/vsubububb_sat are dedicated; their sem
                // (hvx_addsub.rs) clamps via `ctx.satu_n(r, 8)`, setting USR:OVF
                // on any clamped lane.
                let mut ovf = false;
                for i in 0..128u8 {
                    let a = Self::get_lane(&u, i, 8) as i32; // unsigned byte
                    let b = Self::get_lane(&v, i, 8) as u8 as i8 as i32; // signed byte
                    let r = if *sub { a - b } else { a + b };
                    if r < 0 || r > 255 {
                        ovf = true;
                    }
                    let s = r.clamp(0, 255) as u64;
                    Self::set_lane(&mut out, i, 8, s);
                }
                Self::write_vec(ctx, *dst, out);
                if ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            // HVX vsetq / vsetq2: build a Q vector predicate from a scalar length.
            OpKind::VSetPredQ { dst, scalar, v2 } => {
                let rt = ctx.read_vreg(*scalar) as u32;
                let mut q = [0u64; 16];
                if *v2 {
                    // vsetq2: set bits 0..=((Rt-1) & 127) (Rt==0 -> all 128).
                    let last = (rt.wrapping_sub(1) & 127) as usize;
                    for i in 0..=last {
                        q[i >> 6] |= 1u64 << (i & 63);
                    }
                } else {
                    // vsetq: set the low (Rt & 127) bits.
                    let n = (rt & 127) as usize;
                    for i in 0..n {
                        q[i >> 6] |= 1u64 << (i & 63);
                    }
                }
                Self::write_vec(ctx, *dst, q);
            }

            // HVX shuffeqh/shuffeqw: Q-predicate shrink/shuffle.
            OpKind::VShuffEqQ {
                dst,
                src1,
                src2,
                stride,
            } => {
                let qs = Self::read_vec(ctx, *src1);
                let qt = Self::read_vec(ctx, *src2);
                let qbit = |q: &VecValue, i: usize| (q[i >> 6] >> (i & 63)) & 1 != 0;
                let st = *stride as usize;
                let mut q = [0u64; 16];
                for i in 0..128usize {
                    let bit = if i & st != 0 {
                        qbit(&qs, i - st)
                    } else {
                        qbit(&qt, i)
                    };
                    if bit {
                        q[i >> 6] |= 1u64 << (i & 63);
                    }
                }
                Self::write_vec(ctx, *dst, q);
            }

            // HVX vmpahhsat/vmpauhuhsat/vmpsuhuhsat: saturating halfword mpa pair-scalar.
            OpKind::VMpaHhSat {
                dst,
                src,
                table,
                signed_u,
                signed_t,
                shl,
                sub,
            } => {
                let vx = Self::read_vec(ctx, *dst);
                let vu = Self::read_vec(ctx, *src);
                let rtt = ctx.read_vreg(*table);
                let mut out = [0u64; 16];
                // vmpahhsat/vmpauhuhsat/vmpsuhuhsat are dedicated; their sem
                // (hvx_mpys.rs) clamps via `ctx.sat_n(prod >> 16, 16)`, setting
                // USR:OVF on any clamped lane.
                let mut ovf = false;
                for i in 0..64u8 {
                    let x = Self::get_lane(&vx, i, 16) as u16 as i16 as i64; // Vx.h signed
                    let raw = Self::get_lane(&vu, i, 16) as u16;
                    let u = if *signed_u {
                        raw as i16 as i64
                    } else {
                        raw as i64
                    };
                    let idx = ((raw >> 14) & 3) as u64;
                    let t_raw = ((rtt >> (idx * 16)) & 0xffff) as u16;
                    let t = if *signed_t {
                        t_raw as i16 as i64
                    } else {
                        t_raw as i64
                    };
                    let addend = t << 15;
                    // vmps subtracts the scalar term; vmpa adds it.
                    let prod = ((x * u) << *shl) + if *sub { -addend } else { addend };
                    let v = prod >> 16;
                    if v < -(1i64 << 15) || v > (1i64 << 15) - 1 {
                        ovf = true;
                    }
                    let r = v.clamp(-(1i64 << 15), (1i64 << 15) - 1);
                    Self::set_lane(&mut out, i, 16, r as u64 & 0xffff);
                }
                Self::write_vec(ctx, *dst, out);
                if ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            // HVX vmpyhsat_acc: Vxx.w[i] += sat32(Vu.h[2i/2i+1] * Rt.h[0/1]).
            OpKind::VMpyHsatAcc {
                dst_lo,
                dst_hi,
                src,
                scalar,
            } => {
                let vu = Self::read_vec(ctx, *src);
                let rt = ctx.read_vreg(*scalar) as u32;
                let rt0 = (rt & 0xffff) as u16 as i16 as i64;
                let rt1 = ((rt >> 16) & 0xffff) as u16 as i16 as i64;
                let mut lo = Self::read_vec(ctx, *dst_lo);
                let mut hi = Self::read_vec(ctx, *dst_hi);
                let smin = -(1i64 << 31);
                let smax = (1i64 << 31) - 1;
                // vmpyhsat_acc is dedicated; its sem (hvx_mpyv.rs) clamps via
                // `ctx.sat_n(.., 32)`, setting USR:OVF on any clamped lane.
                let mut ovf = false;
                for i in 0..32u8 {
                    let p0 = (Self::get_lane(&vu, 2 * i, 16) as u16 as i16 as i64) * rt0;
                    let p1 = (Self::get_lane(&vu, 2 * i + 1, 16) as u16 as i16 as i64) * rt1;
                    let a0 = Self::get_lane(&lo, i, 32) as u32 as i32 as i64;
                    let a1 = Self::get_lane(&hi, i, 32) as u32 as i32 as i64;
                    let r0 = a0 + p0;
                    let r1 = a1 + p1;
                    if r0 < smin || r0 > smax || r1 < smin || r1 > smax {
                        ovf = true;
                    }
                    let s0 = r0.clamp(smin, smax);
                    let s1 = r1.clamp(smin, smax);
                    Self::set_lane(&mut lo, i, 32, s0 as u64 & 0xffff_ffff);
                    Self::set_lane(&mut hi, i, 32, s1 as u64 & 0xffff_ffff);
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
                if ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            // HVX vasr_into: shift Vu.w into the running accumulator pair Vxx.
            OpKind::VAsrInto {
                dst_lo,
                dst_hi,
                src,
                amount,
            } => {
                let vu = Self::read_vec(ctx, *src);
                let vv = Self::read_vec(ctx, *amount);
                let mut x0 = Self::read_vec(ctx, *dst_lo); // Vxx.v[0]
                let mut x1 = Self::read_vec(ctx, *dst_hi); // Vxx.v[1]
                for i in 0..32u8 {
                    // fSE32_64(Vu.w[i]) << 32 — Vu.w is SIGN-extended in the sem.
                    let shift = ((Self::get_lane(&vu, i, 32) as u32 as i32 as i64) << 32) as i64;
                    let xlo = Self::get_lane(&x0, i, 32) as u32 as i64; // ZE lo
                    // SE hi: (fSE32_64(x0.w[i]) << 32) | ZE lo (matches sem's get_w<<32).
                    let xhi = (Self::get_lane(&x0, i, 32) as u32 as i32 as i64) << 32;
                    let mask = xhi | xlo;
                    let lomask: i64 = (1i64 << 32) - 1;
                    let vvw = Self::get_lane(&vv, i, 32) as u32 as i32;
                    let count = -(0x40 & vvw) + (vvw & 0x3f);
                    let result: i64 = if count == -0x40 {
                        0
                    } else if count < 0 {
                        let n = (-count) as u32;
                        (shift << n) | (mask & (lomask << n))
                    } else {
                        let n = count as u32;
                        (shift >> n) | (mask & ((lomask as u64 >> n) as i64))
                    };
                    Self::set_lane(&mut x1, i, 32, ((result >> 32) & 0xffff_ffff) as u64);
                    Self::set_lane(&mut x0, i, 32, (result & 0xffff_ffff) as u64);
                }
                Self::write_vec(ctx, *dst_lo, x0);
                Self::write_vec(ctx, *dst_hi, x1);
            }

            // HVX v6mpy: V69 byte-matrix multiply with packed signed-10-bit coeffs.
            OpKind::V6Mpy {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                src2_lo,
                src2_hi,
                horizontal,
                phase,
                acc,
            } => {
                let u0 = Self::read_vec(ctx, *src_lo); // Vuu.v[0]
                let u1 = Self::read_vec(ctx, *src_hi); // Vuu.v[1]
                let cv0 = Self::read_vec(ctx, *src2_lo); // Vvv.v[0] -> c0j
                let cv1 = Self::read_vec(ctx, *src2_hi); // Vvv.v[1] -> c1j
                // unsigned byte k (0..3) of word lane i.
                let ub = |b: &VecValue, i: u8, k: u8| -> i64 {
                    (Self::get_lane(b, i * 4 + k, 8) & 0xff) as i64
                };
                // signed 10-bit coeff j (0..2) of word lane i: lo8 from ub[j], hi2 from ub[3]>>(2j).
                let coeff = |b: &VecValue, i: u8, j: u8| -> i64 {
                    let hi2 = (ub(b, i, 3) >> (2 * j)) & 3;
                    let lo8 = ub(b, i, j);
                    let v10 = (hi2 << 8) | lo8;
                    ((v10 & 0x3ff) << 54) >> 54
                };
                let terms = Self::v6mpy_terms(*horizontal, *phase);
                let mut o0 = if *acc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut o1 = if *acc {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                for i in 0..32u8 {
                    let c = [
                        coeff(&cv0, i, 0),
                        coeff(&cv0, i, 1),
                        coeff(&cv0, i, 2),
                        coeff(&cv1, i, 0),
                        coeff(&cv1, i, 1),
                        coeff(&cv1, i, 2),
                    ];
                    let mut s0 = if *acc {
                        Self::get_lane(&o0, i, 32) as u32 as i32 as i64
                    } else {
                        0
                    };
                    let mut s1 = if *acc {
                        Self::get_lane(&o1, i, 32) as u32 as i32 as i64
                    } else {
                        0
                    };
                    for &(vsel, byte, ci, osel) in terms {
                        let uv = if vsel == 0 { &u0 } else { &u1 };
                        let prod = ub(uv, i, byte) * c[ci as usize];
                        if osel == 0 {
                            s0 = s0.wrapping_add(prod);
                        } else {
                            s1 = s1.wrapping_add(prod);
                        }
                    }
                    Self::set_lane(&mut o0, i, 32, s0 as u64 & 0xffff_ffff);
                    Self::set_lane(&mut o1, i, 32, s1 as u64 & 0xffff_ffff);
                }
                Self::write_vec(ctx, *dst_lo, o0);
                Self::write_vec(ctx, *dst_hi, o1);
            }

            OpKind::VCondMove {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                pred,
                negate,
            } => {
                let p = ctx.read_vreg(*pred) & 1;
                let take = if *negate { p == 0 } else { p != 0 };
                if take {
                    let lo = Self::read_vec(ctx, *src_lo);
                    Self::write_vec(ctx, *dst_lo, lo);
                    if let Some(hi) = dst_hi {
                        let hv = Self::read_vec(ctx, *src_hi);
                        Self::write_vec(ctx, *hi, hv);
                    }
                }
                // CANCEL (no write) when the condition is false.
            }

            OpKind::VPrefixSumQ {
                dst,
                mask_q,
                elem,
                lanes,
            } => {
                let m = Self::read_vec(ctx, *mask_q);
                let elem_bits = elem.bytes() * 8;
                let ebytes = elem.bytes() as usize;
                let mut result = [0u64; 16];
                let mut acc: u64 = 0;
                for lane in 0..*lanes {
                    let base = lane as usize * ebytes;
                    for byte in 0..ebytes {
                        let bidx = base + byte;
                        acc = acc.wrapping_add((m[bidx >> 6] >> (bidx & 63)) & 1);
                    }
                    Self::set_lane(&mut result, lane, elem_bits, acc);
                }
                Self::write_vec(ctx, *dst, result);
            }

            // HVX histogram family. Read-modify-writes the WHOLE V0..V31 register
            // file (treated as a 32 x 128-byte bin matrix), tallying values from
            // the 128-byte input vector (re-read from the `.tmp` load's address in
            // guest memory). Ported exactly from sem/hvx_hist.rs.
            OpKind::VHist {
                input,
                aligned,
                mask_q,
                use_q,
                imm_match,
                sat,
                kind,
            } => {
                // 1) Read the 128 input bytes from memory at the .tmp address.
                let mut ea = self.compute_address(ctx, input);
                if *aligned {
                    ea &= !127u64;
                }
                let mut inp = [0u8; 128];
                memory.read(ea, &mut inp)?;

                // 2) Read the WHOLE V file into a 32 x 128-byte bin matrix.
                let mut file = [[0u8; 128]; 32];
                for r in 0..32u8 {
                    let v = Self::read_vec(ctx, VReg::Arch(ArchReg::Hexagon(HexagonReg::V(r))));
                    for w in 0..16usize {
                        file[r as usize][w * 8..w * 8 + 8].copy_from_slice(&v[w].to_le_bytes());
                    }
                }

                // q-mask (vector-byte predicate bits) for the q-forms.
                let qv = if *use_q {
                    Some(Self::read_vec(ctx, *mask_q))
                } else {
                    None
                };
                // Q layout in a VecValue: bit i lives in lane (i>>6), bit (i&63).
                let qbit = |q: &VecValue, i: usize| -> bool { (q[i >> 6] >> (i & 63)) & 1 != 0 };
                let get_uh = |f: &[[u8; 128]; 32], reg: usize, i: usize| -> u32 {
                    u16::from_le_bytes([f[reg][i * 2], f[reg][i * 2 + 1]]) as u32
                };
                let set_uh = |f: &mut [[u8; 128]; 32], reg: usize, i: usize, val: u32| {
                    f[reg][i * 2..i * 2 + 2].copy_from_slice(&(val as u16).to_le_bytes());
                };
                let get_uw = |f: &[[u8; 128]; 32], reg: usize, i: usize| -> u32 {
                    u32::from_le_bytes([
                        f[reg][i * 4],
                        f[reg][i * 4 + 1],
                        f[reg][i * 4 + 2],
                        f[reg][i * 4 + 3],
                    ])
                };
                let set_uw = |f: &mut [[u8; 128]; 32], reg: usize, i: usize, val: u32| {
                    f[reg][i * 4..i * 4 + 4].copy_from_slice(&val.to_le_bytes());
                };

                // 3) Run the bin-update loop for this family.
                match *kind {
                    // vhist / vhistq: 8 lanes x 16 bytes -> uh bins, += 1.
                    0 => {
                        for lane in 0..8usize {
                            for i in 0..16usize {
                                if let Some(ref q) = qv {
                                    if !qbit(q, 16 * lane + i) {
                                        continue;
                                    }
                                }
                                let value = inp[16 * lane + i] as usize;
                                let regno = value >> 3;
                                let element = value & 7;
                                let idx = 8 * lane + element;
                                let cur = get_uh(&file, regno, idx);
                                set_uh(&mut file, regno, idx, cur.wrapping_add(1) & 0xffff);
                            }
                        }
                    }
                    // vwhist128 family: 64 halfwords -> uw bins, += weight.
                    1 => {
                        for i in 0..64usize {
                            let bucket = inp[2 * i] as usize;
                            let weight = inp[2 * i + 1] as u32;
                            let vindex = (bucket >> 3) & 0x1f;
                            let elindex = ((i >> 1) & !3) | ((bucket >> 1) & 3);
                            let mut cond = true;
                            if let Some(u) = imm_match {
                                cond &= (bucket & 1) as u8 == *u;
                            }
                            if let Some(ref q) = qv {
                                cond &= qbit(q, 2 * i);
                            }
                            if cond {
                                let cur = get_uw(&file, vindex, elindex);
                                set_uw(&mut file, vindex, elindex, cur.wrapping_add(weight));
                            }
                        }
                    }
                    // vwhist256 family: 64 halfwords -> uh bins, += weight (opt sat).
                    _ => {
                        for i in 0..64usize {
                            let bucket = inp[2 * i] as usize;
                            let weight = inp[2 * i + 1] as u32;
                            let vindex = (bucket >> 3) & 0x1f;
                            let elindex = (i & !7) | (bucket & 7);
                            let cond = match qv {
                                Some(ref q) => qbit(q, 2 * i),
                                None => true,
                            };
                            if cond {
                                let sum = get_uh(&file, vindex, elindex).wrapping_add(weight);
                                let val = if *sat { sum.min(0xffff) } else { sum & 0xffff };
                                set_uh(&mut file, vindex, elindex, val);
                            }
                        }
                    }
                }

                // 4) Write the WHOLE V file back.
                for r in 0..32u8 {
                    let mut v = [0u64; 16];
                    for w in 0..16usize {
                        v[w] = u64::from_le_bytes([
                            file[r as usize][w * 8],
                            file[r as usize][w * 8 + 1],
                            file[r as usize][w * 8 + 2],
                            file[r as usize][w * 8 + 3],
                            file[r as usize][w * 8 + 4],
                            file[r as usize][w * 8 + 5],
                            file[r as usize][w * 8 + 6],
                            file[r as usize][w * 8 + 7],
                        ]);
                    }
                    Self::write_vec(ctx, VReg::Arch(ArchReg::Hexagon(HexagonReg::V(r))), v);
                }
            }

            OpKind::VBlend {
                dst,
                mask_q,
                src_true,
                src_false,
            } => {
                let m = Self::read_vec(ctx, *mask_q);
                let t = Self::read_vec(ctx, *src_true);
                let f = Self::read_vec(ctx, *src_false);
                let mut result = [0u64; 16];
                for byte in 0..128usize {
                    let bit_set = (m[byte >> 6] >> (byte & 63)) & 1 != 0;
                    let src = if bit_set { &t } else { &f };
                    Self::set_lane(
                        &mut result,
                        byte as u8,
                        8,
                        Self::get_lane(src, byte as u8, 8),
                    );
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VShiftV {
                dst,
                src,
                amount,
                elem,
                lanes,
                kind,
            } => {
                let s = Self::read_vec(ctx, *src);
                let amt = Self::read_vec(ctx, *amount);
                let nbits = elem.bytes() * 8;
                let n_amt = nbits.trailing_zeros() + 1; // 16->5, 32->6
                let mut result = [0u64; 16];
                for i in 0..*lanes {
                    let raw = Self::get_lane(&s, i, nbits);
                    // sign-extend the low n_amt bits of the amount lane.
                    let araw = Self::get_lane(&amt, i, nbits) & ((1u64 << n_amt) - 1);
                    let sh = 64 - n_amt;
                    let shamt = (((araw << sh) as i64) >> sh) as i32;
                    let sext = |v: u64| -> i64 {
                        let sh = 64 - nbits;
                        ((v << sh) as i64) >> sh
                    };
                    let out: u64 = match kind {
                        VShiftVKind::AshiftL => {
                            let sa = sext(raw);
                            if shamt >= 0 {
                                (sa << shamt) as u64
                            } else {
                                (sa >> (-shamt)) as u64
                            }
                        }
                        VShiftVKind::AshiftR => {
                            let sa = sext(raw);
                            if shamt >= 0 {
                                (sa >> shamt) as u64
                            } else {
                                (sa << (-shamt)) as u64
                            }
                        }
                        VShiftVKind::LshiftR => {
                            if shamt >= 0 {
                                raw >> shamt
                            } else {
                                raw << (-shamt)
                            }
                        }
                    };
                    Self::set_lane(&mut result, i, nbits, out);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VMulShiftSat {
                dst,
                src1,
                src2,
                src_elem,
                lanes,
                signed1,
                signed2,
                shift_left,
                round,
                sat_bits,
                out_shift,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let nbits = src_elem.bytes() * 8;
                let ext = |raw: u64, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - nbits;
                        ((raw << sh) as i64) >> sh
                    } else {
                        raw as i64
                    }
                };
                let mut result = [0u64; 16];
                for i in 0..*lanes {
                    let mut p = ext(Self::get_lane(&a, i, nbits), *signed1)
                        .wrapping_mul(ext(Self::get_lane(&b, i, nbits), *signed2));
                    p <<= *shift_left;
                    if *round {
                        p += 1i64 << (*out_shift - 1);
                    }
                    if *sat_bits != 0 {
                        let lo = -(1i64 << (*sat_bits - 1));
                        let hi = (1i64 << (*sat_bits - 1)) - 1;
                        p = p.clamp(lo, hi);
                    }
                    Self::set_lane(&mut result, i, nbits, (p >> *out_shift) as u64);
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VNarrowShiftSat {
                dst,
                src_lo,
                src_hi,
                src_elem,
                amount,
                arith,
                round,
                sat,
                set_ovf,
            } => {
                let lo_src = Self::read_vec(ctx, *src_lo);
                let hi_src = Self::read_vec(ctx, *src_hi);
                let wbits = src_elem.bytes() * 8; // wide source element bits
                let nbits = wbits / 2; // narrow output element bits
                let wide_lanes = (1024 / wbits) as u8;
                // Rt-sourced shift amounts are masked to narrow_bits-1 bits
                // (sem: `rt & 0xF` for word->half, `rt & 0x7` for half->byte);
                // immediates (vround/vsat) are used verbatim.
                let shamt: u32 = match amount {
                    SrcOperand::Reg(r) => (ctx.read_vreg(*r) as u32) & (nbits - 1),
                    SrcOperand::Imm(v) | SrcOperand::Imm64(v) => *v as u32,
                    _ => 0,
                };
                // Extend a wide lane to i64 per signedness.
                let ext = |raw: u64| -> i64 {
                    if *arith {
                        let sh = 64 - wbits;
                        ((raw << sh) as i64) >> sh
                    } else {
                        raw as i64
                    }
                };
                // Shift-round one wide lane and saturate to the narrow width.
                // Returns (narrowed value, clamped?) where `clamped` mirrors the
                // sem's `ctx.sat_n`/`ctx.satu_n` overflow flag (value outside the
                // target range BEFORE clamping).
                let narrow = |raw: u64| -> (u64, bool) {
                    let mut v = ext(raw);
                    if *round && shamt > 0 {
                        v += 1i64 << (shamt - 1);
                    }
                    v >>= shamt;
                    match sat {
                        // signed narrow
                        1 => {
                            let lo = -(1i64 << (nbits - 1));
                            let hi = (1i64 << (nbits - 1)) - 1;
                            let c = v < lo || v > hi;
                            ((v.clamp(lo, hi) as u64) & ((1u64 << nbits) - 1), c)
                        }
                        // unsigned narrow
                        2 => {
                            let hi = (1i64 << nbits) - 1;
                            let c = v < 0 || v > hi;
                            ((v.clamp(0, hi) as u64) & ((1u64 << nbits) - 1), c)
                        }
                        // truncate
                        _ => ((v as u64) & ((1u64 << nbits) - 1), false),
                    }
                };
                let mut result = [0u64; 16];
                let mut ovf = false;
                for i in 0..wide_lanes {
                    // even/low sub-lane <- src_lo (Vv); odd/high <- src_hi (Vu)
                    let (lv, lc) = narrow(Self::get_lane(&lo_src, i, wbits));
                    Self::set_lane(&mut result, 2 * i, nbits, lv);
                    let (hv, hc) = narrow(Self::get_lane(&hi_src, i, wbits));
                    Self::set_lane(&mut result, 2 * i + 1, nbits, hv);
                    ovf |= lc | hc;
                }
                Self::write_vec(ctx, *dst, result);
                if *set_ovf && ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VSatDW {
                dst,
                src_lo,
                src_hi,
            } => {
                let lo = Self::read_vec(ctx, *src_lo);
                let hi = Self::read_vec(ctx, *src_hi);
                let mut result = [0u64; 16];
                // vsatdw is dedicated; its sem (hvx_round.rs) clamps via
                // `ctx.sat_n(val, 32)`, which sets USR:OVF on any clamped lane.
                let mut ovf = false;
                for i in 0..32u8 {
                    let h = Self::get_lane(&hi, i, 32) as i32 as i64; // sign-extended high word
                    let l = Self::get_lane(&lo, i, 32); // zero-extended low word
                    let val = (h << 32) | (l as i64);
                    if val < i32::MIN as i64 || val > i32::MAX as i64 {
                        ovf = true;
                    }
                    let s = val.clamp(i32::MIN as i64, i32::MAX as i64) as i32 as u32;
                    Self::set_lane(&mut result, i, 32, s as u64);
                }
                Self::write_vec(ctx, *dst, result);
                if ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VNarrowShiftV {
                dst,
                src_lo,
                src_hi,
                amount,
                src_elem,
                arith,
                round,
            } => {
                let lo_src = Self::read_vec(ctx, *src_lo);
                let hi_src = Self::read_vec(ctx, *src_hi);
                let amt = Self::read_vec(ctx, *amount);
                let wbits = src_elem.bytes() * 8;
                let nbits = wbits / 2;
                let wide_lanes = (1024 / wbits) as u8;
                let ext = |raw: u64| -> i64 {
                    if *arith {
                        let sh = 64 - wbits;
                        ((raw << sh) as i64) >> sh
                    } else {
                        raw as i64
                    }
                };
                // amount sub-lanes are narrow-width; mask to log2(narrow_bits).
                let amask = nbits - 1;
                // vasrv* always saturate to the unsigned narrow range via
                // `ctx.satu_n` (hvx_round.rs), so every clamped lane sets USR:OVF.
                let narrow = |raw: u64, s: u32| -> (u64, bool) {
                    let mut v = ext(raw);
                    if *round && s > 0 {
                        v += 1i64 << (s - 1);
                    }
                    v >>= s;
                    let hi = (1i64 << nbits) - 1;
                    let c = v < 0 || v > hi;
                    ((v.clamp(0, hi) as u64) & ((1u64 << nbits) - 1), c)
                };
                let mut result = [0u64; 16];
                let mut ovf = false;
                for i in 0..wide_lanes {
                    let s0 = (Self::get_lane(&amt, 2 * i, nbits) as u32) & amask;
                    let (v0, c0) = narrow(Self::get_lane(&lo_src, i, wbits), s0);
                    Self::set_lane(&mut result, 2 * i, nbits, v0);
                    let s1 = (Self::get_lane(&amt, 2 * i + 1, nbits) as u32) & amask;
                    let (v1, c1) = narrow(Self::get_lane(&hi_src, i, wbits), s1);
                    Self::set_lane(&mut result, 2 * i + 1, nbits, v1);
                    ovf |= c0 | c1;
                }
                Self::write_vec(ctx, *dst, result);
                if ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VPairPairReduceMul {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                src2_lo,
                src2_hi,
                narrow_elem,
                out_elem,
                signed1,
                signed2,
            } => {
                let u0 = Self::read_vec(ctx, *src_lo);
                let u1 = Self::read_vec(ctx, *src_hi);
                let v0 = Self::read_vec(ctx, *src2_lo);
                let v1 = Self::read_vec(ctx, *src2_hi);
                let nbits = narrow_elem.bytes() * 8;
                let obits = out_elem.bytes() * 8;
                let olanes = (1024 / obits) as u8;
                let ex = |v: u64, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - nbits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                let mut lo = [0u64; 16];
                let mut hi = [0u64; 16];
                for i in 0..olanes {
                    let plo = ex(Self::get_lane(&u0, i * 2, nbits), *signed1)
                        * ex(Self::get_lane(&v0, i * 2, nbits), *signed2)
                        + ex(Self::get_lane(&u1, i * 2, nbits), *signed1)
                            * ex(Self::get_lane(&v1, i * 2, nbits), *signed2);
                    let phi = ex(Self::get_lane(&u0, i * 2 + 1, nbits), *signed1)
                        * ex(Self::get_lane(&v0, i * 2 + 1, nbits), *signed2)
                        + ex(Self::get_lane(&u1, i * 2 + 1, nbits), *signed1)
                            * ex(Self::get_lane(&v1, i * 2 + 1, nbits), *signed2);
                    Self::set_lane(&mut lo, i, obits, plo as u64);
                    Self::set_lane(&mut hi, i, obits, phi as u64);
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VPairReduceMul {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                src2,
                pair_elem,
                rt_elem,
                out_elem,
                signed1,
                signed2,
                acc,
            } => {
                let u0 = Self::read_vec(ctx, *src_lo);
                let u1 = Self::read_vec(ctx, *src_hi);
                let r = Self::read_vec(ctx, *src2);
                let pbits = pair_elem.bytes() * 8;
                let rbits = rt_elem.bytes() * 8;
                let obits = out_elem.bytes() * 8;
                let olanes = (1024 / obits) as u8;
                let mut lo = if *acc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut hi = if *acc {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                let exg = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - bits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                let rt = |k: u8| exg(Self::get_lane(&r, k, rbits), rbits, *signed2);
                for i in 0..olanes {
                    let plo = exg(Self::get_lane(&u0, i * 2, pbits), pbits, *signed1) * rt(0)
                        + exg(Self::get_lane(&u1, i * 2, pbits), pbits, *signed1) * rt(1);
                    let phi = exg(Self::get_lane(&u0, i * 2 + 1, pbits), pbits, *signed1) * rt(2)
                        + exg(Self::get_lane(&u1, i * 2 + 1, pbits), pbits, *signed1) * rt(3);
                    let alo = if *acc {
                        Self::get_lane(&lo, i, obits) as i64
                    } else {
                        0
                    };
                    let ahi = if *acc {
                        Self::get_lane(&hi, i, obits) as i64
                    } else {
                        0
                    };
                    Self::set_lane(&mut lo, i, obits, alo.wrapping_add(plo) as u64);
                    Self::set_lane(&mut hi, i, obits, ahi.wrapping_add(phi) as u64);
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VSlideReduceMul {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                src2,
                src_elem,
                rt_elem,
                out_elem,
                mode,
                signed1,
                signed2,
                sat,
                set_ovf,
                acc,
            } => {
                let v0 = Self::read_vec(ctx, *src_lo);
                let v1 = Self::read_vec(ctx, *src_hi);
                let r = Self::read_vec(ctx, *src2);
                let nbits = src_elem.bytes() * 8; // multiplicand width
                let rbits = rt_elem.bytes() * 8; // Rt sub-lane width
                let obits = out_elem.bytes() * 8; // output width
                let olanes = (1024 / obits) as u8;
                let ext = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - bits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                // narrow multiplicand lane reader
                let m = |vec: &VecValue, lane: u8| {
                    ext(Self::get_lane(vec, lane, nbits), nbits, *signed1)
                };
                // Rt sub-lane reader (from the I32-broadcast `src2`)
                let rt = |lane: u8| ext(Self::get_lane(&r, lane, rbits), rbits, *signed2);
                let mut lo = if *acc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut hi = if *acc && *mode != 2 {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                // Returns (saturated value, clamped?). Only mode 2 saturates; its
                // sem (hvx_rmpy.rs) clamps via `ctx.sat_n`, flagging USR:OVF.
                let satn = |s: i64| -> (i64, bool) {
                    if *sat && obits < 64 {
                        let l = -(1i64 << (obits - 1));
                        let h = (1i64 << (obits - 1)) - 1;
                        (s.clamp(l, h), s < l || s > h)
                    } else {
                        (s, false)
                    }
                };
                let mut ovf = false;
                for i in 0..olanes {
                    let n0 = (2 * i) as u8; // narrow lane 2i
                    let n1 = (2 * i + 1) as u8; // narrow lane 2i+1
                    let rb0 = rt(n0); // Rt[(2i)%subs] via broadcast
                    let rb1 = rt(n1); // Rt[(2i+1)%subs]
                    match *mode {
                        0 => {
                            // _dv 2-tap sliding (pair -> pair)
                            let alo = if *acc {
                                Self::get_lane(&lo, i, obits) as i64
                            } else {
                                0
                            };
                            let s0 = alo
                                .wrapping_add(m(&v0, n0).wrapping_mul(rb0))
                                .wrapping_add(m(&v0, n1).wrapping_mul(rb1));
                            Self::set_lane(&mut lo, i, obits, s0 as u64);
                            let ahi = if *acc {
                                Self::get_lane(&hi, i, obits) as i64
                            } else {
                                0
                            };
                            let s1 = ahi
                                .wrapping_add(m(&v0, n1).wrapping_mul(rb0))
                                .wrapping_add(m(&v1, n0).wrapping_mul(rb1));
                            Self::set_lane(&mut hi, i, obits, s1 as u64);
                        }
                        1 => {
                            // vtmpy 3-tap sliding with a free (un-multiplied) addend tap
                            let alo = if *acc {
                                Self::get_lane(&lo, i, obits) as i64
                            } else {
                                0
                            };
                            let s0 = alo
                                .wrapping_add(m(&v0, n0).wrapping_mul(rb0))
                                .wrapping_add(m(&v0, n1).wrapping_mul(rb1))
                                .wrapping_add(m(&v1, n0));
                            Self::set_lane(&mut lo, i, obits, s0 as u64);
                            let ahi = if *acc {
                                Self::get_lane(&hi, i, obits) as i64
                            } else {
                                0
                            };
                            let s1 = ahi
                                .wrapping_add(m(&v0, n1).wrapping_mul(rb0))
                                .wrapping_add(m(&v1, n0).wrapping_mul(rb1))
                                .wrapping_add(m(&v1, n1));
                            Self::set_lane(&mut hi, i, obits, s1 as u64);
                        }
                        _ => {
                            // mode 2: pair -> single, straddle, saturated. Rt taps are
                            // fixed sub-lanes 0/1 (Rt.h[0], Rt.h[1]) read from the
                            // I32-broadcast src2.
                            let acc_v = if *acc {
                                ext(Self::get_lane(&lo, i, obits), obits, true)
                            } else {
                                0
                            };
                            let s = acc_v
                                .wrapping_add(m(&v0, n1).wrapping_mul(rt(0)))
                                .wrapping_add(m(&v1, n0).wrapping_mul(rt(1)));
                            let (sv, c) = satn(s);
                            ovf |= c;
                            Self::set_lane(&mut lo, i, obits, sv as u64);
                        }
                    }
                }
                Self::write_vec(ctx, *dst_lo, lo);
                if *mode != 2 {
                    Self::write_vec(ctx, *dst_hi, hi);
                }
                if *set_ovf && ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VRotReduceMulPair {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                src2,
                src_elem,
                rt_elem,
                out_elem,
                imm,
                mode,
                signed1,
                signed2,
                acc,
                abs_diff,
            } => {
                let v0 = Self::read_vec(ctx, *src_lo);
                let v1 = Self::read_vec(ctx, *src_hi);
                let r = Self::read_vec(ctx, *src2);
                let nbits = src_elem.bytes() * 8; // multiplicand width
                let rbits = rt_elem.bytes() * 8; // Rt sub-lane width
                let obits = out_elem.bytes() * 8; // output width (I32)
                let olanes = (1024 / obits) as u8;
                let ext = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - bits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                // narrow multiplicand lane reader
                let m = |vec: &VecValue, lane: u8| {
                    ext(Self::get_lane(vec, lane, nbits), nbits, *signed1)
                };
                // Rt sub-lane reader (from the I32-broadcast `src2`)
                let rt = |lane: u8| ext(Self::get_lane(&r, lane, rbits), rbits, *signed2);
                let mut lo = if *acc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut hi = if *acc {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                // per-tap kernel: mul (a*b) or sum-of-abs-diff (|a-b|).
                let kern = |a: i64, b: i64| -> i64 {
                    if *abs_diff {
                        (a - b).abs()
                    } else {
                        a.wrapping_mul(b)
                    }
                };
                let im = (*imm as usize) & 1;
                for i in 0..olanes {
                    match *mode {
                        0 => {
                            // byte window, #u1 source-select + Rt byte rotate by -imm.
                            let base = (i as u8) * 4;
                            // sel = imm ? src_hi : src_lo (taps 0 and 2 of dst_lo/hi)
                            let sel: &VecValue = if im != 0 { &v1 } else { &v0 };
                            // rb(n) = Rt.byte[(n - imm) & 3]
                            let rb = |n: usize| rt(((n.wrapping_sub(im)) & 3) as u8);
                            let alo = if *acc {
                                ext(Self::get_lane(&lo, i, obits), obits, true)
                            } else {
                                0
                            };
                            let s0 = alo
                                .wrapping_add(kern(m(sel, base), rb(0)))
                                .wrapping_add(kern(m(&v0, base + 1), rb(1)))
                                .wrapping_add(kern(m(&v0, base + 2), rb(2)))
                                .wrapping_add(kern(m(&v0, base + 3), rb(3)));
                            Self::set_lane(&mut lo, i, obits, s0 as u64);
                            let ahi = if *acc {
                                ext(Self::get_lane(&hi, i, obits), obits, true)
                            } else {
                                0
                            };
                            let s1 = ahi
                                .wrapping_add(kern(m(&v1, base), rb(2)))
                                .wrapping_add(kern(m(&v1, base + 1), rb(3)))
                                .wrapping_add(kern(m(sel, base + 2), rb(0)))
                                .wrapping_add(kern(m(&v0, base + 3), rb(1)));
                            Self::set_lane(&mut hi, i, obits, s1 as u64);
                        }
                        _ => {
                            // mode 1: vdsaduh halfword window (imm ignored).
                            // r0 = Rt.uh[0] = t.h[0]; r1 = Rt.uh[1] = t.h[1].
                            let r0 = rt(0);
                            let r1 = rt(1);
                            let n0 = (i as u8) * 2; // halfword lane 2i
                            let n1 = (i as u8) * 2 + 1; // halfword lane 2i+1
                            let alo = if *acc {
                                ext(Self::get_lane(&lo, i, obits), obits, true)
                            } else {
                                0
                            };
                            let s0 = alo
                                .wrapping_add(kern(m(&v0, n0), r0))
                                .wrapping_add(kern(m(&v0, n1), r1));
                            Self::set_lane(&mut lo, i, obits, s0 as u64);
                            let ahi = if *acc {
                                ext(Self::get_lane(&hi, i, obits), obits, true)
                            } else {
                                0
                            };
                            let s1 = ahi
                                .wrapping_add(kern(m(&v0, n1), r0))
                                .wrapping_add(kern(m(&v1, n0), r1));
                            Self::set_lane(&mut hi, i, obits, s1 as u64);
                        }
                    }
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VMulSubLane {
                dst,
                src1,
                src2,
                out_elem,
                sub_elem,
                odd,
                signed1,
                signed2,
                acc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let obits = out_elem.bytes() * 8;
                let sbits = sub_elem.bytes() * 8;
                let olanes = (1024 / obits) as u8;
                let ratio = (obits / sbits) as u8;
                let mut out = if *acc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                let exts = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - bits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                for i in 0..olanes {
                    let s1 = exts(Self::get_lane(&a, i, obits), obits, *signed1);
                    let sub_idx = i * ratio + if *odd { 1 } else { 0 };
                    let s2 = exts(Self::get_lane(&b, sub_idx, sbits), sbits, *signed2);
                    let accv = if *acc {
                        Self::get_lane(&out, i, obits) as i64
                    } else {
                        0
                    };
                    Self::set_lane(
                        &mut out,
                        i,
                        obits,
                        accv.wrapping_add(s1.wrapping_mul(s2)) as u64,
                    );
                }
                Self::write_vec(ctx, *dst, out);
            }

            OpKind::VMulSubLaneFrac {
                dst,
                src1,
                src2,
                out_elem,
                sub_elem,
                odd,
                signed1,
                signed2,
                shl1,
                rnd,
                shift,
                sat,
                acc,
                rnd2,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let d = if *acc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                let obits = out_elem.bytes() * 8;
                let sbits = sub_elem.bytes() * 8;
                let olanes = (1024 / obits) as u8;
                let ratio = (obits / sbits) as u8;
                let exf = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - bits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                let mut out = [0u64; 16];
                for i in 0..olanes {
                    let s1 = exf(Self::get_lane(&a, i, obits), obits, *signed1);
                    let sub_idx = i * ratio + if *odd { 1 } else { 0 };
                    let s2 = exf(Self::get_lane(&b, sub_idx, sbits), sbits, *signed2);
                    let mut p = s1.wrapping_mul(s2);
                    if *shl1 {
                        p <<= 1;
                    }
                    if *acc {
                        // sacc: add the existing full-precision dst lane before shifting.
                        p += exf(Self::get_lane(&d, i, obits), obits, true);
                    }
                    if *rnd2 {
                        p = ((p >> (*shift - 1)) + 1) >> 1;
                    } else {
                        if *rnd && *shift > 0 {
                            p += 1i64 << (*shift - 1);
                        }
                        p >>= *shift;
                    }
                    if *sat && obits < 64 {
                        let lo = -(1i64 << (obits - 1));
                        let hi = (1i64 << (obits - 1)) - 1;
                        p = p.clamp(lo, hi);
                    }
                    Self::set_lane(&mut out, i, obits, p as u64);
                }
                Self::write_vec(ctx, *dst, out);
            }

            OpKind::VMulSubLaneSh {
                dst,
                src1,
                src2,
                out_elem,
                sub_elem,
                odd1,
                odd2,
                signed1,
                signed2,
                shl,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let obits = out_elem.bytes() * 8;
                let sbits = sub_elem.bytes() * 8;
                let olanes = (1024 / obits) as u8;
                let ratio = (obits / sbits) as u8;
                let exts = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - bits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                let mut out = [0u64; 16];
                for i in 0..olanes {
                    let i1 = i * ratio + if *odd1 { 1 } else { 0 };
                    let i2 = i * ratio + if *odd2 { 1 } else { 0 };
                    let s1 = exts(Self::get_lane(&a, i1, sbits), sbits, *signed1);
                    let s2 = exts(Self::get_lane(&b, i2, sbits), sbits, *signed2);
                    let p = s1.wrapping_mul(s2).wrapping_shl(*shl as u32);
                    Self::set_lane(&mut out, i, obits, p as u64);
                }
                Self::write_vec(ctx, *dst, out);
            }

            OpKind::VMulWord64Pair {
                dst_lo,
                dst_hi,
                src1,
                src2,
                mode,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                // word i: 32-bit lane; src2 sub-halfwords at 2i (even/uh0) and 2i+1 (odd/h1).
                let mut lo = [0u64; 16];
                let mut hi = [0u64; 16];
                let old_lo = if *mode == 1 {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let old_hi = if *mode == 1 {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                for i in 0..32u8 {
                    let uw = Self::get_lane(&a, i, 32) as u32 as i32 as i64;
                    if *mode == 0 {
                        // vmpyewuh_64: src2.uh[2i] (low, unsigned).
                        let uh0 = (Self::get_lane(&b, i, 32) as u32 & 0xffff) as i64;
                        let prod = uw * uh0;
                        Self::set_lane(&mut hi, i, 32, (prod >> 16) as u32 as u64);
                        Self::set_lane(&mut lo, i, 32, (prod << 16) as u32 as u64);
                    } else {
                        // vmpyowh_64_acc: src2.h[2i+1] (high, signed), accumulate dst_hi.
                        let h1 = ((Self::get_lane(&b, i, 32) as u32) >> 16) as u16 as i16 as i64;
                        let acc_hi = Self::get_lane(&old_hi, i, 32) as u32 as i32 as i64;
                        let prod = uw * h1 + acc_hi;
                        Self::set_lane(&mut hi, i, 32, (prod >> 16) as u32 as u64);
                        let lo_h0 = ((Self::get_lane(&old_lo, i, 32) as u32) >> 16) & 0xffff;
                        let lo_h1 = (prod as u32) & 0xffff;
                        Self::set_lane(&mut lo, i, 32, ((lo_h1 << 16) | lo_h0) as u64);
                    }
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VMulEvenWiden {
                dst,
                src1,
                src2,
                src_elem,
                signed1,
                signed2,
                acc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let nbits = src_elem.bytes() * 8;
                let wbits = nbits * 2;
                let olanes = (1024 / wbits) as u8;
                let mut out = if *acc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                let ext = |v: u64, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - nbits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                for i in 0..olanes {
                    let p = ext(Self::get_lane(&a, i * 2, nbits), *signed1)
                        .wrapping_mul(ext(Self::get_lane(&b, i * 2, nbits), *signed2));
                    let acc_v = if *acc {
                        Self::get_lane(&out, i, wbits) as i64
                    } else {
                        0
                    };
                    Self::set_lane(&mut out, i, wbits, acc_v.wrapping_add(p) as u64);
                }
                Self::write_vec(ctx, *dst, out);
            }

            OpKind::VReduceMul {
                dst,
                src1,
                src2,
                src1_elem,
                src2_elem,
                out_elem,
                taps,
                signed1,
                signed2,
                sat,
                set_ovf,
                acc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let n1 = src1_elem.bytes() * 8;
                let n2 = src2_elem.bytes() * 8;
                let obits = out_elem.bytes() * 8;
                let olanes = (1024 / obits) as u8;
                let mut out = if *acc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                let ext = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let shift = 64 - bits;
                        ((v << shift) as i64) >> shift
                    } else {
                        v as i64
                    }
                };
                let mut ovf = false;
                for i in 0..olanes {
                    let mut s: i64 = if *acc {
                        // accumulator low `obits` bits, sign-extended for saturating sum.
                        ext(Self::get_lane(&out, i, obits), obits, true)
                    } else {
                        0
                    };
                    for k in 0..*taps {
                        let idx = i * *taps + k;
                        s = s.wrapping_add(
                            ext(Self::get_lane(&a, idx, n1), n1, *signed1).wrapping_mul(ext(
                                Self::get_lane(&b, idx, n2),
                                n2,
                                *signed2,
                            )),
                        );
                    }
                    if *sat && obits < 64 {
                        let lo = -(1i64 << (obits - 1));
                        let hi = (1i64 << (obits - 1)) - 1;
                        // The saturating reduce opcodes clamp via `ctx.sat_n`,
                        // which flags USR:OVF on any clamped lane.
                        if s < lo || s > hi {
                            ovf = true;
                        }
                        s = s.clamp(lo, hi);
                    }
                    Self::set_lane(&mut out, i, obits, s as u64);
                }
                Self::write_vec(ctx, *dst, out);
                if *set_ovf && ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VMov { dst, src, width } => {
                let val = Self::read_vec(ctx, *src);
                if matches!(op.x86_hint, Some(X86OpHint::SseMov { .. })) {
                    let mut result = Self::read_vec(ctx, *dst);
                    let words = width.bytes() as usize / 8;
                    result[..words].copy_from_slice(&val[..words]);
                    Self::write_vec(ctx, *dst, result);
                } else if matches!(
                    op.x86_hint,
                    Some(X86OpHint::VexOp { .. } | X86OpHint::EvexOp { .. })
                ) && matches!(dst, VReg::Arch(ArchReg::X86(_)))
                {
                    let mut result = [0; 16];
                    let words = width.bytes() as usize / 8;
                    result[..words].copy_from_slice(&val[..words]);
                    Self::write_vec(ctx, *dst, result);
                } else {
                    Self::write_vec(ctx, *dst, val);
                }
            }

            OpKind::VShift {
                dst,
                src,
                amount,
                shift,
                elem,
                lanes,
            } => {
                let amt = match amount {
                    SrcOperand::Imm(val) => *val as u32,
                    SrcOperand::Reg(reg) => ctx.read_vreg(*reg) as u32,
                    _ => 0,
                };
                let elem_bits = elem.bytes() * 8;
                let mask = if elem_bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << elem_bits) - 1
                };
                let src_val = Self::read_vec(ctx, *src);
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let val = Self::get_lane(&src_val, lane, elem_bits);
                    let shifted = match shift {
                        ShiftOp::Lsl => (val << (amt % elem_bits)) & mask,
                        ShiftOp::Lsr => (val >> (amt % elem_bits)) & mask,
                        ShiftOp::Asr => {
                            // Sign-extend the element to i64 before the arithmetic
                            // shift (get_lane zero-extends), so high lanes are
                            // replicated with the element's sign bit, not 0.
                            let sv = if elem_bits >= 64 {
                                val as i64
                            } else {
                                let sh = 64 - elem_bits;
                                ((val << sh) as i64) >> sh
                            };
                            ((sv >> (amt % elem_bits)) as u64) & mask
                        }
                        _ => val,
                    };
                    Self::set_lane(&mut result, lane, elem_bits, shifted);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VBroadcast {
                dst,
                scalar,
                elem,
                lanes,
            } => {
                // Splat the low `elem` bits of the scalar register into every lane.
                let elem_bits = elem.bytes() * 8;
                let val = ctx.read_vreg(*scalar);
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    Self::set_lane(&mut result, lane, elem_bits, val);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane,
                elem,
            } => {
                let mut value = Self::read_vec(ctx, *vec);
                Self::set_lane(&mut value, *lane, elem.bytes() * 8, ctx.read_vreg(*scalar));
                Self::write_vec(ctx, *dst, value);
            }

            OpKind::VExtractLane {
                dst,
                vec,
                lane,
                elem,
                sign,
            } => {
                let bits = elem.bytes() * 8;
                let raw = Self::get_lane(&Self::read_vec(ctx, *vec), *lane, bits);
                let value = if *sign == SignExtend::Sign && bits < 64 {
                    (((raw << (64 - bits)) as i64) >> (64 - bits)) as u64
                } else {
                    raw
                };
                ctx.write_vreg(*dst, value);
            }

            OpKind::VCmp {
                dst,
                src1,
                src2,
                cond,
                elem,
                lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let bits = elem.bytes() * 8;
                let signed = |value: u64| -> i64 {
                    if bits == 64 {
                        value as i64
                    } else {
                        let shift = 64 - bits;
                        ((value << shift) as i64) >> shift
                    }
                };
                let int_cmp = |av: u64, bv: u64| match cond {
                    VecCmpCond::Eq => av == bv,
                    VecCmpCond::Ne => av != bv,
                    VecCmpCond::Lt => signed(av) < signed(bv),
                    VecCmpCond::Le => signed(av) <= signed(bv),
                    VecCmpCond::Gt => signed(av) > signed(bv),
                    VecCmpCond::Ge => signed(av) >= signed(bv),
                    VecCmpCond::Ltu => av < bv,
                    VecCmpCond::Leu => av <= bv,
                    VecCmpCond::Gtu => av > bv,
                    VecCmpCond::Geu => av >= bv,
                };
                let fp_cmp = |av: f64, bv: f64| match cond {
                    VecCmpCond::Eq => av == bv,
                    VecCmpCond::Ne => av != bv,
                    VecCmpCond::Lt | VecCmpCond::Ltu => av < bv,
                    VecCmpCond::Le | VecCmpCond::Leu => av <= bv,
                    VecCmpCond::Gt | VecCmpCond::Gtu => av > bv,
                    VecCmpCond::Ge | VecCmpCond::Geu => av >= bv,
                };
                let f16_to_f64 = |raw: u16| -> f64 {
                    let sign = (u32::from(raw & 0x8000)) << 16;
                    let exp = (raw >> 10) & 0x1f;
                    let frac = raw & 0x03ff;
                    let bits32 = if exp == 0 {
                        if frac == 0 {
                            sign
                        } else {
                            let shift = frac.leading_zeros() - 6;
                            let normalized = (u32::from(frac) << (shift + 1)) & 0x03ff;
                            sign | ((112 - shift) << 23) | (normalized << 13)
                        }
                    } else if exp == 0x1f {
                        sign | 0x7f80_0000 | (u32::from(frac) << 13)
                    } else {
                        sign | ((u32::from(exp) + 112) << 23) | (u32::from(frac) << 13)
                    };
                    f64::from(f32::from_bits(bits32))
                };

                let mut result = [0u64; 16];
                let true_value = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                for lane in 0..*lanes {
                    let av = Self::get_lane(&a, lane, bits);
                    let bv = Self::get_lane(&b, lane, bits);
                    let matched = match elem {
                        VecElementType::I8
                        | VecElementType::I16
                        | VecElementType::I32
                        | VecElementType::I64 => int_cmp(av, bv),
                        VecElementType::F16 => fp_cmp(f16_to_f64(av as u16), f16_to_f64(bv as u16)),
                        VecElementType::F32 => fp_cmp(
                            f64::from(f32::from_bits(av as u32)),
                            f64::from(f32::from_bits(bv as u32)),
                        ),
                        VecElementType::F64 => fp_cmp(f64::from_bits(av), f64::from_bits(bv)),
                    };
                    if matched {
                        Self::set_lane(&mut result, lane, bits, true_value);
                    }
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VShuffle {
                dst,
                src1,
                src2,
                indices,
                elem,
                lanes,
            } => {
                let first = Self::read_vec(ctx, *src1);
                let second = src2.map(|reg| Self::read_vec(ctx, reg));
                let selectors = Self::read_vec(ctx, *indices);
                let bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let index = Self::get_lane(&selectors, lane, bits);
                    let selected = if index < u64::from(*lanes) {
                        Self::get_lane(&first, index as u8, bits)
                    } else if let Some(second) = &second {
                        let second_index = index - u64::from(*lanes);
                        if second_index < u64::from(*lanes) {
                            Self::get_lane(second, second_index as u8, bits)
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    Self::set_lane(&mut result, lane, bits, selected);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VInterleave {
                dst,
                src1,
                src2,
                elem,
                lanes,
                block_lanes,
                high,
            } => {
                debug_assert!(*block_lanes != 0 && *block_lanes % 2 == 0);
                debug_assert!(*lanes % *block_lanes == 0);
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let bits = elem.bytes() * 8;
                let half = *block_lanes / 2;
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let within_block = lane % *block_lanes;
                    let block_base = lane - within_block;
                    let source_lane = block_base + if *high { half } else { 0 } + within_block / 2;
                    let source = if within_block & 1 == 0 {
                        &first
                    } else {
                        &second
                    };
                    let selected = Self::get_lane(source, source_lane, bits);
                    Self::set_lane(&mut result, lane, bits, selected);
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VByteShuffle {
                dst,
                src,
                control,
                lanes,
                block_lanes,
            } => {
                debug_assert!(block_lanes.is_power_of_two());
                debug_assert!(*block_lanes != 0 && *lanes % *block_lanes == 0);
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let source = Self::read_vec(ctx, *src);
                let selectors = Self::read_vec(ctx, *control);
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let selector = Self::get_lane(&selectors, lane, 8) as u8;
                    let selected = if selector & 0x80 != 0 {
                        0
                    } else {
                        let block_base = (lane / *block_lanes) * *block_lanes;
                        let source_lane = block_base + (selector & (*block_lanes - 1));
                        Self::get_lane(&source, source_lane, 8)
                    };
                    Self::set_lane(&mut result, lane, 8, selected);
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VHorizontalBin {
                dst,
                src1,
                src2,
                elem,
                lanes,
                block_lanes,
                subtract,
                saturating,
            } => {
                debug_assert!(*block_lanes != 0 && *block_lanes % 2 == 0);
                debug_assert!(*lanes % *block_lanes == 0);
                debug_assert!(matches!(elem, VecElementType::I16 | VecElementType::I32));
                debug_assert!(!*saturating || *elem == VecElementType::I16);
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let bits = elem.bytes() * 8;
                let mask = (1u64 << bits) - 1;
                let calculate = |a: u64, b: u64| -> u64 {
                    if *saturating {
                        let shift = 64 - bits;
                        let lhs = ((a << shift) as i64) >> shift;
                        let rhs = ((b << shift) as i64) >> shift;
                        let value = if *subtract { lhs - rhs } else { lhs + rhs };
                        let low = -(1i64 << (bits - 1));
                        let high = (1i64 << (bits - 1)) - 1;
                        value.clamp(low, high) as u64 & mask
                    } else if *subtract {
                        a.wrapping_sub(b) & mask
                    } else {
                        a.wrapping_add(b) & mask
                    }
                };
                let mut result = [0u64; 16];
                let half = *block_lanes / 2;
                for block_base in (0..*lanes).step_by(*block_lanes as usize) {
                    for pair in 0..half {
                        let lhs_lane = block_base + pair * 2;
                        let rhs_lane = lhs_lane + 1;
                        Self::set_lane(
                            &mut result,
                            block_base + pair,
                            bits,
                            calculate(
                                Self::get_lane(&first, lhs_lane, bits),
                                Self::get_lane(&first, rhs_lane, bits),
                            ),
                        );
                        Self::set_lane(
                            &mut result,
                            block_base + half + pair,
                            bits,
                            calculate(
                                Self::get_lane(&second, lhs_lane, bits),
                                Self::get_lane(&second, rhs_lane, bits),
                            ),
                        );
                    }
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VLoad { dst, addr, width } => {
                let effective_addr = self.compute_address(ctx, addr);
                let mut buf = [0u8; 64];
                let size = width.bytes() as usize;
                memory.read(effective_addr, &mut buf[..size])?;

                let mut vec = if matches!(op.x86_hint, Some(X86OpHint::SseMov { .. })) {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                let words = (size + 7) / 8;
                for i in 0..words {
                    let start = i * 8;
                    let end = start + 8;
                    vec[i] = u64::from_le_bytes(buf[start..end].try_into().unwrap());
                }

                Self::write_vec(ctx, *dst, vec);
            }

            OpKind::PredVLoad {
                dst,
                cond,
                addr,
                width,
            } => {
                if ctx.read_vreg(*cond) & 1 != 0 {
                    let effective_addr = self.compute_address(ctx, addr);
                    let mut buf = [0u8; 64];
                    let size = width.bytes() as usize;
                    memory.read(effective_addr, &mut buf[..size])?;

                    let mut vec = [0u64; 16];
                    for (word, chunk) in buf[..size].chunks_exact(8).enumerate() {
                        vec[word] = u64::from_le_bytes(chunk.try_into().unwrap());
                    }
                    Self::write_vec(ctx, *dst, vec);
                }
            }

            OpKind::VStore { src, addr, width } => {
                let effective_addr = self.compute_address(ctx, addr);
                let val = Self::read_vec(ctx, *src);

                let size = width.bytes() as usize;
                let mut buf = [0u8; 64];
                let words = (size + 7) / 8;
                for i in 0..words {
                    let start = i * 8;
                    let end = start + 8;
                    buf[start..end].copy_from_slice(&val[i].to_le_bytes());
                }

                memory.write(effective_addr, &buf[..size])?;
            }

            // ==================================================================
            // FLAG OPERATIONS
            // ==================================================================
            OpKind::ReadFlags { dst } => {
                ctx.flags.materialize_all();
                let rflags = ctx.flags.materialized.to_rflags();
                ctx.write_vreg(*dst, rflags);
            }

            OpKind::WriteFlags { src } => {
                let rflags = ctx.read_vreg(*src);
                ctx.flags.materialized =
                    crate::smir::ir::flags::MaterializedFlags::from_rflags(rflags);
                ctx.flags.lazy = None;
            }

            OpKind::SetCF { value } => {
                ctx.flags.materialize_all();
                ctx.flags.materialized.cf = *value;
            }

            OpKind::SetDF { value } => {
                ctx.flags.materialize_all();
                ctx.flags.materialized.df = *value;
            }

            OpKind::CmcCF => {
                let cf = ctx.flags.get_cf();
                ctx.flags.materialize_all();
                ctx.flags.materialized.cf = !cf;
            }

            OpKind::MaterializeFlags => {
                ctx.flags.materialize_all();
            }

            OpKind::X86LoadMxcsr { addr } => {
                let effective_addr = self.compute_address(ctx, addr);
                let mut bytes = [0u8; 4];
                memory.read(effective_addr, &mut bytes)?;
                if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                    x86.mxcsr = u32::from_le_bytes(bytes);
                }
            }

            OpKind::X86StoreMxcsr { addr } => {
                let effective_addr = self.compute_address(ctx, addr);
                let value = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0,
                };
                memory.write(effective_addr, &value.to_le_bytes())?;
            }

            OpKind::X86X87Control { kind, addr } => match kind {
                X86X87ControlKind::Init => {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.x87.init();
                    }
                }
                X86X87ControlKind::ClearExceptions => {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.x87.clear_exceptions();
                    }
                }
                X86X87ControlKind::EnterMmx => {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.x87.tag_word = 0;
                    }
                }
                X86X87ControlKind::EmptyMmx => {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.x87.tag_word = 0xFFFF;
                    }
                }
                X86X87ControlKind::StoreStatusAx => {
                    let status = match &ctx.arch_regs {
                        ArchRegState::X86_64(x86) => x86.x87.status_word,
                        _ => 0,
                    };
                    Self::write_x86_partial(
                        ctx,
                        VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                        status as u64,
                        OpWidth::W16,
                    );
                }
                X86X87ControlKind::LoadControlWord => {
                    let effective_addr = self.compute_address(
                        ctx,
                        addr.as_ref().expect("x87 FLDCW requires an address"),
                    );
                    let mut bytes = [0u8; 2];
                    memory.read(effective_addr, &mut bytes)?;
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.x87.control_word = u16::from_le_bytes(bytes);
                    }
                }
                X86X87ControlKind::LoadEnvironment(width) => {
                    let effective_addr = self.compute_address(
                        ctx,
                        addr.as_ref().expect("x87 FLDENV requires an address"),
                    );
                    let len = Self::x86_x87_environment_len(*width);
                    let mut image = [0u8; 28];
                    memory.read(effective_addr, &mut image[..len])?;
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        Self::restore_x86_x87_environment(&mut x86.x87, &image[..len], *width);
                    }
                }
                X86X87ControlKind::StoreEnvironment(width) => {
                    let effective_addr = self.compute_address(
                        ctx,
                        addr.as_ref().expect("x87 FNSTENV requires an address"),
                    );
                    let (image, len) = match &ctx.arch_regs {
                        ArchRegState::X86_64(x86) => {
                            Self::x86_x87_environment_image(&x86.x87, *width)
                        }
                        _ => ([0u8; 28], Self::x86_x87_environment_len(*width)),
                    };
                    memory.write(effective_addr, &image[..len])?;
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        // The saved FCW is the pre-instruction value; exception
                        // masks become set only after the complete store.
                        x86.x87.control_word |= 0x003F;
                    }
                }
                X86X87ControlKind::RestoreState(width) => {
                    let effective_addr = self.compute_address(
                        ctx,
                        addr.as_ref().expect("x87 FRSTOR requires an address"),
                    );
                    let len = Self::x86_x87_environment_len(*width) + 80;
                    let mut image = [0u8; 108];
                    memory.read(effective_addr, &mut image[..len])?;
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        Self::restore_x86_x87_state(&mut x86.x87, &image[..len], *width);
                    }
                }
                X86X87ControlKind::SaveState(width) => {
                    let effective_addr = self.compute_address(
                        ctx,
                        addr.as_ref().expect("x87 FNSAVE requires an address"),
                    );
                    let (image, len) = match &ctx.arch_regs {
                        ArchRegState::X86_64(x86) => Self::x86_x87_state_image(&x86.x87, *width),
                        _ => ([0u8; 108], Self::x86_x87_environment_len(*width) + 80),
                    };
                    memory.write(effective_addr, &image[..len])?;
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.x87.init();
                    }
                }
                X86X87ControlKind::StoreControlWord | X86X87ControlKind::StoreStatusWord => {
                    let effective_addr = self.compute_address(
                        ctx,
                        addr.as_ref()
                            .expect("x87 status/control store requires an address"),
                    );
                    let value = match &ctx.arch_regs {
                        ArchRegState::X86_64(x86) => {
                            if *kind == X86X87ControlKind::StoreControlWord {
                                x86.x87.control_word
                            } else {
                                x86.x87.status_word
                            }
                        }
                        _ => 0,
                    };
                    memory.write(effective_addr, &value.to_le_bytes())?;
                }
            },

            OpKind::X86X87Data {
                kind,
                addr,
                st,
                fop,
            } => {
                self.execute_x86_x87_data(
                    ctx,
                    memory,
                    op.guest_pc,
                    *kind,
                    addr.as_ref(),
                    *st,
                    *fop,
                )?;
            }

            OpKind::X86FxSave { addr, rex_w } => {
                let effective_addr = self.compute_address(ctx, addr);
                if effective_addr & 0xF != 0 {
                    return Err(MemoryError::Alignment {
                        addr: effective_addr,
                        required: 16,
                    });
                }
                let image = Self::x86_fxsave_image(ctx, *rex_w);
                // Bytes 464:511 are explicitly available to software and are
                // not modified by FXSAVE.
                memory.write(effective_addr, &image)?;
            }

            OpKind::X86FxRstor { addr, rex_w } => {
                let effective_addr = self.compute_address(ctx, addr);
                if effective_addr & 0xF != 0 {
                    return Err(MemoryError::Alignment {
                        addr: effective_addr,
                        required: 16,
                    });
                }
                let mut image = [0u8; 512];
                memory.read(effective_addr, &mut image)?;
                let mxcsr = u32::from_le_bytes(image[24..28].try_into().unwrap());
                if mxcsr & !0x0000_FFFF != 0 {
                    return Err(MemoryError::AccessViolation {
                        addr: effective_addr,
                        write: false,
                    });
                }
                // Commit only after the complete image and MXCSR validation
                // succeed, preserving architectural state on a restore fault.
                Self::restore_x86_fxsave_image(ctx, &image, *rex_w);
            }

            OpKind::X86XSave {
                addr,
                rex_w,
                kind,
                src_low,
                src_high,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                if effective_addr & 0x3F != 0 {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                } else {
                    let requested = (ctx.read_vreg(*src_low) as u32 as u64)
                        | ((ctx.read_vreg(*src_high) as u32 as u64) << 32);
                    match kind {
                        X86XSaveKind::XSave | X86XSaveKind::XSaveOpt => {
                            Self::save_x86_xsave_standard(
                                ctx,
                                memory,
                                effective_addr,
                                *rex_w,
                                requested,
                                *kind,
                            )?;
                        }
                        X86XSaveKind::XSaveC | X86XSaveKind::XSaveS => {
                            Self::save_x86_xsave_compacted(
                                ctx,
                                memory,
                                effective_addr,
                                *rex_w,
                                requested,
                            )?;
                        }
                    }
                }
            }

            OpKind::X86XRstor {
                addr,
                rex_w,
                supervisor,
                src_low,
                src_high,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                if effective_addr & 0x3F != 0 {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                } else {
                    let requested = (ctx.read_vreg(*src_low) as u32 as u64)
                        | ((ctx.read_vreg(*src_high) as u32 as u64) << 32);
                    if !Self::restore_x86_xsave(
                        ctx,
                        memory,
                        effective_addr,
                        *rex_w,
                        requested,
                        *supervisor,
                    )? {
                        ctx.request_exit(ExitReason::GeneralProtection {
                            addr: op.guest_pc,
                            error_code: 0,
                        });
                    }
                }
            }

            OpKind::X86Cmpxchg8b16b {
                addr,
                wide,
                locked,
                compare_lo,
                compare_hi,
                new_lo,
                new_hi,
                dst_lo,
                dst_hi,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                if *wide && effective_addr & 0xF != 0 {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                } else {
                    let compare_lo_value = ctx.read_vreg(*compare_lo);
                    let compare_hi_value = ctx.read_vreg(*compare_hi);
                    let new_lo_value = ctx.read_vreg(*new_lo);
                    let new_hi_value = ctx.read_vreg(*new_hi);
                    let (old_lo, old_hi, success) = if *wide {
                        let (old, success) = memory.compare_and_swap_128(
                            effective_addr,
                            [compare_lo_value, compare_hi_value],
                            [new_lo_value, new_hi_value],
                            if *locked {
                                MemoryOrder::SeqCst
                            } else {
                                MemoryOrder::Relaxed
                            },
                            MemoryOrder::Relaxed,
                        )?;
                        (old[0], old[1], success)
                    } else {
                        let expected = (compare_lo_value as u32 as u64)
                            | ((compare_hi_value as u32 as u64) << 32);
                        let replacement =
                            (new_lo_value as u32 as u64) | ((new_hi_value as u32 as u64) << 32);
                        let (old, success) = memory.compare_and_swap_writeback(
                            effective_addr,
                            expected,
                            replacement,
                            MemWidth::B8,
                            if *locked {
                                MemoryOrder::SeqCst
                            } else {
                                MemoryOrder::Relaxed
                            },
                            MemoryOrder::Relaxed,
                        )?;
                        (old as u32 as u64, old >> 32, success)
                    };
                    if !success {
                        Self::write_x86_partial(
                            ctx,
                            *dst_lo,
                            old_lo,
                            if *wide { OpWidth::W64 } else { OpWidth::W32 },
                        );
                        Self::write_x86_partial(
                            ctx,
                            *dst_hi,
                            old_hi,
                            if *wide { OpWidth::W64 } else { OpWidth::W32 },
                        );
                    }
                    ctx.flags.materialize_all();
                    ctx.flags.materialized.zf = success;
                }
            }

            OpKind::X86Random { dst, width, seed } => {
                let (value, success) = Self::x86_hardware_random(*width, *seed);
                Self::write_x86_partial(ctx, *dst, if success { value } else { 0 }, *width);
                ctx.flags.materialize_all();
                ctx.flags.materialized.cf = success;
                ctx.flags.materialized.of = false;
                ctx.flags.materialized.sf = false;
                ctx.flags.materialized.zf = false;
                ctx.flags.materialized.af = false;
                ctx.flags.materialized.pf = false;
            }

            OpKind::X86ReadPid { dst } => {
                let value = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.tsc_aux as u64,
                    _ => 0,
                };
                // IA32_TSC_AUX is 32 bits. Both architectural RDPID
                // destination spellings therefore produce the same zero-
                // extended GPR value; operand-size prefixes are ignored.
                Self::write_x86_partial(ctx, *dst, value, OpWidth::W32);
            }

            OpKind::X86XGetBv {
                dst_low,
                dst_high,
                selector,
            } => {
                let selector = ctx.read_vreg(*selector) as u32;
                let value = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => match selector {
                        0 => Some(x86.xcr0),
                        1 => Some(x86.xgetbv1 & x86.xcr0),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(value) = value {
                    ctx.write_vreg(*dst_low, value as u32 as u64);
                    ctx.write_vreg(*dst_high, (value >> 32) as u32 as u64);
                } else {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                }
            }

            OpKind::X86XSetBv {
                selector,
                src_low,
                src_high,
            } => {
                let selector = ctx.read_vreg(*selector) as u32;
                let value = (ctx.read_vreg(*src_low) as u32 as u64)
                    | ((ctx.read_vreg(*src_high) as u32 as u64) << 32);
                const AVX512_STATE: u64 = (1 << 5) | (1 << 6) | (1 << 7);
                const SUPPORTED: u64 = 0x7 | AVX512_STATE | (1 << 19);
                let avx512 = value & AVX512_STATE;
                let invalid = selector != 0
                    || value & 1 == 0
                    || value & !SUPPORTED != 0
                    || (value & (1 << 2) != 0 && value & (1 << 1) == 0)
                    || (avx512 != 0 && (avx512 != AVX512_STATE || value & 0x6 != 0x6));
                if invalid {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                } else if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                    x86.xcr0 = value;
                }
            }

            OpKind::TestCondition { dst, cond } => {
                let result = if ctx.flags.eval_condition(*cond) {
                    1
                } else {
                    0
                };
                ctx.write_vreg(*dst, result);
            }

            OpKind::SetCC { dst, cond, width } => {
                let result = if ctx.flags.eval_condition(*cond) {
                    1u64
                } else {
                    0
                };
                Self::write_x86_partial(ctx, *dst, result & width.mask(), *width);
            }

            // ==================================================================
            // SYSTEM / PRIVILEGED
            // ==================================================================
            OpKind::Syscall { num, args } => {
                let num_val = ctx.read_vreg(*num);
                let arg_vals: Vec<u64> = args.iter().map(|a| ctx.read_vreg(*a)).collect();
                ctx.request_exit(ExitReason::Syscall {
                    num: num_val,
                    args: arg_vals,
                });
            }

            OpKind::Swi { imm } => {
                ctx.request_exit(ExitReason::Syscall {
                    num: *imm as u64,
                    args: vec![],
                });
            }

            OpKind::ReadSysReg { dst, reg: _ } => {
                // Simplified: return 0
                ctx.write_vreg(*dst, 0);
            }

            OpKind::WriteSysReg { reg: _, src: _ } => {
                // Simplified: no-op
            }

            OpKind::X86ReadTsc { dst_lo, dst_hi } => {
                let tsc = ctx.cycle_count;
                Self::write_x86_partial(ctx, *dst_lo, tsc & u32::MAX as u64, OpWidth::W32);
                Self::write_x86_partial(ctx, *dst_hi, tsc >> 32, OpWidth::W32);
            }

            // ==================================================================
            // META / DEBUG
            // ==================================================================
            OpKind::Nop => {}

            OpKind::Undefined { opcode } => {
                ctx.request_exit(ExitReason::Undefined {
                    addr: ctx.pc,
                    opcode: *opcode,
                });
            }

            OpKind::Breakpoint => {
                ctx.request_exit(ExitReason::Breakpoint { addr: ctx.pc });
            }

            OpKind::VShuffleBitQM {
                dst,
                src,
                indices,
                mask: write_mask,
                width,
            } => {
                let src_val = Self::read_vec(ctx, *src);
                let idx_val = Self::read_vec(ctx, *indices);
                let mut result = 0u64;
                let bytes = width.bytes();

                for qword_idx in 0..(bytes / 8) {
                    let lane_base = (qword_idx * 8) as u8;
                    let qword = Self::get_lane(&src_val, qword_idx as u8, 64);
                    for byte_idx in 0..8 {
                        let idx = Self::get_lane(&idx_val, lane_base + byte_idx as u8, 8) & 0x3f;
                        let bit = (qword >> idx) & 1;
                        result |= bit << (qword_idx * 8 + byte_idx);
                    }
                }

                let mask = if bytes >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bytes) - 1
                };
                let write_mask = write_mask.map_or(u64::MAX, |mask| ctx.read_vreg(mask));
                ctx.write_vreg(*dst, result & mask & write_mask);
            }

            OpKind::VCompress {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => {
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let control = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let lanes = width.lanes(*elem) as u8;
                let bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                let mut output = 0u8;
                for lane in 0..lanes {
                    if control & (1u64 << lane) != 0 {
                        let value = Self::get_lane(&source, lane, bits);
                        Self::set_lane(&mut result, output, bits, value);
                        output += 1;
                    }
                }
                if !zeroing {
                    for lane in output..lanes {
                        let value = Self::get_lane(&old, lane, bits);
                        Self::set_lane(&mut result, lane, bits, value);
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VExpand {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => {
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let control = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let lanes = width.lanes(*elem) as u8;
                let bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                let mut input = 0u8;
                for lane in 0..lanes {
                    if control & (1u64 << lane) != 0 {
                        let value = Self::get_lane(&source, input, bits);
                        Self::set_lane(&mut result, lane, bits, value);
                        input += 1;
                    } else if !zeroing {
                        let value = Self::get_lane(&old, lane, bits);
                        Self::set_lane(&mut result, lane, bits, value);
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86NarrowInt {
                dst,
                src,
                mask,
                src_elem,
                dst_elem,
                width,
                mode,
                zeroing,
            } => {
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let control = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let lanes = width.lanes(*src_elem) as u8;
                let src_bits = src_elem.bytes() * 8;
                let dst_bits = dst_elem.bytes() * 8;
                let dst_mask = if dst_bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << dst_bits) - 1
                };
                let mut result = [0u64; 16];
                for lane in 0..lanes {
                    if control & (1u64 << lane) != 0 {
                        let raw = Self::get_lane(&source, lane, src_bits);
                        let shift = 128 - src_bits;
                        let signed = (i128::from(raw) << shift) >> shift;
                        let value = match mode {
                            X86NarrowMode::Truncate => raw & dst_mask,
                            X86NarrowMode::SignedSaturate => {
                                let low = -(1i128 << (dst_bits - 1));
                                let high = (1i128 << (dst_bits - 1)) - 1;
                                signed.clamp(low, high) as u64 & dst_mask
                            }
                            X86NarrowMode::UnsignedSaturate => raw.min(dst_mask),
                        };
                        Self::set_lane(&mut result, lane, dst_bits, value);
                    } else if !zeroing {
                        let value = Self::get_lane(&old, lane, dst_bits);
                        Self::set_lane(&mut result, lane, dst_bits, value);
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VDotProduct {
                dst,
                acc,
                src1,
                src2,
                mask,
                src_elem,
                acc_elem,
                width,
                src1_unsigned,
                saturate,
                zeroing,
            } => {
                debug_assert!(matches!(src_elem, VecElementType::I8 | VecElementType::I16));
                debug_assert!(matches!(
                    acc_elem,
                    VecElementType::I16 | VecElementType::I32
                ));
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let src_bits = src_elem.bytes() * 8;
                let acc_bits = acc_elem.bytes() * 8;
                debug_assert!(acc_bits >= src_bits && acc_bits % src_bits == 0);

                // Snapshot every input before writing `dst`: VNNI normally aliases
                // dst/acc, while PMADDUBSW and PMADDWD can alias either multiplicand.
                let accumulator = Self::read_vec(ctx, *acc);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let terms = acc_bits / src_bits;
                let lanes = width.lanes(*acc_elem) as u8;
                let src_mask = (1u64 << src_bits) - 1;
                let acc_mask = (1u64 << acc_bits) - 1;
                let signed = |value: u64, bits: u32| -> i128 {
                    let shift = 128 - bits;
                    ((i128::from(value) << shift) >> shift) as i128
                };
                let acc_low = -(1i128 << (acc_bits - 1));
                let acc_high = (1i128 << (acc_bits - 1)) - 1;
                let mut result = [0u64; 16];

                for lane in 0..lanes {
                    let mut sum = signed(Self::get_lane(&accumulator, lane, acc_bits), acc_bits);
                    let first_term = u32::from(lane) * terms;
                    for term in 0..terms {
                        let source_lane = (first_term + term) as u8;
                        let a_raw = Self::get_lane(&first, source_lane, src_bits) & src_mask;
                        let b_raw = Self::get_lane(&second, source_lane, src_bits) & src_mask;
                        let a = if *src1_unsigned {
                            i128::from(a_raw)
                        } else {
                            signed(a_raw, src_bits)
                        };
                        let b = signed(b_raw, src_bits);
                        sum += a * b;
                    }
                    let narrowed = if *saturate {
                        sum.clamp(acc_low, acc_high)
                    } else {
                        sum
                    };
                    Self::set_lane(&mut result, lane, acc_bits, narrowed as u64 & acc_mask);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &accumulator,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *acc_elem,
                );
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VMpsadbw {
                dst,
                src1,
                src2,
                mask,
                width,
                imm,
                zeroing,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                // Snapshot both inputs before writing because both the legacy
                // and non-destructive register forms may alias the destination.
                // AVX10.2 merge masking also reads the pre-instruction dst.
                let blocks = match width {
                    VecWidth::V128 => 1u8,
                    VecWidth::V256 => 2,
                    VecWidth::V512 => 4,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let old_dst = Self::read_vec(ctx, *dst);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                for block in 0..blocks {
                    // The low imm3 controls even-numbered 128-bit lanes and
                    // the high imm3 controls odd-numbered lanes. AVX10.2
                    // repeats the pair for lanes 2 and 3 at VL=512.
                    let control = if block & 1 == 0 { *imm } else { *imm >> 3 };
                    let first_select = ((control >> 2) & 1) * 4;
                    let second_select = (control & 3) * 4;
                    let block_base = block * 16;
                    for output in 0..8u8 {
                        let mut sum = 0u16;
                        for tap in 0..4u8 {
                            let first_byte =
                                Self::get_lane(&first, block_base + first_select + output + tap, 8)
                                    as u8;
                            let second_byte =
                                Self::get_lane(&second, block_base + second_select + tap, 8) as u8;
                            sum += u16::from(first_byte.abs_diff(second_byte));
                        }
                        Self::set_lane(&mut result, block * 8 + output, 16, u64::from(sum));
                    }
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old_dst,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    VecElementType::I16,
                );
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VSadBytes {
                dst,
                src1,
                src2,
                width,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                // Snapshot both inputs before writing: every register form may
                // alias the destination architecturally.
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                for block in 0..(width.bytes() / 8) as u8 {
                    let mut sum = 0u16;
                    for byte in 0..8u8 {
                        let lane = block * 8 + byte;
                        let a = Self::get_lane(&first, lane, 8) as u8;
                        let b = Self::get_lane(&second, lane, 8) as u8;
                        sum += u16::from(a.abs_diff(b));
                    }
                    Self::set_lane(&mut result, block, 64, u64::from(sum));
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::X86Phminposuw { dst, src } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                // Snapshot before writing because the two-operand source may
                // alias the destination in both legacy and VEX encodings.
                let source = Self::read_vec(ctx, *src);
                let mut minimum = Self::get_lane(&source, 0, 16) as u16;
                let mut index = 0u8;
                for lane in 1..8u8 {
                    let candidate = Self::get_lane(&source, lane, 16) as u16;
                    if candidate < minimum {
                        minimum = candidate;
                        index = lane;
                    }
                }
                let mut result = [0u64; 16];
                result[0] = u64::from(minimum) | (u64::from(index) << 16);
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::X86MovMask {
                dst,
                src,
                elem,
                lanes,
                dst_width,
            } => {
                let source = Self::read_vec(ctx, *src);
                let lane_bits = elem.bytes() * 8;
                let mut mask = 0u64;
                for lane in 0..*lanes {
                    let sign = Self::get_lane(&source, lane, lane_bits) >> (lane_bits - 1);
                    mask |= (sign & 1) << lane;
                }
                Self::write_x86_partial(ctx, *dst, mask, *dst_width);
            }

            OpKind::X86MovdQ {
                dst,
                src,
                width,
                zero_upper,
            } => {
                if matches!(
                    dst,
                    VReg::Arch(ArchReg::X86(X86Reg::Mm(_) | X86Reg::Xmm(_)))
                ) {
                    let scalar = ctx.read_vreg(*src) & width.mask();
                    let old = Self::read_vec(ctx, *dst);
                    let mut result = if *zero_upper { [0; 16] } else { old };
                    result[0] = scalar;
                    result[1] = 0;
                    Self::write_vec(ctx, *dst, result);
                } else {
                    let scalar = Self::read_vec(ctx, *src)[0] & width.mask();
                    Self::write_x86_partial(ctx, *dst, scalar, *width);
                }
            }

            OpKind::X86Aes {
                dst,
                src1,
                src2,
                width,
                op,
                imm,
            } => {
                use crate::isa::x86_64::execute::crypto::aes;

                let first = Self::read_vec(ctx, *src1);
                let second = src2.map(|reg| Self::read_vec(ctx, reg));
                let mut result = [0u64; 16];
                for lane in 0..(width.bytes() / 16) as usize {
                    let word = lane * 2;
                    let (lo, hi) = match op {
                        X86AesOp::Enc => aes::aesenc(
                            first[word],
                            first[word + 1],
                            second.as_ref().unwrap()[word],
                            second.as_ref().unwrap()[word + 1],
                        ),
                        X86AesOp::EncLast => aes::aesenclast(
                            first[word],
                            first[word + 1],
                            second.as_ref().unwrap()[word],
                            second.as_ref().unwrap()[word + 1],
                        ),
                        X86AesOp::Dec => aes::aesdec(
                            first[word],
                            first[word + 1],
                            second.as_ref().unwrap()[word],
                            second.as_ref().unwrap()[word + 1],
                        ),
                        X86AesOp::DecLast => aes::aesdeclast(
                            first[word],
                            first[word + 1],
                            second.as_ref().unwrap()[word],
                            second.as_ref().unwrap()[word + 1],
                        ),
                        X86AesOp::InvMixColumns => aes::aesimc(first[word], first[word + 1]),
                        X86AesOp::KeygenAssist => {
                            aes::aeskeygenassist(first[word], first[word + 1], *imm)
                        }
                    };
                    result[word] = lo;
                    result[word + 1] = hi;
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sha512Msg1 { dst, src } => {
                let old = Self::read_vec(ctx, *dst);
                let source = Self::read_vec(ctx, *src);
                let sigma0 = |x: u64| x.rotate_right(1) ^ x.rotate_right(8) ^ (x >> 7);
                let mut result = [0u64; 16];
                result[0] = old[0].wrapping_add(sigma0(old[1]));
                result[1] = old[1].wrapping_add(sigma0(old[2]));
                result[2] = old[2].wrapping_add(sigma0(old[3]));
                result[3] = old[3].wrapping_add(sigma0(source[0]));
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sha512Msg2 { dst, src } => {
                let old = Self::read_vec(ctx, *dst);
                let source = Self::read_vec(ctx, *src);
                let sigma1 = |x: u64| x.rotate_right(19) ^ x.rotate_right(61) ^ (x >> 6);
                let w16 = old[0].wrapping_add(sigma1(source[2]));
                let w17 = old[1].wrapping_add(sigma1(source[3]));
                let w18 = old[2].wrapping_add(sigma1(w16));
                let w19 = old[3].wrapping_add(sigma1(w17));
                let mut result = [0u64; 16];
                result[..4].copy_from_slice(&[w16, w17, w18, w19]);
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sha512Rounds2 { dst, state, wk } => {
                let cdgh = Self::read_vec(ctx, *dst);
                let abef = Self::read_vec(ctx, *state);
                let constants = Self::read_vec(ctx, *wk);
                let mut a = abef[3];
                let mut b = abef[2];
                let mut c = cdgh[3];
                let mut d = cdgh[2];
                let mut e = abef[1];
                let mut f = abef[0];
                let mut g = cdgh[1];
                let mut h = cdgh[0];
                for &round_constant in &constants[..2] {
                    let choose = (e & f) ^ (g & !e);
                    let majority = (a & b) ^ (a & c) ^ (b & c);
                    let big1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
                    let big0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
                    let t1 = choose
                        .wrapping_add(big1)
                        .wrapping_add(round_constant)
                        .wrapping_add(h);
                    let next_a = t1.wrapping_add(majority).wrapping_add(big0);
                    let next_e = t1.wrapping_add(d);
                    (h, g, f, e, d, c, b, a) = (g, f, e, next_e, c, b, a, next_a);
                }
                let mut result = [0u64; 16];
                result[..4].copy_from_slice(&[f, e, b, a]);
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sm3Msg1 { dst, src1, src2 } => {
                let old = Self::read_vec(ctx, *dst);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let lane = |value: &VecValue, index| Self::get_lane(value, index, 32) as u32;
                let p1 = |x: u32| x ^ x.rotate_left(15) ^ x.rotate_left(23);
                let mut result = [0u64; 16];
                for index in 0..4u8 {
                    let mut tmp = lane(&old, index) ^ lane(&second, index);
                    if index < 3 {
                        tmp ^= lane(&first, index).rotate_left(15);
                    }
                    Self::set_lane(&mut result, index, 32, u64::from(p1(tmp)));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sm3Msg2 { dst, src1, src2 } => {
                let old = Self::read_vec(ctx, *dst);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let lane = |value: &VecValue, index| Self::get_lane(value, index, 32) as u32;
                let mut words = [0u32; 4];
                for index in 0..4u8 {
                    words[index as usize] = lane(&first, index).rotate_left(7)
                        ^ lane(&second, index)
                        ^ lane(&old, index);
                }
                words[3] ^=
                    words[0].rotate_left(6) ^ words[0].rotate_left(15) ^ words[0].rotate_left(30);
                let mut result = [0u64; 16];
                for (index, value) in words.into_iter().enumerate() {
                    Self::set_lane(&mut result, index as u8, 32, u64::from(value));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sm3Rounds2 {
                dst,
                state,
                words,
                imm,
            } => {
                let cdgh = Self::read_vec(ctx, *dst);
                let abef = Self::read_vec(ctx, *state);
                let message = Self::read_vec(ctx, *words);
                let lane = |value: &VecValue, index| Self::get_lane(value, index, 32) as u32;
                let mut a = lane(&abef, 3);
                let mut b = lane(&abef, 2);
                let mut c = lane(&cdgh, 3).rotate_left(9);
                let mut d = lane(&cdgh, 2).rotate_left(9);
                let mut e = lane(&abef, 1);
                let mut f = lane(&abef, 0);
                let mut g = lane(&cdgh, 1).rotate_left(19);
                let mut h = lane(&cdgh, 0).rotate_left(19);
                let w = [
                    lane(&message, 0),
                    lane(&message, 1),
                    lane(&message, 2),
                    lane(&message, 3),
                ];
                let round = imm & 0x3E;
                let mut constant = if round < 16 {
                    0x79CC_4519u32
                } else {
                    0x7A87_9D8A
                }
                .rotate_left(u32::from(round));
                for index in 0..2usize {
                    let a12 = a.rotate_left(12);
                    let s1 = a12.wrapping_add(e).wrapping_add(constant).rotate_left(7);
                    let s2 = s1 ^ a12;
                    let ff = if round < 16 {
                        a ^ b ^ c
                    } else {
                        (a & b) | (a & c) | (b & c)
                    };
                    let gg = if round < 16 {
                        e ^ f ^ g
                    } else {
                        (e & f) | (!e & g)
                    };
                    let t1 = ff
                        .wrapping_add(d)
                        .wrapping_add(s2)
                        .wrapping_add(w[index] ^ w[index + 2]);
                    let t2 = gg.wrapping_add(h).wrapping_add(s1).wrapping_add(w[index]);
                    let next_e = t2 ^ t2.rotate_left(9) ^ t2.rotate_left(17);
                    (d, c, b, a) = (c, b.rotate_left(9), a, t1);
                    (h, g, f, e) = (g, f.rotate_left(19), e, next_e);
                    constant = constant.rotate_left(1);
                }
                let mut result = [0u64; 16];
                for (index, value) in [f, e, b, a].into_iter().enumerate() {
                    Self::set_lane(&mut result, index as u8, 32, u64::from(value));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sm4 {
                dst,
                src1,
                src2,
                width,
                key_schedule,
            } => {
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let substitute = |value: u32| {
                    let bytes = value.to_le_bytes();
                    u32::from_le_bytes([
                        X86_SM4_SBOX[bytes[0] as usize],
                        X86_SM4_SBOX[bytes[1] as usize],
                        X86_SM4_SBOX[bytes[2] as usize],
                        X86_SM4_SBOX[bytes[3] as usize],
                    ])
                };
                let transform = |value: u32| {
                    let value = substitute(value);
                    if *key_schedule {
                        value ^ value.rotate_left(13) ^ value.rotate_left(23)
                    } else {
                        value
                            ^ value.rotate_left(2)
                            ^ value.rotate_left(10)
                            ^ value.rotate_left(18)
                            ^ value.rotate_left(24)
                    }
                };
                let groups = width.bytes() / 16;
                let mut result = [0u64; 16];
                for group in 0..groups as u8 {
                    let base = group * 4;
                    let p = [
                        Self::get_lane(&first, base, 32) as u32,
                        Self::get_lane(&first, base + 1, 32) as u32,
                        Self::get_lane(&first, base + 2, 32) as u32,
                        Self::get_lane(&first, base + 3, 32) as u32,
                    ];
                    let keys = [
                        Self::get_lane(&second, base, 32) as u32,
                        Self::get_lane(&second, base + 1, 32) as u32,
                        Self::get_lane(&second, base + 2, 32) as u32,
                        Self::get_lane(&second, base + 3, 32) as u32,
                    ];
                    let c0 = p[0] ^ transform(p[1] ^ p[2] ^ p[3] ^ keys[0]);
                    let c1 = p[1] ^ transform(p[2] ^ p[3] ^ c0 ^ keys[1]);
                    let c2 = p[2] ^ transform(p[3] ^ c0 ^ c1 ^ keys[2]);
                    let c3 = p[3] ^ transform(c0 ^ c1 ^ c2 ^ keys[3]);
                    for (lane, value) in [c0, c1, c2, c3].into_iter().enumerate() {
                        Self::set_lane(&mut result, base + lane as u8, 32, u64::from(value));
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Convert16ToFp32 {
                dst,
                src,
                width,
                fp16,
                odd,
                broadcast,
            } => {
                let packed = if *broadcast {
                    None
                } else {
                    Some(Self::read_vec(ctx, *src))
                };
                let scalar = if *broadcast {
                    ctx.read_vreg(*src) as u16
                } else {
                    0
                };
                let mut result = [0u64; 16];
                let lanes = width.lanes(VecElementType::F32) as u8;
                for lane in 0..lanes {
                    let input = if *broadcast {
                        scalar
                    } else {
                        Self::get_lane(packed.as_ref().unwrap(), lane * 2 + u8::from(*odd), 16)
                            as u16
                    };
                    let converted = if *fp16 {
                        Self::x86_fp16_to_fp32_bits(input)
                    } else {
                        u32::from(input) << 16
                    };
                    Self::set_lane(&mut result, lane, 32, u64::from(converted));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedShiftImm {
                dst,
                src,
                width,
                elem,
                shift,
                amount,
                byte_lane,
            } => {
                let input = Self::read_vec(ctx, *src);
                let mut result = [0u64; 16];
                if *byte_lane {
                    let amount = usize::from(*amount);
                    for block in 0..(width.bytes() / 16) as usize {
                        for lane in 0..16usize {
                            let source_lane = match shift {
                                ShiftOp::Lsl => lane.checked_sub(amount),
                                ShiftOp::Lsr => {
                                    lane.checked_add(amount).filter(|index| *index < 16)
                                }
                                _ => unreachable!(),
                            };
                            if let Some(source_lane) = source_lane {
                                let value =
                                    Self::get_lane(&input, (block * 16 + source_lane) as u8, 8);
                                Self::set_lane(&mut result, (block * 16 + lane) as u8, 8, value);
                            }
                        }
                    }
                } else {
                    let bits = elem.bytes() * 8;
                    let lanes = width.lanes(*elem) as u8;
                    let amount = u32::from(*amount);
                    let mask = if bits == 64 {
                        u64::MAX
                    } else {
                        (1u64 << bits) - 1
                    };
                    for lane in 0..lanes {
                        let value = Self::get_lane(&input, lane, bits);
                        let shifted = if amount >= bits {
                            if *shift == ShiftOp::Asr && value & (1u64 << (bits - 1)) != 0 {
                                mask
                            } else {
                                0
                            }
                        } else {
                            match shift {
                                ShiftOp::Lsl => (value << amount) & mask,
                                ShiftOp::Lsr => value >> amount,
                                ShiftOp::Asr => {
                                    let signed = if bits == 64 {
                                        value as i64
                                    } else {
                                        ((value << (64 - bits)) as i64) >> (64 - bits)
                                    };
                                    ((signed >> amount) as u64) & mask
                                }
                                _ => unreachable!(),
                            }
                        };
                        Self::set_lane(&mut result, lane, bits, shifted);
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedAlignRight {
                dst,
                high,
                low,
                width,
                amount,
            } => {
                let high = Self::read_vec(ctx, *high);
                let low = Self::read_vec(ctx, *low);
                let mut result = [0u64; 16];
                let width_bytes = width.bytes() as usize;
                let block_bytes = usize::min(width_bytes, 16);
                for block in 0..width_bytes / block_bytes {
                    let base = block * block_bytes;
                    for lane in 0..block_bytes {
                        let selected = usize::from(*amount) + lane;
                        let value = if selected < block_bytes {
                            Self::get_lane(&low, (base + selected) as u8, 8)
                        } else if selected < block_bytes * 2 {
                            Self::get_lane(&high, (base + selected - block_bytes) as u8, 8)
                        } else {
                            0
                        };
                        Self::set_lane(&mut result, (base + lane) as u8, 8, value);
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedShuffleImm {
                dst,
                src,
                width,
                elem,
                imm,
                high_words,
            } => {
                let input = Self::read_vec(ctx, *src);
                let mut result = [0u64; 16];
                let lanes = width.lanes(*elem) as u8;
                let block_lanes = if *elem == VecElementType::I32 { 4 } else { 8 };
                let bits = elem.bytes() * 8;
                for lane in 0..lanes {
                    let within = lane % block_lanes;
                    let block = lane - within;
                    let shuffled = match high_words {
                        None => true,
                        Some(true) => within >= 4,
                        Some(false) => within < 4,
                    };
                    let selector = if shuffled {
                        let output = within % 4;
                        block
                            + if *high_words == Some(true) { 4 } else { 0 }
                            + ((*imm >> (output * 2)) & 3)
                    } else {
                        lane
                    };
                    let value = Self::get_lane(&input, selector, bits);
                    Self::set_lane(&mut result, lane, bits, value);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86ThreeDNow {
                dst,
                src1,
                src2,
                kind,
            } => {
                let first = Self::read_vec(ctx, *src1)[0];
                let second = Self::read_vec(ctx, *src2)[0];
                let mut result = [0u64; 16];
                result[0] = Self::x86_three_d_now_eval(*kind, first, second);
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedShift {
                dst,
                src,
                count,
                width,
                elem,
                shift,
            } => {
                let input = Self::read_vec(ctx, *src);
                let mut result = [0u64; 16];
                let bits = elem.bytes() * 8;
                let lanes = width.lanes(*elem) as u8;
                let amount = ctx.read_vreg(*count);
                let mask = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                for lane in 0..lanes {
                    let value = Self::get_lane(&input, lane, bits);
                    let shifted = if amount >= u64::from(bits) {
                        if *shift == ShiftOp::Asr && value & (1u64 << (bits - 1)) != 0 {
                            mask
                        } else {
                            0
                        }
                    } else {
                        match shift {
                            ShiftOp::Lsl => (value << amount) & mask,
                            ShiftOp::Lsr => value >> amount,
                            ShiftOp::Asr => {
                                let signed = if bits == 64 {
                                    value as i64
                                } else {
                                    ((value << (64 - bits)) as i64) >> (64 - bits)
                                };
                                ((signed >> amount) as u64) & mask
                            }
                            _ => unreachable!(),
                        }
                    };
                    Self::set_lane(&mut result, lane, bits, shifted);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedShiftVariable {
                dst,
                src,
                count,
                mask: write_mask,
                width,
                elem,
                shift,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let input = Self::read_vec(ctx, *src);
                let counts = Self::read_vec(ctx, *count);
                let mut result = [0u64; 16];
                let bits = elem.bytes() * 8;
                let lanes = width.lanes(*elem) as u8;
                let mask = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                for lane in 0..lanes {
                    let value = Self::get_lane(&input, lane, bits);
                    let amount = Self::get_lane(&counts, lane, bits);
                    let shifted = if amount >= u64::from(bits) {
                        if *shift == ShiftOp::Asr && value & (1u64 << (bits - 1)) != 0 {
                            mask
                        } else {
                            0
                        }
                    } else {
                        match shift {
                            ShiftOp::Lsl => (value << amount) & mask,
                            ShiftOp::Lsr => value >> amount,
                            ShiftOp::Asr => {
                                let signed = if bits == 64 {
                                    value as i64
                                } else {
                                    ((value << (64 - bits)) as i64) >> (64 - bits)
                                };
                                ((signed >> amount) as u64) & mask
                            }
                            _ => unreachable!(),
                        }
                    };
                    Self::set_lane(&mut result, lane, bits, shifted);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    write_mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedRotate {
                dst,
                src,
                count,
                mask,
                amount,
                width,
                elem,
                left,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let input = Self::read_vec(ctx, *src);
                let counts = count.map(|register| Self::read_vec(ctx, register));
                let mut result = [0u64; 16];
                let bits = elem.bytes() * 8;
                let lanes = width.lanes(*elem) as u8;
                for lane in 0..lanes {
                    let value = Self::get_lane(&input, lane, bits);
                    let raw_count = counts.as_ref().map_or(u64::from(*amount), |values| {
                        Self::get_lane(values, lane, bits)
                    });
                    let reduced = (raw_count % u64::from(bits)) as u32;
                    let rotated = match (bits, left) {
                        (32, true) => u64::from((value as u32).rotate_left(reduced)),
                        (32, false) => u64::from((value as u32).rotate_right(reduced)),
                        (64, true) => value.rotate_left(reduced),
                        (64, false) => value.rotate_right(reduced),
                        _ => unreachable!(),
                    };
                    Self::set_lane(&mut result, lane, bits, rotated);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86TernaryLogic {
                dst,
                src1,
                src2,
                src3,
                mask,
                imm,
                width,
                elem,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let c = Self::read_vec(ctx, *src3);
                let mut result = [0u64; 16];
                for word in 0..(width.bytes() / 8) as usize {
                    let mut out = 0u64;
                    for index in 0..8u8 {
                        if imm & (1 << index) != 0 {
                            out |= if index & 4 != 0 { a[word] } else { !a[word] }
                                & if index & 2 != 0 { b[word] } else { !b[word] }
                                & if index & 1 != 0 { c[word] } else { !c[word] };
                        }
                    }
                    result[word] = out;
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedFunnelShift {
                dst,
                src,
                fill,
                count,
                mask: write_mask,
                amount,
                width,
                elem,
                left,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let primary = Self::read_vec(ctx, *src);
                let secondary = Self::read_vec(ctx, *fill);
                let counts = count.map(|register| Self::read_vec(ctx, register));
                let bits = elem.bytes() * 8;
                let lanes = width.lanes(*elem) as u8;
                let mask = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let mut result = [0u64; 16];
                for lane in 0..lanes {
                    let value = Self::get_lane(&primary, lane, bits);
                    let fill_value = Self::get_lane(&secondary, lane, bits);
                    let raw_count = counts.as_ref().map_or(u64::from(*amount), |values| {
                        Self::get_lane(values, lane, bits)
                    });
                    let reduced = (raw_count % u64::from(bits)) as u32;
                    let shifted = if reduced == 0 {
                        value
                    } else if *left {
                        ((value << reduced) | (fill_value >> (bits - reduced))) & mask
                    } else {
                        (value >> reduced) | ((fill_value << (bits - reduced)) & mask)
                    };
                    Self::set_lane(&mut result, lane, bits, shifted);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    write_mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86MultiShiftQB {
                dst,
                control,
                source,
                mask,
                width,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let controls = Self::read_vec(ctx, *control);
                let data = Self::read_vec(ctx, *source);
                let mut result = [0u64; 16];
                for qword in 0..(width.bytes() / 8) as u8 {
                    let value = Self::get_lane(&data, qword, 64);
                    for byte in 0..8u8 {
                        let lane = qword * 8 + byte;
                        let shift = Self::get_lane(&controls, lane, 8) as u32 & 63;
                        Self::set_lane(&mut result, lane, 8, value.rotate_right(shift) & 0xFF);
                    }
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    VecElementType::I8,
                );
                Self::write_vec(ctx, *dst, result);
            }

            // ==================================================================
            // AVX10 OPERATIONS (Stubs - not yet implemented in interpreter)
            // ==================================================================
            OpKind::VFma {
                dst,
                src1,
                src2,
                acc,
                elem,
                lanes,
                negate_product,
                negate_acc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let c = Self::read_vec(ctx, *acc);
                let mut out = [0u64; 16];
                for lane in 0..*lanes {
                    let bits = match elem {
                        VecElementType::F32 => {
                            let mut av = f32::from_bits(Self::get_lane(&a, lane, 32) as u32);
                            let bv = f32::from_bits(Self::get_lane(&b, lane, 32) as u32);
                            let mut cv = f32::from_bits(Self::get_lane(&c, lane, 32) as u32);
                            if *negate_product {
                                av = -av;
                            }
                            if *negate_acc {
                                cv = -cv;
                            }
                            u64::from(av.mul_add(bv, cv).to_bits())
                        }
                        VecElementType::F64 => {
                            let mut av = f64::from_bits(Self::get_lane(&a, lane, 64));
                            let bv = f64::from_bits(Self::get_lane(&b, lane, 64));
                            let mut cv = f64::from_bits(Self::get_lane(&c, lane, 64));
                            if *negate_product {
                                av = -av;
                            }
                            if *negate_acc {
                                cv = -cv;
                            }
                            av.mul_add(bv, cv).to_bits()
                        }
                        _ => unreachable!("VFma requires F32 or F64 elements"),
                    };
                    Self::set_lane(&mut out, lane, elem.bytes() * 8, bits);
                }
                Self::write_vec(ctx, *dst, out);
            }

            OpKind::X86FP16Fma {
                dst,
                src1,
                src2,
                src3,
                mask,
                kind,
                order,
                round,
                lanes,
            } => {
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let third = Self::read_vec(ctx, *src3);
                let active = mask.map_or(u64::MAX, |mask| ctx.read_vreg(mask));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = match round {
                    FpRoundMode::Dynamic => match (mxcsr >> 13) & 3 {
                        0 => FpRoundMode::RoundNearest,
                        1 => FpRoundMode::RoundDown,
                        2 => FpRoundMode::RoundUp,
                        _ => FpRoundMode::RoundTowardZero,
                    },
                    FpRoundMode::RoundNearest
                    | FpRoundMode::RoundDown
                    | FpRoundMode::RoundUp
                    | FpRoundMode::RoundTowardZero => *round,
                    FpRoundMode::RoundNearestTiesAway => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let sign = 0x8000u64;
                let mut result = [0u64; 16];
                let mut status = 0u32;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        continue;
                    }
                    let sources = [
                        Self::get_lane(&first, lane, 16),
                        Self::get_lane(&second, lane, 16),
                        Self::get_lane(&third, lane, 16),
                    ];
                    if sources
                        .iter()
                        .any(|bits| Self::x86_simd_fp_is_denormal(*bits, X86_SIMD_F16))
                    {
                        status |= 1 << 1;
                    }

                    let (mut a, b, mut c) = match order {
                        X86FmaOrder::Order132 => (sources[0], sources[2], sources[1]),
                        X86FmaOrder::Order213 => (sources[1], sources[0], sources[2]),
                        X86FmaOrder::Order231 => (sources[1], sources[2], sources[0]),
                    };
                    let invalid_product = (Self::x86_simd_fp_is_zero(a, X86_SIMD_F16)
                        && Self::x86_simd_fp_is_infinite(b, X86_SIMD_F16))
                        || (Self::x86_simd_fp_is_infinite(a, X86_SIMD_F16)
                            && Self::x86_simd_fp_is_zero(b, X86_SIMD_F16));
                    let any_snan = sources
                        .iter()
                        .any(|bits| Self::x86_simd_fp_is_snan(*bits, X86_SIMD_F16));
                    if invalid_product || any_snan {
                        status |= 1;
                    }

                    let bits = if let Some(nan) = sources
                        .iter()
                        .copied()
                        .find(|bits| Self::x86_simd_fp_is_nan(*bits, X86_SIMD_F16))
                    {
                        Self::x86_simd_fp_quiet_nan(nan, X86_SIMD_F16)
                    } else {
                        let negate_product = matches!(
                            kind,
                            X86FmaKind::NegativeMultiplyAdd | X86FmaKind::NegativeMultiplySub
                        );
                        let negate_acc = match kind {
                            X86FmaKind::Sub | X86FmaKind::NegativeMultiplySub => true,
                            X86FmaKind::AddSub => lane & 1 == 0,
                            X86FmaKind::SubAdd => lane & 1 != 0,
                            X86FmaKind::Add | X86FmaKind::NegativeMultiplyAdd => false,
                        };
                        if negate_product {
                            a ^= sign;
                        }
                        if negate_acc {
                            c ^= sign;
                        }
                        let computed =
                            Self::x86_simd_fp_fma_non_nan(a, b, c, X86_SIMD_F16, mode, mxcsr);
                        status |= computed.status;
                        computed.bits
                    };
                    Self::set_lane(&mut result, lane, 16, bits);
                }
                if *round == FpRoundMode::Dynamic {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86FourFma {
                dst,
                src0,
                src1,
                src2,
                src3,
                mem,
                mask,
                scalar,
                negate_product,
                mask_zeroing,
            } => {
                // Snapshot every architectural input before computing: the
                // destination may be one of the four aligned source-block
                // registers.
                let old_dst = Self::read_vec(ctx, *dst);
                let sources = [
                    Self::read_vec(ctx, *src0),
                    Self::read_vec(ctx, *src1),
                    Self::read_vec(ctx, *src2),
                    Self::read_vec(ctx, *src3),
                ];
                let memory = Self::read_vec(ctx, *mem);
                let active = mask.map_or(u64::MAX, |mask| ctx.read_vreg(mask));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = match (mxcsr >> 13) & 3 {
                    0 => FpRoundMode::RoundNearest,
                    1 => FpRoundMode::RoundDown,
                    2 => FpRoundMode::RoundUp,
                    _ => FpRoundMode::RoundTowardZero,
                };
                let lanes = if *scalar { 1 } else { 16 };
                let mut result = [0u64; 16];
                for lane in 0..lanes {
                    if active & (1u64 << lane) != 0 || !mask_zeroing {
                        Self::set_lane(&mut result, lane, 32, Self::get_lane(&old_dst, lane, 32));
                    }
                }
                if *scalar {
                    for lane in 1..4u8 {
                        Self::set_lane(&mut result, lane, 32, Self::get_lane(&old_dst, lane, 32));
                    }
                }

                // Intel specifies exception priority by FMA boundary. Compute
                // all lanes in one boundary, commit its sticky status, and
                // stop before the next boundary when any exception is
                // unmasked. The architectural destination is written only
                // after all four boundaries complete.
                for stage in 0..4u8 {
                    let multiplier = Self::get_lane(&memory, stage, 32);
                    let mut stage_status = 0u32;
                    for lane in 0..lanes {
                        if active & (1u64 << lane) == 0 {
                            continue;
                        }
                        let computed = Self::x86_f32_fma_boundary(
                            Self::get_lane(&sources[stage as usize], lane, 32),
                            multiplier,
                            Self::get_lane(&result, lane, 32),
                            *negate_product,
                            mode,
                            mxcsr,
                        );
                        stage_status |= computed.status;
                        Self::set_lane(&mut result, lane, 32, computed.bits);
                    }
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= stage_status;
                    }
                    if Self::x86_simd_fp_unmasked(stage_status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86FourDotProduct {
                dst,
                src0,
                src1,
                src2,
                src3,
                mem,
                mask,
                saturating,
                mask_zeroing,
            } => {
                // Snapshot the destination and aligned source block before any
                // result is produced because the destination may alias a
                // source register used by a later iteration.
                let old_dst = Self::read_vec(ctx, *dst);
                let sources = [
                    Self::read_vec(ctx, *src0),
                    Self::read_vec(ctx, *src1),
                    Self::read_vec(ctx, *src2),
                    Self::read_vec(ctx, *src3),
                ];
                let memory = Self::read_vec(ctx, *mem);
                let active = mask.map_or(u64::MAX, |mask| ctx.read_vreg(mask));
                let mut result = [0u64; 16];

                for lane in 0..16u8 {
                    if active & (1u64 << lane) == 0 {
                        if !mask_zeroing {
                            Self::set_lane(
                                &mut result,
                                lane,
                                32,
                                Self::get_lane(&old_dst, lane, 32),
                            );
                        }
                        continue;
                    }

                    let mut accumulator =
                        i64::from(Self::get_lane(&old_dst, lane, 32) as u32 as i32);
                    for stage in 0..4u8 {
                        let memory_word = Self::get_lane(&memory, stage, 32) as u32;
                        let memory_low = i64::from(memory_word as u16 as i16);
                        let memory_high = i64::from((memory_word >> 16) as u16 as i16);
                        let source_low = i64::from(Self::get_lane(
                            &sources[stage as usize],
                            lane * 2,
                            16,
                        ) as u16 as i16);
                        let source_high =
                            i64::from(Self::get_lane(&sources[stage as usize], lane * 2 + 1, 16)
                                as u16 as i16);
                        let sum = accumulator + source_low * memory_low + source_high * memory_high;
                        accumulator = if *saturating {
                            sum.clamp(i64::from(i32::MIN), i64::from(i32::MAX))
                        } else {
                            i64::from(sum as i32)
                        };
                    }
                    Self::set_lane(&mut result, lane, 32, accumulator as u32 as u64);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86FP16Complex {
                dst,
                src1,
                src2,
                mask,
                width: _,
                pairs,
                scalar,
                mask_zeroing,
                accumulate,
                conjugate,
                round,
            } => {
                let old_dst = Self::read_vec(ctx, *dst);
                let left = Self::read_vec(ctx, *src1);
                let right = Self::read_vec(ctx, *src2);
                let active = mask.map_or(u64::MAX, |mask| ctx.read_vreg(mask));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = match round {
                    FpRoundMode::Dynamic => match (mxcsr >> 13) & 3 {
                        0 => FpRoundMode::RoundNearest,
                        1 => FpRoundMode::RoundDown,
                        2 => FpRoundMode::RoundUp,
                        _ => FpRoundMode::RoundTowardZero,
                    },
                    FpRoundMode::RoundNearest
                    | FpRoundMode::RoundDown
                    | FpRoundMode::RoundUp
                    | FpRoundMode::RoundTowardZero => *round,
                    FpRoundMode::RoundNearestTiesAway => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let mut result = [0u64; 16];
                if *scalar {
                    for lane in 2..8 {
                        Self::set_lane(&mut result, lane, 16, Self::get_lane(&left, lane, 16));
                    }
                }

                let mut status = 0u32;
                for pair in 0..*pairs {
                    let real_lane = pair * 2;
                    let imag_lane = real_lane + 1;
                    if active & (1u64 << pair) == 0 {
                        if !mask_zeroing {
                            Self::set_lane(
                                &mut result,
                                real_lane,
                                16,
                                Self::get_lane(&old_dst, real_lane, 16),
                            );
                            Self::set_lane(
                                &mut result,
                                imag_lane,
                                16,
                                Self::get_lane(&old_dst, imag_lane, 16),
                            );
                        }
                        continue;
                    }

                    let ar = Self::get_lane(&left, real_lane, 16);
                    let ai = Self::get_lane(&left, imag_lane, 16);
                    let br = Self::get_lane(&right, real_lane, 16);
                    let bi = Self::get_lane(&right, imag_lane, 16);
                    let (tmp_real, tmp_imag) = if *accumulate {
                        let dr = Self::get_lane(&old_dst, real_lane, 16);
                        let di = Self::get_lane(&old_dst, imag_lane, 16);
                        (
                            Self::x86_fp16_fma_boundary(ar, br, dr, false, mode, mxcsr),
                            Self::x86_fp16_fma_boundary(ai, br, di, false, mode, mxcsr),
                        )
                    } else {
                        (
                            Self::x86_simd_fp_mul(
                                ar,
                                br,
                                X86_SIMD_F16,
                                mode,
                                mxcsr & !((1 << 6) | (1 << 15)),
                            ),
                            Self::x86_simd_fp_mul(
                                ai,
                                br,
                                X86_SIMD_F16,
                                mode,
                                mxcsr & !((1 << 6) | (1 << 15)),
                            ),
                        )
                    };
                    status |= tmp_real.status | tmp_imag.status;
                    let real = Self::x86_fp16_fma_boundary(
                        ai,
                        bi,
                        tmp_real.bits,
                        !*conjugate,
                        mode,
                        mxcsr,
                    );
                    let imag =
                        Self::x86_fp16_fma_boundary(ar, bi, tmp_imag.bits, *conjugate, mode, mxcsr);
                    status |= real.status | imag.status;
                    Self::set_lane(&mut result, real_lane, 16, real.bits);
                    Self::set_lane(&mut result, imag_lane, 16, imag.bits);
                }
                if *round == FpRoundMode::Dynamic {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VPermute {
                dst,
                src1,
                src2,
                indices,
                elem,
                width,
                ..
            } => {
                let table1 = Self::read_vec(ctx, *src1);
                let table2 = src2.map(|reg| Self::read_vec(ctx, reg));
                let controls = Self::read_vec(ctx, *indices);
                let lanes = width.lanes(*elem) as u8;
                let bits = elem.bytes() * 8;
                let table_lanes = u64::from(lanes) * if table2.is_some() { 2 } else { 1 };
                debug_assert!(table_lanes.is_power_of_two());
                let mut result = [0u64; 16];
                for lane in 0..lanes {
                    let selected = Self::get_lane(&controls, lane, bits) & (table_lanes - 1);
                    let value = if selected < u64::from(lanes) {
                        Self::get_lane(&table1, selected as u8, bits)
                    } else {
                        Self::get_lane(
                            table2.as_ref().expect("second permute table"),
                            (selected - u64::from(lanes)) as u8,
                            bits,
                        )
                    };
                    Self::set_lane(&mut result, lane, bits, value);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PermuteBytesWords {
                dst,
                table1,
                table2,
                indices,
                mask,
                elem,
                width,
                zeroing,
                ..
            } => {
                let old = Self::read_vec(ctx, *dst);
                let first = Self::read_vec(ctx, *table1);
                let second = table2.map(|reg| Self::read_vec(ctx, reg));
                let controls = Self::read_vec(ctx, *indices);
                let lanes = width.lanes(*elem) as u8;
                let bits = elem.bytes() * 8;
                let table_lanes = u64::from(lanes) * if second.is_some() { 2 } else { 1 };
                let mut result = [0u64; 16];
                for lane in 0..lanes {
                    let selected = Self::get_lane(&controls, lane, bits) & (table_lanes - 1);
                    let value = if selected < u64::from(lanes) {
                        Self::get_lane(&first, selected as u8, bits)
                    } else {
                        Self::get_lane(
                            second.as_ref().expect("second permute table"),
                            (selected - u64::from(lanes)) as u8,
                            bits,
                        )
                    };
                    Self::set_lane(&mut result, lane, bits, value);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VPopcnt {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let input = Self::read_vec(ctx, *src);
                let bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                for lane in 0..width.lanes(*elem) as u8 {
                    let count = Self::get_lane(&input, lane, bits).count_ones();
                    Self::set_lane(&mut result, lane, bits, u64::from(count));
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VConflict {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let input = Self::read_vec(ctx, *src);
                let bits = elem.bytes() * 8;
                let lanes = width.lanes(*elem) as u8;
                let mut result = [0u64; 16];
                for lane in 0..lanes {
                    let value = Self::get_lane(&input, lane, bits);
                    let mut conflicts = 0u64;
                    for previous in 0..lane {
                        if Self::get_lane(&input, previous, bits) == value {
                            conflicts |= 1u64 << previous;
                        }
                    }
                    Self::set_lane(&mut result, lane, bits, conflicts);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VLeadingZeros {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let input = Self::read_vec(ctx, *src);
                let bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                for lane in 0..width.lanes(*elem) as u8 {
                    let value = Self::get_lane(&input, lane, bits);
                    let count = if bits == 32 {
                        (value as u32).leading_zeros()
                    } else {
                        value.leading_zeros()
                    };
                    Self::set_lane(&mut result, lane, bits, u64::from(count));
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VCvtFP32ToBF16 {
                dst,
                src1,
                src2,
                mask,
                width,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let first = Self::read_vec(ctx, *src1);
                let second = src2.map(|reg| Self::read_vec(ctx, reg));
                let fp32_lanes = width.lanes(VecElementType::F32) as u8;
                let mut result = [0u64; 16];
                if let Some(second) = second {
                    for lane in 0..fp32_lanes {
                        let low = Self::get_lane(&second, lane, 32) as u32;
                        let high = Self::get_lane(&first, lane, 32) as u32;
                        Self::set_lane(
                            &mut result,
                            lane,
                            16,
                            u64::from(Self::x86_fp32_to_bf16_bits(low)),
                        );
                        Self::set_lane(
                            &mut result,
                            lane + fp32_lanes,
                            16,
                            u64::from(Self::x86_fp32_to_bf16_bits(high)),
                        );
                    }
                } else {
                    for lane in 0..fp32_lanes {
                        let input = Self::get_lane(&first, lane, 32) as u32;
                        Self::set_lane(
                            &mut result,
                            lane,
                            16,
                            u64::from(Self::x86_fp32_to_bf16_bits(input)),
                        );
                    }
                }
                if let Some(mask_bits) = mask.map(|mask| ctx.read_vreg(mask)) {
                    let result_lanes = if second.is_some() {
                        fp32_lanes * 2
                    } else {
                        fp32_lanes
                    };
                    for lane in 0..result_lanes {
                        if mask_bits & (1u64 << lane) == 0 {
                            let inactive = if *zeroing {
                                0
                            } else {
                                Self::get_lane(&old, lane, 16)
                            };
                            Self::set_lane(&mut result, lane, 16, inactive);
                        }
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VDotProductBF16 {
                dst,
                acc,
                src1,
                src2,
                mask,
                width,
                zeroing,
            } => {
                let accumulator = Self::read_vec(ctx, *acc);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                for lane in 0..width.lanes(VecElementType::F32) as u8 {
                    let acc_bits = Self::get_lane(&accumulator, lane, 32) as u32;
                    let a_low = Self::get_lane(&first, lane * 2, 16) as u16;
                    let a_high = Self::get_lane(&first, lane * 2 + 1, 16) as u16;
                    let b_low = Self::get_lane(&second, lane * 2, 16) as u16;
                    let b_high = Self::get_lane(&second, lane * 2 + 1, 16) as u16;

                    // Intel Table 5-4 overrides evaluation order for NaN
                    // propagation: low pair, high pair, then accumulator.
                    let nan = [a_low, b_low, a_high, b_high]
                        .into_iter()
                        .find_map(|value| {
                            Self::x86_bf16_is_nan(value).then(|| Self::x86_bf16_quiet_nan(value))
                        })
                        .or_else(|| {
                            Self::x86_simd_fp_is_nan(u64::from(acc_bits), X86_SIMD_F32).then(|| {
                                Self::x86_simd_fp_quiet_nan(u64::from(acc_bits), X86_SIMD_F32)
                                    as u32
                            })
                        });
                    let value = if let Some(nan) = nan {
                        nan
                    } else {
                        let acc = Self::x86_fp32_ftz(acc_bits);
                        let high = f32::from_bits(Self::x86_bf16_to_fp32_daz(a_high)).mul_add(
                            f32::from_bits(Self::x86_bf16_to_fp32_daz(b_high)),
                            f32::from_bits(acc),
                        );
                        let high_bits = if high.is_nan() {
                            0xFFC0_0000
                        } else {
                            Self::x86_fp32_ftz(high.to_bits())
                        };
                        let low = f32::from_bits(Self::x86_bf16_to_fp32_daz(a_low)).mul_add(
                            f32::from_bits(Self::x86_bf16_to_fp32_daz(b_low)),
                            f32::from_bits(high_bits),
                        );
                        if low.is_nan() {
                            0xFFC0_0000
                        } else {
                            Self::x86_fp32_ftz(low.to_bits())
                        }
                    };
                    Self::set_lane(&mut result, lane, 32, u64::from(value));
                }
                Self::apply_vector_mask(
                    &mut result,
                    &accumulator,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    VecElementType::F32,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VMultiplyAdd52 {
                dst,
                acc,
                src1,
                src2,
                mask,
                width,
                high,
                zeroing,
            } => {
                const MASK52: u64 = (1u64 << 52) - 1;
                let accumulator = Self::read_vec(ctx, *acc);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                for lane in 0..width.lanes(VecElementType::I64) as u8 {
                    let lhs = Self::get_lane(&first, lane, 64) & MASK52;
                    let rhs = Self::get_lane(&second, lane, 64) & MASK52;
                    let product = u128::from(lhs) * u128::from(rhs);
                    let addend = if *high {
                        ((product >> 52) as u64) & MASK52
                    } else {
                        product as u64 & MASK52
                    };
                    let value = Self::get_lane(&accumulator, lane, 64).wrapping_add(addend);
                    Self::set_lane(&mut result, lane, 64, value);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &accumulator,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    VecElementType::I64,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VDotProductExt {
                dst,
                acc,
                src1,
                src2,
                src_elem,
                acc_elem,
                width,
                src1_signed,
                src2_signed,
                saturate,
            } => {
                debug_assert!(matches!(src_elem, VecElementType::I8 | VecElementType::I16));
                debug_assert_eq!(*acc_elem, VecElementType::I32);

                // Snapshot all operands before the architectural accumulator is
                // overwritten. VEX dot products permit dst to alias either
                // multiplicand as well as the implicit accumulator.
                let accumulator = Self::read_vec(ctx, *acc);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let src_bits = src_elem.bytes() * 8;
                let terms = 32 / src_bits;
                let lanes = width.lanes(VecElementType::I32) as u8;
                let src_mask = (1u64 << src_bits) - 1;
                let sign_extend = |value: u64| -> i128 {
                    let shift = 128 - src_bits;
                    (i128::from(value) << shift) >> shift
                };
                let unsigned_result = !src1_signed && !src2_signed;
                let mut result = [0u64; 16];

                for lane in 0..lanes {
                    let acc_raw = Self::get_lane(&accumulator, lane, 32) as u32;
                    let mut sum = if unsigned_result {
                        i128::from(acc_raw)
                    } else {
                        i128::from(acc_raw as i32)
                    };
                    let first_term = u32::from(lane) * terms;
                    for term in 0..terms {
                        let source_lane = (first_term + term) as u8;
                        let a_raw = Self::get_lane(&first, source_lane, src_bits) & src_mask;
                        let b_raw = Self::get_lane(&second, source_lane, src_bits) & src_mask;
                        let a = if *src1_signed {
                            sign_extend(a_raw)
                        } else {
                            i128::from(a_raw)
                        };
                        let b = if *src2_signed {
                            sign_extend(b_raw)
                        } else {
                            i128::from(b_raw)
                        };
                        sum += a * b;
                    }
                    let value = if *saturate {
                        if unsigned_result {
                            sum.clamp(0, i128::from(u32::MAX)) as u32
                        } else {
                            sum.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32 as u32
                        }
                    } else {
                        sum as u32
                    };
                    Self::set_lane(&mut result, lane, 32, u64::from(value));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VFP16Arith {
                dst,
                src1,
                src2,
                mask,
                op,
                round,
                width,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let rounding = match round {
                    FpRoundMode::Dynamic => ((mxcsr >> 13) & 3) as u8,
                    FpRoundMode::RoundNearest => 0,
                    FpRoundMode::RoundDown => 1,
                    FpRoundMode::RoundUp => 2,
                    FpRoundMode::RoundTowardZero => 3,
                    FpRoundMode::RoundNearestTiesAway => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let mask_bits = mask.map(|mask| ctx.read_vreg(mask));
                let mut result = [0u64; 16];
                let mut status = 0u32;
                for lane in 0..width.lanes(VecElementType::F16) as u8 {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        if !*zeroing {
                            Self::set_lane(&mut result, lane, 16, Self::get_lane(&old, lane, 16));
                        }
                        continue;
                    }
                    let a_bits = Self::get_lane(&first, lane, 16) as u16;
                    let b_bits = Self::get_lane(&second, lane, 16) as u16;
                    let a = Self::x86_fp16_to_f32(a_bits);
                    let b = Self::x86_fp16_to_f32(b_bits);
                    let value = match op {
                        Avx10FP16Op::Min | Avx10FP16Op::Max => {
                            // AVX512-FP16 always handles denormal FP16 inputs;
                            // MXCSR.DAZ is ignored, but the denormal-operand
                            // exception remains architecturally observable.
                            if Self::x86_simd_fp_is_denormal(u64::from(a_bits), X86_SIMD_F16)
                                || Self::x86_simd_fp_is_denormal(u64::from(b_bits), X86_SIMD_F16)
                            {
                                status |= 1 << 1;
                            }
                            if Self::x86_simd_fp_is_snan(u64::from(a_bits), X86_SIMD_F16)
                                || Self::x86_simd_fp_is_snan(u64::from(b_bits), X86_SIMD_F16)
                            {
                                status |= 1;
                            }
                            // Intel MIN/MAX selects source 2 for unordered or
                            // equal operands. Preserve the selected FP16 bits
                            // exactly, including an SNaN in source 2.
                            if (*op == Avx10FP16Op::Min && a < b)
                                || (*op == Avx10FP16Op::Max && a > b)
                            {
                                a_bits
                            } else {
                                b_bits
                            }
                        }
                        Avx10FP16Op::Add => Self::x86_f32_to_fp16(a + b, rounding),
                        Avx10FP16Op::Sub => Self::x86_f32_to_fp16(a - b, rounding),
                        Avx10FP16Op::Mul => Self::x86_f32_to_fp16(a * b, rounding),
                        Avx10FP16Op::Div => Self::x86_f32_to_fp16(a / b, rounding),
                        Avx10FP16Op::Sqrt => Self::x86_f32_to_fp16(a.sqrt(), rounding),
                    };
                    Self::set_lane(&mut result, lane, 16, u64::from(value));
                }
                if matches!(op, Avx10FP16Op::Min | Avx10FP16Op::Max)
                    && *round == FpRoundMode::Dynamic
                {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VMin { .. }
            | OpKind::VCvtBF16ToFP32 { .. }
            | OpKind::VCvtFpToIntSat { .. }
            | OpKind::VMinMax { .. } => {
                // AVX10 operations not yet implemented in interpreter
                // These would require full vector register state tracking
                ctx.request_exit(ExitReason::Undefined {
                    addr: ctx.pc,
                    opcode: 0,
                });
            }
        }

        Ok(())
    }


    /// Execute block terminator
    pub(crate) fn execute_terminator(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        term: &Terminator,
    ) -> BlockResult {
        match term {
            Terminator::Branch { target } => {
                let addr = self
                    .block_addrs
                    .get(target)
                    .copied()
                    .unwrap_or(target.0 as u64);
                BlockResult::Continue(addr)
            }

            Terminator::CondBranch {
                cond,
                true_target,
                false_target,
            } => {
                let cond_val = ctx.read_vreg(*cond);
                let target = if cond_val != 0 {
                    true_target
                } else {
                    false_target
                };
                let addr = self
                    .block_addrs
                    .get(target)
                    .copied()
                    .unwrap_or(target.0 as u64);
                BlockResult::Continue(addr)
            }

            Terminator::Switch {
                index,
                targets,
                default,
            } => {
                let idx = ctx.read_vreg(*index) as usize;
                let target = if idx < targets.len() {
                    &targets[idx]
                } else {
                    default
                };
                let addr = self
                    .block_addrs
                    .get(target)
                    .copied()
                    .unwrap_or(target.0 as u64);
                BlockResult::Continue(addr)
            }

            Terminator::IndirectBranch { target, .. } => {
                let addr = ctx.read_vreg(*target);
                BlockResult::Continue(addr)
            }

            Terminator::IndirectBranchMem { addr, .. } => {
                let target_addr = self.compute_address(ctx, addr);
                let val = self
                    .load_memory(memory, target_addr, MemWidth::B8, SignExtend::Zero)
                    .unwrap_or(0);
                BlockResult::Continue(val)
            }

            Terminator::Return { values: _ } => {
                // Get return address from arch-specific location
                let ret_addr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => {
                        // Pop from stack
                        let rsp = x86.gpr[4];
                        let mut buf = [0u8; 8];
                        if memory.read(rsp, &mut buf).is_ok() {
                            u64::from_le_bytes(buf)
                        } else {
                            0
                        }
                    }
                    ArchRegState::Aarch64(arm) => arm.x[30], // LR
                    ArchRegState::Hexagon(hex) => hex.lr as u64,
                    ArchRegState::RiscV(rv) => rv.x[1], // ra
                };
                BlockResult::Exit(ExitReason::Return { to: ret_addr })
            }

            Terminator::Call {
                target,
                args: _,
                continuation,
            } => {
                let target_addr = match target {
                    CallTarget::GuestAddr(addr) => *addr,
                    CallTarget::GuestAddrInterworking { addr, .. } => *addr,
                    CallTarget::Direct(fid) => self
                        .func_cache
                        .get(&(fid.0 as u64))
                        .map(|f| f.guest_range.0)
                        .unwrap_or(0),
                    CallTarget::Indirect(reg) => ctx.read_vreg(*reg),
                    CallTarget::IndirectInterworking(reg) => {
                        u64::from(ctx.read_vreg(*reg) as u32) & !1
                    }
                    CallTarget::IndirectMem(addr) => {
                        let target_addr = self.compute_address(ctx, addr);
                        self.load_memory(memory, target_addr, MemWidth::B8, SignExtend::Zero)
                            .unwrap_or(0)
                    }
                    CallTarget::Runtime(_) => {
                        // Return to continuation for runtime calls
                        let addr = self
                            .block_addrs
                            .get(continuation)
                            .copied()
                            .unwrap_or(continuation.0 as u64);
                        return BlockResult::Continue(addr);
                    }
                };
                BlockResult::Continue(target_addr)
            }

            Terminator::TailCall { target, args: _ } => {
                let target_addr = match target {
                    CallTarget::GuestAddr(addr) => *addr,
                    CallTarget::GuestAddrInterworking { addr, .. } => *addr,
                    CallTarget::Direct(fid) => self
                        .func_cache
                        .get(&(fid.0 as u64))
                        .map(|f| f.guest_range.0)
                        .unwrap_or(0),
                    CallTarget::Indirect(reg) => ctx.read_vreg(*reg),
                    CallTarget::IndirectInterworking(reg) => {
                        u64::from(ctx.read_vreg(*reg) as u32) & !1
                    }
                    CallTarget::IndirectMem(addr) => {
                        let target_addr = self.compute_address(ctx, addr);
                        self.load_memory(memory, target_addr, MemWidth::B8, SignExtend::Zero)
                            .unwrap_or(0)
                    }
                    CallTarget::Runtime(_) => 0,
                };
                BlockResult::Continue(target_addr)
            }

            Terminator::Trap { kind } => {
                match kind {
                    TrapKind::Halt => BlockResult::Exit(ExitReason::Halt),
                    TrapKind::Breakpoint => {
                        BlockResult::Exit(ExitReason::Breakpoint { addr: ctx.pc })
                    }
                    TrapKind::SystemCall => {
                        // Already handled in Syscall op
                        BlockResult::Continue(ctx.pc)
                    }
                    TrapKind::Undefined | TrapKind::InvalidOpcode => {
                        BlockResult::Exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        })
                    }
                    TrapKind::DivideByZero | TrapKind::Overflow | TrapKind::Bounds => {
                        BlockResult::Exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        })
                    }
                }
            }

            Terminator::Unreachable => BlockResult::Exit(ExitReason::Undefined {
                addr: ctx.pc,
                opcode: 0,
            }),
        }
    }
}
