//! x86_64 instruction lifter.
//!
//! This module lifts x86_64 machine code to SMIR. Unlike AArch64 which has a clean
//! decoder, x86 decoding is interleaved with lifting due to variable-length encoding.

use std::collections::HashSet;

use crate::smir::flags::FlagUpdate;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
};
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};
use crate::smir::memory::MemoryError;
use crate::smir::ops::{OpKind, SmirOp};
use crate::smir::types::*;

// ============================================================================
// x86_64 Lifter
// ============================================================================

/// x86_64 instruction lifter
pub struct X86_64Lifter {
    /// Whether to use strict mode (fail on unsupported instructions)
    strict: bool,
}

impl Default for X86_64Lifter {
    fn default() -> Self {
        Self::new()
    }
}

impl X86_64Lifter {
    /// Create a new x86_64 lifter
    pub fn new() -> Self {
        X86_64Lifter { strict: false }
    }

    /// Create a lifter in strict mode
    pub fn strict() -> Self {
        X86_64Lifter { strict: true }
    }
}

// ============================================================================
// Prefix Decoding
// ============================================================================

/// Lookup table for prefix detection
static PREFIX_LUT: [u8; 256] = {
    let mut lut = [0u8; 256];
    // Segment overrides
    lut[0x26] = 1; // ES
    lut[0x2E] = 1; // CS
    lut[0x36] = 1; // SS
    lut[0x3E] = 1; // DS
    lut[0x64] = 1; // FS
    lut[0x65] = 1; // GS
                   // Operand/address size
    lut[0x66] = 1;
    lut[0x67] = 1;
    // LOCK, REP
    lut[0xF0] = 1;
    lut[0xF2] = 1;
    lut[0xF3] = 1;
    // REX (0x40-0x4F)
    let mut i = 0x40u8;
    while i <= 0x4F {
        lut[i as usize] = 1;
        i += 1;
    }
    lut
};

/// Decoded x86 instruction prefix state
#[derive(Clone, Debug, Default)]
pub struct X86Prefix {
    /// REX prefix if present
    pub rex: Option<u8>,
    /// Operand size override (0x66)
    pub operand_size_override: bool,
    /// Address size override (0x67)
    pub address_size_override: bool,
    /// REP/REPNE prefix
    pub rep_prefix: Option<u8>,
    /// Segment override
    pub segment_override: Option<u8>,
    /// LOCK prefix
    pub lock: bool,
    /// Cursor position after prefixes
    pub cursor: usize,
}

impl X86Prefix {
    /// Get REX.W flag
    #[inline]
    pub fn rex_w(&self) -> bool {
        self.rex.map_or(false, |r| r & 0x08 != 0)
    }

    /// Get REX.R flag (extends ModR/M reg field)
    #[inline]
    pub fn rex_r(&self) -> u8 {
        self.rex.map_or(0, |r| (r & 0x04) << 1)
    }

    /// Get REX.X flag (extends SIB index field)
    #[inline]
    pub fn rex_x(&self) -> u8 {
        self.rex.map_or(0, |r| (r & 0x02) << 2)
    }

    /// Get REX.B flag (extends ModR/M r/m or opcode reg)
    #[inline]
    pub fn rex_b(&self) -> u8 {
        self.rex.map_or(0, |r| (r & 0x01) << 3)
    }

    /// Check if any REX prefix is present
    #[inline]
    pub fn has_rex(&self) -> bool {
        self.rex.is_some()
    }

    /// Compute operand size for 64-bit mode
    #[inline]
    pub fn op_size(&self) -> u8 {
        if self.rex_w() {
            8
        } else if self.operand_size_override {
            2
        } else {
            4
        }
    }

    /// Compute operand width for SMIR
    #[inline]
    pub fn op_width(&self) -> OpWidth {
        match self.op_size() {
            1 => OpWidth::W8,
            2 => OpWidth::W16,
            4 => OpWidth::W32,
            8 => OpWidth::W64,
            _ => OpWidth::W32,
        }
    }
}

/// Decode instruction prefixes
fn decode_prefixes(bytes: &[u8]) -> Result<X86Prefix, LiftError> {
    if bytes.is_empty() {
        return Err(LiftError::Incomplete {
            addr: 0,
            have: 0,
            need: 1,
        });
    }

    let mut prefix = X86Prefix::default();
    let mut cursor = 0;

    while cursor < bytes.len() {
        let b = bytes[cursor];
        if PREFIX_LUT[b as usize] == 0 {
            break;
        }

        match b {
            0x66 => prefix.operand_size_override = true,
            0x67 => prefix.address_size_override = true,
            0x40..=0x4F => prefix.rex = Some(b),
            0xF0 => prefix.lock = true,
            0xF2 | 0xF3 => prefix.rep_prefix = Some(b),
            0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 => {
                prefix.segment_override = Some(b);
            }
            _ => break,
        }
        cursor += 1;
    }

    prefix.cursor = cursor;
    Ok(prefix)
}

// ============================================================================
// ModR/M and SIB Decoding
// ============================================================================

/// Decoded ModR/M result
#[derive(Clone, Debug)]
pub struct ModRm {
    /// ModR/M byte value
    pub byte: u8,
    /// mod field (0-3)
    pub mod_bits: u8,
    /// reg field with REX.R (0-15)
    pub reg: u8,
    /// r/m field with REX.B (0-15)
    pub rm: u8,
    /// Is this a memory operand (mod != 3)?
    pub is_memory: bool,
    /// Decoded memory address (if is_memory)
    pub addr: Option<X86Address>,
    /// Total bytes consumed (including SIB and displacement)
    pub bytes_consumed: usize,
}

/// x86 memory address representation for lifting
#[derive(Clone, Debug)]
pub struct X86Address {
    /// Base register (None for absolute addresses)
    pub base: Option<u8>,
    /// Index register (None if no index)
    pub index: Option<u8>,
    /// Scale (1, 2, 4, or 8)
    pub scale: u8,
    /// Displacement
    pub disp: i64,
    /// RIP-relative addressing?
    pub rip_relative: bool,
}

/// Decode ModR/M byte and any following SIB/displacement
fn decode_modrm(bytes: &[u8], prefix: &X86Prefix, addr: u64) -> Result<ModRm, LiftError> {
    if bytes.is_empty() {
        return Err(LiftError::Incomplete {
            addr,
            have: 0,
            need: 1,
        });
    }

    let modrm = bytes[0];
    let mod_bits = modrm >> 6;
    let reg_field = (modrm >> 3) & 0x07;
    let rm_field = modrm & 0x07;

    let reg = reg_field | prefix.rex_r();
    let rm = rm_field | prefix.rex_b();

    if mod_bits == 3 {
        // Register operand
        return Ok(ModRm {
            byte: modrm,
            mod_bits,
            reg,
            rm,
            is_memory: false,
            addr: None,
            bytes_consumed: 1,
        });
    }

    // Memory operand - decode SIB and displacement
    let mut consumed = 1;
    let mut x86_addr = X86Address {
        base: None,
        index: None,
        scale: 1,
        disp: 0,
        rip_relative: false,
    };

    if rm_field == 4 {
        // SIB byte follows
        if bytes.len() < 2 {
            return Err(LiftError::Incomplete {
                addr,
                have: bytes.len(),
                need: 2,
            });
        }
        let sib = bytes[1];
        consumed += 1;

        let scale = 1u8 << (sib >> 6);
        let index_field = (sib >> 3) & 0x07;
        let base_field = sib & 0x07;

        let index = index_field | prefix.rex_x();
        let base = base_field | prefix.rex_b();

        x86_addr.scale = scale;

        // Index = 4 means no index
        if index != 4 {
            x86_addr.index = Some(index);
        }

        // Handle base
        if base_field == 5 && mod_bits == 0 {
            // No base, disp32 follows
            if bytes.len() < consumed + 4 {
                return Err(LiftError::Incomplete {
                    addr,
                    have: bytes.len(),
                    need: consumed + 4,
                });
            }
            let disp = i32::from_le_bytes([
                bytes[consumed],
                bytes[consumed + 1],
                bytes[consumed + 2],
                bytes[consumed + 3],
            ]) as i64;
            consumed += 4;
            x86_addr.disp = disp;
        } else {
            x86_addr.base = Some(base);
        }
    } else if rm_field == 5 && mod_bits == 0 {
        // RIP-relative addressing in 64-bit mode
        if bytes.len() < 5 {
            return Err(LiftError::Incomplete {
                addr,
                have: bytes.len(),
                need: 5,
            });
        }
        let disp = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as i64;
        consumed += 4;
        x86_addr.disp = disp;
        x86_addr.rip_relative = true;
    } else {
        // Regular register indirect
        x86_addr.base = Some(rm);
    }

    // Handle displacement for mod=1 (disp8) and mod=2 (disp32)
    match mod_bits {
        1 => {
            if bytes.len() < consumed + 1 {
                return Err(LiftError::Incomplete {
                    addr,
                    have: bytes.len(),
                    need: consumed + 1,
                });
            }
            x86_addr.disp = bytes[consumed] as i8 as i64;
            consumed += 1;
        }
        2 => {
            if bytes.len() < consumed + 4 {
                return Err(LiftError::Incomplete {
                    addr,
                    have: bytes.len(),
                    need: consumed + 4,
                });
            }
            x86_addr.disp = i32::from_le_bytes([
                bytes[consumed],
                bytes[consumed + 1],
                bytes[consumed + 2],
                bytes[consumed + 3],
            ]) as i64;
            consumed += 4;
        }
        _ => {}
    }

    Ok(ModRm {
        byte: modrm,
        mod_bits,
        reg,
        rm,
        is_memory: true,
        addr: Some(x86_addr),
        bytes_consumed: consumed,
    })
}

// ============================================================================
// Register Helpers
// ============================================================================

impl X86_64Lifter {
    /// Convert x86 register number to VReg
    fn x86_gpr(&self, reg: u8) -> VReg {
        VReg::Arch(ArchReg::X86(X86Reg::gpr(reg)))
    }

    /// Get x86 register by number
    fn gpr(&self, reg: u8) -> VReg {
        self.x86_gpr(reg & 0x0F)
    }

    /// Get RSP register
    fn rsp(&self) -> VReg {
        VReg::Arch(ArchReg::X86(X86Reg::Rsp))
    }

    /// Convert op_size to OpWidth
    fn size_to_width(&self, size: u8) -> OpWidth {
        match size {
            1 => OpWidth::W8,
            2 => OpWidth::W16,
            4 => OpWidth::W32,
            8 => OpWidth::W64,
            _ => OpWidth::W32,
        }
    }

    /// Convert op_size to MemWidth
    fn size_to_memwidth(&self, size: u8) -> MemWidth {
        match size {
            1 => MemWidth::B1,
            2 => MemWidth::B2,
            4 => MemWidth::B4,
            8 => MemWidth::B8,
            _ => MemWidth::B4,
        }
    }

    /// Convert x86 address to SMIR Address, optionally generating pre-ops
    fn x86_addr_to_smir(
        &self,
        x86_addr: &X86Address,
        next_rip: u64,
        ctx: &mut LiftContext,
    ) -> (Address, Vec<SmirOp>) {
        let mut pre_ops = Vec::new();
        let pc = ctx.guest_pc;
        let disp_i32 = |disp: i64| -> Option<i32> {
            if disp >= i32::MIN as i64 && disp <= i32::MAX as i64 {
                Some(disp as i32)
            } else {
                None
            }
        };

        if x86_addr.rip_relative {
            // RIP-relative: compute RIP + disp as immediate address
            let effective = (next_rip as i64 + x86_addr.disp) as u64;
            return (Address::Absolute(effective), pre_ops);
        }

        match (x86_addr.base, x86_addr.index) {
            (None, None) => {
                // Absolute address
                (Address::Absolute(x86_addr.disp as u64), pre_ops)
            }
            (Some(base), None) => {
                if x86_addr.disp == 0 {
                    (Address::Direct(self.gpr(base)), pre_ops)
                } else {
                    (
                        Address::BaseOffset {
                            base: self.gpr(base),
                            offset: x86_addr.disp,
                        },
                        pre_ops,
                    )
                }
            }
            (None, Some(index)) => {
                if let Some(disp) = disp_i32(x86_addr.disp) {
                    (
                        Address::BaseIndexScale {
                            base: None,
                            index: self.gpr(index),
                            scale: x86_addr.scale,
                            disp,
                        },
                        pre_ops,
                    )
                } else {
                    // Fallback to computed address
                    let tmp = ctx.alloc_vreg();
                    if x86_addr.scale > 1 {
                        pre_ops.push(SmirOp::new(
                            OpId(0),
                            pc,
                            OpKind::Shl {
                                dst: tmp,
                                src: self.gpr(index),
                                amount: SrcOperand::Imm(x86_addr.scale.trailing_zeros() as i64),
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            },
                        ));
                        if x86_addr.disp != 0 {
                            let tmp2 = ctx.alloc_vreg();
                            pre_ops.push(SmirOp::new(
                                OpId(1),
                                pc,
                                OpKind::Add {
                                    dst: tmp2,
                                    src1: tmp,
                                    src2: SrcOperand::Imm(x86_addr.disp),
                                    width: OpWidth::W64,
                                    flags: FlagUpdate::None,
                                },
                            ));
                            (Address::Direct(tmp2), pre_ops)
                        } else {
                            (Address::Direct(tmp), pre_ops)
                        }
                    } else if x86_addr.disp != 0 {
                        pre_ops.push(SmirOp::new(
                            OpId(0),
                            pc,
                            OpKind::Add {
                                dst: tmp,
                                src1: self.gpr(index),
                                src2: SrcOperand::Imm(x86_addr.disp),
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            },
                        ));
                        (Address::Direct(tmp), pre_ops)
                    } else {
                        (Address::Direct(self.gpr(index)), pre_ops)
                    }
                }
            }
            (Some(base), Some(index)) => {
                if let Some(disp) = disp_i32(x86_addr.disp) {
                    (
                        Address::BaseIndexScale {
                            base: Some(self.gpr(base)),
                            index: self.gpr(index),
                            scale: x86_addr.scale,
                            disp,
                        },
                        pre_ops,
                    )
                } else {
                    // Fallback to computed address
                    let tmp_idx = ctx.alloc_vreg();
                    let tmp_sum = ctx.alloc_vreg();

                    // Scale the index
                    if x86_addr.scale > 1 {
                        pre_ops.push(SmirOp::new(
                            OpId(0),
                            pc,
                            OpKind::Shl {
                                dst: tmp_idx,
                                src: self.gpr(index),
                                amount: SrcOperand::Imm(x86_addr.scale.trailing_zeros() as i64),
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            },
                        ));
                        pre_ops.push(SmirOp::new(
                            OpId(1),
                            pc,
                            OpKind::Add {
                                dst: tmp_sum,
                                src1: self.gpr(base),
                                src2: SrcOperand::Reg(tmp_idx),
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            },
                        ));
                    } else {
                        pre_ops.push(SmirOp::new(
                            OpId(0),
                            pc,
                            OpKind::Add {
                                dst: tmp_sum,
                                src1: self.gpr(base),
                                src2: SrcOperand::Reg(self.gpr(index)),
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            },
                        ));
                    }

                    if x86_addr.disp != 0 {
                        (
                            Address::BaseOffset {
                                base: tmp_sum,
                                offset: x86_addr.disp,
                            },
                            pre_ops,
                        )
                    } else {
                        (Address::Direct(tmp_sum), pre_ops)
                    }
                }
            }
        }
    }

    /// Map x86 condition code (0-15) to SMIR Condition
    fn x86_cond(&self, cc: u8) -> Condition {
        match cc & 0x0F {
            0x0 => Condition::Overflow,   // O
            0x1 => Condition::NoOverflow, // NO
            0x2 => Condition::Ult,        // B/C/NAE
            0x3 => Condition::Uge,        // AE/NB/NC
            0x4 => Condition::Eq,         // E/Z
            0x5 => Condition::Ne,         // NE/NZ
            0x6 => Condition::Ule,        // BE/NA
            0x7 => Condition::Ugt,        // A/NBE
            0x8 => Condition::Negative,   // S
            0x9 => Condition::Positive,   // NS
            0xA => Condition::Parity,     // P/PE
            0xB => Condition::NoParity,   // NP/PO
            0xC => Condition::Slt,        // L/NGE
            0xD => Condition::Sge,        // GE/NL
            0xE => Condition::Sle,        // LE/NG
            0xF => Condition::Sgt,        // G/NLE
            _ => Condition::Always,
        }
    }
}

// ============================================================================
// Instruction Lifting
// ============================================================================

impl X86_64Lifter {
    /// Lift arithmetic instruction (ADD, SUB, ADC, SBC, CMP)
    fn lift_arith(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        // Determine operation type from opcode
        let (is_8bit, dir_rm_reg) = match opcode & 0x07 {
            0 => (true, true),   // rm8, r8
            1 => (false, true),  // rm, r
            2 => (true, false),  // r8, rm8
            3 => (false, false), // r, rm
            4 => (true, true),   // AL, imm8 (handled separately)
            5 => (false, true),  // rAX, imm (handled separately)
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                })
            }
        };

        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);

        // Decode ModR/M
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;

        // Get source and destination
        let (dst, src) = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            if dir_rm_reg {
                // rm is destination, reg is source
                let tmp = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: tmp,
                        addr: addr.clone(),
                        width: self.size_to_memwidth(op_size),
                        sign: SignExtend::Zero,
                    },
                ));
                (tmp, self.gpr(modrm.reg))
            } else {
                // reg is destination, rm is source
                let tmp = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: tmp,
                        addr,
                        width: self.size_to_memwidth(op_size),
                        sign: SignExtend::Zero,
                    },
                ));
                (self.gpr(modrm.reg), tmp)
            }
        } else if dir_rm_reg {
            (self.gpr(modrm.rm), self.gpr(modrm.reg))
        } else {
            (self.gpr(modrm.reg), self.gpr(modrm.rm))
        };

        // Determine operation from opcode high bits
        let result = ctx.alloc_vreg();
        let op_kind = match (opcode >> 3) & 0x07 {
            0 => OpKind::Add {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags: FlagUpdate::All,
            },
            1 => OpKind::Or {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags: FlagUpdate::All,
            },
            2 => OpKind::Adc {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags: FlagUpdate::All,
            },
            3 => OpKind::Sbb {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags: FlagUpdate::All,
            },
            4 => OpKind::And {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags: FlagUpdate::All,
            },
            5 => OpKind::Sub {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags: FlagUpdate::All,
            },
            6 => OpKind::Xor {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags: FlagUpdate::All,
            },
            7 => {
                // CMP - subtract but don't store
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Cmp {
                        src1: dst,
                        src2: SrcOperand::Reg(src),
                        width,
                    },
                ));
                return Ok(LiftResult::fallthrough(
                    ops,
                    prefix.cursor + modrm.bytes_consumed,
                ));
            }
            _ => unreachable!(),
        };

        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, op_kind));

        // Write back if destination was memory
        if modrm.is_memory && dir_rm_reg {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, _) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: result,
                    addr,
                    width: self.size_to_memwidth(op_size),
                },
            ));
        } else if !modrm.is_memory {
            // Register destination - copy result
            let dst_reg = if dir_rm_reg {
                self.gpr(modrm.rm)
            } else {
                self.gpr(modrm.reg)
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: dst_reg,
                    src: SrcOperand::Reg(result),
                    width,
                },
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift shift instructions with immediate (C0/C1)
    fn lift_shift_imm(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let op_size = if opcode == 0xC0 { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        if bytes.len() < modrm.bytes_consumed + 1 {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: modrm.bytes_consumed + 1,
            });
        }

        let imm = bytes[modrm.bytes_consumed] as i64;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + 1;
        let mut ops = Vec::new();

        let group = (modrm.byte >> 3) & 0x07;

        let (src, addr) = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr: addr.clone(),
                    width: self.size_to_memwidth(op_size),
                    sign: SignExtend::Zero,
                },
            ));
            (tmp, Some(addr))
        } else {
            (self.gpr(modrm.rm), None)
        };

        let result = ctx.alloc_vreg();
        let op_kind = match group {
            4 => OpKind::Shl {
                dst: result,
                src,
                amount: SrcOperand::Imm(imm),
                width,
                flags: FlagUpdate::All,
            },
            5 => OpKind::Shr {
                dst: result,
                src,
                amount: SrcOperand::Imm(imm),
                width,
                flags: FlagUpdate::All,
            },
            7 => OpKind::Sar {
                dst: result,
                src,
                amount: SrcOperand::Imm(imm),
                width,
                flags: FlagUpdate::All,
            },
            _ => {
                if self.strict {
                    return Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: format!("shift group {}", group),
                    });
                }
                return Ok(LiftResult::fallthrough(
                    vec![SmirOp::new(OpId(0), pc, OpKind::Nop)],
                    prefix.cursor + modrm.bytes_consumed + 1,
                ));
            }
        };

        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, op_kind));

        if let Some(addr) = addr {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: result,
                    addr,
                    width: self.size_to_memwidth(op_size),
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: self.gpr(modrm.rm),
                    src: SrcOperand::Reg(result),
                    width,
                },
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + 1,
        ))
    }

    /// Lift shift instructions with implicit count = 1 (D0/D1)
    fn lift_shift_one(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let op_size = if opcode == 0xD0 { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();

        let group = (modrm.byte >> 3) & 0x07;

        let (src, addr) = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr: addr.clone(),
                    width: self.size_to_memwidth(op_size),
                    sign: SignExtend::Zero,
                },
            ));
            (tmp, Some(addr))
        } else {
            (self.gpr(modrm.rm), None)
        };

        let result = ctx.alloc_vreg();
        let op_kind = match group {
            4 => OpKind::Shl {
                dst: result,
                src,
                amount: SrcOperand::Imm(1),
                width,
                flags: FlagUpdate::All,
            },
            5 => OpKind::Shr {
                dst: result,
                src,
                amount: SrcOperand::Imm(1),
                width,
                flags: FlagUpdate::All,
            },
            7 => OpKind::Sar {
                dst: result,
                src,
                amount: SrcOperand::Imm(1),
                width,
                flags: FlagUpdate::All,
            },
            _ => {
                if self.strict {
                    return Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: format!("shift group {}", group),
                    });
                }
                return Ok(LiftResult::fallthrough(
                    vec![SmirOp::new(OpId(0), pc, OpKind::Nop)],
                    prefix.cursor + modrm.bytes_consumed,
                ));
            }
        };

        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, op_kind));

        if let Some(addr) = addr {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: result,
                    addr,
                    width: self.size_to_memwidth(op_size),
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: self.gpr(modrm.rm),
                    src: SrcOperand::Reg(result),
                    width,
                },
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift MOV r, imm (B8-BF)
    fn lift_mov_r_imm(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let reg = (opcode & 0x07) | prefix.rex_b();
        let op_size = prefix.op_size();
        let width = self.size_to_width(op_size);

        // In 64-bit mode with REX.W, we can have 64-bit immediate
        let (imm, imm_size): (i64, usize) = if prefix.rex_w() {
            if bytes.len() < 8 {
                return Err(LiftError::Incomplete {
                    addr: pc,
                    have: bytes.len(),
                    need: 8,
                });
            }
            (
                i64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ]),
                8,
            )
        } else {
            match op_size {
                2 => {
                    if bytes.len() < 2 {
                        return Err(LiftError::Incomplete {
                            addr: pc,
                            have: bytes.len(),
                            need: 2,
                        });
                    }
                    (i16::from_le_bytes([bytes[0], bytes[1]]) as i64, 2)
                }
                _ => {
                    if bytes.len() < 4 {
                        return Err(LiftError::Incomplete {
                            addr: pc,
                            have: bytes.len(),
                            need: 4,
                        });
                    }
                    (
                        i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64,
                        4,
                    )
                }
            }
        };

        let src = if prefix.rex_w() {
            SrcOperand::Imm64(imm)
        } else {
            SrcOperand::Imm(imm)
        };

        let ops = vec![SmirOp::new(
            OpId(0),
            pc,
            OpKind::Mov {
                dst: self.gpr(reg),
                src,
                width,
            },
        )];

        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_size))
    }

    /// Lift MOV r8, imm8 (B0-B7)
    fn lift_mov_r8_imm8(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let reg = (opcode & 0x07) | prefix.rex_b();

        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: 0,
                need: 1,
            });
        }

        let imm = bytes[0] as i8 as i64;

        let ops = vec![SmirOp::new(
            OpId(0),
            pc,
            OpKind::Mov {
                dst: self.gpr(reg),
                src: SrcOperand::Imm(imm),
                width: OpWidth::W8,
            },
        )];

        Ok(LiftResult::fallthrough(ops, prefix.cursor + 1))
    }

    /// Lift PUSH r64 (50-57)
    fn lift_push_r64(
        &self,
        opcode: u8,
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let reg = (opcode & 0x07) | prefix.rex_b();
        let mut ops = Vec::new();

        // RSP -= 8
        ops.push(SmirOp::new(
            OpId(0),
            pc,
            OpKind::Sub {
                dst: self.rsp(),
                src1: self.rsp(),
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        // [RSP] = reg
        ops.push(SmirOp::new(
            OpId(1),
            pc,
            OpKind::Store {
                src: self.gpr(reg),
                addr: Address::Direct(self.rsp()),
                width: MemWidth::B8,
            },
        ));

        Ok(LiftResult::fallthrough(ops, prefix.cursor))
    }

    /// Lift POP r64 (58-5F)
    fn lift_pop_r64(
        &self,
        opcode: u8,
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let reg = (opcode & 0x07) | prefix.rex_b();
        let mut ops = Vec::new();

        // reg = [RSP]
        ops.push(SmirOp::new(
            OpId(0),
            pc,
            OpKind::Load {
                dst: self.gpr(reg),
                addr: Address::Direct(self.rsp()),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        ));

        // RSP += 8
        ops.push(SmirOp::new(
            OpId(1),
            pc,
            OpKind::Add {
                dst: self.rsp(),
                src1: self.rsp(),
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        Ok(LiftResult::fallthrough(ops, prefix.cursor))
    }

    /// Lift CALL rel32 (E8)
    fn lift_call_rel32(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.len() < 4 {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: 4,
            });
        }

        let rel = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64;
        let insn_len = prefix.cursor + 4;
        let next_rip = pc + insn_len as u64;
        let target = (next_rip as i64 + rel) as u64;

        let mut ops = Vec::new();

        // Push return address
        ops.push(SmirOp::new(
            OpId(0),
            pc,
            OpKind::Sub {
                dst: self.rsp(),
                src1: self.rsp(),
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        ops.push(SmirOp::new(
            OpId(1),
            pc,
            OpKind::Store {
                src: VReg::Imm(next_rip as i64),
                addr: Address::Direct(self.rsp()),
                width: MemWidth::B8,
            },
        ));

        Ok(LiftResult {
            ops,
            bytes_consumed: insn_len,
            control_flow: ControlFlow::Call {
                target: CallTarget::GuestAddr(target),
            },
            branch_targets: vec![target],
        })
    }

    /// Lift RET (C3)
    fn lift_ret(
        &self,
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let mut ops = Vec::new();
        let ret_addr = ctx.alloc_vreg();

        // Load return address
        ops.push(SmirOp::new(
            OpId(0),
            pc,
            OpKind::Load {
                dst: ret_addr,
                addr: Address::Direct(self.rsp()),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        ));

        // RSP += 8
        ops.push(SmirOp::new(
            OpId(1),
            pc,
            OpKind::Add {
                dst: self.rsp(),
                src1: self.rsp(),
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        Ok(LiftResult::ret(ops, prefix.cursor))
    }

    /// Lift JMP rel8 (EB)
    fn lift_jmp_rel8(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: 0,
                need: 1,
            });
        }

        let rel = bytes[0] as i8 as i64;
        let insn_len = prefix.cursor + 1;
        let target = (pc as i64 + insn_len as i64 + rel) as u64;

        Ok(LiftResult::branch(vec![], insn_len, target))
    }

    /// Lift JMP rel32 (E9)
    fn lift_jmp_rel32(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.len() < 4 {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: 4,
            });
        }

        let rel = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64;
        let insn_len = prefix.cursor + 4;
        let target = (pc as i64 + insn_len as i64 + rel) as u64;

        Ok(LiftResult::branch(vec![], insn_len, target))
    }

    /// Lift Jcc rel8 (70-7F)
    fn lift_jcc_rel8(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: 0,
                need: 1,
            });
        }

        let cc = opcode & 0x0F;
        let cond = self.x86_cond(cc);
        let rel = bytes[0] as i8 as i64;
        let insn_len = prefix.cursor + 1;
        let next_pc = pc + insn_len as u64;
        let target = (next_pc as i64 + rel) as u64;

        Ok(LiftResult::cond_branch(
            vec![],
            insn_len,
            cond,
            target,
            next_pc,
        ))
    }

    /// Lift LEA (8D)
    fn lift_lea(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let modrm = decode_modrm(bytes, prefix, pc)?;

        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;

        let x86_addr = modrm.addr.as_ref().unwrap();
        let (addr, mut ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Lea {
                dst: self.gpr(modrm.reg),
                addr,
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift NOP (90)
    fn lift_nop(&self, prefix: &X86Prefix, _pc: u64) -> Result<LiftResult, LiftError> {
        Ok(LiftResult::fallthrough(vec![], prefix.cursor))
    }

    /// Lift MOV r/m, r and MOV r, r/m (88-8B)
    fn lift_mov_rm_r(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_8bit = (opcode & 0x01) == 0;
        let dir_reg_rm = (opcode & 0x02) != 0; // true = reg is src, rm is dst

        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;

        if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            if dir_reg_rm {
                // MOV r, rm - load from memory
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: self.gpr(modrm.reg),
                        addr,
                        width: mem_width,
                        sign: SignExtend::Zero,
                    },
                ));
            } else {
                // MOV rm, r - store to memory
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src: self.gpr(modrm.reg),
                        addr,
                        width: mem_width,
                    },
                ));
            }
        } else {
            // Register to register
            let (dst, src) = if dir_reg_rm {
                (self.gpr(modrm.reg), self.gpr(modrm.rm))
            } else {
                (self.gpr(modrm.rm), self.gpr(modrm.reg))
            };

            ops.push(SmirOp::new(
                OpId(0),
                pc,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(src),
                    width,
                },
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift TEST r/m, r (84/85)
    fn lift_test_rm_r(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_8bit = (opcode & 0x01) == 0;
        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;

        let (src1, src2) = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr,
                    width: self.size_to_memwidth(op_size),
                    sign: SignExtend::Zero,
                },
            ));
            (tmp, self.gpr(modrm.reg))
        } else {
            (self.gpr(modrm.rm), self.gpr(modrm.reg))
        };

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Test {
                src1,
                src2: SrcOperand::Reg(src2),
                width,
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift XOR r/m, r and XOR r, r/m (30-33)
    fn lift_xor_rm_r(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_8bit = (opcode & 0x01) == 0;
        let dir_reg_rm = (opcode & 0x02) != 0;

        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;

        let (dst, src1, src2) = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            if dir_reg_rm {
                // XOR r, rm
                let tmp = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: tmp,
                        addr,
                        width: self.size_to_memwidth(op_size),
                        sign: SignExtend::Zero,
                    },
                ));
                (self.gpr(modrm.reg), self.gpr(modrm.reg), tmp)
            } else {
                // XOR rm, r - load-modify-store
                let tmp = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: tmp,
                        addr: addr.clone(),
                        width: self.size_to_memwidth(op_size),
                        sign: SignExtend::Zero,
                    },
                ));
                let result = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Xor {
                        dst: result,
                        src1: tmp,
                        src2: SrcOperand::Reg(self.gpr(modrm.reg)),
                        width,
                        flags: FlagUpdate::All,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src: result,
                        addr,
                        width: self.size_to_memwidth(op_size),
                    },
                ));
                return Ok(LiftResult::fallthrough(
                    ops,
                    prefix.cursor + modrm.bytes_consumed,
                ));
            }
        } else if dir_reg_rm {
            (self.gpr(modrm.reg), self.gpr(modrm.reg), self.gpr(modrm.rm))
        } else {
            (self.gpr(modrm.rm), self.gpr(modrm.rm), self.gpr(modrm.reg))
        };

        let result = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Xor {
                dst: result,
                src1,
                src2: SrcOperand::Reg(src2),
                width,
                flags: FlagUpdate::All,
            },
        ));

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(result),
                width,
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift the main instruction
    fn lift_insn_inner(
        &self,
        pc: u64,
        bytes: &[u8],
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: 0,
                need: 1,
            });
        }

        // Decode prefixes
        let prefix = decode_prefixes(bytes)?;
        let opcode_bytes = &bytes[prefix.cursor..];

        if opcode_bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: prefix.cursor + 1,
            });
        }

        let opcode = opcode_bytes[0];
        let after_opcode = &opcode_bytes[1..];

        match opcode {
            // NOP / PAUSE (with REP prefix)
            0x90 => {
                if prefix.rep_prefix == Some(0xF3) {
                    // PAUSE - treat as NOP for lifting
                    Ok(LiftResult::fallthrough(vec![], prefix.cursor + 1))
                } else {
                    self.lift_nop(
                        &X86Prefix {
                            cursor: prefix.cursor + 1,
                            ..prefix
                        },
                        pc,
                    )
                }
            }

            // HLT
            0xF4 => Ok(LiftResult {
                ops: vec![],
                bytes_consumed: prefix.cursor + 1,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::Halt,
                },
                branch_targets: vec![],
            }),

            // Two-byte opcode prefix
            0x0F => self.lift_0f_opcode(after_opcode, &prefix, pc, ctx),

            // Control flow
            0xEB => self.lift_jmp_rel8(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xE9 => self.lift_jmp_rel32(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xE8 => self.lift_call_rel32(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xC3 => self.lift_ret(
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x70..=0x7F => self.lift_jcc_rel8(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Data movement
            0xB0..=0xB7 => self.lift_mov_r8_imm8(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xB8..=0xBF => self.lift_mov_r_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x88..=0x8B => self.lift_mov_rm_r(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x8D => self.lift_lea(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x50..=0x57 => self.lift_push_r64(
                opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x58..=0x5F => self.lift_pop_r64(
                opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Arithmetic
            0x00..=0x05 => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // ADD
            0x08..=0x0D => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // OR
            0x10..=0x15 => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // ADC
            0x18..=0x1D => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // SBB
            0x20..=0x25 => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // AND
            0x28..=0x2D => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // SUB
            0x30..=0x33 => self.lift_xor_rm_r(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x38..=0x3D => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // CMP

            // Logic
            0x84 | 0x85 => self.lift_test_rm_r(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Shift/rotate group (C0/C1) - immediate
            0xC0 | 0xC1 => self.lift_shift_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Shift/rotate group (D0/D1) - count = 1
            0xD0 | 0xD1 => self.lift_shift_one(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Unsupported - return error with mnemonic
            _ => {
                if self.strict {
                    Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: format!("0x{:02X}", opcode),
                    })
                } else {
                    // In non-strict mode, emit a Nop and continue
                    Ok(LiftResult::fallthrough(
                        vec![SmirOp::new(OpId(0), pc, OpKind::Nop)],
                        prefix.cursor + 1,
                    ))
                }
            }
        }
    }

    /// Lift 0F-prefixed (two-byte) opcodes
    fn lift_0f_opcode(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + 1,
                need: prefix.cursor + 2,
            });
        }

        let opcode2 = bytes[0];
        let after_opcode = &bytes[1..];
        let prefix2 = X86Prefix {
            cursor: prefix.cursor + 2,
            ..prefix.clone()
        };

        match opcode2 {
            // Jcc rel32 (0F 80 - 0F 8F)
            0x80..=0x8F => {
                if after_opcode.len() < 4 {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: prefix.cursor + 2 + after_opcode.len(),
                        need: prefix.cursor + 6,
                    });
                }

                let cc = opcode2 & 0x0F;
                let cond = self.x86_cond(cc);
                let rel = i32::from_le_bytes([
                    after_opcode[0],
                    after_opcode[1],
                    after_opcode[2],
                    after_opcode[3],
                ]) as i64;

                let insn_len = prefix.cursor + 6;
                let next_pc = pc + insn_len as u64;
                let target = (next_pc as i64 + rel) as u64;

                Ok(LiftResult::cond_branch(
                    vec![],
                    insn_len,
                    cond,
                    target,
                    next_pc,
                ))
            }

            // SETcc (0F 90 - 0F 9F)
            0x90..=0x9F => {
                let cc = opcode2 & 0x0F;
                let cond = self.x86_cond(cc);

                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;

                if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::SetCC {
                            dst: tmp,
                            cond,
                            width: OpWidth::W8,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Store {
                            src: tmp,
                            addr,
                            width: MemWidth::B1,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(0),
                        pc,
                        OpKind::SetCC {
                            dst: self.gpr(modrm.rm),
                            cond,
                            width: OpWidth::W8,
                        },
                    ));
                }

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // CMOVcc (0F 40 - 0F 4F)
            0x40..=0x4F => {
                let cc = opcode2 & 0x0F;
                let cond = self.x86_cond(cc);
                let op_size = prefix.op_size();
                let width = self.size_to_width(op_size);

                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;

                let src = if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: tmp,
                            addr,
                            width: self.size_to_memwidth(op_size),
                            sign: SignExtend::Zero,
                        },
                    ));
                    tmp
                } else {
                    self.gpr(modrm.rm)
                };

                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::CMove {
                        dst: self.gpr(modrm.reg),
                        src,
                        cond,
                        width,
                    },
                ));

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // MOVZX r, r/m8 (0F B6)
            0xB6 => {
                let op_size = prefix.op_size();
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;

                let src = if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: tmp,
                            addr,
                            width: MemWidth::B1,
                            sign: SignExtend::Zero,
                        },
                    ));
                    tmp
                } else {
                    self.gpr(modrm.rm)
                };

                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::ZeroExtend {
                        dst: self.gpr(modrm.reg),
                        src,
                        from_width: OpWidth::W8,
                        to_width: self.size_to_width(op_size),
                    },
                ));

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // MOVZX r, r/m16 (0F B7)
            0xB7 => {
                let op_size = prefix.op_size();
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;

                let src = if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: tmp,
                            addr,
                            width: MemWidth::B2,
                            sign: SignExtend::Zero,
                        },
                    ));
                    tmp
                } else {
                    self.gpr(modrm.rm)
                };

                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::ZeroExtend {
                        dst: self.gpr(modrm.reg),
                        src,
                        from_width: OpWidth::W16,
                        to_width: self.size_to_width(op_size),
                    },
                ));

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // MOVSX r, r/m8 (0F BE)
            0xBE => {
                let op_size = prefix.op_size();
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;

                let src = if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: tmp,
                            addr,
                            width: MemWidth::B1,
                            sign: SignExtend::Sign,
                        },
                    ));
                    tmp
                } else {
                    self.gpr(modrm.rm)
                };

                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::SignExtend {
                        dst: self.gpr(modrm.reg),
                        src,
                        from_width: OpWidth::W8,
                        to_width: self.size_to_width(op_size),
                    },
                ));

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // MOVSX r, r/m16 (0F BF)
            0xBF => {
                let op_size = prefix.op_size();
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;

                let src = if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: tmp,
                            addr,
                            width: MemWidth::B2,
                            sign: SignExtend::Sign,
                        },
                    ));
                    tmp
                } else {
                    self.gpr(modrm.rm)
                };

                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::SignExtend {
                        dst: self.gpr(modrm.reg),
                        src,
                        from_width: OpWidth::W16,
                        to_width: self.size_to_width(op_size),
                    },
                ));

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // IMUL r, r/m (0F AF)
            0xAF => {
                let op_size = prefix.op_size();
                let width = self.size_to_width(op_size);
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;

                let src = if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: tmp,
                            addr,
                            width: self.size_to_memwidth(op_size),
                            sign: SignExtend::Zero,
                        },
                    ));
                    tmp
                } else {
                    self.gpr(modrm.rm)
                };

                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::MulS {
                        dst_lo: self.gpr(modrm.reg),
                        dst_hi: None,
                        src1: self.gpr(modrm.reg),
                        src2: SrcOperand::Reg(src),
                        width,
                        flags: FlagUpdate::All,
                    },
                ));

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // SYSCALL (0F 05)
            0x05 => Ok(LiftResult {
                ops: vec![],
                bytes_consumed: prefix.cursor + 2,
                control_flow: ControlFlow::Syscall,
                branch_targets: vec![],
            }),

            // SYSRET (0F 07)
            0x07 => {
                // Treat as return for lifting purposes
                Ok(LiftResult::ret(vec![], prefix.cursor + 2))
            }

            // NOP (0F 1F /0) - multi-byte NOP
            0x1F => {
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                Ok(LiftResult::fallthrough(
                    vec![],
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            _ => {
                if self.strict {
                    Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: format!("0x0F 0x{:02X}", opcode2),
                    })
                } else {
                    Ok(LiftResult::fallthrough(
                        vec![SmirOp::new(OpId(0), pc, OpKind::Nop)],
                        prefix.cursor + 2,
                    ))
                }
            }
        }
    }
}

// ============================================================================
// SmirLifter Implementation
// ============================================================================

impl SmirLifter for X86_64Lifter {
    fn source_arch(&self) -> SourceArch {
        SourceArch::X86_64
    }

    fn lift_insn(
        &mut self,
        addr: GuestAddr,
        bytes: &[u8],
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        ctx.guest_pc = addr;
        self.lift_insn_inner(addr, bytes, ctx)
    }

    fn lift_block(
        &mut self,
        addr: GuestAddr,
        mem: &dyn MemoryReader,
        ctx: &mut LiftContext,
    ) -> Result<SmirBlock, LiftError> {
        let block_id = ctx.get_or_create_block(addr);
        let mut block = SmirBlock::new(block_id, addr);

        let mut pc = addr;
        let mut buf = [0u8; 15];

        loop {
            // Read instruction bytes
            let bytes = mem
                .read(pc, 15)
                .map_err(|e| LiftError::MemoryError { addr: pc, error: e })?;

            buf[..bytes.len()].copy_from_slice(&bytes);

            ctx.guest_pc = pc;
            let result = self.lift_insn_inner(pc, &buf[..bytes.len()], ctx)?;

            // Add ops to block
            block.ops.extend(result.ops);
            pc += result.bytes_consumed as u64;

            // Check for block-ending control flow
            match result.control_flow {
                ControlFlow::Fallthrough | ControlFlow::NextInsn => continue,
                ControlFlow::Branch { target } | ControlFlow::DirectBranch(target) => {
                    block.terminator = Terminator::Branch {
                        target: ctx.get_or_create_block(target),
                    };
                    break;
                }
                ControlFlow::CondBranch {
                    cond,
                    target,
                    fallthrough,
                } => {
                    // We need a VReg holding the condition result
                    let cond_vreg = ctx.alloc_vreg();
                    block.ops.push(SmirOp::new(
                        OpId(block.ops.len() as u16),
                        pc,
                        OpKind::TestCondition {
                            dst: cond_vreg,
                            cond,
                        },
                    ));
                    block.terminator = Terminator::CondBranch {
                        cond: cond_vreg,
                        true_target: ctx.get_or_create_block(target),
                        false_target: ctx.get_or_create_block(fallthrough),
                    };
                    break;
                }
                ControlFlow::CondBranchReg {
                    cond,
                    taken,
                    not_taken,
                } => {
                    block.terminator = Terminator::CondBranch {
                        cond,
                        true_target: ctx.get_or_create_block(taken),
                        false_target: ctx.get_or_create_block(not_taken),
                    };
                    break;
                }
                ControlFlow::IndirectBranch { target } => {
                    block.terminator = Terminator::IndirectBranch {
                        target,
                        possible_targets: vec![],
                    };
                    break;
                }
                ControlFlow::Call { target } => {
                    let continuation = ctx.get_or_create_block(pc);
                    block.terminator = Terminator::Call {
                        target,
                        args: vec![],
                        continuation,
                    };
                    break;
                }
                ControlFlow::Return => {
                    block.terminator = Terminator::Return { values: vec![] };
                    break;
                }
                ControlFlow::Trap { kind } => {
                    block.terminator = Terminator::Trap { kind };
                    break;
                }
                ControlFlow::Syscall => {
                    // For syscall, we'll use a TailCall to the syscall runtime
                    block.terminator = Terminator::TailCall {
                        target: CallTarget::Runtime(crate::smir::ir::RuntimeFunc::Syscall),
                        args: vec![],
                    };
                    break;
                }
            }
        }

        Ok(block)
    }

    fn lift_function(
        &mut self,
        entry: GuestAddr,
        mem: &dyn MemoryReader,
        ctx: &mut LiftContext,
    ) -> Result<SmirFunction, LiftError> {
        let entry_block = ctx.get_or_create_block(entry);
        let mut func = SmirFunction::new(FunctionId(entry as u32), entry_block, entry);

        // Work queue of blocks to lift
        let mut worklist = vec![entry];
        let mut visited = HashSet::new();

        while let Some(block_addr) = worklist.pop() {
            if visited.contains(&block_addr) {
                continue;
            }
            visited.insert(block_addr);

            let block = self.lift_block(block_addr, mem, ctx)?;

            // Add branch targets to worklist
            match &block.terminator {
                Terminator::Branch { target } => {
                    if let Some(&addr) =
                        ctx.block_cache.iter().find_map(
                            |(a, id)| {
                                if id == target {
                                    Some(a)
                                } else {
                                    None
                                }
                            },
                        )
                    {
                        worklist.push(addr);
                    }
                }
                Terminator::CondBranch {
                    true_target,
                    false_target,
                    ..
                } => {
                    for target in [true_target, false_target] {
                        if let Some(&addr) = ctx.block_cache.iter().find_map(|(a, id)| {
                            if id == target {
                                Some(a)
                            } else {
                                None
                            }
                        }) {
                            worklist.push(addr);
                        }
                    }
                }
                _ => {}
            }

            func.add_block(block);
        }

        Ok(func)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Test memory reader for unit tests
    struct TestMemory {
        data: Vec<u8>,
        base: u64,
    }

    impl TestMemory {
        fn new(base: u64, data: Vec<u8>) -> Self {
            TestMemory { data, base }
        }
    }

    impl MemoryReader for TestMemory {
        fn read(&self, addr: u64, size: usize) -> Result<Vec<u8>, MemoryError> {
            if addr < self.base {
                return Err(MemoryError::OutOfBounds { addr });
            }
            let offset = (addr - self.base) as usize;
            if offset >= self.data.len() {
                return Err(MemoryError::OutOfBounds { addr });
            }
            // Return as many bytes as possible up to size
            let available = (self.data.len() - offset).min(size);
            Ok(self.data[offset..offset + available].to_vec())
        }
    }

    #[test]
    fn test_prefix_decode() {
        // No prefix
        let prefix = decode_prefixes(&[0x90]).unwrap();
        assert_eq!(prefix.cursor, 0);
        assert!(!prefix.has_rex());

        // REX.W prefix
        let prefix = decode_prefixes(&[0x48, 0xB8]).unwrap();
        assert_eq!(prefix.cursor, 1);
        assert!(prefix.rex_w());
        assert_eq!(prefix.op_size(), 8);

        // Operand size override
        let prefix = decode_prefixes(&[0x66, 0xB8]).unwrap();
        assert_eq!(prefix.cursor, 1);
        assert!(prefix.operand_size_override);
        assert_eq!(prefix.op_size(), 2);
    }

    #[test]
    fn test_modrm_decode() {
        // MOD=3 (register)
        let prefix = X86Prefix::default();
        let modrm = decode_modrm(&[0xC0], &prefix, 0).unwrap();
        assert!(!modrm.is_memory);
        assert_eq!(modrm.reg, 0);
        assert_eq!(modrm.rm, 0);

        // MOD=0, RM=5 (RIP-relative)
        let modrm = decode_modrm(&[0x05, 0x10, 0x00, 0x00, 0x00], &prefix, 0).unwrap();
        assert!(modrm.is_memory);
        assert!(modrm.addr.as_ref().unwrap().rip_relative);
        assert_eq!(modrm.bytes_consumed, 5);
    }

    #[test]
    fn test_lift_nop() {
        let mut lifter = X86_64Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::X86_64);

        // NOP
        let result = lifter.lift_insn(0x1000, &[0x90], &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 1);
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    }

    #[test]
    fn test_lift_mov_r_imm() {
        let mut lifter = X86_64Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::X86_64);

        // MOV EAX, 0x12345678
        let result = lifter
            .lift_insn(0x1000, &[0xB8, 0x78, 0x56, 0x34, 0x12], &mut ctx)
            .unwrap();
        assert_eq!(result.bytes_consumed, 5);
        assert_eq!(result.ops.len(), 1);

        // MOV RAX, 0x123456789ABCDEF0 (REX.W prefix)
        let result = lifter
            .lift_insn(
                0x1000,
                &[0x48, 0xB8, 0xF0, 0xDE, 0xBC, 0x9A, 0x78, 0x56, 0x34, 0x12],
                &mut ctx,
            )
            .unwrap();
        assert_eq!(result.bytes_consumed, 10);
    }

    #[test]
    fn test_lift_jmp() {
        let mut lifter = X86_64Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::X86_64);

        // JMP rel8
        let result = lifter.lift_insn(0x1000, &[0xEB, 0x10], &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 2);
        assert!(matches!(
            result.control_flow,
            ControlFlow::Branch { target: 0x1012 }
        ));

        // JMP rel32
        let result = lifter
            .lift_insn(0x1000, &[0xE9, 0x00, 0x10, 0x00, 0x00], &mut ctx)
            .unwrap();
        assert_eq!(result.bytes_consumed, 5);
        assert!(matches!(
            result.control_flow,
            ControlFlow::Branch { target: 0x2005 }
        ));
    }

    #[test]
    fn test_lift_jcc() {
        let mut lifter = X86_64Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::X86_64);

        // JE rel8
        let result = lifter.lift_insn(0x1000, &[0x74, 0x10], &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 2);
        match result.control_flow {
            ControlFlow::CondBranch {
                cond,
                target,
                fallthrough,
            } => {
                assert_eq!(cond, Condition::Eq);
                assert_eq!(target, 0x1012);
                assert_eq!(fallthrough, 0x1002);
            }
            _ => panic!("Expected CondBranch"),
        }
    }

    #[test]
    fn test_lift_push_pop() {
        let mut lifter = X86_64Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::X86_64);

        // PUSH RAX
        let result = lifter.lift_insn(0x1000, &[0x50], &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 1);
        assert_eq!(result.ops.len(), 2); // SUB RSP + STORE

        // POP RAX
        let result = lifter.lift_insn(0x1000, &[0x58], &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 1);
        assert_eq!(result.ops.len(), 2); // LOAD + ADD RSP
    }

    #[test]
    fn test_lift_call_ret() {
        let mut lifter = X86_64Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::X86_64);

        // CALL rel32
        let result = lifter
            .lift_insn(0x1000, &[0xE8, 0x00, 0x10, 0x00, 0x00], &mut ctx)
            .unwrap();
        assert_eq!(result.bytes_consumed, 5);
        assert!(matches!(
            result.control_flow,
            ControlFlow::Call {
                target: CallTarget::GuestAddr(0x2005)
            }
        ));

        // RET
        let result = lifter.lift_insn(0x1000, &[0xC3], &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 1);
        assert!(matches!(result.control_flow, ControlFlow::Return));
    }

    #[test]
    fn test_lift_block() {
        let mut lifter = X86_64Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::X86_64);

        // Simple block: MOV EAX, 1; RET
        let mem = TestMemory::new(0x1000, vec![0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3]);
        let block = lifter.lift_block(0x1000, &mem, &mut ctx).unwrap();

        assert_eq!(block.guest_pc, 0x1000);
        assert!(matches!(block.terminator, Terminator::Return { .. }));
    }
}
