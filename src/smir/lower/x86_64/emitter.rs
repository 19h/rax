//! X86Emitter: low-level x86-64 machine-code encoders

use crate::smir::lower::x86_64::*;
use std::collections::HashMap;

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86VecAlign, X86VecMap, X86X87ControlKind,
};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, Condition, DispSize, FenceKind, FpRoundMode, GuestAddr, MemWidth,
    OpWidth, ShiftOp, SignExtend, SrcOperand, VLaneOp, VReg, VecCmpCond, VecElementType,
    VecUnaryOp, VecWidth, X86Reg,
};
use crate::smir::ir::{
    CallTarget, SmirBlock, SmirFunction, Terminator, X86InstructionBytes,
    x86_evex_native_replay_spans,
};

use crate::smir::lower::regalloc::{PhysReg, RegAlloc, RegLocation};
use crate::smir::lower::{
    CodeBuffer, LowerError, LowerResult, RelocKind, RelocTarget, Relocation, SmirLowerer,
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CALL_FN_OFFSET, X86_GUEST_CPL_OFFSET,
    X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET, X86_GUEST_CTX_OFFSET, X86_GUEST_EXIT_PC_OFFSET,
    X86_GUEST_FS_BASE_OFFSET, X86_GUEST_GS_BASE_OFFSET, X86_GUEST_K_OFFSET,
    X86_GUEST_LOAD_FN_OFFSET, X86_GUEST_MXCSR_OFFSET, X86_GUEST_PAIR_LOAD_FN_OFFSET,
    X86_GUEST_PAIR_STORE_FN_OFFSET, X86_GUEST_RFLAGS_OFFSET, X86_GUEST_STORE_FN_OFFSET,
    X86_GUEST_TSC_AUX_OFFSET, X86_GUEST_VEC_LOAD_FN_OFFSET, X86_GUEST_VEC_STORE_FN_OFFSET,
    X86_GUEST_X87_TAG_WORD_OFFSET, X86_GUEST_XCR0_OFFSET, X86_GUEST_XGETBV1_OFFSET,
    X86_GUEST_ZMM_OFFSET, X86_HOST_MXCSR_OFFSET, X86_STATE_PTR_AT_RBP,
};

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
    pub(crate) fn emit_rex(&mut self, w: bool, r: PhysReg, x: Option<PhysReg>, b: PhysReg) {
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

    pub(crate) fn emit_rex_force(&mut self, w: bool, r: PhysReg, x: Option<PhysReg>, b: PhysReg) {
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
        self.code.emit_u8(rex);
    }

    /// Emit REX prefix for 64-bit operation with single register
    pub(crate) fn emit_rex_w(&mut self, reg: PhysReg) {
        let mut rex = 0x48u8; // REX.W
        if reg.is_extended() {
            rex |= 0x01; // REX.B
        }
        self.code.emit_u8(rex);
    }

    /// Emit REX prefix for two-register operation
    pub(crate) fn emit_rex_rr(&mut self, w: bool, reg: PhysReg, rm: PhysReg) {
        self.emit_rex(w, reg, None, rm);
    }

    /// Emit optional REX for width
    pub(crate) fn emit_rex_for_width(&mut self, width: OpWidth, r: PhysReg, rm: PhysReg) {
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
                    if r.is_extended() || rm.is_extended() {
                        self.emit_rex_rr(false, r, rm);
                    } else {
                        self.emit_rex_force(false, r, None, rm);
                    }
                }
            }
            OpWidth::W128 => {
                // XMM operations handled separately
            }
        }
    }

    /// REX/operand-size prefix for a width-EXTENDING reg-reg op (movzx/movsx),
    /// where the destination and source widths differ. The destination width
    /// drives REX.W and the 0x66 prefix, but a W8 *source* in SPL/BPL/SIL/DIL
    /// still requires a REX prefix to be PRESENT — otherwise ModRM rm 4-7 selects
    /// the legacy high bytes AH/CH/DH/BH. `emit_rex_for_width` keys that rule on
    /// the single operand width, so it misses it here (it sees the wider dst).
    pub(crate) fn emit_rex_ext(
        &mut self,
        dst_width: OpWidth,
        src_width: OpWidth,
        dst: PhysReg,
        src: PhysReg,
    ) {
        if matches!(dst_width, OpWidth::W16) {
            self.code.emit_u8(0x66);
        }
        let w = matches!(dst_width, OpWidth::W64);
        let byte_src_needs_rex = matches!(src_width, OpWidth::W8)
            && matches!(
                src,
                PhysReg::Rsp | PhysReg::Rbp | PhysReg::Rsi | PhysReg::Rdi
            );
        if w || dst.is_extended() || src.is_extended() || byte_src_needs_rex {
            self.emit_rex_force(w, dst, None, src);
        }
    }

    pub(crate) fn emit_rex_for_width_mem(
        &mut self,
        width: OpWidth,
        base: PhysReg,
        index: Option<PhysReg>,
    ) {
        let needs_rex = base.is_extended() || index.map_or(false, |reg| reg.is_extended());
        match width {
            OpWidth::W64 => self.emit_rex(true, PhysReg::Rax, index, base),
            OpWidth::W32 => {
                if needs_rex {
                    self.emit_rex(false, PhysReg::Rax, index, base);
                }
            }
            OpWidth::W16 => {
                self.code.emit_u8(0x66);
                if needs_rex {
                    self.emit_rex(false, PhysReg::Rax, index, base);
                }
            }
            OpWidth::W8 => {
                if needs_rex {
                    self.emit_rex(false, PhysReg::Rax, index, base);
                }
            }
            OpWidth::W128 => {}
        }
    }

    pub(crate) fn emit_rex_for_mem(&mut self, base: PhysReg, index: Option<PhysReg>) {
        if base.is_extended() || index.map_or(false, |reg| reg.is_extended()) {
            self.emit_rex(false, PhysReg::Rax, index, base);
        }
    }

    pub(crate) fn emit_rex_for_width_mem_reg(
        &mut self,
        width: OpWidth,
        reg: PhysReg,
        base: PhysReg,
        index: Option<PhysReg>,
    ) {
        let needs_rex =
            reg.is_extended() || base.is_extended() || index.map_or(false, |r| r.is_extended());
        match width {
            OpWidth::W64 => self.emit_rex(true, reg, index, base),
            OpWidth::W32 => {
                if needs_rex {
                    self.emit_rex(false, reg, index, base);
                }
            }
            OpWidth::W16 => {
                self.code.emit_u8(0x66);
                if needs_rex {
                    self.emit_rex(false, reg, index, base);
                }
            }
            OpWidth::W8 => {
                if needs_rex
                    || matches!(
                        reg,
                        PhysReg::Rsp | PhysReg::Rbp | PhysReg::Rsi | PhysReg::Rdi
                    )
                {
                    if needs_rex {
                        self.emit_rex(false, reg, index, base);
                    } else {
                        self.emit_rex_force(false, reg, index, base);
                    }
                }
            }
            OpWidth::W128 => {}
        }
    }

    pub(crate) fn emit_rex_for_xmm(&mut self, reg: PhysReg, rm: PhysReg) {
        if reg.is_extended() || rm.is_extended() {
            self.emit_rex(false, reg, None, rm);
        }
    }

    pub(crate) fn emit_rex_for_xmm_mem(
        &mut self,
        reg: PhysReg,
        base: PhysReg,
        index: Option<PhysReg>,
    ) {
        let needs_rex =
            reg.is_extended() || base.is_extended() || index.map_or(false, |r| r.is_extended());
        if needs_rex {
            self.emit_rex(false, reg, index, base);
        }
    }

    pub(crate) fn emit_imm_by_width(&mut self, imm: i64, width: OpWidth) {
        match width {
            OpWidth::W8 => self.code.emit_u8(imm as u8),
            OpWidth::W16 => self.code.emit_u16(imm as u16),
            OpWidth::W32 => self.code.emit_u32(imm as u32),
            OpWidth::W64 => self.code.emit_i32(imm as i32),
            OpWidth::W128 => {}
        }
    }

    // ========================================================================
    // ModR/M and SIB
    // ========================================================================

    /// Emit ModR/M byte
    /// ModR/M = mod(2) | reg(3) | rm(3)
    pub(crate) fn emit_modrm(&mut self, mode: u8, reg: PhysReg, rm: PhysReg) {
        let byte = (mode << 6) | (reg.low3() << 3) | rm.low3();
        self.code.emit_u8(byte);
    }

    /// Emit ModR/M for register-register operation (mod=11)
    pub(crate) fn emit_modrm_rr(&mut self, reg: PhysReg, rm: PhysReg) {
        self.emit_modrm(0b11, reg, rm);
    }

    /// Emit ModR/M with /digit extension
    pub(crate) fn emit_modrm_digit(&mut self, mode: u8, digit: u8, rm: PhysReg) {
        let byte = (mode << 6) | (digit << 3) | rm.low3();
        self.code.emit_u8(byte);
    }

    /// Emit SIB byte
    /// SIB = scale(2) | index(3) | base(3)
    pub(crate) fn emit_sib(&mut self, scale: u8, index: PhysReg, base: PhysReg) {
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
    pub(crate) fn emit_modrm_mem(&mut self, reg: PhysReg, base: PhysReg, disp: i32) {
        self.emit_modrm_mem_disp(reg, base, disp, DispSize::Auto);
    }

    pub(crate) fn emit_modrm_mem_disp(
        &mut self,
        reg: PhysReg,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
    ) -> Option<usize> {
        // RSP/R12 needs SIB byte
        let needs_sib = base == PhysReg::Rsp || base == PhysReg::R12;

        // RBP/R13 with no displacement needs explicit disp8=0
        let force_disp = (base == PhysReg::Rbp || base == PhysReg::R13) && disp == 0;

        let (mode, disp_bytes) = match disp_size {
            DispSize::Auto => {
                if disp == 0 && !force_disp {
                    (0b00, 0) // [base]
                } else if disp >= -128 && disp <= 127 {
                    (0b01, 1) // [base + disp8]
                } else {
                    (0b10, 4) // [base + disp32]
                }
            }
            DispSize::Disp8 => (0b01, 1),
            DispSize::Disp32 => (0b10, 4),
        };

        if needs_sib {
            self.emit_modrm(mode, reg, PhysReg::Rsp); // rm=100 signals SIB
            self.emit_sib(1, PhysReg::Rsp, base); // index=RSP means no index
        } else {
            self.emit_modrm(mode, reg, base);
        }

        let disp_offset = if disp_bytes > 0 {
            let off = self.code.position();
            match disp_bytes {
                1 => self.code.emit_i8(disp as i8),
                4 => self.code.emit_i32(disp),
                _ => {}
            }
            Some(off)
        } else {
            None
        };

        disp_offset
    }

    /// Emit ModR/M for [base + index*scale + disp]
    pub(crate) fn emit_modrm_sib(
        &mut self,
        reg: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
    ) {
        self.emit_modrm_sib_disp(reg, base, index, scale, disp, DispSize::Auto);
    }

    pub(crate) fn emit_modrm_sib_disp(
        &mut self,
        reg: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
    ) -> Option<usize> {
        let (mode, base_reg, disp_bytes) = match base {
            Some(b) => match disp_size {
                DispSize::Auto => {
                    if disp == 0 && b != PhysReg::Rbp && b != PhysReg::R13 {
                        (0b00, b, 0)
                    } else if disp >= -128 && disp <= 127 {
                        (0b01, b, 1)
                    } else {
                        (0b10, b, 4)
                    }
                }
                DispSize::Disp8 => (0b01, b, 1),
                DispSize::Disp32 => (0b10, b, 4),
            },
            None => (0b00, PhysReg::Rbp, 4), // disp32 only mode
        };

        self.emit_modrm(mode, reg, PhysReg::Rsp); // rm=100 signals SIB
        self.emit_sib(scale, index, base_reg);

        let disp_offset = if disp_bytes > 0 {
            let off = self.code.position();
            match disp_bytes {
                1 => self.code.emit_i8(disp as i8),
                4 => self.code.emit_i32(disp),
                _ => {}
            }
            Some(off)
        } else {
            None
        };

        disp_offset
    }

    pub(crate) fn emit_modrm_pcrel(&mut self, reg: PhysReg, disp: i32) -> usize {
        // mod=00, rm=101 indicates RIP-relative
        self.emit_modrm(0b00, reg, PhysReg::Rbp);
        let off = self.code.position();
        self.code.emit_i32(disp);
        off
    }

    /// Emit ModR/M for absolute address [disp32] (no base, no index)
    /// Uses SIB mode with base=RBP (101), index=RSP (100) meaning no index
    pub(crate) fn emit_modrm_abs(&mut self, reg: PhysReg, addr: u64) {
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
        if dst == src && width != OpWidth::W32 {
            // A same-register move is a true no-op for 8/16/64/128-bit widths,
            // but a 32-bit `mov eax, eax` ZERO-EXTENDS bits 63:32 (the canonical
            // x86-64 zero-extend idiom), so it must still be emitted.
            return;
        }

        self.emit_rex_for_width(width, src, dst);

        let opcode = match width {
            OpWidth::W8 => 0x88,
            _ => 0x89,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(src, dst);
    }

    /// MOV r/m, imm using ModR/M encoding
    pub fn emit_mov_rm_imm(&mut self, dst: PhysReg, imm: i64, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        let opcode = match width {
            OpWidth::W8 => 0xC6,
            _ => 0xC7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 0, dst);

        match width {
            OpWidth::W8 => self.code.emit_u8(imm as u8),
            OpWidth::W16 => self.code.emit_u16(imm as u16),
            OpWidth::W32 => self.code.emit_u32(imm as u32),
            OpWidth::W64 => self.code.emit_i32(imm as i32),
            OpWidth::W128 => {}
        }
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
                    self.emit_mov_ri_imm64(dst, imm);
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

    /// MOV r64, imm64 (always use imm64 encoding)
    pub fn emit_mov_ri_imm64(&mut self, dst: PhysReg, imm: i64) {
        self.emit_rex_w(dst);
        self.code.emit_u8(0xB8 + dst.low3());
        self.code.emit_u64(imm as u64);
    }

    /// MOV r64, [base + disp]
    pub fn emit_mov_rm(&mut self, dst: PhysReg, base: PhysReg, disp: i32, width: OpWidth) {
        self.emit_mov_rm_disp(dst, base, disp, DispSize::Auto, width);
    }

    pub fn emit_mov_rm_disp(
        &mut self,
        dst: PhysReg,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        width: OpWidth,
    ) {
        self.emit_rex_for_width(width, dst, base);

        let opcode = match width {
            OpWidth::W8 => 0x8A,
            _ => 0x8B,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_mem_disp(dst, base, disp, disp_size);
    }

    pub fn emit_mov_mi_disp(
        &mut self,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        imm: i64,
        width: OpWidth,
    ) {
        self.emit_rex_for_width_mem(width, base, None);
        let opcode = match width {
            OpWidth::W8 => 0xC6,
            _ => 0xC7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_mem_disp(PhysReg::Rax, base, disp, disp_size);
        self.emit_imm_by_width(imm, width);
    }

    /// MOV [base + disp], r64
    pub fn emit_mov_mr(&mut self, base: PhysReg, disp: i32, src: PhysReg, width: OpWidth) {
        self.emit_mov_mr_disp(base, disp, DispSize::Auto, src, width);
    }

    pub fn emit_mov_mr_disp(
        &mut self,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        src: PhysReg,
        width: OpWidth,
    ) {
        self.emit_rex_for_width(width, src, base);

        let opcode = match width {
            OpWidth::W8 => 0x88,
            _ => 0x89,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_mem_disp(src, base, disp, disp_size);
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

    pub fn emit_mov_mi_abs(&mut self, addr: u64, imm: i64, width: OpWidth) {
        self.emit_rex_for_width_mem(width, PhysReg::Rbp, None);
        let opcode = match width {
            OpWidth::W8 => 0xC6,
            _ => 0xC7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_abs(PhysReg::Rax, addr);
        self.emit_imm_by_width(imm, width);
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
        self.emit_mov_rm_sib_disp(dst, base, index, scale, disp, DispSize::Auto, width);
    }

    pub fn emit_mov_rm_sib_disp(
        &mut self,
        dst: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        width: OpWidth,
    ) {
        // REX prefix - use index for the rm extension bit since it's in the SIB
        let base_for_rex = base.unwrap_or(PhysReg::Rax);
        let w = width == OpWidth::W64;
        if width == OpWidth::W8
            && !dst.is_extended()
            && !base_for_rex.is_extended()
            && !index.is_extended()
            && matches!(
                dst,
                PhysReg::Rsp | PhysReg::Rbp | PhysReg::Rsi | PhysReg::Rdi
            )
        {
            self.emit_rex_force(false, dst, Some(index), base_for_rex);
        } else {
            self.emit_rex(w, dst, Some(index), base_for_rex);
        }

        let opcode = match width {
            OpWidth::W8 => 0x8A,
            _ => 0x8B,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_sib_disp(dst, base, index, scale, disp, disp_size);
    }

    pub fn emit_mov_mi_sib_disp(
        &mut self,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        imm: i64,
        width: OpWidth,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        self.emit_rex_for_width_mem(width, base_reg, Some(index));
        let opcode = match width {
            OpWidth::W8 => 0xC6,
            _ => 0xC7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_sib_disp(PhysReg::Rax, base, index, scale, disp, disp_size);
        self.emit_imm_by_width(imm, width);
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
        self.emit_mov_mr_sib_disp(base, index, scale, disp, DispSize::Auto, src, width);
    }

    pub fn emit_mov_mr_sib_disp(
        &mut self,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        src: PhysReg,
        width: OpWidth,
    ) {
        let base_for_rex = base.unwrap_or(PhysReg::Rax);
        let w = width == OpWidth::W64;
        if width == OpWidth::W8
            && !src.is_extended()
            && !base_for_rex.is_extended()
            && !index.is_extended()
            && matches!(
                src,
                PhysReg::Rsp | PhysReg::Rbp | PhysReg::Rsi | PhysReg::Rdi
            )
        {
            self.emit_rex_force(false, src, Some(index), base_for_rex);
        } else {
            self.emit_rex(w, src, Some(index), base_for_rex);
        }

        let opcode = match width {
            OpWidth::W8 => 0x88,
            _ => 0x89,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_sib_disp(src, base, index, scale, disp, disp_size);
    }

    /// MOV r64, [rip + disp32]
    pub fn emit_mov_rm_pcrel(&mut self, dst: PhysReg, disp: i32, width: OpWidth) -> usize {
        self.emit_rex_for_width(width, dst, PhysReg::Rbp);

        let opcode = match width {
            OpWidth::W8 => 0x8A,
            _ => 0x8B,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_pcrel(dst, disp)
    }

    pub fn emit_mov_mi_pcrel(&mut self, disp: i32, width: OpWidth, imm: i64) -> usize {
        self.emit_rex_for_width_mem(width, PhysReg::Rbp, None);
        let opcode = match width {
            OpWidth::W8 => 0xC6,
            _ => 0xC7,
        };
        self.code.emit_u8(opcode);
        let offset = self.emit_modrm_pcrel(PhysReg::Rax, disp);
        self.emit_imm_by_width(imm, width);
        offset
    }

    /// MOV [rip + disp32], r64
    pub fn emit_mov_mr_pcrel(&mut self, disp: i32, src: PhysReg, width: OpWidth) -> usize {
        self.emit_rex_for_width(width, src, PhysReg::Rbp);

        let opcode = match width {
            OpWidth::W8 => 0x88,
            _ => 0x89,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_pcrel(src, disp)
    }

    /// REP STOS (store AL/AX/EAX/RAX to [RDI])
    pub fn emit_rep_stos(&mut self, width: MemWidth) {
        self.code.emit_u8(0xF3); // REP prefix
        match width {
            MemWidth::B1 => {
                self.code.emit_u8(0xAA); // STOSB
            }
            MemWidth::B2 => {
                self.code.emit_u8(0x66); // Operand size override
                self.code.emit_u8(0xAB); // STOSW
            }
            MemWidth::B4 => {
                self.code.emit_u8(0xAB); // STOSD
            }
            MemWidth::B8 => {
                self.code.emit_u8(0x48); // REX.W
                self.code.emit_u8(0xAB); // STOSQ
            }
            MemWidth::B16 | MemWidth::B32 | MemWidth::B64 => {}
        }
    }

    /// REP MOVS (move [RSI] -> [RDI])
    pub fn emit_rep_movs(&mut self, width: MemWidth) {
        self.code.emit_u8(0xF3); // REP prefix
        match width {
            MemWidth::B1 => {
                self.code.emit_u8(0xA4); // MOVSB
            }
            MemWidth::B2 => {
                self.code.emit_u8(0x66); // Operand size override
                self.code.emit_u8(0xA5); // MOVSW
            }
            MemWidth::B4 => {
                self.code.emit_u8(0xA5); // MOVSD
            }
            MemWidth::B8 => {
                self.code.emit_u8(0x48); // REX.W
                self.code.emit_u8(0xA5); // MOVSQ
            }
            MemWidth::B16 | MemWidth::B32 | MemWidth::B64 => {}
        }
    }

    /// Emit a scalar x86 string instruction in canonical prefix order.
    pub fn emit_x86_string(
        &mut self,
        kind: X86StringKind,
        rep: X86RepMode,
        width: MemWidth,
        address_width: OpWidth,
    ) -> Result<(), LowerError> {
        match address_width {
            OpWidth::W32 => self.code.emit_u8(0x67),
            OpWidth::W64 => {}
            _ => {
                return Err(LowerError::InvalidOperand {
                    op: "X86String".to_string(),
                    operand: format!("unsupported address width {address_width:?}"),
                });
            }
        }
        match rep {
            X86RepMode::None => {}
            X86RepMode::Rep | X86RepMode::Repe => self.code.emit_u8(0xF3),
            X86RepMode::Repne => self.code.emit_u8(0xF2),
        }

        let byte_form = width == MemWidth::B1;
        if !byte_form {
            match width {
                MemWidth::B2 => self.code.emit_u8(0x66),
                MemWidth::B4 => {}
                MemWidth::B8 => self.code.emit_u8(0x48),
                MemWidth::B1 => unreachable!(),
                MemWidth::B16 | MemWidth::B32 | MemWidth::B64 => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("X86String width {width:?}"),
                    });
                }
            }
        }

        let opcode = match (kind, byte_form) {
            (X86StringKind::Movs, true) => 0xA4,
            (X86StringKind::Movs, false) => 0xA5,
            (X86StringKind::Cmps, true) => 0xA6,
            (X86StringKind::Cmps, false) => 0xA7,
            (X86StringKind::Stos, true) => 0xAA,
            (X86StringKind::Stos, false) => 0xAB,
            (X86StringKind::Lods, true) => 0xAC,
            (X86StringKind::Lods, false) => 0xAD,
            (X86StringKind::Scas, true) => 0xAE,
            (X86StringKind::Scas, false) => 0xAF,
        };
        self.code.emit_u8(opcode);
        Ok(())
    }

    /// MOVZX r64, r/m8 or r/m16
    pub fn emit_movzx(
        &mut self,
        dst: PhysReg,
        src: PhysReg,
        src_width: OpWidth,
        dst_width: OpWidth,
    ) {
        self.emit_rex_ext(dst_width, src_width, dst, src);

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
        self.emit_rex_ext(dst_width, src_width, dst, src);

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

    pub fn emit_movzx_rm_disp(
        &mut self,
        dst: PhysReg,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        src_width: OpWidth,
        dst_width: OpWidth,
    ) {
        if src_width == OpWidth::W32 {
            self.emit_rex_for_width_mem_reg(OpWidth::W32, dst, base, None);
            self.code.emit_u8(0x8B);
            self.emit_modrm_mem_disp(dst, base, disp, disp_size);
            return;
        }

        self.emit_rex_for_width_mem_reg(dst_width, dst, base, None);
        match src_width {
            OpWidth::W8 => {
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xB6);
            }
            OpWidth::W16 => {
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xB7);
            }
            _ => {}
        }
        self.emit_modrm_mem_disp(dst, base, disp, disp_size);
    }

    pub fn emit_movzx_rm_sib_disp(
        &mut self,
        dst: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        src_width: OpWidth,
        dst_width: OpWidth,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        if src_width == OpWidth::W32 {
            self.emit_rex_for_width_mem_reg(OpWidth::W32, dst, base_reg, Some(index));
            self.code.emit_u8(0x8B);
            self.emit_modrm_sib_disp(dst, base, index, scale, disp, disp_size);
            return;
        }

        self.emit_rex_for_width_mem_reg(dst_width, dst, base_reg, Some(index));
        match src_width {
            OpWidth::W8 => {
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xB6);
            }
            OpWidth::W16 => {
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xB7);
            }
            _ => {}
        }
        self.emit_modrm_sib_disp(dst, base, index, scale, disp, disp_size);
    }

    pub fn emit_movzx_rm_abs(
        &mut self,
        dst: PhysReg,
        addr: u64,
        src_width: OpWidth,
        dst_width: OpWidth,
    ) {
        if src_width == OpWidth::W32 {
            self.emit_rex_for_width(OpWidth::W32, dst, PhysReg::Rax);
            self.code.emit_u8(0x8B);
            self.emit_modrm_abs(dst, addr);
            return;
        }

        self.emit_rex_for_width(dst_width, dst, PhysReg::Rax);
        match src_width {
            OpWidth::W8 => {
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xB6);
            }
            OpWidth::W16 => {
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xB7);
            }
            _ => {}
        }
        self.emit_modrm_abs(dst, addr);
    }

    pub fn emit_movzx_rm_pcrel(
        &mut self,
        dst: PhysReg,
        disp: i32,
        src_width: OpWidth,
        dst_width: OpWidth,
    ) -> usize {
        if src_width == OpWidth::W32 {
            self.emit_rex_for_width(OpWidth::W32, dst, PhysReg::Rbp);
            self.code.emit_u8(0x8B);
            return self.emit_modrm_pcrel(dst, disp);
        }

        self.emit_rex_for_width(dst_width, dst, PhysReg::Rbp);
        match src_width {
            OpWidth::W8 => {
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xB6);
            }
            OpWidth::W16 => {
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0xB7);
            }
            _ => {}
        }
        self.emit_modrm_pcrel(dst, disp)
    }

    pub fn emit_movsx_rm_disp(
        &mut self,
        dst: PhysReg,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        src_width: OpWidth,
        dst_width: OpWidth,
    ) {
        self.emit_rex_for_width_mem_reg(dst_width, dst, base, None);
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
                self.code.emit_u8(0x63);
            }
            _ => {}
        }
        self.emit_modrm_mem_disp(dst, base, disp, disp_size);
    }

    pub fn emit_movsx_rm_sib_disp(
        &mut self,
        dst: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        src_width: OpWidth,
        dst_width: OpWidth,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        self.emit_rex_for_width_mem_reg(dst_width, dst, base_reg, Some(index));
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
                self.code.emit_u8(0x63);
            }
            _ => {}
        }
        self.emit_modrm_sib_disp(dst, base, index, scale, disp, disp_size);
    }

    pub fn emit_movsx_rm_abs(
        &mut self,
        dst: PhysReg,
        addr: u64,
        src_width: OpWidth,
        dst_width: OpWidth,
    ) {
        self.emit_rex_for_width(dst_width, dst, PhysReg::Rax);
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
                self.code.emit_u8(0x63);
            }
            _ => {}
        }
        self.emit_modrm_abs(dst, addr);
    }

    pub fn emit_movsx_rm_pcrel(
        &mut self,
        dst: PhysReg,
        disp: i32,
        src_width: OpWidth,
        dst_width: OpWidth,
    ) -> usize {
        self.emit_rex_for_width(dst_width, dst, PhysReg::Rbp);
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
                self.code.emit_u8(0x63);
            }
            _ => {}
        }
        self.emit_modrm_pcrel(dst, disp)
    }

    pub fn emit_sse_mov_rr(&mut self, prefix: Option<u8>, opcode: u8, reg: PhysReg, rm: PhysReg) {
        if let Some(prefix) = prefix {
            self.code.emit_u8(prefix);
        }
        self.emit_rex_for_xmm(reg, rm);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(reg, rm);
    }

    pub fn emit_mmx_rr(&mut self, opcode: u8, reg: PhysReg, rm: PhysReg) {
        debug_assert!(reg.is_mmx() && rm.is_mmx());
        self.code.emit_u8(0x0F);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(reg, rm);
    }

    pub fn emit_mmx_0f38_rr(&mut self, opcode: u8, reg: PhysReg, rm: PhysReg) {
        debug_assert!(reg.is_mmx() && rm.is_mmx());
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x38);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(reg, rm);
    }

    pub fn emit_mmx_0f3a_rr_imm(&mut self, opcode: u8, reg: PhysReg, rm: PhysReg, imm: u8) {
        debug_assert!(reg.is_mmx() && rm.is_mmx());
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x3A);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(reg, rm);
        self.code.emit_u8(imm);
    }

    pub fn emit_mmx_rr_imm(&mut self, opcode: u8, reg: PhysReg, rm: PhysReg, imm: u8) {
        debug_assert!(reg.is_mmx() && rm.is_mmx());
        self.code.emit_u8(0x0F);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(reg, rm);
        self.code.emit_u8(imm);
    }

    /// Emit prefix-free MMX PINSRW/PEXTRW register forms. PINSRW encodes the
    /// MM register in ModR/M.reg and extends the GPR source with REX.B;
    /// PEXTRW reverses the register classes and extends its GPR destination
    /// with REX.R. REX never extends the three-bit MM register file.
    pub fn emit_mmx_word_lane_rr_imm(&mut self, opcode: u8, mm: PhysReg, gpr: PhysReg, imm: u8) {
        debug_assert!(
            matches!(opcode, 0xC4 | 0xC5) && mm.is_mmx() && !gpr.is_mmx() && !gpr.is_vec()
        );
        let (reg, rm) = if opcode == 0xC4 { (mm, gpr) } else { (gpr, mm) };
        if gpr.is_extended() {
            self.emit_rex(false, reg, None, rm);
        }
        self.code.emit_u8(0x0F);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(reg, rm);
        self.code.emit_u8(imm);
    }

    pub fn emit_mmx_shift_imm(&mut self, opcode: u8, digit: u8, rm: PhysReg, imm: u8) {
        debug_assert!(rm.is_mmx() && digit < 8);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, digit, rm);
        self.code.emit_u8(imm);
    }

    pub fn emit_sse_fp_to_int_rr(
        &mut self,
        prefix: u8,
        opcode: u8,
        dst: PhysReg,
        src: PhysReg,
        width: OpWidth,
    ) {
        self.code.emit_u8(prefix);
        self.emit_rex(width == OpWidth::W64, dst, None, src);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(dst, src);
    }

    pub fn emit_sse_mov_rm_disp(
        &mut self,
        prefix: Option<u8>,
        opcode: u8,
        reg: PhysReg,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
    ) {
        if let Some(prefix) = prefix {
            self.code.emit_u8(prefix);
        }
        self.emit_rex_for_xmm_mem(reg, base, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(opcode);
        self.emit_modrm_mem_disp(reg, base, disp, disp_size);
    }

    pub fn emit_sse_mov_rm_sib_disp(
        &mut self,
        prefix: Option<u8>,
        opcode: u8,
        reg: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        if let Some(prefix) = prefix {
            self.code.emit_u8(prefix);
        }
        self.emit_rex_for_xmm_mem(reg, base_reg, Some(index));
        self.code.emit_u8(0x0F);
        self.code.emit_u8(opcode);
        self.emit_modrm_sib_disp(reg, base, index, scale, disp, disp_size);
    }

    pub fn emit_sse_mov_rm_abs(&mut self, prefix: Option<u8>, opcode: u8, reg: PhysReg, addr: u64) {
        if let Some(prefix) = prefix {
            self.code.emit_u8(prefix);
        }
        self.emit_rex_for_xmm(reg, PhysReg::Rax);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(opcode);
        self.emit_modrm_abs(reg, addr);
    }

    pub fn emit_sse_mov_rm_pcrel(
        &mut self,
        prefix: Option<u8>,
        opcode: u8,
        reg: PhysReg,
        disp: i32,
    ) -> usize {
        if let Some(prefix) = prefix {
            self.code.emit_u8(prefix);
        }
        self.emit_rex_for_xmm(reg, PhysReg::Rbp);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(opcode);
        self.emit_modrm_pcrel(reg, disp)
    }

    pub fn emit_sse_op38_rr(&mut self, prefix: Option<u8>, opcode: u8, reg: PhysReg, rm: PhysReg) {
        if let Some(prefix) = prefix {
            self.code.emit_u8(prefix);
        }
        self.emit_rex_for_xmm(reg, rm);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x38);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(reg, rm);
    }

    /// Emit a legacy 0F-map MOVMSK/PMOVMSKB register form. The ModR/M.reg
    /// operand is a GPR while ModR/M.rm is XMM, but both extension bits use the
    /// ordinary REX layout. `w` preserves the decoded legacy destination width.
    pub(crate) fn emit_sse_mov_mask_rr(
        &mut self,
        prefix: Option<u8>,
        opcode: u8,
        reg: PhysReg,
        rm: PhysReg,
        w: bool,
    ) {
        if let Some(prefix) = prefix {
            self.code.emit_u8(prefix);
        }
        if w || reg.is_extended() || rm.is_extended() {
            self.emit_rex(w, reg, None, rm);
        }
        self.code.emit_u8(0x0F);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(reg, rm);
    }

    /// Emit a legacy MOVD/MOVQ register transfer. ModR/M.reg always names the
    /// XMM operand and ModR/M.rm always names the GPR operand, independently
    /// of the transfer direction selected by opcode 6E or 7E.
    pub(crate) fn emit_sse_movd_q_rr(
        &mut self,
        opcode: u8,
        xmm: PhysReg,
        gpr: PhysReg,
        width: OpWidth,
    ) {
        self.code.emit_u8(0x66);
        let w = width == OpWidth::W64;
        if w || xmm.is_extended() || gpr.is_extended() {
            self.emit_rex(w, xmm, None, gpr);
        }
        self.code.emit_u8(0x0F);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(xmm, gpr);
    }

    /// Emit a prefix-free legacy MMX MOVD/MOVQ register transfer. ModR/M.reg
    /// names the three-bit MM register and ModR/M.rm names the GPR; REX.R is
    /// therefore never used, while REX.B and REX.W select the GPR and width.
    pub(crate) fn emit_mmx_movd_q_rr(
        &mut self,
        opcode: u8,
        mm: PhysReg,
        gpr: PhysReg,
        width: OpWidth,
    ) {
        let w = width == OpWidth::W64;
        if w || gpr.is_extended() {
            self.emit_rex(w, mm, None, gpr);
        }
        self.code.emit_u8(0x0F);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(mm, gpr);
    }

    pub fn emit_sse_op3a_rr_imm(
        &mut self,
        prefix: Option<u8>,
        opcode: u8,
        reg: PhysReg,
        rm: PhysReg,
        imm: u8,
    ) {
        if let Some(prefix) = prefix {
            self.code.emit_u8(prefix);
        }
        self.emit_rex_for_xmm(reg, rm);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x3A);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(reg, rm);
        self.code.emit_u8(imm);
    }

    pub fn emit_sse_op38_rm_disp(
        &mut self,
        prefix: Option<u8>,
        opcode: u8,
        reg: PhysReg,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
    ) {
        if let Some(prefix) = prefix {
            self.code.emit_u8(prefix);
        }
        self.emit_rex_for_xmm_mem(reg, base, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x38);
        self.code.emit_u8(opcode);
        self.emit_modrm_mem_disp(reg, base, disp, disp_size);
    }

    pub fn emit_sse_op38_rm_sib_disp(
        &mut self,
        prefix: Option<u8>,
        opcode: u8,
        reg: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        if let Some(prefix) = prefix {
            self.code.emit_u8(prefix);
        }
        self.emit_rex_for_xmm_mem(reg, base_reg, Some(index));
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x38);
        self.code.emit_u8(opcode);
        self.emit_modrm_sib_disp(reg, base, index, scale, disp, disp_size);
    }

    pub fn emit_sse_op38_rm_abs(
        &mut self,
        prefix: Option<u8>,
        opcode: u8,
        reg: PhysReg,
        addr: u64,
    ) {
        if let Some(prefix) = prefix {
            self.code.emit_u8(prefix);
        }
        self.emit_rex_for_xmm(reg, PhysReg::Rax);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x38);
        self.code.emit_u8(opcode);
        self.emit_modrm_abs(reg, addr);
    }

    pub fn emit_sse_op38_rm_pcrel(
        &mut self,
        prefix: Option<u8>,
        opcode: u8,
        reg: PhysReg,
        disp: i32,
    ) -> usize {
        if let Some(prefix) = prefix {
            self.code.emit_u8(prefix);
        }
        self.emit_rex_for_xmm(reg, PhysReg::Rbp);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x38);
        self.code.emit_u8(opcode);
        self.emit_modrm_pcrel(reg, disp)
    }

    pub(crate) fn vex_pp_bits(pp: X86SsePrefix) -> u8 {
        match pp {
            X86SsePrefix::None => 0,
            X86SsePrefix::OpSize => 1,
            X86SsePrefix::Rep => 2,
            X86SsePrefix::Repne => 3,
        }
    }

    pub(crate) fn vex_map_bits(map: X86VecMap) -> u8 {
        match map {
            X86VecMap::Map0F => 0x01,
            X86VecMap::Map0F38 => 0x02,
            X86VecMap::Map0F3A => 0x03,
            X86VecMap::Map5 => 0x05,
            X86VecMap::Map6 => 0x06,
        }
    }

    pub(crate) fn emit_vex_prefix(
        &mut self,
        map: X86VecMap,
        pp: X86SsePrefix,
        width: VecWidth,
        w: bool,
        r: u8,
        x: u8,
        b: u8,
        vvvv: u8,
    ) {
        let l_bit = match width {
            VecWidth::V256 => 1,
            _ => 0,
        };
        let pp_bits = Self::vex_pp_bits(pp);
        let vvvv_inv = (!vvvv) & 0x0F;
        let r_inv = if r != 0 { 0 } else { 1 };
        let x_inv = if x != 0 { 0 } else { 1 };
        let b_inv = if b != 0 { 0 } else { 1 };

        if map == X86VecMap::Map0F && !w && x == 0 && b == 0 {
            self.code.emit_u8(0xC5);
            let byte2 = (r_inv << 7) | (vvvv_inv << 3) | (l_bit << 2) | pp_bits;
            self.code.emit_u8(byte2);
        } else {
            self.code.emit_u8(0xC4);
            let map_bits = Self::vex_map_bits(map) & 0x1F;
            let byte2 = (r_inv << 7) | (x_inv << 6) | (b_inv << 5) | map_bits;
            let byte3 = ((w as u8) << 7) | (vvvv_inv << 3) | (l_bit << 2) | pp_bits;
            self.code.emit_u8(byte2);
            self.code.emit_u8(byte3);
        }
    }

    pub(crate) fn emit_evex_prefix(
        &mut self,
        map: X86VecMap,
        pp: X86SsePrefix,
        width: VecWidth,
        w: bool,
        r: u8,
        x: u8,
        b: u8,
        r2: u8,
        x2: u8,
        b2: u8,
        vvvv: u8,
    ) {
        let pp_bits = Self::vex_pp_bits(pp);
        let vvvv_low = vvvv & 0x0F;
        let vvvv_high = (vvvv >> 4) & 0x01;
        let vvvv_inv = (!vvvv_low) & 0x0F;
        let vprime_inv = if vvvv_high != 0 { 0 } else { 1 };
        let r_inv = if r != 0 { 0 } else { 1 };
        // EVEX.P0 bit 6 extends an address index, or the ModR/M.rm vector
        // register in register-direct encodings. Callers provide those as `x`
        // and `b2`, respectively.
        let x_or_b2_inv = if x != 0 || b2 != 0 { 0 } else { 1 };
        let b_inv = if b != 0 { 0 } else { 1 };
        let r2_inv = if r2 != 0 { 0 } else { 1 };
        let _ = x2;

        let l_bits = match width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            VecWidth::V64 => 0,
        };

        self.code.emit_u8(0x62);
        let map_bits = Self::vex_map_bits(map) & 0x0F;
        let byte2 = (r_inv << 7) | (x_or_b2_inv << 6) | (b_inv << 5) | (r2_inv << 4) | map_bits;
        let byte3 = ((w as u8) << 7) | (vvvv_inv << 3) | 0x04 | pp_bits;
        let byte4 = (l_bits << 5) | (vprime_inv << 3);
        self.code.emit_u8(byte2);
        self.code.emit_u8(byte3);
        self.code.emit_u8(byte4);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_evex_masked_rr(
        &mut self,
        map: X86VecMap,
        pp: X86SsePrefix,
        width: VecWidth,
        w: bool,
        opcode: u8,
        reg: PhysReg,
        rm: PhysReg,
        aaa: u8,
        zeroing: bool,
        broadcast_or_round: bool,
        round: FpRoundMode,
    ) {
        let r = reg.vec_ext();
        let r2 = reg.vec_ext2();
        let b = rm.vec_ext();
        let b2 = rm.vec_ext2();
        let r_inv = u8::from(r == 0);
        let r2_inv = u8::from(r2 == 0);
        let b_inv = u8::from(b == 0);
        let b2_inv = u8::from(b2 == 0);
        let map_bits = Self::vex_map_bits(map) & 0x0F;
        let pp_bits = Self::vex_pp_bits(pp);
        let byte2 = (r_inv << 7) | (b2_inv << 6) | (b_inv << 5) | (r2_inv << 4) | map_bits;
        let byte3 = ((w as u8) << 7) | (0x0F << 3) | 0x04 | pp_bits;
        let ll_or_rc = if broadcast_or_round && round != FpRoundMode::Dynamic {
            match round {
                FpRoundMode::RoundNearest => 0,
                FpRoundMode::RoundDown => 1,
                FpRoundMode::RoundUp => 2,
                FpRoundMode::RoundTowardZero => 3,
                _ => 0,
            }
        } else {
            match width {
                VecWidth::V128 | VecWidth::V64 => 0,
                VecWidth::V256 => 1,
                VecWidth::V512 => 2,
            }
        };
        let byte4 = ((zeroing as u8) << 7)
            | (ll_or_rc << 5)
            | ((broadcast_or_round as u8) << 4)
            | (1 << 3)
            | (aaa & 7);
        self.code.emit_u8(0x62);
        self.code.emit_u8(byte2);
        self.code.emit_u8(byte3);
        self.code.emit_u8(byte4);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(reg, rm);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_evex_unary_fp_rr(
        &mut self,
        map: X86VecMap,
        pp: X86SsePrefix,
        width: VecWidth,
        w: bool,
        opcode: u8,
        dst: PhysReg,
        merge: Option<PhysReg>,
        src: PhysReg,
        aaa: u8,
        zeroing: bool,
        suppress_exceptions: bool,
        imm: Option<u8>,
    ) {
        let r_inv = u8::from(dst.vec_ext() == 0);
        let r2_inv = u8::from(dst.vec_ext2() == 0);
        let b_inv = u8::from(src.vec_ext() == 0);
        let b2_inv = u8::from(src.vec_ext2() == 0);
        let vvvv = merge.map_or(0, |reg| reg.encoding() & 0x1F);
        let vvvv_inv = (!vvvv) & 0x0F;
        let vprime_inv = u8::from(vvvv & 0x10 == 0);
        let byte2 = (r_inv << 7)
            | (b2_inv << 6)
            | (b_inv << 5)
            | (r2_inv << 4)
            | (Self::vex_map_bits(map) & 0x0F);
        let byte3 = ((w as u8) << 7) | (vvvv_inv << 3) | 0x04 | Self::vex_pp_bits(pp);
        // VGETEXP/VGETMANT/VRNDSCALE use SAE without embedded rounding. With
        // EVEX.b set, EVEX.L'L is ignored and the canonical encoding is 00b.
        let ll = if suppress_exceptions {
            0
        } else {
            match width {
                VecWidth::V128 | VecWidth::V64 => 0,
                VecWidth::V256 => 1,
                VecWidth::V512 => 2,
            }
        };
        let byte4 = ((zeroing as u8) << 7)
            | (ll << 5)
            | ((suppress_exceptions as u8) << 4)
            | (vprime_inv << 3)
            | (aaa & 7);
        self.code.emit_u8(0x62);
        self.code.emit_u8(byte2);
        self.code.emit_u8(byte3);
        self.code.emit_u8(byte4);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(dst, src);
        if let Some(imm) = imm {
            self.code.emit_u8(imm);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_evex_fp_rrr(
        &mut self,
        map: X86VecMap,
        pp: X86SsePrefix,
        width: VecWidth,
        w: bool,
        opcode: u8,
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
        aaa: u8,
        zeroing: bool,
        round: FpRoundMode,
        suppress_exceptions: bool,
    ) {
        let r_inv = u8::from(dst.vec_ext() == 0);
        let r2_inv = u8::from(dst.vec_ext2() == 0);
        let b_inv = u8::from(src2.vec_ext() == 0);
        let b2_inv = u8::from(src2.vec_ext2() == 0);
        let vvvv = src1.encoding() & 0x1F;
        let vvvv_inv = (!vvvv) & 0x0F;
        let vprime_inv = u8::from(vvvv & 0x10 == 0);
        let byte2 = (r_inv << 7)
            | (b2_inv << 6)
            | (b_inv << 5)
            | (r2_inv << 4)
            | (Self::vex_map_bits(map) & 0x0F);
        let byte3 = ((w as u8) << 7) | (vvvv_inv << 3) | 0x04 | Self::vex_pp_bits(pp);
        let ll_or_rc = if suppress_exceptions {
            match round {
                FpRoundMode::RoundNearest => 0,
                FpRoundMode::RoundDown => 1,
                FpRoundMode::RoundUp => 2,
                FpRoundMode::RoundTowardZero => 3,
                _ => unreachable!("validated VSCALEF embedded rounding mode"),
            }
        } else {
            match width {
                VecWidth::V128 | VecWidth::V64 => 0,
                VecWidth::V256 => 1,
                VecWidth::V512 => 2,
            }
        };
        let byte4 = ((zeroing as u8) << 7)
            | (ll_or_rc << 5)
            | ((suppress_exceptions as u8) << 4)
            | (vprime_inv << 3)
            | (aaa & 7);
        self.code.emit_u8(0x62);
        self.code.emit_u8(byte2);
        self.code.emit_u8(byte3);
        self.code.emit_u8(byte4);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(dst, src2);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_evex_fp_rrr_imm_sae(
        &mut self,
        map: X86VecMap,
        pp: X86SsePrefix,
        width: VecWidth,
        w: bool,
        opcode: u8,
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
        aaa: u8,
        zeroing: bool,
        suppress_exceptions: bool,
        imm: u8,
    ) {
        // VRANGE and VFIXUPIMM use EVEX.b as SAE without embedded rounding
        // control; the architectural encoding requires L'L=00 with SAE.
        self.emit_evex_fp_rrr(
            map,
            pp,
            width,
            w,
            opcode,
            dst,
            src1,
            src2,
            aaa,
            zeroing,
            FpRoundMode::RoundNearest,
            suppress_exceptions,
        );
        self.code.emit_u8(imm);
    }

    pub fn emit_vex_rrr(
        &mut self,
        map: X86VecMap,
        pp: X86SsePrefix,
        width: VecWidth,
        w: bool,
        opcode: u8,
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    ) {
        let r = dst.vec_ext();
        let b = src2.vec_ext();
        let vvvv = src1.encoding() & 0x1F;
        self.emit_vex_prefix(map, pp, width, w, r, 0, b, vvvv);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(dst, src2);
    }

    pub fn emit_evex_rrr(
        &mut self,
        map: X86VecMap,
        pp: X86SsePrefix,
        width: VecWidth,
        w: bool,
        opcode: u8,
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    ) {
        let r = dst.vec_ext();
        let r2 = dst.vec_ext2();
        let b = src2.vec_ext();
        let b2 = src2.vec_ext2();
        let vvvv = src1.encoding() & 0x1F;
        self.emit_evex_prefix(map, pp, width, w, r, 0, b, r2, 0, b2, vvvv);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(dst, src2);
    }

    // ========================================================================
    // ALU Instructions (two-operand)
    // ========================================================================

    /// Generic ALU r/m, r instruction
    pub(crate) fn emit_alu_rr(&mut self, opcode: u8, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, src, dst);

        let op = match width {
            OpWidth::W8 => opcode,
            _ => opcode + 1,
        };
        self.code.emit_u8(op);
        self.emit_modrm_rr(src, dst);
    }

    pub(crate) fn emit_alu_rr_dir(
        &mut self,
        opcode: u8,
        dst: PhysReg,
        src: PhysReg,
        width: OpWidth,
        encoding: X86AluEncoding,
    ) {
        if encoding == X86AluEncoding::RegRm {
            self.emit_rex_for_width(width, dst, src);
            let op = match width {
                OpWidth::W8 => opcode + 2,
                _ => opcode + 3,
            };
            self.code.emit_u8(op);
            self.emit_modrm_rr(dst, src);
        } else {
            self.emit_alu_rr(opcode, dst, src, width);
        }
    }

    pub(crate) fn alu_op_byte(opcode: u8, width: OpWidth, encoding: X86AluEncoding) -> u8 {
        match width {
            OpWidth::W8 => match encoding {
                X86AluEncoding::RegRm => opcode + 2,
                _ => opcode,
            },
            _ => match encoding {
                X86AluEncoding::RegRm => opcode + 3,
                _ => opcode + 1,
            },
        }
    }

    pub(crate) fn emit_alu_mem_disp(
        &mut self,
        opcode: u8,
        reg: PhysReg,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        width: OpWidth,
        encoding: X86AluEncoding,
    ) {
        self.emit_rex_for_width_mem_reg(width, reg, base, None);
        let op = Self::alu_op_byte(opcode, width, encoding);
        self.code.emit_u8(op);
        self.emit_modrm_mem_disp(reg, base, disp, disp_size);
    }

    pub(crate) fn emit_alu_mem_sib_disp(
        &mut self,
        opcode: u8,
        reg: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        width: OpWidth,
        encoding: X86AluEncoding,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        self.emit_rex_for_width_mem_reg(width, reg, base_reg, Some(index));
        let op = Self::alu_op_byte(opcode, width, encoding);
        self.code.emit_u8(op);
        self.emit_modrm_sib_disp(reg, base, index, scale, disp, disp_size);
    }

    pub(crate) fn emit_alu_mem_abs(
        &mut self,
        opcode: u8,
        reg: PhysReg,
        addr: u64,
        width: OpWidth,
        encoding: X86AluEncoding,
    ) {
        self.emit_rex_for_width_mem_reg(width, reg, PhysReg::Rbp, None);
        let op = Self::alu_op_byte(opcode, width, encoding);
        self.code.emit_u8(op);
        self.emit_modrm_abs(reg, addr);
    }

    pub(crate) fn emit_alu_mem_pcrel(
        &mut self,
        opcode: u8,
        reg: PhysReg,
        disp: i32,
        width: OpWidth,
        encoding: X86AluEncoding,
    ) -> usize {
        self.emit_rex_for_width_mem_reg(width, reg, PhysReg::Rbp, None);
        let op = Self::alu_op_byte(opcode, width, encoding);
        self.code.emit_u8(op);
        self.emit_modrm_pcrel(reg, disp)
    }

    /// Generic ALU r/m, imm instruction
    pub(crate) fn emit_alu_ri(&mut self, digit: u8, dst: PhysReg, imm: i64, width: OpWidth) {
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
                    // Immediate width follows the operand: 16-bit op (0x66
                    // prefix) takes imm16, else imm32 (sign-extended for 64-bit).
                    // Emitting imm32 for a W16 op left 2 stray bytes that the CPU
                    // decoded as a separate instruction (`add [rax],al`).
                    if width == OpWidth::W16 {
                        self.code.emit_u16(imm as u16);
                    } else {
                        self.code.emit_i32(imm as i32);
                    }
                }
            }
        }
    }

    pub(crate) fn digit_reg(digit: u8) -> PhysReg {
        match digit & 0x7 {
            0 => PhysReg::Rax,
            1 => PhysReg::Rcx,
            2 => PhysReg::Rdx,
            3 => PhysReg::Rbx,
            4 => PhysReg::Rsp,
            5 => PhysReg::Rbp,
            6 => PhysReg::Rsi,
            _ => PhysReg::Rdi,
        }
    }

    pub(crate) fn emit_alu_mi_disp(
        &mut self,
        digit: u8,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        imm: i64,
        width: OpWidth,
    ) {
        self.emit_rex_for_width_mem(width, base, None);
        let reg = Self::digit_reg(digit);
        let use_imm8 = width != OpWidth::W8 && imm >= -128 && imm <= 127;
        let opcode = if width == OpWidth::W8 {
            0x80
        } else if use_imm8 {
            0x83
        } else {
            0x81
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_mem_disp(reg, base, disp, disp_size);
        if width == OpWidth::W8 || use_imm8 {
            self.code.emit_i8(imm as i8);
        } else if width == OpWidth::W16 {
            self.code.emit_u16(imm as u16);
        } else {
            self.code.emit_i32(imm as i32);
        }
    }

    pub(crate) fn emit_alu_mi_sib_disp(
        &mut self,
        digit: u8,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        imm: i64,
        width: OpWidth,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        self.emit_rex_for_width_mem(width, base_reg, Some(index));
        let reg = Self::digit_reg(digit);
        let use_imm8 = width != OpWidth::W8 && imm >= -128 && imm <= 127;
        let opcode = if width == OpWidth::W8 {
            0x80
        } else if use_imm8 {
            0x83
        } else {
            0x81
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_sib_disp(reg, base, index, scale, disp, disp_size);
        if width == OpWidth::W8 || use_imm8 {
            self.code.emit_i8(imm as i8);
        } else if width == OpWidth::W16 {
            self.code.emit_u16(imm as u16);
        } else {
            self.code.emit_i32(imm as i32);
        }
    }

    pub(crate) fn emit_alu_mi_abs(&mut self, digit: u8, addr: u64, imm: i64, width: OpWidth) {
        self.emit_rex_for_width_mem(width, PhysReg::Rbp, None);
        let reg = Self::digit_reg(digit);
        let use_imm8 = width != OpWidth::W8 && imm >= -128 && imm <= 127;
        let opcode = if width == OpWidth::W8 {
            0x80
        } else if use_imm8 {
            0x83
        } else {
            0x81
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_abs(reg, addr);
        if width == OpWidth::W8 || use_imm8 {
            self.code.emit_i8(imm as i8);
        } else if width == OpWidth::W16 {
            self.code.emit_u16(imm as u16);
        } else {
            self.code.emit_i32(imm as i32);
        }
    }

    pub(crate) fn emit_alu_mi_pcrel(
        &mut self,
        digit: u8,
        disp: i32,
        imm: i64,
        width: OpWidth,
    ) -> usize {
        self.emit_rex_for_width_mem(width, PhysReg::Rbp, None);
        let reg = Self::digit_reg(digit);
        let use_imm8 = width != OpWidth::W8 && imm >= -128 && imm <= 127;
        let opcode = if width == OpWidth::W8 {
            0x80
        } else if use_imm8 {
            0x83
        } else {
            0x81
        };
        self.code.emit_u8(opcode);
        let offset = self.emit_modrm_pcrel(reg, disp);
        if width == OpWidth::W8 || use_imm8 {
            self.code.emit_i8(imm as i8);
        } else if width == OpWidth::W16 {
            self.code.emit_u16(imm as u16);
        } else {
            self.code.emit_i32(imm as i32);
        }
        offset
    }

    pub(crate) fn emit_alu_acc_imm(&mut self, opcode: u8, imm: i64, width: OpWidth) {
        match width {
            OpWidth::W8 => {
                self.code.emit_u8(opcode);
                self.code.emit_u8(imm as u8);
            }
            OpWidth::W16 => {
                self.code.emit_u8(0x66);
                self.code.emit_u8(opcode + 1);
                self.code.emit_u16(imm as u16);
            }
            OpWidth::W32 => {
                self.code.emit_u8(opcode + 1);
                self.code.emit_u32(imm as u32);
            }
            OpWidth::W64 => {
                self.emit_rex_w(PhysReg::Rax);
                self.code.emit_u8(opcode + 1);
                self.code.emit_i32(imm as i32);
            }
            OpWidth::W128 => {}
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
        self.code
            .emit_u8(if width == OpWidth::W8 { 0xF6 } else { 0xF7 });
        self.emit_modrm_digit(0b11, 0, op1);
        // TEST follows the operand width through W32; W64 alone uses the
        // architectural sign-extended imm32. Emitting four bytes for W16 made
        // the trailing 00 00 decode as `add byte ptr [rax],al`.
        self.emit_imm_by_width(imm, width);
    }

    pub fn emit_test_mr_disp(
        &mut self,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        reg: PhysReg,
        width: OpWidth,
    ) {
        self.emit_rex_for_width_mem_reg(width, reg, base, None);
        let opcode = match width {
            OpWidth::W8 => 0x84,
            _ => 0x85,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_mem_disp(reg, base, disp, disp_size);
    }

    pub fn emit_test_mr_sib_disp(
        &mut self,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        reg: PhysReg,
        width: OpWidth,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        self.emit_rex_for_width_mem_reg(width, reg, base_reg, Some(index));
        let opcode = match width {
            OpWidth::W8 => 0x84,
            _ => 0x85,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_sib_disp(reg, base, index, scale, disp, disp_size);
    }

    pub fn emit_test_mr_abs(&mut self, addr: u64, reg: PhysReg, width: OpWidth) {
        self.emit_rex_for_width_mem_reg(width, reg, PhysReg::Rbp, None);
        let opcode = match width {
            OpWidth::W8 => 0x84,
            _ => 0x85,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_abs(reg, addr);
    }

    pub fn emit_test_mr_pcrel(&mut self, disp: i32, reg: PhysReg, width: OpWidth) -> usize {
        self.emit_rex_for_width_mem_reg(width, reg, PhysReg::Rbp, None);
        let opcode = match width {
            OpWidth::W8 => 0x84,
            _ => 0x85,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_pcrel(reg, disp)
    }

    pub fn emit_test_mi_disp(
        &mut self,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        imm: i64,
        width: OpWidth,
    ) {
        self.emit_rex_for_width_mem(width, base, None);
        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_mem_disp(PhysReg::Rax, base, disp, disp_size);
        self.emit_imm_by_width(imm, width);
    }

    pub fn emit_test_mi_sib_disp(
        &mut self,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        imm: i64,
        width: OpWidth,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        self.emit_rex_for_width_mem(width, base_reg, Some(index));
        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_sib_disp(PhysReg::Rax, base, index, scale, disp, disp_size);
        self.emit_imm_by_width(imm, width);
    }

    pub fn emit_test_mi_abs(&mut self, addr: u64, imm: i64, width: OpWidth) {
        self.emit_rex_for_width_mem(width, PhysReg::Rbp, None);
        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_abs(PhysReg::Rax, addr);
        self.emit_imm_by_width(imm, width);
    }

    pub fn emit_test_mi_pcrel(&mut self, disp: i32, imm: i64, width: OpWidth) -> usize {
        self.emit_rex_for_width_mem(width, PhysReg::Rbp, None);
        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        let offset = self.emit_modrm_pcrel(PhysReg::Rax, disp);
        self.emit_imm_by_width(imm, width);
        offset
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

    pub fn emit_group3_m_disp(
        &mut self,
        digit: u8,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        width: OpWidth,
    ) {
        self.emit_rex_for_width_mem(width, base, None);
        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_mem_disp(Self::digit_reg(digit), base, disp, disp_size);
    }

    pub fn emit_group3_m_sib_disp(
        &mut self,
        digit: u8,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        width: OpWidth,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        self.emit_rex_for_width_mem(width, base_reg, Some(index));
        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_sib_disp(Self::digit_reg(digit), base, index, scale, disp, disp_size);
    }

    pub fn emit_group3_m_abs(&mut self, digit: u8, addr: u64, width: OpWidth) {
        self.emit_rex_for_width_mem(width, PhysReg::Rbp, None);
        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_abs(Self::digit_reg(digit), addr);
    }

    pub fn emit_group3_m_pcrel(&mut self, digit: u8, disp: i32, width: OpWidth) -> usize {
        self.emit_rex_for_width_mem(width, PhysReg::Rbp, None);
        let opcode = match width {
            OpWidth::W8 => 0xF6,
            _ => 0xF7,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_pcrel(Self::digit_reg(digit), disp)
    }

    pub fn emit_group5_m_disp(&mut self, digit: u8, base: PhysReg, disp: i32, disp_size: DispSize) {
        self.emit_rex_for_mem(base, None);
        self.code.emit_u8(0xFF);
        self.emit_modrm_mem_disp(Self::digit_reg(digit), base, disp, disp_size);
    }

    pub fn emit_group5_m_sib_disp(
        &mut self,
        digit: u8,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        self.emit_rex_for_mem(base_reg, Some(index));
        self.code.emit_u8(0xFF);
        self.emit_modrm_sib_disp(Self::digit_reg(digit), base, index, scale, disp, disp_size);
    }

    pub fn emit_group5_m_abs(&mut self, digit: u8, addr: u64) {
        self.emit_rex_for_mem(PhysReg::Rbp, None);
        self.code.emit_u8(0xFF);
        self.emit_modrm_abs(Self::digit_reg(digit), addr);
    }

    pub fn emit_group5_m_pcrel(&mut self, digit: u8, disp: i32) -> usize {
        self.emit_rex_for_mem(PhysReg::Rbp, None);
        self.code.emit_u8(0xFF);
        self.emit_modrm_pcrel(Self::digit_reg(digit), disp)
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

    /// ROL r/m, imm8
    pub fn emit_rol_ri(&mut self, dst: PhysReg, amount: u8, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        if amount == 1 {
            let opcode = match width {
                OpWidth::W8 => 0xD0,
                _ => 0xD1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 0, dst);
        } else {
            let opcode = match width {
                OpWidth::W8 => 0xC0,
                _ => 0xC1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 0, dst);
            self.code.emit_u8(amount);
        }
    }

    /// ROL r/m, CL
    pub fn emit_rol_cl(&mut self, dst: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        let opcode = match width {
            OpWidth::W8 => 0xD2,
            _ => 0xD3,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 0, dst);
    }

    /// ROR r/m, imm8
    pub fn emit_ror_ri(&mut self, dst: PhysReg, amount: u8, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        if amount == 1 {
            let opcode = match width {
                OpWidth::W8 => 0xD0,
                _ => 0xD1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 1, dst);
        } else {
            let opcode = match width {
                OpWidth::W8 => 0xC0,
                _ => 0xC1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 1, dst);
            self.code.emit_u8(amount);
        }
    }

    /// ROR r/m, CL
    pub fn emit_ror_cl(&mut self, dst: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        let opcode = match width {
            OpWidth::W8 => 0xD2,
            _ => 0xD3,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 1, dst);
    }

    /// RCL r/m, imm8
    pub fn emit_rcl_ri(&mut self, dst: PhysReg, amount: u8, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        if amount == 1 {
            let opcode = match width {
                OpWidth::W8 => 0xD0,
                _ => 0xD1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 2, dst);
        } else {
            let opcode = match width {
                OpWidth::W8 => 0xC0,
                _ => 0xC1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 2, dst);
            self.code.emit_u8(amount);
        }
    }

    /// RCL r/m, CL
    pub fn emit_rcl_cl(&mut self, dst: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        let opcode = match width {
            OpWidth::W8 => 0xD2,
            _ => 0xD3,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 2, dst);
    }

    /// RCR r/m, imm8
    pub fn emit_rcr_ri(&mut self, dst: PhysReg, amount: u8, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        if amount == 1 {
            let opcode = match width {
                OpWidth::W8 => 0xD0,
                _ => 0xD1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 3, dst);
        } else {
            let opcode = match width {
                OpWidth::W8 => 0xC0,
                _ => 0xC1,
            };
            self.code.emit_u8(opcode);
            self.emit_modrm_digit(0b11, 3, dst);
            self.code.emit_u8(amount);
        }
    }

    /// RCR r/m, CL
    pub fn emit_rcr_cl(&mut self, dst: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, PhysReg::Rax, dst);

        let opcode = match width {
            OpWidth::W8 => 0xD2,
            _ => 0xD3,
        };
        self.code.emit_u8(opcode);
        self.emit_modrm_digit(0b11, 3, dst);
    }

    pub(crate) fn shift_opcode(width: OpWidth, count: ShiftCount) -> u8 {
        match count {
            ShiftCount::One => match width {
                OpWidth::W8 => 0xD0,
                _ => 0xD1,
            },
            ShiftCount::Cl => match width {
                OpWidth::W8 => 0xD2,
                _ => 0xD3,
            },
            ShiftCount::Imm(_) => match width {
                OpWidth::W8 => 0xC0,
                _ => 0xC1,
            },
        }
    }

    pub fn emit_shift_m_disp(
        &mut self,
        digit: u8,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        width: OpWidth,
        count: ShiftCount,
    ) {
        self.emit_rex_for_width_mem(width, base, None);
        let opcode = Self::shift_opcode(width, count);
        self.code.emit_u8(opcode);
        self.emit_modrm_mem_disp(Self::digit_reg(digit), base, disp, disp_size);
        if let ShiftCount::Imm(imm) = count {
            self.code.emit_u8(imm);
        }
    }

    pub fn emit_shift_m_sib_disp(
        &mut self,
        digit: u8,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        width: OpWidth,
        count: ShiftCount,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        self.emit_rex_for_width_mem(width, base_reg, Some(index));
        let opcode = Self::shift_opcode(width, count);
        self.code.emit_u8(opcode);
        self.emit_modrm_sib_disp(Self::digit_reg(digit), base, index, scale, disp, disp_size);
        if let ShiftCount::Imm(imm) = count {
            self.code.emit_u8(imm);
        }
    }

    pub fn emit_shift_m_abs(&mut self, digit: u8, addr: u64, width: OpWidth, count: ShiftCount) {
        self.emit_rex_for_width_mem(width, PhysReg::Rbp, None);
        let opcode = Self::shift_opcode(width, count);
        self.code.emit_u8(opcode);
        self.emit_modrm_abs(Self::digit_reg(digit), addr);
        if let ShiftCount::Imm(imm) = count {
            self.code.emit_u8(imm);
        }
    }

    pub fn emit_shift_m_pcrel(
        &mut self,
        digit: u8,
        disp: i32,
        width: OpWidth,
        count: ShiftCount,
    ) -> usize {
        self.emit_rex_for_width_mem(width, PhysReg::Rbp, None);
        let opcode = Self::shift_opcode(width, count);
        self.code.emit_u8(opcode);
        let offset = self.emit_modrm_pcrel(Self::digit_reg(digit), disp);
        if let ShiftCount::Imm(imm) = count {
            self.code.emit_u8(imm);
        }
        offset
    }

    pub fn emit_shld_rr_imm(&mut self, dst: PhysReg, src: PhysReg, imm: u8, width: OpWidth) {
        self.emit_rex_for_width(width, src, dst);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xA4);
        self.emit_modrm_rr(src, dst);
        self.code.emit_u8(imm);
    }

    pub fn emit_shld_rr_cl(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, src, dst);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xA5);
        self.emit_modrm_rr(src, dst);
    }

    pub fn emit_shrd_rr_imm(&mut self, dst: PhysReg, src: PhysReg, imm: u8, width: OpWidth) {
        self.emit_rex_for_width(width, src, dst);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xAC);
        self.emit_modrm_rr(src, dst);
        self.code.emit_u8(imm);
    }

    pub fn emit_shrd_rr_cl(&mut self, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.emit_rex_for_width(width, src, dst);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xAD);
        self.emit_modrm_rr(src, dst);
    }

    pub fn emit_shld_mr_disp(
        &mut self,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        src: PhysReg,
        imm: Option<u8>,
        width: OpWidth,
    ) {
        self.emit_rex_for_width_mem_reg(width, src, base, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(if imm.is_some() { 0xA4 } else { 0xA5 });
        self.emit_modrm_mem_disp(src, base, disp, disp_size);
        if let Some(val) = imm {
            self.code.emit_u8(val);
        }
    }

    pub fn emit_shld_mr_sib_disp(
        &mut self,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        src: PhysReg,
        imm: Option<u8>,
        width: OpWidth,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        self.emit_rex_for_width_mem_reg(width, src, base_reg, Some(index));
        self.code.emit_u8(0x0F);
        self.code.emit_u8(if imm.is_some() { 0xA4 } else { 0xA5 });
        self.emit_modrm_sib_disp(src, base, index, scale, disp, disp_size);
        if let Some(val) = imm {
            self.code.emit_u8(val);
        }
    }

    pub fn emit_shld_mr_abs(&mut self, addr: u64, src: PhysReg, imm: Option<u8>, width: OpWidth) {
        self.emit_rex_for_width_mem_reg(width, src, PhysReg::Rbp, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(if imm.is_some() { 0xA4 } else { 0xA5 });
        self.emit_modrm_abs(src, addr);
        if let Some(val) = imm {
            self.code.emit_u8(val);
        }
    }

    pub fn emit_shld_mr_pcrel(
        &mut self,
        disp: i32,
        src: PhysReg,
        imm: Option<u8>,
        width: OpWidth,
    ) -> usize {
        self.emit_rex_for_width_mem_reg(width, src, PhysReg::Rbp, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(if imm.is_some() { 0xA4 } else { 0xA5 });
        let offset = self.emit_modrm_pcrel(src, disp);
        if let Some(val) = imm {
            self.code.emit_u8(val);
        }
        offset
    }

    pub fn emit_shrd_mr_disp(
        &mut self,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        src: PhysReg,
        imm: Option<u8>,
        width: OpWidth,
    ) {
        self.emit_rex_for_width_mem_reg(width, src, base, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(if imm.is_some() { 0xAC } else { 0xAD });
        self.emit_modrm_mem_disp(src, base, disp, disp_size);
        if let Some(val) = imm {
            self.code.emit_u8(val);
        }
    }

    pub fn emit_shrd_mr_sib_disp(
        &mut self,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        src: PhysReg,
        imm: Option<u8>,
        width: OpWidth,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        self.emit_rex_for_width_mem_reg(width, src, base_reg, Some(index));
        self.code.emit_u8(0x0F);
        self.code.emit_u8(if imm.is_some() { 0xAC } else { 0xAD });
        self.emit_modrm_sib_disp(src, base, index, scale, disp, disp_size);
        if let Some(val) = imm {
            self.code.emit_u8(val);
        }
    }

    pub fn emit_shrd_mr_abs(&mut self, addr: u64, src: PhysReg, imm: Option<u8>, width: OpWidth) {
        self.emit_rex_for_width_mem_reg(width, src, PhysReg::Rbp, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(if imm.is_some() { 0xAC } else { 0xAD });
        self.emit_modrm_abs(src, addr);
        if let Some(val) = imm {
            self.code.emit_u8(val);
        }
    }

    pub fn emit_shrd_mr_pcrel(
        &mut self,
        disp: i32,
        src: PhysReg,
        imm: Option<u8>,
        width: OpWidth,
    ) -> usize {
        self.emit_rex_for_width_mem_reg(width, src, PhysReg::Rbp, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(if imm.is_some() { 0xAC } else { 0xAD });
        let offset = self.emit_modrm_pcrel(src, disp);
        if let Some(val) = imm {
            self.code.emit_u8(val);
        }
        offset
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

    pub(crate) fn emit_imul_immediate(&mut self, imm: i32, width: OpWidth, use_imm8: bool) {
        if use_imm8 {
            self.code.emit_i8(imm as i8);
        } else if width == OpWidth::W16 {
            self.code.emit_u16(imm as u16);
        } else {
            self.code.emit_i32(imm);
        }
    }

    /// IMUL r, r/m using a base-plus-displacement memory source.
    pub fn emit_imul_rm_disp(
        &mut self,
        dst: PhysReg,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        width: OpWidth,
    ) {
        self.emit_rex_for_width_mem_reg(width, dst, base, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xAF);
        self.emit_modrm_mem_disp(dst, base, disp, disp_size);
    }

    /// IMUL r, r/m, imm (three-operand form)
    pub fn emit_imul_rri(&mut self, dst: PhysReg, src: PhysReg, imm: i32, width: OpWidth) {
        self.emit_rex_for_width(width, dst, src);
        let use_imm8 = (-128..=127).contains(&imm);
        self.code.emit_u8(if use_imm8 { 0x6B } else { 0x69 });
        self.emit_modrm_rr(dst, src);
        self.emit_imul_immediate(imm, width, use_imm8);
    }

    pub fn emit_imul_rri_force(
        &mut self,
        dst: PhysReg,
        src: PhysReg,
        imm: i32,
        width: OpWidth,
        use_imm8: bool,
    ) {
        self.emit_rex_for_width(width, dst, src);
        self.code.emit_u8(if use_imm8 { 0x6B } else { 0x69 });
        self.emit_modrm_rr(dst, src);
        self.emit_imul_immediate(imm, width, use_imm8);
    }

    pub fn emit_imul_rmi_disp(
        &mut self,
        dst: PhysReg,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        imm: i32,
        width: OpWidth,
        use_imm8: bool,
    ) {
        self.emit_rex_for_width_mem_reg(width, dst, base, None);
        self.code.emit_u8(if use_imm8 { 0x6B } else { 0x69 });
        self.emit_modrm_mem_disp(dst, base, disp, disp_size);
        self.emit_imul_immediate(imm, width, use_imm8);
    }

    pub fn emit_imul_rmi_sib_disp(
        &mut self,
        dst: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        imm: i32,
        width: OpWidth,
        use_imm8: bool,
    ) {
        let base_reg = base.unwrap_or(PhysReg::Rbp);
        self.emit_rex_for_width_mem_reg(width, dst, base_reg, Some(index));
        self.code.emit_u8(if use_imm8 { 0x6B } else { 0x69 });
        self.emit_modrm_sib_disp(dst, base, index, scale, disp, disp_size);
        self.emit_imul_immediate(imm, width, use_imm8);
    }

    pub fn emit_imul_rmi_abs(
        &mut self,
        dst: PhysReg,
        addr: u64,
        imm: i32,
        width: OpWidth,
        use_imm8: bool,
    ) {
        self.emit_rex_for_width_mem_reg(width, dst, PhysReg::Rbp, None);
        self.code.emit_u8(if use_imm8 { 0x6B } else { 0x69 });
        self.emit_modrm_abs(dst, addr);
        self.emit_imul_immediate(imm, width, use_imm8);
    }

    pub fn emit_imul_rmi_pcrel(
        &mut self,
        dst: PhysReg,
        disp: i32,
        imm: i32,
        width: OpWidth,
        use_imm8: bool,
    ) -> usize {
        self.emit_rex_for_width_mem_reg(width, dst, PhysReg::Rbp, None);
        self.code.emit_u8(if use_imm8 { 0x6B } else { 0x69 });
        let offset = self.emit_modrm_pcrel(dst, disp);
        self.emit_imul_immediate(imm, width, use_imm8);
        offset
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

    /// CWD (sign-extend AX into DX:AX)
    pub fn emit_cwd(&mut self) {
        self.code.emit_u8(0x66);
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

    pub fn emit_push16(&mut self, src: PhysReg) {
        self.code.emit_u8(0x66);
        self.emit_push(src);
    }

    pub fn emit_push_imm8(&mut self, imm: i8) {
        self.code.emit_u8(0x6A);
        self.code.emit_i8(imm);
    }

    pub fn emit_push_imm8_16(&mut self, imm: i8) {
        self.code.emit_u8(0x66);
        self.emit_push_imm8(imm);
    }

    pub fn emit_push_imm16(&mut self, imm: i16) {
        self.code.emit_u8(0x66);
        self.code.emit_u8(0x68);
        self.code.emit_u16(imm as u16);
    }

    pub fn emit_push_imm32(&mut self, imm: i32) {
        self.code.emit_u8(0x68);
        self.code.emit_i32(imm);
    }

    /// POP r64
    pub fn emit_pop(&mut self, dst: PhysReg) {
        if dst.is_extended() {
            self.code.emit_u8(0x41); // REX.B
        }
        self.code.emit_u8(0x58 + dst.low3());
    }

    pub fn emit_pop16(&mut self, dst: PhysReg) {
        self.code.emit_u8(0x66);
        self.emit_pop(dst);
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

    /// RET imm16
    pub fn emit_ret_imm16(&mut self, imm: u16) {
        self.code.emit_u8(0xC2);
        self.code.emit_u16(imm);
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

    /// CMOVcc r, [base+disp]
    pub fn emit_cmovcc_rm_disp(
        &mut self,
        cond: X86Cond,
        dst: PhysReg,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        width: OpWidth,
    ) {
        self.emit_rex_for_width_mem_reg(width, dst, base, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x40 + cond as u8);
        self.emit_modrm_mem_disp(dst, base, disp, disp_size);
    }

    // ========================================================================
    // Miscellaneous
    // ========================================================================

    /// NOP (single-byte)
    pub fn emit_nop(&mut self) {
        self.code.emit_u8(0x90);
    }

    /// MFENCE
    pub fn emit_mfence(&mut self) {
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xAE);
        self.code.emit_u8(0xF0);
    }

    /// CLC
    pub fn emit_clc(&mut self) {
        self.code.emit_u8(0xF8);
    }

    /// STC
    pub fn emit_stc(&mut self) {
        self.code.emit_u8(0xF9);
    }

    /// CMC
    pub fn emit_cmc(&mut self) {
        self.code.emit_u8(0xF5);
    }

    /// CLD
    pub fn emit_cld(&mut self) {
        self.code.emit_u8(0xFC);
    }

    /// STD
    pub fn emit_std(&mut self) {
        self.code.emit_u8(0xFD);
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
        self.emit_lea_disp(dst, base, disp, DispSize::Auto);
    }

    pub fn emit_lea_disp(&mut self, dst: PhysReg, base: PhysReg, disp: i32, disp_size: DispSize) {
        self.emit_lea_disp_width(dst, base, disp, disp_size, OpWidth::W64);
    }

    pub fn emit_lea_disp_width(
        &mut self,
        dst: PhysReg,
        base: PhysReg,
        disp: i32,
        disp_size: DispSize,
        width: OpWidth,
    ) {
        self.emit_rex_for_width_mem_reg(width, dst, base, None);
        self.code.emit_u8(0x8D);
        self.emit_modrm_mem_disp(dst, base, disp, disp_size);
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
        self.emit_lea_sib_disp(dst, base, index, scale, disp, DispSize::Auto);
    }

    pub fn emit_lea_sib_disp(
        &mut self,
        dst: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
    ) {
        self.emit_lea_sib_disp_width(dst, base, index, scale, disp, disp_size, OpWidth::W64);
    }

    pub fn emit_lea_sib_disp_width(
        &mut self,
        dst: PhysReg,
        base: Option<PhysReg>,
        index: PhysReg,
        scale: u8,
        disp: i32,
        disp_size: DispSize,
        width: OpWidth,
    ) {
        self.emit_rex_for_width_mem_reg(width, dst, base.unwrap_or(PhysReg::Rbp), Some(index));
        self.code.emit_u8(0x8D);
        self.emit_modrm_sib_disp(dst, base, index, scale, disp, disp_size);
    }

    /// LEA r64, [rip + disp32]
    pub fn emit_lea_pcrel(&mut self, dst: PhysReg, disp: i32) -> usize {
        self.emit_lea_pcrel_width(dst, disp, OpWidth::W64)
    }

    pub fn emit_lea_pcrel_width(&mut self, dst: PhysReg, disp: i32, width: OpWidth) -> usize {
        self.emit_rex_for_width_mem_reg(width, dst, PhysReg::Rbp, None);
        self.code.emit_u8(0x8D);
        self.emit_modrm_pcrel(dst, disp)
    }

    /// XCHG register, register.
    pub fn emit_xchg(&mut self, r1: PhysReg, r2: PhysReg, width: OpWidth) {
        if width != OpWidth::W8 && r1 != r2 && (r1 == PhysReg::Rax || r2 == PhysReg::Rax) {
            let other = if r1 == PhysReg::Rax { r2 } else { r1 };
            // 90+rd extends its opcode-encoded register with REX.B, not REX.R.
            self.emit_rex_for_width(width, PhysReg::Rax, other);
            self.code.emit_u8(0x90 + other.low3());
            return;
        }

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

    /// BT/BTS/BTR/BTC r/m, r. The register operand supplies the bit index.
    pub(crate) fn emit_bit_test_rr(
        &mut self,
        kind: BitTestRegOp,
        operand: PhysReg,
        index: PhysReg,
        width: OpWidth,
    ) {
        self.emit_rex_for_width(width, index, operand);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(kind.register_opcode());
        self.emit_modrm_rr(index, operand);
    }

    /// Group-8 BT/BTS/BTR/BTC r/m, imm8.
    pub(crate) fn emit_bit_test_ri(
        &mut self,
        kind: BitTestRegOp,
        operand: PhysReg,
        index: u8,
        width: OpWidth,
    ) {
        self.emit_rex_for_width(width, PhysReg::Rax, operand);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xBA);
        self.emit_modrm_digit(0b11, kind.immediate_digit(), operand);
        self.code.emit_u8(index);
    }

    /// Group-8 BT/BTS/BTR/BTC [base + disp], imm8.
    pub(crate) fn emit_bit_test_mi_disp(
        &mut self,
        kind: BitTestRegOp,
        base: PhysReg,
        disp: i32,
        index: u8,
        width: OpWidth,
    ) {
        self.emit_rex_for_width_mem(width, base, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xBA);
        self.emit_modrm_mem_disp(
            Self::digit_reg(kind.immediate_digit()),
            base,
            disp,
            DispSize::Auto,
        );
        self.code.emit_u8(index);
    }

    /// CRC32 r32/r64, r/m8/r/m16/r/m32/r/m64 (SSE4.2).
    pub(crate) fn emit_crc32_rr(&mut self, dst: PhysReg, data: PhysReg, data_width: OpWidth) {
        self.code.emit_u8(0xF2);
        self.emit_rex_for_width(data_width, dst, data);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x38);
        self.code.emit_u8(if data_width == OpWidth::W8 {
            0xF0
        } else {
            0xF1
        });
        self.emit_modrm_rr(dst, data);
    }

    /// RDRAND/RDSEED r16/r32/r64. The instructions define CF as the readiness
    /// result and clear OF/SF/ZF/AF/PF; every other RFLAGS bit is unchanged.
    pub(crate) fn emit_x86_random(&mut self, dst: PhysReg, width: OpWidth, seed: bool) {
        if width == OpWidth::W16 {
            self.code.emit_u8(0x66);
        }
        self.emit_rex(width == OpWidth::W64, PhysReg::Rax, None, dst);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0xC7);
        self.emit_modrm_digit(0b11, if seed { 7 } else { 6 }, dst);
    }

    /// CRC32 r32/r64, [base + disp] (SSE4.2).
    pub(crate) fn emit_crc32_rm(
        &mut self,
        dst: PhysReg,
        base: PhysReg,
        disp: i32,
        data_width: OpWidth,
    ) {
        self.code.emit_u8(0xF2);
        self.emit_rex_for_width_mem_reg(data_width, dst, base, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x38);
        self.code.emit_u8(if data_width == OpWidth::W8 {
            0xF0
        } else {
            0xF1
        });
        self.emit_modrm_mem_disp(dst, base, disp, DispSize::Auto);
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

    /// BSF/BSR r, [base + disp].
    pub(crate) fn emit_bit_scan_rm(
        &mut self,
        reverse: bool,
        dst: PhysReg,
        base: PhysReg,
        disp: i32,
        width: OpWidth,
    ) {
        self.emit_rex_for_width_mem_reg(width, dst, base, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(if reverse { 0xBD } else { 0xBC });
        self.emit_modrm_mem_disp(dst, base, disp, DispSize::Auto);
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

    /// POPCNT/TZCNT/LZCNT r, [base + disp].
    pub(crate) fn emit_x86_count_rm(
        &mut self,
        kind: X86CountKind,
        dst: PhysReg,
        base: PhysReg,
        disp: i32,
        width: OpWidth,
    ) {
        self.code.emit_u8(0xF3);
        self.emit_rex_for_width_mem_reg(width, dst, base, None);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(match kind {
            X86CountKind::Popcnt => 0xB8,
            X86CountKind::Tzcnt => 0xBC,
            X86CountKind::Lzcnt => 0xBD,
        });
        self.emit_modrm_mem_disp(dst, base, disp, DispSize::Auto);
    }

    /// VEX.LZ.0F38.W{0,1} BMI r, r/m, vvvv
    pub fn emit_vex_bmi_rr(
        &mut self,
        opcode: u8,
        dst: PhysReg,
        src: PhysReg,
        control: PhysReg,
        width: OpWidth,
    ) {
        self.emit_vex_bmi_rr_pp(opcode, X86SsePrefix::None, dst, src, control, width);
    }

    pub fn emit_vex_bmi_rr_pp(
        &mut self,
        opcode: u8,
        pp: X86SsePrefix,
        dst: PhysReg,
        src: PhysReg,
        control: PhysReg,
        width: OpWidth,
    ) {
        let r = (dst.encoding() >> 3) & 0x1;
        let b = (src.encoding() >> 3) & 0x1;
        let vvvv = control.encoding() & 0x0f;
        let w = u8::from(width == OpWidth::W64);
        let pp = Self::vex_pp_bits(pp);

        self.code.emit_u8(0xC4);
        self.code
            .emit_u8((((r ^ 1) & 1) << 7) | (1 << 6) | (((b ^ 1) & 1) << 5) | 0x02);
        self.code
            .emit_u8((w << 7) | (((vvvv ^ 0x0f) & 0x0f) << 3) | pp);
        self.code.emit_u8(opcode);
        self.emit_modrm_rr(dst, src);
    }

    pub fn emit_vex_bmi_rm_disp_pp(
        &mut self,
        opcode: u8,
        pp: X86SsePrefix,
        dst: PhysReg,
        base: PhysReg,
        disp: i32,
        control: PhysReg,
        width: OpWidth,
    ) {
        let r = (dst.encoding() >> 3) & 0x1;
        let b = (base.encoding() >> 3) & 0x1;
        let vvvv = control.encoding() & 0x0f;
        let w = u8::from(width == OpWidth::W64);
        let pp = Self::vex_pp_bits(pp);

        self.code.emit_u8(0xC4);
        self.code
            .emit_u8((((r ^ 1) & 1) << 7) | (1 << 6) | (((b ^ 1) & 1) << 5) | 0x02);
        self.code
            .emit_u8((w << 7) | (((vvvv ^ 0x0f) & 0x0f) << 3) | pp);
        self.code.emit_u8(opcode);
        self.emit_modrm_mem_disp(dst, base, disp, DispSize::Auto);
    }

    /// VEX.LZ.0F38.F3 /1..=/3 BLSR/BLSMSK/BLSI r, r/m.
    pub fn emit_vex_bls_rr(
        &mut self,
        kind: X86BlsKind,
        dst: PhysReg,
        src: PhysReg,
        width: OpWidth,
    ) {
        let group = match kind {
            X86BlsKind::Blsr => 1,
            X86BlsKind::Blsmsk => 2,
            X86BlsKind::Blsi => 3,
        };
        let b = (src.encoding() >> 3) & 0x1;
        let vvvv = dst.encoding() & 0x0f;
        let w = u8::from(width == OpWidth::W64);

        self.code.emit_u8(0xC4);
        self.code
            .emit_u8((1 << 7) | (1 << 6) | (((b ^ 1) & 1) << 5) | 0x02);
        self.code.emit_u8((w << 7) | (((vvvv ^ 0x0f) & 0x0f) << 3));
        self.code.emit_u8(0xF3);
        self.code.emit_u8(0xC0 | (group << 3) | src.low3());
    }

    /// ADCX/ADOX r, r/m (requires ADX support).
    pub fn emit_adx_rr(&mut self, kind: X86AdxKind, dst: PhysReg, src: PhysReg, width: OpWidth) {
        self.code.emit_u8(match kind {
            X86AdxKind::Adcx => 0x66,
            X86AdxKind::Adox => 0xF3,
        });
        self.emit_rex_for_width(width, dst, src);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x38);
        self.code.emit_u8(0xF6);
        self.emit_modrm_rr(dst, src);
    }

    /// ADCX/ADOX r, [rsp], used to preserve a source destroyed by the
    /// three-operand-to-two-operand destination move.
    pub fn emit_adx_rsp_mem(&mut self, kind: X86AdxKind, dst: PhysReg, width: OpWidth) {
        self.code.emit_u8(match kind {
            X86AdxKind::Adcx => 0x66,
            X86AdxKind::Adox => 0xF3,
        });
        self.emit_rex_for_width(width, dst, PhysReg::Rsp);
        self.code.emit_u8(0x0F);
        self.code.emit_u8(0x38);
        self.code.emit_u8(0xF6);
        self.code.emit_u8((dst.low3() << 3) | 0x04);
        self.code.emit_u8(0x24);
    }
}
