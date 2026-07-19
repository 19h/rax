//! MMX state preservation and helper-backed memory transfers.

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::ops::{X86OpHint, X86SsePrefix};
use crate::smir::ir::types::{
    Address, ArchReg, DispSize, MemWidth, SignExtend, VReg, VecWidth, X86Reg,
};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_GUEST_MM_OFFSET};

impl X86_64Lowerer {
    pub fn set_preserve_mmx_helpers(&mut self, on: bool) {
        self.preserve_mmx_helpers = on;
    }

    /// Publish or restore MM0-MM7 around a Rust ABI boundary. The store side
    /// executes host-only EMMS after all eight values are safe in `GuestRegs`;
    /// the guest x87 tag word remains untouched and is committed only by the
    /// lifted architectural `EnterMmx`/`EmptyMmx` operations.
    pub(crate) fn emit_helper_mmx_state(&mut self, base: PhysReg, store: bool) {
        for index in 0..8u8 {
            self.code.emit_u8(0x0F);
            self.code.emit_u8(if store { 0x7F } else { 0x6F });
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_modrm_mem_disp(
                PhysReg::Mm(index),
                base,
                X86_GUEST_MM_OFFSET + i32::from(index) * 8,
                DispSize::Disp32,
            );
        }
        if store {
            self.code.emit_u8(0x0F);
            self.code.emit_u8(0x77); // EMMS: clean the host x87/MMX tag file.
        }
    }

    /// Preserve every architectural register file that a helper may clobber.
    /// Store MMX before vector state so Rust observes an empty host x87 tag
    /// file; reload MMX last so native MMX execution resumes only after all
    /// helper-boundary bookkeeping has completed.
    pub(crate) fn emit_helper_call_state(
        &mut self,
        base: PhysReg,
        store: bool,
        preserve_vectors: bool,
    ) {
        if store {
            if self.preserve_mmx_helpers {
                self.emit_helper_mmx_state(base, true);
            }
            if preserve_vectors {
                self.emit_helper_vector_state(base, true);
            }
        } else {
            if preserve_vectors {
                self.emit_helper_vector_state(base, false);
            }
            if self.preserve_mmx_helpers {
                self.emit_helper_mmx_state(base, false);
            }
        }
    }

    fn mmx_memory_index(
        vector: VReg,
        width: VecWidth,
        hint: Option<X86OpHint>,
        is_load: bool,
    ) -> Option<u8> {
        let VReg::Arch(ArchReg::X86(X86Reg::Mm(index @ 0..=7))) = vector else {
            return None;
        };
        let expected_opcode = if is_load { 0x6F } else { 0x7F };
        (width == VecWidth::V64
            && matches!(
                hint,
                Some(X86OpHint::SseMov {
                    prefix: X86SsePrefix::None,
                    opcode,
                }) if opcode == expected_opcode
            ))
        .then_some(index)
    }

    fn emit_mmx_stack_move(&mut self, reg: PhysReg, store: bool) {
        self.code.emit_u8(0x0F);
        self.code.emit_u8(if store { 0x7F } else { 0x6F });
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_modrm_mem_disp(reg, PhysReg::Rsp, 0, DispSize::Auto);
    }

    /// Route exact legacy `MOVQ mm, m64` and `MOVQ m64, mm` forms through the
    /// scalar MMU helper. A 16-byte host-stack slot stages the 64-bit payload;
    /// the inner helper's two pushes make that slot `[rsp+16]`. Fault cleanup
    /// removes the outer slot before the precise native exit.
    pub(crate) fn emit_jit_mmx_mem_op(
        &mut self,
        guest_pc: u64,
        is_load: bool,
        vector: VReg,
        addr: &Address,
        width: VecWidth,
        hint: Option<X86OpHint>,
    ) -> Result<(), LowerError> {
        let index = Self::mmx_memory_index(vector, width, hint, is_load).ok_or_else(|| {
            LowerError::InvalidOperand {
                op: if is_load { "VLoad" } else { "VStore" }.to_string(),
                operand: "expected exact legacy MMX MOVQ memory form".to_string(),
            }
        })?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        if !is_load {
            self.emit_mmx_stack_move(PhysReg::Mm(index), true);
        }
        self.emit_jit_mem_op(
            guest_pc,
            is_load,
            None,
            is_load.then_some(16),
            None,
            None,
            (!is_load).then_some(16),
            addr,
            MemWidth::B8,
            SignExtend::Zero,
            16,
        )?;
        if is_load {
            self.emit_mmx_stack_move(PhysReg::Mm(index), false);
        }
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        Ok(())
    }

    pub(crate) fn emit_jit_vector_or_mmx_mem_op(
        &mut self,
        guest_pc: u64,
        is_load: bool,
        vector: VReg,
        addr: &Address,
        width: VecWidth,
        hint: Option<X86OpHint>,
    ) -> Result<(), LowerError> {
        if Self::mmx_memory_index(vector, width, hint, is_load).is_some() {
            self.emit_jit_mmx_mem_op(guest_pc, is_load, vector, addr, width, hint)
        } else {
            self.emit_jit_vector_mem_op(guest_pc, is_load, vector, addr, width, hint)
        }
    }
}
