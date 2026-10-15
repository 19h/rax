//! eBPF (IETF RFC 9669) and Solana SBPF (V0–V4) instruction lifter for SMIR.
//!
//! Lifts decoded [`EbpfInsn`] instructions into canonical [`SmirOp`] sequences
//! and [`ControlFlow`] terminators, preserving all dialect-specific semantics:
//! - RFC 9669 sanitized unsigned/signed division and remainder (`DIV`/`MOD` by 0
//!   and `SDIV`/`SMOD` `MIN / -1` overflow).
//! - Solana SBPF trapping unsigned/signed division and remainder (`V0`–`V4` and
//!   `V2` `PQR` class), plus 128-bit high-half multiplication (`UHMUL64`, `SHMUL64`).
//! - 32-bit ALU sign-extension (`SBPF` `V0`, `V1`, `V3`, `V4` for `ADD32`,
//!   `SUB32`, `MUL32`, and `V2` `MOV32_REG`) vs zero-extension.
//! - Solana `SBPF` `V2` reversed `SUB_IMM` operand order (`dst = imm - dst`).
//! - Endian conversion (`LE`, `BE`, `BSWAP`) and big-endian memory loads/stores.
//! - Atomic memory operations (`BPF_ATOMIC` `ADD`, `OR`, `AND`, `XOR`, `XCHG`,
//!   `CMPXCHG`, with and without `BPF_FETCH`).

use std::collections::{BTreeMap, BTreeSet};

mod control_flow;

use crate::isa::ebpf::decode::{
    AluWidth, AtomicKind, BinAluOp, EbpfDecodeError, EbpfDialect, EbpfInsn, EbpfMemWidth, EbpfOp,
    EbpfSrc, EndianOp, INSN_SIZE, JmpCond, SbpfVersion, decode_insn,
};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction};
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

/// eBPF and Solana SBPF instruction lifter.
#[derive(Clone, Debug)]
pub struct EbpfLifter {
    /// Target dialect (RFC 9669 or Solana SBPF V0–V4).
    pub dialect: EbpfDialect,
    /// Instruction and memory byte order.
    pub endian: Endian,
    /// Legacy SBPF function keys mapped to absolute guest byte addresses.
    pub function_registry: BTreeMap<u32, u64>,
    /// Loader syscall keys, needed to preserve legacy dual resolution.
    pub syscall_keys: BTreeSet<u32>,
}

impl EbpfLifter {
    /// Create a lifter with an explicit dialect and endianness.
    pub const fn new(dialect: EbpfDialect, endian: Endian) -> Self {
        Self {
            dialect,
            endian,
            function_registry: BTreeMap::new(),
            syscall_keys: BTreeSet::new(),
        }
    }

    /// Create an IETF RFC 9669 / Linux kernel v4 little-endian eBPF lifter.
    pub const fn rfc9669() -> Self {
        Self::new(EbpfDialect::Rfc9669, Endian::Little)
    }

    /// Create a Solana SBPF lifter for the specified [`SbpfVersion`].
    pub const fn solana(version: SbpfVersion) -> Self {
        Self::new(EbpfDialect::Solana(version), Endian::Little)
    }

    /// Register the same executable function key/address used by the direct CPU.
    pub fn register_function(&mut self, key: u32, target_pc: u64) {
        self.function_registry.insert(key, target_pc);
    }

    /// Register a loader syscall key. A legacy key may also name an internal
    /// function; SMIR exits to the host first and resumes the internal call.
    pub fn register_syscall(&mut self, key: u32) {
        self.syscall_keys.insert(key);
    }

    fn emit_syscall(
        &self,
        key: u32,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let num = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Mov {
                dst: num,
                src: SrcOperand::Imm(key as i64),
                width: OpWidth::W64,
            },
        ));
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Syscall {
                num,
                args: (1..=5).map(Self::reg).collect(),
            },
        ));
    }

    #[inline]
    fn reg(num: u8) -> VReg {
        VReg::Arch(ArchReg::Ebpf(EbpfReg::R(num)))
    }

    #[inline]
    const fn alu_op_width(width: AluWidth) -> OpWidth {
        match width {
            AluWidth::W32 => OpWidth::W32,
            AluWidth::W64 => OpWidth::W64,
        }
    }

    #[inline]
    const fn mem_width(width: EbpfMemWidth) -> MemWidth {
        match width {
            EbpfMemWidth::B1 => MemWidth::B1,
            EbpfMemWidth::B2 => MemWidth::B2,
            EbpfMemWidth::B4 => MemWidth::B4,
            EbpfMemWidth::B8 => MemWidth::B8,
        }
    }

    fn operand_to_src(op: EbpfSrc, width: AluWidth) -> SrcOperand {
        match op {
            EbpfSrc::Reg(r) => SrcOperand::Reg(Self::reg(r)),
            EbpfSrc::Imm(imm) => match width {
                AluWidth::W32 => SrcOperand::Imm((imm as u32) as i64),
                AluWidth::W64 => SrcOperand::Imm(imm as i64),
            },
        }
    }

    fn operand_to_vreg(
        op: EbpfSrc,
        width: AluWidth,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        match op {
            EbpfSrc::Reg(r) => Self::reg(r),
            EbpfSrc::Imm(imm) => {
                let v = ctx.alloc_vreg();
                let imm_val = match width {
                    AluWidth::W32 => (imm as u32) as i64,
                    AluWidth::W64 => imm as i64,
                };
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Mov {
                        dst: v,
                        src: SrcOperand::Imm(imm_val),
                        width: Self::alu_op_width(width),
                    },
                ));
                v
            }
        }
    }

    /// Emit RFC 9669 sanitized unsigned/signed division or modulo:
    /// - Unsigned `DIV` by 0 -> `0`
    /// - Unsigned `MOD` by 0 -> `dst` (masked to 32 bits for `ALU32`)
    /// - Signed `SDIV` by 0 -> `0`; `MIN / -1` -> `MIN` (masked to 32 bits for `ALU32`)
    /// - Signed `SMOD` by 0 -> `dst` (masked to 32 bits for `ALU32`); `MIN % -1` -> `0`
    #[allow(clippy::too_many_arguments)]
    fn emit_sanitized_div_rem(
        &self,
        is_signed: bool,
        is_rem: bool,
        imm_zero_extended_64: bool,
        width: AluWidth,
        dst: u8,
        src: EbpfSrc,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let op_width = Self::alu_op_width(width);
        let dst_reg = Self::reg(dst);
        let src_vreg = match src {
            EbpfSrc::Reg(r) => Self::reg(r),
            EbpfSrc::Imm(imm) => {
                let v = ctx.alloc_vreg();
                let imm_val = match (width, imm_zero_extended_64) {
                    (AluWidth::W32, _) | (AluWidth::W64, true) => (imm as u32) as i64,
                    (AluWidth::W64, false) => imm as i64,
                };
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Mov {
                        dst: v,
                        src: SrcOperand::Imm(imm_val),
                        width: op_width,
                    },
                ));
                v
            }
        };

        // Snapshot initial `dst` and `src` values (masked to width) in case `dst == src`
        let lhs = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Mov {
                dst: lhs,
                src: SrcOperand::Reg(dst_reg),
                width: op_width,
            },
        ));

        let rhs = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Mov {
                dst: rhs,
                src: SrcOperand::Reg(src_vreg),
                width: op_width,
            },
        ));

        // 1. Check `rhs == 0`
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Cmp {
                src1: rhs,
                src2: SrcOperand::Imm(0),
                width: op_width,
            },
        ));
        let is_zero = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::SetCC {
                dst: is_zero,
                cond: Condition::Eq,
                width: OpWidth::W8,
            },
        ));

        let (need_safe_div, is_ovf_opt) = if is_signed {
            let min_val: i64 = match width {
                AluWidth::W32 => i32::MIN as u32 as i64,
                AluWidth::W64 => i64::MIN,
            };
            let neg_one: i64 = match width {
                AluWidth::W32 => (-1i32 as u32) as i64,
                AluWidth::W64 => -1i64,
            };

            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Cmp {
                    src1: lhs,
                    src2: SrcOperand::Imm(min_val),
                    width: op_width,
                },
            ));
            let is_min = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::SetCC {
                    dst: is_min,
                    cond: Condition::Eq,
                    width: OpWidth::W8,
                },
            ));

            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Cmp {
                    src1: rhs,
                    src2: SrcOperand::Imm(neg_one),
                    width: op_width,
                },
            ));
            let is_neg1 = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::SetCC {
                    dst: is_neg1,
                    cond: Condition::Eq,
                    width: OpWidth::W8,
                },
            ));

            let is_ovf = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::And {
                    dst: is_ovf,
                    src1: is_min,
                    src2: SrcOperand::Reg(is_neg1),
                    width: OpWidth::W8,
                    flags: FlagUpdate::None,
                },
            ));

            let need_safe = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Or {
                    dst: need_safe,
                    src1: is_zero,
                    src2: SrcOperand::Reg(is_ovf),
                    width: OpWidth::W8,
                    flags: FlagUpdate::None,
                },
            ));
            (need_safe, Some(is_ovf))
        } else {
            (is_zero, None)
        };

        // Substitute divisor = 1 when `need_safe_div != 0` so `DivU`/`DivS` never traps
        let one_vreg = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Mov {
                dst: one_vreg,
                src: SrcOperand::Imm(1),
                width: op_width,
            },
        ));
        let zero_vreg = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Mov {
                dst: zero_vreg,
                src: SrcOperand::Imm(0),
                width: op_width,
            },
        ));

        let safe_rhs = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Select {
                dst: safe_rhs,
                cond: need_safe_div,
                src_true: one_vreg,
                src_false: rhs,
                width: op_width,
            },
        ));

        let q = ctx.alloc_vreg();
        let r = ctx.alloc_vreg();
        let div_kind = if is_signed {
            OpKind::DivS {
                quot: q,
                rem: Some(r),
                src1: lhs,
                src2: SrcOperand::Reg(safe_rhs),
                width: op_width,
                flags: FlagUpdate::None,
            }
        } else {
            OpKind::DivU {
                quot: q,
                rem: Some(r),
                src1: lhs,
                src2: SrcOperand::Reg(safe_rhs),
                width: op_width,
                flags: FlagUpdate::None,
            }
        };
        ops.push(SmirOp::new(ctx.next_op_id(), addr, div_kind));

        let normal_res = if is_rem { r } else { q };

        // Apply signed overflow override if applicable:
        // - SDIV(MIN, -1) -> lhs (which is MIN masked to width)
        // - SMOD(MIN, -1) -> 0
        let after_ovf = if let Some(is_ovf) = is_ovf_opt {
            let tmp = ctx.alloc_vreg();
            let ovf_val = if is_rem { zero_vreg } else { lhs };
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Select {
                    dst: tmp,
                    cond: is_ovf,
                    src_true: ovf_val,
                    src_false: normal_res,
                    width: op_width,
                },
            ));
            tmp
        } else {
            normal_res
        };

        // Apply divide-by-zero override:
        // - DIV/SDIV by 0 -> 0
        // - MOD/SMOD by 0 -> lhs (which is dst masked to width)
        let zero_val = if is_rem { lhs } else { zero_vreg };
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Select {
                dst: dst_reg,
                cond: is_zero,
                src_true: zero_val,
                src_false: after_ovf,
                width: op_width,
            },
        ));
    }

    fn lift_bin_alu(
        &self,
        width: AluWidth,
        op: BinAluOp,
        dst: u8,
        src: EbpfSrc,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let op_width = Self::alu_op_width(width);
        let dst_reg = Self::reg(dst);
        let mut sign_extend_32_after = false;

        match op {
            BinAluOp::Add { sign_extend_32 } => {
                sign_extend_32_after = sign_extend_32;
                let src2 = Self::operand_to_src(src, width);
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Add {
                        dst: dst_reg,
                        src1: dst_reg,
                        src2,
                        width: op_width,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            BinAluOp::Sub {
                sign_extend_32,
                swap_operands,
            } => {
                sign_extend_32_after = sign_extend_32;
                if swap_operands {
                    let imm_vreg = Self::operand_to_vreg(src, width, addr, ctx, ops);
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Sub {
                            dst: dst_reg,
                            src1: imm_vreg,
                            src2: SrcOperand::Reg(dst_reg),
                            width: op_width,
                            flags: FlagUpdate::None,
                        },
                    ));
                } else {
                    let src2 = Self::operand_to_src(src, width);
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Sub {
                            dst: dst_reg,
                            src1: dst_reg,
                            src2,
                            width: op_width,
                            flags: FlagUpdate::None,
                        },
                    ));
                }
            }
            BinAluOp::Mul { sign_extend_32 } => {
                sign_extend_32_after = sign_extend_32;
                let src2 = Self::operand_to_src(src, width);
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::MulU {
                        dst_lo: dst_reg,
                        dst_hi: None,
                        src1: dst_reg,
                        src2,
                        width: op_width,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            BinAluOp::Uhmul64 => {
                // Solana SBPF V2 PQR UHMUL64 zero-extends immediate operands from 32 bits
                let src2 = match src {
                    EbpfSrc::Reg(r) => SrcOperand::Reg(Self::reg(r)),
                    EbpfSrc::Imm(imm) => SrcOperand::Imm((imm as u32) as i64),
                };
                let discard_lo = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::MulU {
                        dst_lo: discard_lo,
                        dst_hi: Some(dst_reg),
                        src1: dst_reg,
                        src2,
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            BinAluOp::Shmul64 => {
                let src2 = Self::operand_to_src(src, AluWidth::W64);
                let discard_lo = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::MulS {
                        dst_lo: discard_lo,
                        dst_hi: Some(dst_reg),
                        src1: dst_reg,
                        src2,
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            BinAluOp::Udiv {
                trap_on_zero,
                imm_zero_extended_64,
            }
            | BinAluOp::Urem {
                trap_on_zero,
                imm_zero_extended_64,
            } => {
                let is_rem = matches!(op, BinAluOp::Urem { .. });
                if trap_on_zero {
                    let src2 = match (src, width, imm_zero_extended_64) {
                        (EbpfSrc::Reg(r), _, _) => SrcOperand::Reg(Self::reg(r)),
                        (EbpfSrc::Imm(imm), AluWidth::W32, _)
                        | (EbpfSrc::Imm(imm), AluWidth::W64, true) => {
                            SrcOperand::Imm((imm as u32) as i64)
                        }
                        (EbpfSrc::Imm(imm), AluWidth::W64, false) => SrcOperand::Imm(imm as i64),
                    };
                    let q = if is_rem { ctx.alloc_vreg() } else { dst_reg };
                    let r = if is_rem { Some(dst_reg) } else { None };
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::DivU {
                            quot: q,
                            rem: r,
                            src1: dst_reg,
                            src2,
                            width: op_width,
                            flags: FlagUpdate::None,
                        },
                    ));
                } else {
                    self.emit_sanitized_div_rem(
                        false,
                        is_rem,
                        imm_zero_extended_64,
                        width,
                        dst,
                        src,
                        addr,
                        ctx,
                        ops,
                    );
                }
            }
            BinAluOp::Sdiv {
                trap_on_zero_or_overflow,
            }
            | BinAluOp::Srem {
                trap_on_zero_or_overflow,
            } => {
                let is_rem = matches!(op, BinAluOp::Srem { .. });
                if trap_on_zero_or_overflow {
                    let src2 = Self::operand_to_src(src, width);
                    let q = if is_rem { ctx.alloc_vreg() } else { dst_reg };
                    let r = if is_rem { Some(dst_reg) } else { None };
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::DivS {
                            quot: q,
                            rem: r,
                            src1: dst_reg,
                            src2,
                            width: op_width,
                            flags: FlagUpdate::None,
                        },
                    ));
                } else {
                    self.emit_sanitized_div_rem(
                        true, is_rem, false, width, dst, src, addr, ctx, ops,
                    );
                }
            }
            BinAluOp::Or => {
                let src2 = Self::operand_to_src(src, width);
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Or {
                        dst: dst_reg,
                        src1: dst_reg,
                        src2,
                        width: op_width,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            BinAluOp::And => {
                let src2 = Self::operand_to_src(src, width);
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::And {
                        dst: dst_reg,
                        src1: dst_reg,
                        src2,
                        width: op_width,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            BinAluOp::Lsh => {
                let amount = Self::operand_to_src(src, width);
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Shl {
                        dst: dst_reg,
                        src: dst_reg,
                        amount,
                        width: op_width,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            BinAluOp::Rsh => {
                let amount = Self::operand_to_src(src, width);
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Shr {
                        dst: dst_reg,
                        src: dst_reg,
                        amount,
                        width: op_width,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            BinAluOp::Arsh => {
                let amount = Self::operand_to_src(src, width);
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Sar {
                        dst: dst_reg,
                        src: dst_reg,
                        amount,
                        width: op_width,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            BinAluOp::Xor => {
                let src2 = Self::operand_to_src(src, width);
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Xor {
                        dst: dst_reg,
                        src1: dst_reg,
                        src2,
                        width: op_width,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            BinAluOp::Mov {
                sign_extend_from_bits,
                sign_extend_to_64,
            } => {
                if let Some(bits) = sign_extend_from_bits {
                    let from_width = match bits {
                        8 => OpWidth::W8,
                        16 => OpWidth::W16,
                        32 => OpWidth::W32,
                        _ => OpWidth::W32,
                    };
                    let to_width = if sign_extend_to_64 {
                        OpWidth::W64
                    } else {
                        op_width
                    };
                    let src_vreg = Self::operand_to_vreg(src, width, addr, ctx, ops);
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::SignExtend {
                            dst: dst_reg,
                            src: src_vreg,
                            from_width,
                            to_width,
                        },
                    ));
                } else {
                    let src_op = Self::operand_to_src(src, width);
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Mov {
                            dst: dst_reg,
                            src: src_op,
                            width: op_width,
                        },
                    ));
                }
            }
            BinAluOp::Hor64Imm => {
                if let EbpfSrc::Imm(imm) = src {
                    let shifted = ((imm as u32 as u64) << 32) as i64;
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Or {
                            dst: dst_reg,
                            src1: dst_reg,
                            src2: SrcOperand::Imm(shifted),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                }
            }
        }

        if width == AluWidth::W32 && sign_extend_32_after {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::SignExtend {
                    dst: dst_reg,
                    src: dst_reg,
                    from_width: OpWidth::W32,
                    to_width: OpWidth::W64,
                },
            ));
        }
    }

    fn lift_decoded(
        &self,
        insn: &EbpfInsn,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let mut ops = Vec::new();
        let next_pc = addr.wrapping_add(u64::from(insn.size));

        match insn.op {
            EbpfOp::LoadImm64 {
                dst,
                src_type,
                imm64,
            } => {
                if self.dialect == EbpfDialect::Rfc9669 && src_type != 0 {
                    return Err(LiftError::Unsupported {
                        addr,
                        mnemonic: format!("LDDW subtype {src_type} requires a platform resolver"),
                    });
                }
                let dst_reg = Self::reg(dst);
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Mov {
                        dst: dst_reg,
                        src: SrcOperand::Imm64(imm64 as i64),
                        width: OpWidth::W64,
                    },
                ));
                Ok((ops, ControlFlow::Fallthrough))
            }

            EbpfOp::LoadReg {
                dst,
                base,
                off,
                width,
                sign_extend,
            } => {
                let dst_reg = Self::reg(dst);
                let base_reg = Self::reg(base);
                let mem_addr = Address::BaseOffset {
                    base: base_reg,
                    offset: i64::from(off),
                    disp_size: DispSize::Auto,
                };
                let mw = Self::mem_width(width);
                if self.endian == Endian::Big && width != EbpfMemWidth::B1 {
                    let raw_tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Load {
                            dst: raw_tmp,
                            addr: mem_addr,
                            width: mw,
                            sign: SignExtend::Zero,
                        },
                    ));
                    let swap_width = match width {
                        EbpfMemWidth::B2 => OpWidth::W16,
                        EbpfMemWidth::B4 => OpWidth::W32,
                        EbpfMemWidth::B8 => OpWidth::W64,
                        EbpfMemWidth::B1 => unreachable!(),
                    };
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Bswap {
                            dst: dst_reg,
                            src: raw_tmp,
                            width: swap_width,
                        },
                    ));
                    if sign_extend && swap_width != OpWidth::W64 {
                        ops.push(SmirOp::new(
                            ctx.next_op_id(),
                            addr,
                            OpKind::SignExtend {
                                dst: dst_reg,
                                src: dst_reg,
                                from_width: swap_width,
                                to_width: OpWidth::W64,
                            },
                        ));
                    }
                } else {
                    let sign = if sign_extend {
                        SignExtend::Sign
                    } else {
                        SignExtend::Zero
                    };
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Load {
                            dst: dst_reg,
                            addr: mem_addr,
                            width: mw,
                            sign,
                        },
                    ));
                }
                Ok((ops, ControlFlow::Fallthrough))
            }

            EbpfOp::StoreImm {
                base,
                off,
                imm,
                width,
            } => {
                let base_reg = Self::reg(base);
                let mem_addr = Address::BaseOffset {
                    base: base_reg,
                    offset: i64::from(off),
                    disp_size: DispSize::Auto,
                };
                let mw = Self::mem_width(width);
                let val_u64 = match width {
                    EbpfMemWidth::B1 => (imm as u8) as u64,
                    EbpfMemWidth::B2 => {
                        let v = imm as u16;
                        u64::from(if self.endian == Endian::Big {
                            v.swap_bytes()
                        } else {
                            v
                        })
                    }
                    EbpfMemWidth::B4 => {
                        let v = imm as u32;
                        u64::from(if self.endian == Endian::Big {
                            v.swap_bytes()
                        } else {
                            v
                        })
                    }
                    EbpfMemWidth::B8 => {
                        let v = (imm as i64) as u64;
                        if self.endian == Endian::Big {
                            v.swap_bytes()
                        } else {
                            v
                        }
                    }
                };
                let tmp = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Mov {
                        dst: tmp,
                        src: SrcOperand::Imm64(val_u64 as i64),
                        width: OpWidth::W64,
                    },
                ));
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Store {
                        src: tmp,
                        addr: mem_addr,
                        width: mw,
                    },
                ));
                Ok((ops, ControlFlow::Fallthrough))
            }

            EbpfOp::StoreReg {
                base,
                off,
                src,
                width,
            } => {
                let base_reg = Self::reg(base);
                let src_reg = Self::reg(src);
                let mem_addr = Address::BaseOffset {
                    base: base_reg,
                    offset: i64::from(off),
                    disp_size: DispSize::Auto,
                };
                let mw = Self::mem_width(width);
                let store_src = if self.endian == Endian::Big && width != EbpfMemWidth::B1 {
                    let swap_width = match width {
                        EbpfMemWidth::B2 => OpWidth::W16,
                        EbpfMemWidth::B4 => OpWidth::W32,
                        EbpfMemWidth::B8 => OpWidth::W64,
                        EbpfMemWidth::B1 => unreachable!(),
                    };
                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Bswap {
                            dst: tmp,
                            src: src_reg,
                            width: swap_width,
                        },
                    ));
                    tmp
                } else {
                    src_reg
                };
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Store {
                        src: store_src,
                        addr: mem_addr,
                        width: mw,
                    },
                ));
                Ok((ops, ControlFlow::Fallthrough))
            }

            EbpfOp::Atomic {
                base,
                off,
                src,
                width,
                kind,
            } => {
                let base_reg = Self::reg(base);
                let src_reg = Self::reg(src);
                let mem_addr = Address::BaseOffset {
                    base: base_reg,
                    offset: i64::from(off),
                    disp_size: DispSize::Auto,
                };
                let mw = Self::mem_width(width);
                match kind {
                    AtomicKind::Add { fetch }
                    | AtomicKind::Or { fetch }
                    | AtomicKind::And { fetch }
                    | AtomicKind::Xor { fetch } => {
                        let atomic_op = match kind {
                            AtomicKind::Add { .. } => AtomicOp::Add,
                            AtomicKind::Or { .. } => AtomicOp::Or,
                            AtomicKind::And { .. } => AtomicOp::And,
                            AtomicKind::Xor { .. } => AtomicOp::Xor,
                            _ => unreachable!(),
                        };
                        let old_dst = if fetch { src_reg } else { ctx.alloc_vreg() };
                        ops.push(SmirOp::new(
                            ctx.next_op_id(),
                            addr,
                            OpKind::AtomicRmw {
                                dst: old_dst,
                                addr: mem_addr,
                                src: src_reg,
                                op: atomic_op,
                                width: mw,
                                order: MemoryOrder::SeqCst,
                            },
                        ));
                    }
                    AtomicKind::Xchg => {
                        ops.push(SmirOp::new(
                            ctx.next_op_id(),
                            addr,
                            OpKind::AtomicRmw {
                                dst: src_reg,
                                addr: mem_addr,
                                src: src_reg,
                                op: AtomicOp::Swap,
                                width: mw,
                                order: MemoryOrder::SeqCst,
                            },
                        ));
                    }
                    AtomicKind::Cmpxchg => {
                        let r0_reg = Self::reg(0);
                        let success_tmp = ctx.alloc_vreg();
                        ops.push(SmirOp::new(
                            ctx.next_op_id(),
                            addr,
                            OpKind::Cas {
                                dst: r0_reg,
                                success: success_tmp,
                                addr: mem_addr,
                                expected: r0_reg,
                                new_val: src_reg,
                                width: mw,
                                order: MemoryOrder::SeqCst,
                            },
                        ));
                    }
                }
                Ok((ops, ControlFlow::Fallthrough))
            }

            EbpfOp::BinAlu {
                width,
                op,
                dst,
                src,
            } => {
                self.lift_bin_alu(width, op, dst, src, addr, ctx, &mut ops);
                Ok((ops, ControlFlow::Fallthrough))
            }

            EbpfOp::Neg { width, dst } => {
                let dst_reg = Self::reg(dst);
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Neg {
                        dst: dst_reg,
                        src: dst_reg,
                        width: Self::alu_op_width(width),
                        flags: FlagUpdate::None,
                    },
                ));
                Ok((ops, ControlFlow::Fallthrough))
            }

            EbpfOp::Endian { dst, op } => {
                let dst_reg = Self::reg(dst);
                let (swap, bits) = match (self.endian, op) {
                    (Endian::Little, EndianOp::ToLe(b)) | (Endian::Big, EndianOp::ToBe(b)) => {
                        (false, b)
                    }
                    (Endian::Little, EndianOp::ToBe(b))
                    | (Endian::Big, EndianOp::ToLe(b))
                    | (_, EndianOp::Bswap(b)) => (true, b),
                };
                let width = match bits {
                    16 => OpWidth::W16,
                    32 => OpWidth::W32,
                    64 => OpWidth::W64,
                    _ => {
                        return Err(LiftError::Unsupported {
                            addr,
                            mnemonic: format!("endian width {bits}"),
                        });
                    }
                };
                if swap {
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Bswap {
                            dst: dst_reg,
                            src: dst_reg,
                            width,
                        },
                    ));
                } else {
                    match width {
                        OpWidth::W16 | OpWidth::W32 => {
                            ops.push(SmirOp::new(
                                ctx.next_op_id(),
                                addr,
                                OpKind::ZeroExtend {
                                    dst: dst_reg,
                                    src: dst_reg,
                                    from_width: width,
                                    to_width: OpWidth::W64,
                                },
                            ));
                        }
                        _ => {}
                    }
                }
                Ok((ops, ControlFlow::Fallthrough))
            }

            EbpfOp::Jump { offset_insns } => {
                let delta_bytes = i64::from(offset_insns).wrapping_mul(INSN_SIZE as i64);
                let target = (next_pc as i64).wrapping_add(delta_bytes) as u64;
                Ok((ops, ControlFlow::Branch { target }))
            }

            EbpfOp::JumpCond {
                width,
                cond,
                dst,
                src,
                offset_insns,
            } => {
                let delta_bytes = i64::from(offset_insns).wrapping_mul(INSN_SIZE as i64);
                let target = (next_pc as i64).wrapping_add(delta_bytes) as u64;
                let fallthrough = next_pc;
                let op_width = Self::alu_op_width(width);
                let dst_reg = Self::reg(dst);
                let src2 = Self::operand_to_src(src, width);

                let smir_cond = match cond {
                    JmpCond::Jset => {
                        ops.push(SmirOp::new(
                            ctx.next_op_id(),
                            addr,
                            OpKind::Test {
                                src1: dst_reg,
                                src2,
                                width: op_width,
                            },
                        ));
                        Condition::Ne
                    }
                    other => {
                        ops.push(SmirOp::new(
                            ctx.next_op_id(),
                            addr,
                            OpKind::Cmp {
                                src1: dst_reg,
                                src2,
                                width: op_width,
                            },
                        ));
                        match other {
                            JmpCond::Jeq => Condition::Eq,
                            JmpCond::Jgt => Condition::Ugt,
                            JmpCond::Jge => Condition::Uge,
                            JmpCond::Jlt => Condition::Ult,
                            JmpCond::Jle => Condition::Ule,
                            JmpCond::Jne => Condition::Ne,
                            JmpCond::Jsgt => Condition::Sgt,
                            JmpCond::Jsge => Condition::Sge,
                            JmpCond::Jslt => Condition::Slt,
                            JmpCond::Jsle => Condition::Sle,
                            JmpCond::Jset => unreachable!(),
                        }
                    }
                };

                let cond_vreg = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::SetCC {
                        dst: cond_vreg,
                        cond: smir_cond,
                        width: OpWidth::W8,
                    },
                ));

                Ok((
                    ops,
                    ControlFlow::CondBranchReg {
                        cond: cond_vreg,
                        taken: target,
                        not_taken: fallthrough,
                    },
                ))
            }

            EbpfOp::CallImm { src_kind, imm } => {
                let static_syscalls = self
                    .dialect
                    .sbpf_version()
                    .is_none_or(|v| v.static_syscalls());
                if src_kind == 1 && static_syscalls {
                    let delta_bytes = i64::from(imm).wrapping_mul(INSN_SIZE as i64);
                    let target_addr = (next_pc as i64).wrapping_add(delta_bytes) as u64;
                    Ok((
                        ops,
                        ControlFlow::Call {
                            target: CallTarget::GuestAddr(target_addr),
                        },
                    ))
                } else if !static_syscalls && self.function_registry.contains_key(&(imm as u32)) {
                    if self.syscall_keys.contains(&(imm as u32)) {
                        self.emit_syscall(imm as u32, addr, ctx, &mut ops);
                    }
                    Ok((
                        ops,
                        ControlFlow::Call {
                            target: CallTarget::GuestAddr(self.function_registry[&(imm as u32)]),
                        },
                    ))
                } else {
                    self.emit_syscall(imm as u32, addr, ctx, &mut ops);
                    Ok((ops, ControlFlow::Syscall))
                }
            }

            EbpfOp::CallReg { target_reg } => Ok((
                ops,
                ControlFlow::Call {
                    target: CallTarget::Indirect(Self::reg(target_reg)),
                },
            )),

            EbpfOp::Exit => Ok((ops, ControlFlow::Return)),
        }
    }
}

impl Default for EbpfLifter {
    fn default() -> Self {
        Self::rfc9669()
    }
}

impl SmirLifter for EbpfLifter {
    fn source_arch(&self) -> SourceArch {
        SourceArch::Ebpf
    }

    fn lift_insn(
        &mut self,
        addr: GuestAddr,
        bytes: &[u8],
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if addr % INSN_SIZE as u64 != 0 {
            return Err(LiftError::InvalidEncoding {
                addr,
                bytes: bytes[..bytes.len().min(16)].to_vec(),
            });
        }
        let insn = match decode_insn(bytes, self.dialect, self.endian) {
            Ok(insn) => insn,
            Err(EbpfDecodeError::UnexpectedEof { needed, available }) => {
                return Err(LiftError::Incomplete {
                    addr,
                    have: available,
                    need: needed,
                });
            }
            Err(_) => {
                let take = bytes.len().min(16);
                return Err(LiftError::InvalidEncoding {
                    addr,
                    bytes: bytes[..take].to_vec(),
                });
            }
        };

        ctx.guest_pc = addr;
        let len = insn.size as usize;
        let (mut ops, control_flow) = self.lift_decoded(&insn, addr, ctx)?;
        ops.insert(
            0,
            SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::EbpfInstruction {
                    dialect: self.dialect,
                    endian: self.endian,
                    size: insn.size,
                },
            ),
        );
        let branch_targets = control_flow.direct_targets();

        Ok(LiftResult {
            ops,
            bytes_consumed: len,
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
        self.lift_block_with_end(addr, mem, ctx)
            .map(|(block, _)| block)
    }

    fn lift_function(
        &mut self,
        entry: GuestAddr,
        mem: &dyn MemoryReader,
        ctx: &mut LiftContext,
    ) -> Result<SmirFunction, LiftError> {
        self.lift_reachable_function(entry, mem, ctx)
    }
}
