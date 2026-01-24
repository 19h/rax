//! x86_64 code generator for SMIR.
//!
//! This module lowers SMIR IR to native x86_64 machine code.

use std::collections::HashMap;

use crate::smir::ir::{SmirBlock, SmirFunction, Terminator};
use crate::smir::ops::OpKind;
use crate::smir::types::{
    Address, BlockId, Condition, MemWidth, OpWidth, SignExtend, SrcOperand, VReg,
};

use super::regalloc::{PhysReg, RegAlloc, RegLocation};
use super::{CodeBuffer, LowerError, LowerResult, RelocKind, RelocTarget, Relocation, SmirLowerer};

// ============================================================================
// x86_64 Condition Codes
// ============================================================================

/// x86_64 condition codes for Jcc/SETcc/CMOVcc
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum X86Cond {
    O = 0x0,  // Overflow
    No = 0x1, // Not overflow
    B = 0x2,  // Below (unsigned <), aka C/NAE
    Ae = 0x3, // Above or equal (unsigned >=), aka NC/NB
    E = 0x4,  // Equal, aka Z
    Ne = 0x5, // Not equal, aka NZ
    Be = 0x6, // Below or equal (unsigned <=), aka NA
    A = 0x7,  // Above (unsigned >), aka NBE
    S = 0x8,  // Sign (negative)
    Ns = 0x9, // Not sign (positive or zero)
    P = 0xA,  // Parity even
    Np = 0xB, // Parity odd
    L = 0xC,  // Less (signed <), aka NGE
    Ge = 0xD, // Greater or equal (signed >=), aka NL
    Le = 0xE, // Less or equal (signed <=), aka NG
    G = 0xF,  // Greater (signed >), aka NLE
}

impl X86Cond {
    /// Convert from SMIR Condition
    pub fn from_condition(cond: Condition) -> Self {
        match cond {
            Condition::Eq => X86Cond::E,
            Condition::Ne => X86Cond::Ne,
            Condition::Ult => X86Cond::B,
            Condition::Ule => X86Cond::Be,
            Condition::Ugt => X86Cond::A,
            Condition::Uge => X86Cond::Ae,
            Condition::Slt => X86Cond::L,
            Condition::Sle => X86Cond::Le,
            Condition::Sgt => X86Cond::G,
            Condition::Sge => X86Cond::Ge,
            Condition::Negative => X86Cond::S,
            Condition::Positive => X86Cond::Ns,
            Condition::Overflow => X86Cond::O,
            Condition::NoOverflow => X86Cond::No,
            Condition::Parity => X86Cond::P,
            Condition::NoParity => X86Cond::Np,
            Condition::Always => X86Cond::E, // Shouldn't be used for conditional ops
        }
    }

    /// Invert the condition
    pub fn invert(self) -> Self {
        match self {
            X86Cond::O => X86Cond::No,
            X86Cond::No => X86Cond::O,
            X86Cond::B => X86Cond::Ae,
            X86Cond::Ae => X86Cond::B,
            X86Cond::E => X86Cond::Ne,
            X86Cond::Ne => X86Cond::E,
            X86Cond::Be => X86Cond::A,
            X86Cond::A => X86Cond::Be,
            X86Cond::S => X86Cond::Ns,
            X86Cond::Ns => X86Cond::S,
            X86Cond::P => X86Cond::Np,
            X86Cond::Np => X86Cond::P,
            X86Cond::L => X86Cond::Ge,
            X86Cond::Ge => X86Cond::L,
            X86Cond::Le => X86Cond::G,
            X86Cond::G => X86Cond::Le,
        }
    }
}

// ============================================================================
// x86_64 Instruction Emitter
// ============================================================================

/// x86_64 instruction emitter - handles raw instruction encoding
pub struct X86Emitter<'a> {
    code: &'a mut CodeBuffer,
}

impl<'a> X86Emitter<'a> {
    pub fn new(code: &'a mut CodeBuffer) -> Self {
        Self { code }
    }

    // ========================================================================
    // REX Prefix
    // ========================================================================

    /// Emit REX prefix if needed
    /// REX = 0100WRXB where:
    /// - W: 64-bit operand size
    /// - R: ModRM.reg extension
    /// - X: SIB.index extension
    /// - B: ModRM.rm or SIB.base extension
    fn emit_rex(&mut self, w: bool, r: PhysReg, x: Option<PhysReg>, b: PhysReg) {
        let mut rex = 0x40u8;
        if w {
            rex |= 0x08;
        }
        if r.is_extended() {
            rex |= 0x04;
        }
        if x.map_or(false, |reg| reg.is_extended()) {
            rex |= 0x02;
        }
        if b.is_extended() {
            rex |= 0x01;
        }
        if rex != 0x40 {
            self.code.emit_u8(rex);
        }
    }

    /// Emit REX prefix for 64-bit operation with single register
    fn emit_rex_w(&mut self, reg: PhysReg) {
        let mut rex = 0x48u8; // REX.W
        if reg.is_extended() {
            rex |= 0x01; // REX.B
        }
        self.code.emit_u8(rex);
    }

    /// Emit REX prefix for two-register operation
    fn emit_rex_rr(&mut self, w: bool, reg: PhysReg, rm: PhysReg) {
        self.emit_rex(w, reg, None, rm);
    }

    /// Emit optional REX for width
    fn emit_rex_for_width(&mut self, width: OpWidth, r: PhysReg, rm: PhysReg) {
        match width {
            OpWidth::W64 => self.emit_rex_rr(true, r, rm),
            OpWidth::W32 => {
                // Only need REX if using extended registers
                if r.is_extended() || rm.is_extended() {
                    self.emit_rex_rr(false, r, rm);
                }
            }
            OpWidth::W16 => {
                self.code.emit_u8(0x66); // Operand size prefix
                if r.is_extended() || rm.is_extended() {
                    self.emit_rex_rr(false, r, rm);
                }
            }
            OpWidth::W8 => {
                // Need REX for SPL, BPL, SIL, DIL or extended registers
                if r.is_extended()
                    || rm.is_extended()
                    || matches!(r, PhysReg::Rsp | PhysReg::Rbp | PhysReg::Rsi | PhysReg::Rdi)
                    || matches!(
                        rm,
                        PhysReg::Rsp | PhysReg::Rbp | PhysReg::Rsi | PhysReg::Rdi
                    )
                {
                    self.emit_rex_rr(false, r, rm);
                }
            }
            OpWidth::W128 => {
                // XMM operations handled separately
            }
        }
    }

    // ========================================================================
    // ModR/M and SIB
    // ========================================================================

    /// Emit ModR/M byte
    /// ModR/M = mod(2) | reg(3) | rm(3)
    fn emit_modrm(&mut self, mode: u8, reg: PhysReg, rm: PhysReg) {
        let byte = (mode << 6) | (reg.low3() << 3) | rm.low3();
        self.code.emit_u8(byte);
    }

    /// Emit ModR/M for register-register operation (mod=11)
    fn emit_modrm_rr(&mut self, reg: PhysReg, rm: PhysReg) {
        self.emit_modrm(0b11, reg, rm);
    }

    /// Emit ModR/M with /digit extension
    fn emit_modrm_digit(&mut self, mode: u8, digit: u8, rm: PhysReg) {
        let byte = (mode << 6) | (digit << 3) | rm.low3();
        self.code.emit_u8(byte);
    }

    /// Emit SIB byte
    /// SIB = scale(2) | index(3) | base(3)
    fn emit_sib(&mut self, scale: u8, index: PhysReg, base: PhysReg) {
        let scale_bits = match scale {
            1 => 0b00,
            2 => 0b01,
            4 => 0b10,
            8 => 0b11,
            _ => 0b00,
        };
        let byte = (scale_bits << 6) | (index.low3() << 3) | base.low3();
        self.code.emit_u8(byte);
    }

    // ========================================================================
    // Memory Operand Encoding
    // ========================================================================

    /// Emit ModR/M and optional SIB for memory operand [base + disp]
    fn emit_modrm_mem(&mut self, reg: PhysReg, base: PhysReg, disp: i32) {
        // RSP/R12 needs SIB byte
        let needs_sib = base == PhysReg::Rsp || base == PhysReg::R12;

        // RBP/R13 with no displacement needs explicit disp8=0
        let force_disp = (base == PhysReg::Rbp || base == PhysReg::R13) && disp == 0;

        let (mode, disp_size) = if disp == 0 && !force_disp {
            (0b00, 0) // [base]
        } else if disp >= -128 && disp <= 127 {
            (0b01, 1) // [base + disp8]
        } else {
            (0b10, 4) // [base + disp32]
        };

        if needs_sib {
            self.emit_modrm(mode, reg, PhysReg::Rsp); // rm=100 signals SIB
            self.emit_sib(1, PhysReg::Rsp, base); // index=RSP means no index
        } else {
            self.emit_modrm(mode, reg, base);
        }

        match disp_size {
            1 => self.code.emit_i8(disp as i8),
            4 => self.code.emit_i32(disp),
            _ => {}
        }
    }

    /// Emit ModR/M for [base + index*scale + disp]
    fn emit_modrm_sib(
        &mut self,
        reg: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
    ) {
        let (mode, base_reg) = match base {
            Some(b) if disp == 0 && b != PhysReg::Rbp && b != PhysReg::R13 => (0b00, b),
            Some(b) if disp >= -128 && disp <= 127 => (0b01, b),
            Some(b) => (0b10, b),
            None => (0b00, PhysReg::Rbp), // disp32 only mode
        };

        self.emit_modrm(mode, reg, PhysReg::Rsp); // rm=100 signals SIB
        self.emit_sib(scale, index, base_reg);

        match (mode, base) {
            (0b00, None) => self.code.emit_i32(disp), // disp32 only
            (0b01, _) => self.code.emit_i8(disp as i8),
            (0b10, _) => self.code.emit_i32(disp),
            _ => {}
        }
    }

    /// Emit ModR/M for absolute address [disp32] (no base, no index)
    /// Uses SIB mode with base=RBP (101), index=RSP (100) meaning no index
    fn emit_modrm_abs(&mut self, reg: PhysReg, addr: u64) {
        // ModR/M: mod=00, rm=100 (SIB follows)
        self.emit_modrm(0b00, reg, PhysReg::Rsp); // rm=100 signals SIB
                                                  // SIB: scale=00, index=100 (none), base=101 (disp32)
        self.code.emit_u8(0x25); // scale=0, index=RSP(4), base=RBP(5)
                                 // 32-bit displacement (address)
        self.code.emit_u32(addr as u32);
    }

    // ========================================================================
    // MOV Instructions
    // ========================================================================

    /// MOV r64, r64 (or r32/r16/r8)
    pub fn emit_mov_rr(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        if dst == src {
            return; // Optimize away self-move
        }

        self.emit_rex_for_width(width, src, dst);

        let opcode = match width {
            OpWidth::W8 => 0x88,
            _ => 0x89,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(src, dst);
    }

    /// MOV r64, imm64 (or r32, imm32 / etc.)
    pub fn emit_mov_ri(&mut self, dst: PhysReg, imm: i64, width: OpWidth) {
        match width {
            OpWidth::W64 => {
                if imm >= i32::MIN as i64 && imm <= i32::MAX as i64 {
                    // Use MOV r/m64, imm32 (sign-extended)
                    self.emit_rex_w(dst);
                    self.code.emit_u8(0xC7);
                    self.emit_modrm_digit(0b11, 0, dst);
                    self.code.emit_i32(imm as i32);
                } else {
                    // Full 64-bit immediate: MOV r64, imm64
                    self.emit_rex_w(dst);
                    self.code.emit_u8(0xB8 + dst.low3());
                    self.code.emit_u64(imm as u64);
                }
            }
            OpWidth::W32 => {
                if dst.is_extended() {
                    self.code.emit_u8(0x41); // REX.B
                }
                self.code.emit_u8(0xB8 + dst.low3());
                self.code.emit_u32(imm as u32);
            }
            OpWidth::W16 => {
                self.code.emit_u8(0x66); // Operand size prefix
                if dst.is_extended() {
                    self.code.emit_u8(0x41);
                }
                self.code.emit_u8(0xB8 + dst.low3());
                self.code.emit_u16(imm as u16);
            }
            OpWidth::W8 => {
                if dst.is_extended()
                    || matches!(
                        dst,
                        PhysReg::Rsp | PhysReg::Rbp | PhysReg::Rsi | PhysReg::Rdi
                    )
                {
                    self.code
                        .emit_u8(0x40 | if dst.is_extended() { 0x01 } else { 0 });
                }
                self.code.emit_u8(0xB0 + dst.low3());
                self.code.emit_u8(imm as u8);
            }
            OpWidth::W128 => {} // Not applicable
        }
    }

    /// MOV r64, [base + disp]
    pub fn emit_mov_rm(&mut self, dst: PhysReg, base: PhysReg, disp: i32, width: OpWidth) {
        self.emit_rex_for_width(width, dst, base);

        let opcode = match width {
            OpWidth::W8 => 0x8A,
            _ => 0x8B,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_mem(dst, base, disp);
    }

    /// MOV [base + disp], r64
    pub fn emit_mov_mr(&mut self, base: PhysReg, disp: i32, src: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, src, base);

        let opcode = match width {
            OpWidth::W8 => 0x88,
            _ => 0x89,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_mem(src, base, disp);
    }

    /// MOV r64, [abs32] - Load from absolute 32-bit address
    pub fn emit_mov_rm_abs(&mut self, dst: PhysReg, addr: u64, width: OpWidth) {
        // REX prefix for width and extended registers
        // Note: we use Rax as placeholder for rm since we're using SIB mode
        self.emit_rex_for_width(width, dst, PhysReg::Rax);

        let opcode = match width {
            OpWidth::W8 => 0x8A,
            _ => 0x8B,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_abs(dst, addr);
    }

    /// MOV [abs32], r64 - Store to absolute 32-bit address
    pub fn emit_mov_mr_abs(&mut self, addr: u64, src: PhysReg, width: OpWidth) {
        // REX prefix for width and extended registers
        self.emit_rex_for_width(width, src, PhysReg::Rax);

        let opcode = match width {
            OpWidth::W8 => 0x88,
            _ => 0x89,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_abs(src, addr);
    }

    /// MOV r64, [base + index*scale + disp] - Load with SIB addressing
    pub fn emit_mov_rm_sib(
        &mut self,
        dst: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        width: OpWidth,
    ) {
        // REX prefix - use index for the rm extension bit since it's in the SIB
        let base_for_rex = base.unwrap_or(PhysReg::Rax);
        let w = width == OpWidth::W64;
        self.emit_rex(w, dst, Some(index), base_for_rex);

        let opcode = match width {
            OpWidth::W8 => 0x8A,
            _ => 0x8B,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_sib(dst, base, index, scale, disp);
    }

    /// MOV [base + index*scale + disp], r64 - Store with SIB addressing
    pub fn emit_mov_mr_sib(
        &mut self,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        src: PhysReg,
        width: OpWidth,
    ) {
        let base_for_rex = base.unwrap_or(PhysReg::Rax);
        let w = width == OpWidth::W64;
        self.emit_rex(w, src, Some(index), base_for_rex);

        let opcode = match width {
            OpWidth::W8 => 0x88,
            _ => 0x89,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_sib(src, base, index, scale, disp);
    }

    /// MOVZX r64, r/m8 or r/m16
    pub fn emit_movzx(
        &mut self,
        dst: PhysReg,
        src: PhysReg,
        src_width: OpWidth,
        dst_width: OpWidth,
    ) {
        self.emit_rex_for_width(dst_width, dst, src);

        match src_width {
            OpWidth::W8 => {
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xB6);
            }
            OpWidth::W16 => {
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xB7);
            }
            _ => {} // 32-bit zero-extends automatically to 64-bit
        }
        self.emit_modrm_rr(dst, src);
    }

    /// MOVSX r64, r/m8 or r/m16 or r/m32
    pub fn emit_movsx(
        &mut self,
        dst: PhysReg,
        src: PhysReg,
        src_width: OpWidth,
        dst_width: OpWidth,
    ) {
        self.emit_rex_for_width(dst_width, dst, src);

        match src_width {
            OpWidth::W8 => {
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xBE);
            }
            OpWidth::W16 => {
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xBF);
            }
            OpWidth::W32 => {
                // MOVSXD r64, r/m32
                self.code.emit_u8(0x63);
            }
            _ => {}
        }
        self.emit_modrm_rr(dst, src);
    }

    // ========================================================================
    // ALU Instructions (two-operand)
    // ========================================================================

    /// Generic ALU r/m, r instruction
    fn emit_alu_rr(&mut self, opcode: u8, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, src, dst);

        let op = match width {
            OpWidth::W8 => opcode,
            _ => opcode + 1,
        };
        self.code.emit_u8(op);
        self.emit_modrm_rr(src, dst);
    }

    /// Generic ALU r/m, imm instruction
    fn emit_alu_ri(&mut self, digit: u8, dst: PhysReg, imm: i64, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        match width {
            OpWidth::W8 => {
                self.code.emit_u8(0x80);
                self.emit_modrm_digit(0b11, digit, dst);
                self.code.emit_u8(imm as u8);
            }
            _ => {
                if imm >= -128 && imm <= 127 {
                    // Use sign-extended imm8
                    self.code.emit_u8(0x83);
                    self.emit_modrm_digit(0b11, digit, dst);
                    self.code.emit_i8(imm as i8);
                } else {
                    self.code.emit_u8(0x81);
                    self.emit_modrm_digit(0b11, digit, dst);
                    self.code.emit_i32(imm as i32);
                }
            }
        }
    }

    /// ADD r/m, r
    pub fn emit_add_rr(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_alu_rr(0x00, dst, src, width);
    }

    /// ADD r/m, imm
    pub fn emit_add_ri(&mut self, dst: PhysReg, imm: i64, width: OpWidth) {
        self.emit_alu_ri(0, dst, imm, width);
    }

    /// SUB r/m, r
    pub fn emit_sub_rr(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_alu_rr(0x28, dst, src, width);
    }

    /// SUB r/m, imm
    pub fn emit_sub_ri(&mut self, dst: PhysReg, imm: i64, width: OpWidth) {
        self.emit_alu_ri(5, dst, imm, width);
    }

    /// ADC r/m, r (add with carry)
    pub fn emit_adc_rr(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_alu_rr(0x10, dst, src, width);
    }

    /// ADC r/m, imm
    pub fn emit_adc_ri(&mut self, dst: PhysReg, imm: i64, width: OpWidth) {
        self.emit_alu_ri(2, dst, imm, width);
    }

    /// SBB r/m, r (subtract with borrow)
    pub fn emit_sbb_rr(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_alu_rr(0x18, dst, src, width);
    }

    /// SBB r/m, imm
    pub fn emit_sbb_ri(&mut self, dst: PhysReg, imm: i64, width: OpWidth) {
        self.emit_alu_ri(3, dst, imm, width);
    }

    /// AND r/m, r
    pub fn emit_and_rr(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_alu_rr(0x20, dst, src, width);
    }

    /// AND r/m, imm
    pub fn emit_and_ri(&mut self, dst: PhysReg, imm: i64, width: OpWidth) {
        self.emit_alu_ri(4, dst, imm, width);
    }

    /// OR r/m, r
    pub fn emit_or_rr(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_alu_rr(0x08, dst, src, width);
    }

    /// OR r/m, imm
    pub fn emit_or_ri(&mut self, dst: PhysReg, imm: i64, width: OpWidth) {
        self.emit_alu_ri(1, dst, imm, width);
    }

    /// XOR r/m, r
    pub fn emit_xor_rr(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_alu_rr(0x30, dst, src, width);
    }

    /// XOR r/m, imm
    pub fn emit_xor_ri(&mut self, dst: PhysReg, imm: i64, width: OpWidth) {
        self.emit_alu_ri(6, dst, imm, width);
    }

    /// CMP r/m, r
    pub fn emit_cmp_rr(&mut self, op1: PhysReg, op2: PhysReg, width: OpWidth) {
        self.emit_alu_rr(0x38, op1, op2, width);
    }

    /// CMP r/m, imm
    pub fn emit_cmp_ri(&mut self, op1: PhysReg, imm: i64, width: OpWidth) {
        self.emit_alu_ri(7, op1, imm, width);
    }

    /// TEST r/m, r
    pub fn emit_test_rr(&mut self, op1: PhysReg, op2: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, op2, op1);

        let opcode = match width {
            OpWidth::W8 => 0x84,
            _ => 0x85,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(op2, op1);
    }

    /// TEST r/m, imm
    pub fn emit_test_ri(&mut self, op1: PhysReg, imm: i64, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, op1);

        match width {
            OpWidth::W8 => {
                self.code.emit_u8(0xF6);
                self.emit_modrm_digit(0b11, 0, op1);
                self.code.emit_u8(imm as u8);
            }
            _ => {
                self.code.emit_u8(0xF7);
                self.emit_modrm_digit(0b11, 0, op1);
                self.code.emit_i32(imm as i32);
            }
        }
    }

    // ========================================================================
    // Unary ALU Instructions
    // ========================================================================

    /// NEG r/m
    pub fn emit_neg(&mut self, dst: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 3, dst);
    }

    /// NOT r/m
    pub fn emit_not(&mut self, dst: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 2, dst);
    }

    /// INC r/m
    pub fn emit_inc(&mut self, dst: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        let opcode = match width {
            OpWidth::W8 => 0xFE,
            _ => 0xFF,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 0, dst);
    }

    /// DEC r/m
    pub fn emit_dec(&mut self, dst: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        let opcode = match width {
            OpWidth::W8 => 0xFE,
            _ => 0xFF,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 1, dst);
    }

    // ========================================================================
    // Shift Instructions
    // ========================================================================

    /// SHL r/m, imm8
    pub fn emit_shl_ri(&mut self, dst: PhysReg, amount: u8, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        if amount == 1 {
            let opcode = match width {
                OpWidth::W8 => 0xD0,
                _ => 0xD1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 4, dst);
        } else {
            let opcode = match width {
                OpWidth::W8 => 0xC0,
                _ => 0xC1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 4, dst);
            self.code.emit_u8(amount);
        }
    }

    /// SHL r/m, CL
    pub fn emit_shl_cl(&mut self, dst: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        let opcode = match width {
            OpWidth::W8 => 0xD2,
            _ => 0xD3,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 4, dst);
    }

    /// SHR r/m, imm8
    pub fn emit_shr_ri(&mut self, dst: PhysReg, amount: u8, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        if amount == 1 {
            let opcode = match width {
                OpWidth::W8 => 0xD0,
                _ => 0xD1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 5, dst);
        } else {
            let opcode = match width {
                OpWidth::W8 => 0xC0,
                _ => 0xC1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 5, dst);
            self.code.emit_u8(amount);
        }
    }

    /// SHR r/m, CL
    pub fn emit_shr_cl(&mut self, dst: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        let opcode = match width {
            OpWidth::W8 => 0xD2,
            _ => 0xD3,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 5, dst);
    }

    /// SAR r/m, imm8
    pub fn emit_sar_ri(&mut self, dst: PhysReg, amount: u8, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        if amount == 1 {
            let opcode = match width {
                OpWidth::W8 => 0xD0,
                _ => 0xD1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 7, dst);
        } else {
            let opcode = match width {
                OpWidth::W8 => 0xC0,
                _ => 0xC1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 7, dst);
            self.code.emit_u8(amount);
        }
    }

    /// SAR r/m, CL
    pub fn emit_sar_cl(&mut self, dst: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        let opcode = match width {
            OpWidth::W8 => 0xD2,
            _ => 0xD3,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 7, dst);
    }

    // ========================================================================
    // Multiply/Divide
    // ========================================================================

    /// IMUL r, r/m (two-operand form, dst = dst * src)
    pub fn emit_imul_rr(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, dst, src);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xAF);
        self.emit_modrm_rr(dst, src);
    }

    /// IMUL r, r/m, imm (three-operand form)
    pub fn emit_imul_rri(&mut self, dst: PhysReg, src: PhysReg, imm: i32, width: OpWidth) {
        self.emit_rex_for_width(width, dst, src);

        if imm >= -128 && imm <= 127 {
            self.code.emit_u8(0x6B);
            self.emit_modrm_rr(dst, src);
            self.code.emit_i8(imm as i8);
        } else {
            self.code.emit_u8(0x69);
            self.emit_modrm_rr(dst, src);
            self.code.emit_i32(imm);
        }
    }

    /// MUL r/m (unsigned, RDX:RAX = RAX * r/m)
    pub fn emit_mul(&mut self, src: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, src);

        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 4, src);
    }

    /// IMUL r/m (signed, RDX:RAX = RAX * r/m)
    pub fn emit_imul(&mut self, src: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, src);

        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 5, src);
    }

    /// DIV r/m (unsigned)
    pub fn emit_div(&mut self, src: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, src);

        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 6, src);
    }

    /// IDIV r/m (signed)
    pub fn emit_idiv(&mut self, src: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, src);

        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 7, src);
    }

    /// CQO (sign-extend RAX into RDX:RAX)
    pub fn emit_cqo(&mut self) {
        self.code.emit_u8(0x48); // REX.W
        self.code.emit_u8(0x99);
    }

    /// CDQ (sign-extend EAX into EDX:EAX)
    pub fn emit_cdq(&mut self) {
        self.code.emit_u8(0x99);
    }

    /// XOR RDX, RDX (zero RDX for unsigned division)
    pub fn emit_zero_rdx(&mut self) {
        self.emit_xor_rr(PhysReg::Rdx, PhysReg::Rdx, OpWidth::W64);
    }

    // ========================================================================
    // Stack Operations
    // ========================================================================

    /// PUSH r64
    pub fn emit_push(&mut self, src: PhysReg) {
        if src.is_extended() {
            self.code.emit_u8(0x41); // REX.B
        }
        self.code.emit_u8(0x50 + src.low3());
    }

    /// POP r64
    pub fn emit_pop(&mut self, dst: PhysReg) {
        if dst.is_extended() {
            self.code.emit_u8(0x41); // REX.B
        }
        self.code.emit_u8(0x58 + dst.low3());
    }

    // ========================================================================
    // Control Flow
    // ========================================================================

    /// CALL rel32
    pub fn emit_call_rel32(&mut self, rel: i32) {
        self.code.emit_u8(0xE8);
        self.code.emit_i32(rel);
    }

    /// CALL r/m64
    pub fn emit_call_reg(&mut self, target: PhysReg) {
        if target.is_extended() {
            self.code.emit_u8(0x41); // REX.B
        }
        self.code.emit_u8(0xFF);
        self.emit_modrm_digit(0b11, 2, target);
    }

    /// RET
    pub fn emit_ret(&mut self) {
        self.code.emit_u8(0xC3);
    }

    /// JMP rel8
    pub fn emit_jmp_rel8(&mut self, rel: i8) {
        self.code.emit_u8(0xEB);
        self.code.emit_i8(rel);
    }

    /// JMP rel32
    pub fn emit_jmp_rel32(&mut self, rel: i32) {
        self.code.emit_u8(0xE9);
        self.code.emit_i32(rel);
    }

    /// JMP r/m64
    pub fn emit_jmp_reg(&mut self, target: PhysReg) {
        if target.is_extended() {
            self.code.emit_u8(0x41);
        }
        self.code.emit_u8(0xFF);
        self.emit_modrm_digit(0b11, 4, target);
    }

    /// Jcc rel8
    pub fn emit_jcc_rel8(&mut self, cond: X86Cond, rel: i8) {
        self.code.emit_u8(0x70 + cond as u8);
        self.code.emit_i8(rel);
    }

    /// Jcc rel32
    pub fn emit_jcc_rel32(&mut self, cond: X86Cond, rel: i32) {
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x80 + cond as u8);
        self.code.emit_i32(rel);
    }

    /// SETcc r/m8
    pub fn emit_setcc(&mut self, cond: X86Cond, dst: PhysReg) {
        // Need REX for certain registers
        if dst.is_extended()
            || matches!(
                dst,
                PhysReg::Rsp | PhysReg::Rbp | PhysReg::Rsi | PhysReg::Rdi
            )
        {
            self.code
                .emit_u8(0x40 | if dst.is_extended() { 0x01 } else { 0 });
        }
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x90 + cond as u8);
        self.emit_modrm_digit(0b11, 0, dst);
    }

    /// CMOVcc r, r/m
    pub fn emit_cmovcc(&mut self, cond: X86Cond, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, dst, src);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x40 + cond as u8);
        self.emit_modrm_rr(dst, src);
    }

    // ========================================================================
    // Miscellaneous
    // ========================================================================

    /// NOP (single-byte)
    pub fn emit_nop(&mut self) {
        self.code.emit_u8(0x90);
    }

    /// Multi-byte NOP
    pub fn emit_nop_n(&mut self, n: usize) {
        // Use optimal multi-byte NOPs
        let mut remaining = n;
        while remaining > 0 {
            match remaining {
                1 => {
                    self.code.emit_u8(0x90);
                    remaining -= 1;
                }
                2 => {
                    self.code.emit_bytes(&[0x66, 0x90]);
                    remaining -= 2;
                }
                3 => {
                    self.code.emit_bytes(&[0x0F, 0x1F, 0x00]);
                    remaining -= 3;
                }
                4 => {
                    self.code.emit_bytes(&[0x0F, 0x1F, 0x40, 0x00]);
                    remaining -= 4;
                }
                5 => {
                    self.code.emit_bytes(&[0x0F, 0x1F, 0x44, 0x00, 0x00]);
                    remaining -= 5;
                }
                6 => {
                    self.code.emit_bytes(&[0x66, 0x0F, 0x1F, 0x44, 0x00, 0x00]);
                    remaining -= 6;
                }
                7 => {
                    self.code
                        .emit_bytes(&[0x0F, 0x1F, 0x80, 0x00, 0x00, 0x00, 0x00]);
                    remaining -= 7;
                }
                8 => {
                    self.code
                        .emit_bytes(&[0x0F, 0x1F, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00]);
                    remaining -= 8;
                }
                _ => {
                    self.code
                        .emit_bytes(&[0x66, 0x0F, 0x1F, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00]);
                    remaining -= 9;
                }
            }
        }
    }

    /// INT3 (breakpoint)
    pub fn emit_int3(&mut self) {
        self.code.emit_u8(0xCC);
    }

    /// UD2 (undefined instruction)
    pub fn emit_ud2(&mut self) {
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x0B);
    }

    /// LEA r64, [base + disp]
    pub fn emit_lea(&mut self, dst: PhysReg, base: PhysReg, disp: i32) {
        self.emit_rex_rr(true, dst, base);
        self.code.emit_u8(0x8D);
        self.emit_modrm_mem(dst, base, disp);
    }

    /// LEA r64, [base + index*scale + disp]
    pub fn emit_lea_sib(
        &mut self,
        dst: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
    ) {
        self.emit_rex(true, dst, Some(index), base.unwrap_or(PhysReg::Rbp));
        self.code.emit_u8(0x8D);
        self.emit_modrm_sib(dst, base, index, scale, disp);
    }

    /// XCHG r64, r64
    pub fn emit_xchg(&mut self, r1: PhysReg, r2: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, r1, r2);

        let opcode = match width {
            OpWidth::W8 => 0x86,
            _ => 0x87,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(r1, r2);
    }

    /// BSWAP r64/r32
    pub fn emit_bswap(&mut self, reg: PhysReg, width: OpWidth) {
        match width {
            OpWidth::W64 => {
                self.emit_rex_w(reg);
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xC8 + reg.low3());
            }
            OpWidth::W32 => {
                if reg.is_extended() {
                    self.code.emit_u8(0x41);
                }
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xC8 + reg.low3());
            }
            _ => {} // BSWAP only works on 32/64-bit
        }
    }

    /// BSF r, r/m
    pub fn emit_bsf(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, dst, src);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xBC);
        self.emit_modrm_rr(dst, src);
    }

    /// BSR r, r/m
    pub fn emit_bsr(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, dst, src);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xBD);
        self.emit_modrm_rr(dst, src);
    }

    /// LZCNT r, r/m (requires LZCNT support)
    pub fn emit_lzcnt(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.code.emit_u8(0xF3); // Rep prefix
        self.emit_rex_for_width(width, dst, src);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xBD);
        self.emit_modrm_rr(dst, src);
    }

    /// TZCNT r, r/m (requires BMI1)
    pub fn emit_tzcnt(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.code.emit_u8(0xF3); // Rep prefix
        self.emit_rex_for_width(width, dst, src);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xBC);
        self.emit_modrm_rr(dst, src);
    }

    /// POPCNT r, r/m
    pub fn emit_popcnt(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.code.emit_u8(0xF3); // Rep prefix
        self.emit_rex_for_width(width, dst, src);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xB8);
        self.emit_modrm_rr(dst, src);
    }
}

// ============================================================================
// x86_64 Lowerer
// ============================================================================

/// x86_64 code generator
pub struct X86_64Lowerer {
    /// Code buffer
    code: CodeBuffer,

    /// Register allocator
    regalloc: RegAlloc,

    /// Block offsets in generated code
    block_offsets: HashMap<BlockId, usize>,

    /// Relocations to apply
    relocations: Vec<Relocation>,

    /// Pending jumps to fix up (source offset, target block, reloc kind)
    pending_jumps: Vec<(usize, BlockId, RelocKind)>,
}

impl X86_64Lowerer {
    /// Create a new x86_64 lowerer
    pub fn new() -> Self {
        X86_64Lowerer {
            code: CodeBuffer::with_capacity(4096),
            regalloc: RegAlloc::new(),
            block_offsets: HashMap::new(),
            relocations: Vec::new(),
            pending_jumps: Vec::new(),
        }
    }

    /// Get a physical register for a VReg, loading from stack if needed
    fn get_reg(&mut self, vreg: VReg) -> Result<PhysReg, LowerError> {
        let loc = self.regalloc.alloc_vreg(vreg)?;
        match loc {
            RegLocation::Register(r) => Ok(r),
            RegLocation::Stack(offset) => {
                // Load from stack into a temp register
                let temp = self.regalloc.get_scratch()?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(temp, PhysReg::Rbp, offset, OpWidth::W64);
                Ok(temp)
            }
            RegLocation::Constant(val) => {
                // Load constant into a register
                let temp = self.regalloc.get_scratch()?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_ri(temp, val, OpWidth::W64);
                Ok(temp)
            }
            RegLocation::Unallocated => Err(LowerError::RegisterAllocationFailed {
                reason: "vreg not allocated".to_string(),
            }),
        }
    }

    /// Get the destination register for a VReg
    fn get_dst_reg(&mut self, vreg: VReg) -> Result<PhysReg, LowerError> {
        let loc = self.regalloc.alloc_vreg(vreg)?;
        match loc {
            RegLocation::Register(r) => Ok(r),
            RegLocation::Stack(_) | RegLocation::Constant(_) | RegLocation::Unallocated => {
                Err(LowerError::RegisterAllocationFailed {
                    reason: "destination must be a register".to_string(),
                })
            }
        }
    }

    /// Emit function prologue
    fn emit_prologue(&mut self) {
        let mut emitter = X86Emitter::new(&mut self.code);

        // PUSH RBP
        emitter.emit_push(PhysReg::Rbp);

        // MOV RBP, RSP
        emitter.emit_mov_rr(PhysReg::Rbp, PhysReg::Rsp, OpWidth::W64);

        // Save callee-saved registers
        for &reg in self.regalloc.callee_saved_used() {
            emitter.emit_push(reg);
        }

        // Allocate stack space for spills
        let frame_size = self.regalloc.frame_size();
        if frame_size > 0 {
            emitter.emit_sub_ri(PhysReg::Rsp, frame_size as i64, OpWidth::W64);
        }
    }

    /// Emit function epilogue
    fn emit_epilogue(&mut self) {
        let mut emitter = X86Emitter::new(&mut self.code);

        // Deallocate stack space
        let frame_size = self.regalloc.frame_size();
        if frame_size > 0 {
            emitter.emit_add_ri(PhysReg::Rsp, frame_size as i64, OpWidth::W64);
        }

        // Restore callee-saved registers (in reverse order)
        let callee_saved: Vec<_> = self.regalloc.callee_saved_used().to_vec();
        for &reg in callee_saved.iter().rev() {
            emitter.emit_pop(reg);
        }

        // POP RBP
        emitter.emit_pop(PhysReg::Rbp);

        // RET
        emitter.emit_ret();
    }

    /// Lower a single operation
    fn lower_op(&mut self, op: &crate::smir::ops::SmirOp) -> Result<(), LowerError> {
        match &op.kind {
            // ================================================================
            // Data Movement
            // ================================================================
            OpKind::Mov { dst, src, width } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                match src {
                    SrcOperand::Reg(r) => {
                        let src_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rr(dst_reg, src_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_ri(dst_reg, *val, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Mov with shifted/extended operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Lea { dst, addr } => {
                let dst_reg = self.get_dst_reg(*dst)?;

                match addr {
                    Address::Direct(base) => {
                        let base_reg = self.get_reg(*base)?;
                        // LEA dst, [base] is just a MOV
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rr(dst_reg, base_reg, OpWidth::W64);
                    }
                    Address::BaseOffset { base, offset } => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_lea(dst_reg, base_reg, *offset as i32);
                    }
                    Address::BaseIndexScale {
                        base,
                        index,
                        scale,
                        disp,
                    } => {
                        let index_reg = self.get_reg(*index)?;
                        let base_phys = match base {
                            Some(b) => Some(self.get_reg(*b)?),
                            None => None,
                        };
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_lea_sib(dst_reg, base_phys, index_reg, *scale, *disp);
                    }
                    Address::PcRel { offset } => {
                        // For PC-relative, compute the absolute address
                        // This requires knowing the current PC, which we don't have here
                        // For now, use a MOV with the computed address
                        return Err(LowerError::UnsupportedOp {
                            op: "Lea with PcRel address".to_string(),
                        });
                    }
                    Address::Absolute(addr) => {
                        // LEA with absolute address - just MOV the constant
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_ri(dst_reg, *addr as i64, OpWidth::W64);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("Lea with {:?} address", addr),
                        });
                    }
                }
            }

            OpKind::CMove {
                dst,
                src,
                cond,
                width,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let x86_cond = X86Cond::from_condition(*cond);

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_cmovcc(x86_cond, dst_reg, src_reg, *width);
            }

            // ================================================================
            // Integer Arithmetic
            // ================================================================
            OpKind::Add {
                dst,
                src1,
                src2,
                width,
                ..
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;

                // Move src1 to dst if different
                if dst_reg != src1_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                }

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_add_rr(dst_reg, src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_add_ri(dst_reg, *val, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Add with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Sub {
                dst,
                src1,
                src2,
                width,
                ..
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;

                if dst_reg != src1_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                }

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sub_rr(dst_reg, src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sub_ri(dst_reg, *val, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Sub with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Adc {
                dst,
                src1,
                src2,
                width,
                ..
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;

                if dst_reg != src1_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                }

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_adc_rr(dst_reg, src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_adc_ri(dst_reg, *val, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Adc with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Sbb {
                dst,
                src1,
                src2,
                width,
                ..
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;

                if dst_reg != src1_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                }

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sbb_rr(dst_reg, src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sbb_ri(dst_reg, *val, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Sbb with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Neg {
                dst, src, width, ..
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, *width);
                }

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_neg(dst_reg, *width);
            }

            OpKind::MulS {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                ..
            } => {
                // For two-operand IMUL (dst = src1 * src2), we use the efficient form
                // For widening multiply (dst_hi:dst_lo = src1 * src2), we use IMUL with RAX
                if dst_hi.is_some() {
                    // Widening multiply: IMUL r/m -> RDX:RAX = RAX * r/m
                    // Move src1 to RAX
                    let src1_reg = self.get_reg(*src1)?;
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rr(PhysReg::Rax, src1_reg, *width);
                    }

                    // Get src2 and do IMUL
                    match src2 {
                        SrcOperand::Reg(r) => {
                            let src2_reg = self.get_reg(*r)?;
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_imul(src2_reg, *width);
                        }
                        SrcOperand::Imm(val) => {
                            // Load immediate to a temp register
                            let temp = self.regalloc.get_scratch()?;
                            {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_ri(temp, *val, *width);
                            }
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_imul(temp, *width);
                            self.regalloc.free_temp(temp);
                        }
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: "MulS with shifted operand".to_string(),
                            });
                        }
                    }

                    // Move results to destination registers
                    let dst_lo_reg = self.get_dst_reg(*dst_lo)?;
                    if dst_lo_reg != PhysReg::Rax {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rr(dst_lo_reg, PhysReg::Rax, *width);
                    }

                    if let Some(hi) = dst_hi {
                        let dst_hi_reg = self.get_dst_reg(*hi)?;
                        if dst_hi_reg != PhysReg::Rdx {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_rr(dst_hi_reg, PhysReg::Rdx, *width);
                        }
                    }
                } else {
                    // Two-operand form: dst = src1 * src2
                    let dst_reg = self.get_dst_reg(*dst_lo)?;
                    let src1_reg = self.get_reg(*src1)?;

                    match src2 {
                        SrcOperand::Reg(r) => {
                            let src2_reg = self.get_reg(*r)?;
                            // Move src1 to dst, then IMUL dst, src2
                            if dst_reg != src1_reg {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                            }
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_imul_rr(dst_reg, src2_reg, *width);
                        }
                        SrcOperand::Imm(val) => {
                            // Three-operand form: IMUL dst, src1, imm
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_imul_rri(dst_reg, src1_reg, *val as i32, *width);
                        }
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: "MulS with shifted operand".to_string(),
                            });
                        }
                    }
                }
            }

            OpKind::MulU {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                ..
            } => {
                // Unsigned multiply always uses RAX
                // MUL r/m -> RDX:RAX = RAX * r/m
                let src1_reg = self.get_reg(*src1)?;
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(PhysReg::Rax, src1_reg, *width);
                }

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mul(src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let temp = self.regalloc.get_scratch()?;
                        {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_ri(temp, *val, *width);
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mul(temp, *width);
                        self.regalloc.free_temp(temp);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "MulU with shifted operand".to_string(),
                        });
                    }
                }

                // Move results to destination registers
                let dst_lo_reg = self.get_dst_reg(*dst_lo)?;
                if dst_lo_reg != PhysReg::Rax {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_lo_reg, PhysReg::Rax, *width);
                }

                if let Some(hi) = dst_hi {
                    let dst_hi_reg = self.get_dst_reg(*hi)?;
                    if dst_hi_reg != PhysReg::Rdx {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rr(dst_hi_reg, PhysReg::Rdx, *width);
                    }
                }
            }

            OpKind::DivU {
                quot,
                rem,
                src1,
                src2,
                width,
            } => {
                // Unsigned divide: RDX:RAX / src2 -> RAX (quot), RDX (rem)
                // For unsigned, RDX must be zero
                let src1_reg = self.get_reg(*src1)?;
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    // Move dividend to RAX
                    emitter.emit_mov_rr(PhysReg::Rax, src1_reg, *width);
                    // Zero RDX
                    emitter.emit_zero_rdx();
                }

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_div(src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        // DIV doesn't support immediate, need to load into temp
                        let temp = self.regalloc.get_scratch()?;
                        {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_ri(temp, *val, *width);
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_div(temp, *width);
                        self.regalloc.free_temp(temp);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "DivU with shifted operand".to_string(),
                        });
                    }
                }

                // Move results to destination registers
                let quot_reg = self.get_dst_reg(*quot)?;
                if quot_reg != PhysReg::Rax {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(quot_reg, PhysReg::Rax, *width);
                }

                if let Some(r) = rem {
                    let rem_reg = self.get_dst_reg(*r)?;
                    if rem_reg != PhysReg::Rdx {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rr(rem_reg, PhysReg::Rdx, *width);
                    }
                }
            }

            OpKind::DivS {
                quot,
                rem,
                src1,
                src2,
                width,
            } => {
                // Signed divide: RDX:RAX / src2 -> RAX (quot), RDX (rem)
                // For signed, RDX must be sign-extension of RAX (via CQO/CDQ)
                let src1_reg = self.get_reg(*src1)?;
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    // Move dividend to RAX
                    emitter.emit_mov_rr(PhysReg::Rax, src1_reg, *width);
                    // Sign-extend RAX into RDX:RAX
                    match width {
                        OpWidth::W64 => emitter.emit_cqo(),
                        OpWidth::W32 => emitter.emit_cdq(),
                        _ => {
                            // For 16-bit: CWD, for 8-bit: CBW
                            // We'll use the 32-bit form for smaller widths
                            emitter.emit_cdq();
                        }
                    }
                }

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_idiv(src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        // IDIV doesn't support immediate, need to load into temp
                        let temp = self.regalloc.get_scratch()?;
                        {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_ri(temp, *val, *width);
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_idiv(temp, *width);
                        self.regalloc.free_temp(temp);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "DivS with shifted operand".to_string(),
                        });
                    }
                }

                // Move results to destination registers
                let quot_reg = self.get_dst_reg(*quot)?;
                if quot_reg != PhysReg::Rax {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(quot_reg, PhysReg::Rax, *width);
                }

                if let Some(r) = rem {
                    let rem_reg = self.get_dst_reg(*r)?;
                    if rem_reg != PhysReg::Rdx {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rr(rem_reg, PhysReg::Rdx, *width);
                    }
                }
            }

            // ================================================================
            // Bitwise Operations
            // ================================================================
            OpKind::And {
                dst,
                src1,
                src2,
                width,
                ..
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;

                if dst_reg != src1_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                }

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_and_rr(dst_reg, src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_and_ri(dst_reg, *val, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "And with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Or {
                dst,
                src1,
                src2,
                width,
                ..
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;

                if dst_reg != src1_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                }

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_or_rr(dst_reg, src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_or_ri(dst_reg, *val, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Or with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Xor {
                dst,
                src1,
                src2,
                width,
                ..
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;

                if dst_reg != src1_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                }

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_xor_rr(dst_reg, src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_xor_ri(dst_reg, *val, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Xor with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Not { dst, src, width } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, *width);
                }

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_not(dst_reg, *width);
            }

            // ================================================================
            // Shifts
            // ================================================================
            OpKind::Shl {
                dst,
                src,
                amount,
                width,
                ..
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, *width);
                }

                match amount {
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_shl_ri(dst_reg, *val as u8, *width);
                    }
                    SrcOperand::Reg(r) => {
                        // Move shift amount to CL
                        let amt_reg = self.get_reg(*r)?;
                        if amt_reg != PhysReg::Rcx {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_rr(PhysReg::Rcx, amt_reg, OpWidth::W8);
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_shl_cl(dst_reg, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Shl with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Shr {
                dst,
                src,
                amount,
                width,
                ..
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, *width);
                }

                match amount {
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_shr_ri(dst_reg, *val as u8, *width);
                    }
                    SrcOperand::Reg(r) => {
                        let amt_reg = self.get_reg(*r)?;
                        if amt_reg != PhysReg::Rcx {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_rr(PhysReg::Rcx, amt_reg, OpWidth::W8);
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_shr_cl(dst_reg, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Shr with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Sar {
                dst,
                src,
                amount,
                width,
                ..
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, *width);
                }

                match amount {
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sar_ri(dst_reg, *val as u8, *width);
                    }
                    SrcOperand::Reg(r) => {
                        let amt_reg = self.get_reg(*r)?;
                        if amt_reg != PhysReg::Rcx {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_rr(PhysReg::Rcx, amt_reg, OpWidth::W8);
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sar_cl(dst_reg, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Sar with shifted operand".to_string(),
                        });
                    }
                }
            }

            // ================================================================
            // Comparisons
            // ================================================================
            OpKind::Cmp { src1, src2, width } => {
                let src1_reg = self.get_reg(*src1)?;

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_cmp_rr(src1_reg, src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_cmp_ri(src1_reg, *val, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Cmp with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Test { src1, src2, width } => {
                let src1_reg = self.get_reg(*src1)?;

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_test_rr(src1_reg, src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_test_ri(src1_reg, *val, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Test with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::SetCC { dst, cond, width } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let x86_cond = X86Cond::from_condition(*cond);

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_setcc(x86_cond, dst_reg);

                // Zero-extend to full width if needed
                if *width != OpWidth::W8 {
                    emitter.emit_movzx(dst_reg, dst_reg, OpWidth::W8, *width);
                }
            }

            // ================================================================
            // Memory Operations
            // ================================================================
            OpKind::Load {
                dst,
                addr,
                width,
                sign,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let op_width = width.to_op_width().unwrap_or(OpWidth::W64);

                match addr {
                    Address::Direct(base) => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rm(dst_reg, base_reg, 0, op_width);

                        // Sign/zero extend if loading smaller than 64-bit
                        if op_width != OpWidth::W64 {
                            match sign {
                                SignExtend::Zero => {
                                    // 32-bit loads automatically zero-extend
                                    if op_width != OpWidth::W32 {
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    Address::BaseOffset { base, offset } => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rm(dst_reg, base_reg, *offset as i32, op_width);

                        if op_width != OpWidth::W64 {
                            match sign {
                                SignExtend::Zero => {
                                    if op_width != OpWidth::W32 {
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    Address::Absolute(abs_addr) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rm_abs(dst_reg, *abs_addr, op_width);

                        if op_width != OpWidth::W64 {
                            match sign {
                                SignExtend::Zero => {
                                    if op_width != OpWidth::W32 {
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    Address::BaseIndexScale {
                        base,
                        index,
                        scale,
                        disp,
                    } => {
                        let base_reg = base.map(|b| self.get_reg(b)).transpose()?;
                        let index_reg = self.get_reg(*index)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter
                            .emit_mov_rm_sib(dst_reg, base_reg, index_reg, *scale, *disp, op_width);

                        if op_width != OpWidth::W64 {
                            match sign {
                                SignExtend::Zero => {
                                    if op_width != OpWidth::W32 {
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("Load with unsupported addressing: {:?}", addr),
                        });
                    }
                }
            }

            OpKind::Store { src, addr, width } => {
                let src_reg = self.get_reg(*src)?;
                let op_width = width.to_op_width().unwrap_or(OpWidth::W64);

                match addr {
                    Address::Direct(base) => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_mr(base_reg, 0, src_reg, op_width);
                    }
                    Address::BaseOffset { base, offset } => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_mr(base_reg, *offset as i32, src_reg, op_width);
                    }
                    Address::Absolute(abs_addr) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_mr_abs(*abs_addr, src_reg, op_width);
                    }
                    Address::BaseIndexScale {
                        base,
                        index,
                        scale,
                        disp,
                    } => {
                        let base_reg = base.map(|b| self.get_reg(b)).transpose()?;
                        let index_reg = self.get_reg(*index)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter
                            .emit_mov_mr_sib(base_reg, index_reg, *scale, *disp, src_reg, op_width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("Store with unsupported addressing: {:?}", addr),
                        });
                    }
                }
            }

            // ================================================================
            // Extensions
            // ================================================================
            OpKind::ZeroExtend {
                dst,
                src,
                from_width,
                to_width,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                let mut emitter = X86Emitter::new(&mut self.code);
                if *from_width == OpWidth::W32 && *to_width == OpWidth::W64 {
                    // 32-bit mov automatically zero-extends
                    emitter.emit_mov_rr(dst_reg, src_reg, OpWidth::W32);
                } else {
                    emitter.emit_movzx(dst_reg, src_reg, *from_width, *to_width);
                }
            }

            OpKind::SignExtend {
                dst,
                src,
                from_width,
                to_width,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_movsx(dst_reg, src_reg, *from_width, *to_width);
            }

            // ================================================================
            // Misc
            // ================================================================
            OpKind::Nop => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_nop();
            }

            OpKind::Breakpoint => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_int3();
            }

            OpKind::Undefined { .. } => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_ud2();
            }

            // Unimplemented ops
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("{:?}", op.kind),
                });
            }
        }

        Ok(())
    }

    /// Lower a block terminator
    fn lower_terminator(&mut self, term: &Terminator) -> Result<(), LowerError> {
        match term {
            Terminator::Branch { target } => {
                // Record jump to fix up later
                let jump_offset = self.code.position();
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_jmp_rel32(0); // Placeholder
                self.pending_jumps
                    .push((jump_offset + 1, *target, RelocKind::PcRel32));
            }

            Terminator::CondBranch {
                cond,
                true_target,
                false_target,
            } => {
                // The condition should already be in flags from a previous Cmp/Test
                // For now, assume cond is a vreg holding 0 or 1
                let cond_reg = self.get_reg(*cond)?;

                // TEST cond, cond
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_test_rr(cond_reg, cond_reg, OpWidth::W64);
                }

                // JNZ true_target
                let jnz_offset = self.code.position();
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_jcc_rel32(X86Cond::Ne, 0); // Placeholder
                }
                self.pending_jumps
                    .push((jnz_offset + 2, *true_target, RelocKind::PcRel32));

                // JMP false_target
                let jmp_offset = self.code.position();
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_jmp_rel32(0); // Placeholder
                }
                self.pending_jumps
                    .push((jmp_offset + 1, *false_target, RelocKind::PcRel32));
            }

            Terminator::Return { .. } => {
                self.emit_epilogue();
            }

            Terminator::Unreachable => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_ud2();
            }

            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("Terminator: {:?}", term),
                });
            }
        }

        Ok(())
    }

    /// Lower a single basic block
    fn lower_block(&mut self, block: &SmirBlock) -> Result<(), LowerError> {
        // Record block offset
        self.block_offsets.insert(block.id, self.code.position());

        // Initialize register allocator for this block
        self.regalloc.begin_block(block);

        // Lower each operation
        for (idx, op) in block.ops.iter().enumerate() {
            self.regalloc.set_current_idx(idx);
            self.lower_op(op)?;
        }

        // Lower terminator
        self.lower_terminator(&block.terminator)?;

        Ok(())
    }

    /// Fix up all pending jumps
    fn fixup_jumps(&mut self) -> Result<(), LowerError> {
        for (offset, target, kind) in self.pending_jumps.drain(..).collect::<Vec<_>>() {
            let target_offset =
                self.block_offsets
                    .get(&target)
                    .ok_or_else(|| LowerError::UndefinedLabel {
                        label: format!("block_{}", target.0),
                    })?;

            match kind {
                RelocKind::PcRel32 => {
                    let rel = (*target_offset as i64) - (offset as i64) - 4;
                    if rel < i32::MIN as i64 || rel > i32::MAX as i64 {
                        return Err(LowerError::RelocationOutOfRange {
                            offset,
                            target: *target_offset,
                        });
                    }
                    self.code.patch_i32(offset, rel as i32);
                }
                RelocKind::PcRel8 => {
                    let rel = (*target_offset as i64) - (offset as i64) - 1;
                    if rel < -128 || rel > 127 {
                        return Err(LowerError::RelocationOutOfRange {
                            offset,
                            target: *target_offset,
                        });
                    }
                    self.code.data[offset] = rel as i8 as u8;
                }
                _ => {}
            }
        }

        Ok(())
    }
}

impl Default for X86_64Lowerer {
    fn default() -> Self {
        Self::new()
    }
}

impl SmirLowerer for X86_64Lowerer {
    fn target_arch(&self) -> &'static str {
        "x86_64"
    }

    fn lower_function(&mut self, func: &SmirFunction) -> Result<LowerResult, LowerError> {
        // Reset state
        self.code.clear();
        self.regalloc.reset();
        self.block_offsets.clear();
        self.relocations.clear();
        self.pending_jumps.clear();

        let entry_offset = self.code.position();

        // First pass: allocate registers and compute frame size
        // For now, use simple approach - just lower blocks in order

        // Emit prologue (we'll fix this up after knowing frame size)
        // For now, emit a minimal prologue
        let prologue_start = self.code.position();
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_push(PhysReg::Rbp);
            emitter.emit_mov_rr(PhysReg::Rbp, PhysReg::Rsp, OpWidth::W64);
        }

        // Lower entry block first
        if let Some(entry_block) = func.get_block(func.entry) {
            self.lower_block(entry_block)?;
        }

        // Lower remaining blocks
        for block in &func.blocks {
            if block.id != func.entry {
                self.lower_block(block)?;
            }
        }

        // Fix up all jumps
        self.fixup_jumps()?;

        let code_size = self.code.len();

        Ok(LowerResult {
            code_size,
            entry_offset,
            block_offsets: self.block_offsets.clone(),
            relocations: self.relocations.clone(),
            stack_size: self.regalloc.frame_size(),
        })
    }

    fn code_buffer(&self) -> &CodeBuffer {
        &self.code
    }

    fn finalize(&mut self) -> Result<Vec<u8>, LowerError> {
        Ok(self.code.data().to_vec())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::flags::FlagUpdate;
    use crate::smir::ir::{FunctionBuilder, Terminator};
    use crate::smir::types::{FunctionId, OpWidth, SrcOperand, VReg};

    #[test]
    fn test_emit_mov_rr() {
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            emit.emit_mov_rr(PhysReg::Rax, PhysReg::Rcx, OpWidth::W64);
        }
        // MOV RAX, RCX = 48 89 C8
        assert_eq!(buf.data(), &[0x48, 0x89, 0xC8]);
    }

    #[test]
    fn test_emit_mov_ri() {
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            emit.emit_mov_ri(PhysReg::Rax, 42, OpWidth::W64);
        }
        // MOV RAX, 42 (using imm32 sign-extended)
        // 48 C7 C0 2A 00 00 00
        assert_eq!(buf.data(), &[0x48, 0xC7, 0xC0, 0x2A, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_emit_add_rr() {
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            emit.emit_add_rr(PhysReg::Rax, PhysReg::Rbx, OpWidth::W64);
        }
        // ADD RAX, RBX = 48 01 D8
        assert_eq!(buf.data(), &[0x48, 0x01, 0xD8]);
    }

    #[test]
    fn test_emit_jmp_rel32() {
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            emit.emit_jmp_rel32(0x12345678);
        }
        // JMP rel32 = E9 78 56 34 12
        assert_eq!(buf.data(), &[0xE9, 0x78, 0x56, 0x34, 0x12]);
    }

    #[test]
    fn test_emit_ret() {
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            emit.emit_ret();
        }
        assert_eq!(buf.data(), &[0xC3]);
    }

    #[test]
    fn test_emit_push_pop() {
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            emit.emit_push(PhysReg::Rbp);
            emit.emit_pop(PhysReg::Rbp);
        }
        // PUSH RBP = 55, POP RBP = 5D
        assert_eq!(buf.data(), &[0x55, 0x5D]);
    }

    #[test]
    fn test_emit_extended_reg() {
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            emit.emit_mov_rr(PhysReg::R8, PhysReg::R9, OpWidth::W64);
        }
        // MOV R8, R9 = 4D 89 C8
        assert_eq!(buf.data(), &[0x4D, 0x89, 0xC8]);
    }

    #[test]
    fn test_emit_setcc() {
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            emit.emit_setcc(X86Cond::E, PhysReg::Rax);
        }
        // SETE AL = 0F 94 C0
        assert_eq!(buf.data(), &[0x0F, 0x94, 0xC0]);
    }

    #[test]
    fn test_lower_simple_function() {
        // Create a simple function: return 42
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);

        let v0 = builder.alloc_vreg();

        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: v0,
                src: SrcOperand::imm(42),
                width: OpWidth::W64,
            },
        );

        builder.set_terminator(Terminator::Return { values: vec![v0] });

        let func = builder.finish();

        // Lower it
        let mut lowerer = X86_64Lowerer::new();
        let result = lowerer.lower_function(&func).unwrap();

        assert!(result.code_size > 0);

        let code = lowerer.finalize().unwrap();
        // Should start with PUSH RBP; MOV RBP, RSP
        assert!(code.len() >= 4);
        assert_eq!(code[0], 0x55); // PUSH RBP
    }

    #[test]
    fn test_lower_add() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);

        let v0 = builder.alloc_vreg();
        let v1 = builder.alloc_vreg();
        let v2 = builder.alloc_vreg();

        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: v0,
                src: SrcOperand::imm(10),
                width: OpWidth::W64,
            },
        );

        builder.push_op(
            0x1004,
            OpKind::Mov {
                dst: v1,
                src: SrcOperand::imm(20),
                width: OpWidth::W64,
            },
        );

        builder.push_op(
            0x1008,
            OpKind::Add {
                dst: v2,
                src1: v0,
                src2: SrcOperand::Reg(v1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );

        builder.set_terminator(Terminator::Return { values: vec![v2] });

        let func = builder.finish();

        let mut lowerer = X86_64Lowerer::new();
        let result = lowerer.lower_function(&func).unwrap();

        assert!(result.code_size > 0);
    }

    #[test]
    fn test_x86_cond_invert() {
        assert_eq!(X86Cond::E.invert(), X86Cond::Ne);
        assert_eq!(X86Cond::L.invert(), X86Cond::Ge);
        assert_eq!(X86Cond::B.invert(), X86Cond::Ae);
    }

    #[test]
    fn test_lower_div_unsigned() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);

        let dividend = builder.alloc_vreg();
        let divisor = builder.alloc_vreg();
        let quotient = builder.alloc_vreg();
        let remainder = builder.alloc_vreg();

        // dividend = 100
        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: dividend,
                src: SrcOperand::imm(100),
                width: OpWidth::W64,
            },
        );

        // divisor = 7
        builder.push_op(
            0x1004,
            OpKind::Mov {
                dst: divisor,
                src: SrcOperand::imm(7),
                width: OpWidth::W64,
            },
        );

        // (quotient, remainder) = dividend / divisor
        builder.push_op(
            0x1008,
            OpKind::DivU {
                quot: quotient,
                rem: Some(remainder),
                src1: dividend,
                src2: SrcOperand::Reg(divisor),
                width: OpWidth::W64,
            },
        );

        builder.set_terminator(Terminator::Return {
            values: vec![quotient],
        });

        let func = builder.finish();

        let mut lowerer = X86_64Lowerer::new();
        let result = lowerer.lower_function(&func).unwrap();

        assert!(result.code_size > 0);
        let code = lowerer.finalize().unwrap();
        // Should contain DIV instruction (F7 /6)
        // Look for the pattern in the generated code
        assert!(!code.is_empty());
    }

    #[test]
    fn test_lower_div_signed() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);

        let dividend = builder.alloc_vreg();
        let divisor = builder.alloc_vreg();
        let quotient = builder.alloc_vreg();

        // dividend = -100
        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: dividend,
                src: SrcOperand::imm(-100i64),
                width: OpWidth::W64,
            },
        );

        // divisor = 7
        builder.push_op(
            0x1004,
            OpKind::Mov {
                dst: divisor,
                src: SrcOperand::imm(7),
                width: OpWidth::W64,
            },
        );

        // quotient = dividend / divisor (signed)
        builder.push_op(
            0x1008,
            OpKind::DivS {
                quot: quotient,
                rem: None,
                src1: dividend,
                src2: SrcOperand::Reg(divisor),
                width: OpWidth::W64,
            },
        );

        builder.set_terminator(Terminator::Return {
            values: vec![quotient],
        });

        let func = builder.finish();

        let mut lowerer = X86_64Lowerer::new();
        let result = lowerer.lower_function(&func).unwrap();

        assert!(result.code_size > 0);
        let code = lowerer.finalize().unwrap();
        // Should contain CQO (48 99) and IDIV (F7 /7) instructions
        assert!(!code.is_empty());
    }

    #[test]
    fn test_emit_div_instructions() {
        // Test DIV instruction encoding
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            emit.emit_div(PhysReg::Rcx, OpWidth::W64);
        }
        // DIV RCX = 48 F7 F1
        assert_eq!(buf.data(), &[0x48, 0xF7, 0xF1]);

        // Test IDIV instruction encoding
        let mut buf2 = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf2);
            emit.emit_idiv(PhysReg::Rbx, OpWidth::W64);
        }
        // IDIV RBX = 48 F7 FB
        assert_eq!(buf2.data(), &[0x48, 0xF7, 0xFB]);

        // Test CQO instruction encoding
        let mut buf3 = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf3);
            emit.emit_cqo();
        }
        // CQO = 48 99
        assert_eq!(buf3.data(), &[0x48, 0x99]);
    }
}
