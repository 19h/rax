//! Fused native lowering for `BT` with a register bit offset into memory.

use super::*;
use crate::smir::lower::X86JitBitOffsetTerm;

impl X86_64Lowerer {
    /// Fold the architectural bit-offset term into the effective address the
    /// memory helper is about to use.
    ///
    /// Emitted where the helper prologue has already spilled every guest GPR to
    /// `GuestRegs` (state pointer in RAX) and RSI holds the base address, so
    /// RDI is free scratch and the incoming guest flags are already saved.
    pub(crate) fn emit_jit_mem_bit_offset_term(
        &mut self,
        term: X86JitBitOffsetTerm,
    ) -> Result<(), LowerError> {
        // rdi = guest[index]
        self.emit_struct_mov(PhysReg::Rax, 7, i32::from(term.index) * 8, false);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            match term.from_width {
                OpWidth::W64 => {}
                OpWidth::W32 | OpWidth::W16 => {
                    emitter.emit_movsx(PhysReg::Rdi, PhysReg::Rdi, term.from_width, OpWidth::W64);
                }
                _ => {
                    return Err(LowerError::InvalidOperand {
                        op: "jit-mem bit offset".to_string(),
                        operand: format!("unsupported offset width {:?}", term.from_width),
                    });
                }
            }
            // (sign_extend(index) >> log2(bits)) << log2(bytes)
            emitter.emit_sar_ri(PhysReg::Rdi, term.shift_right, OpWidth::W64);
            emitter.emit_shl_ri(PhysReg::Rdi, term.shift_left, OpWidth::W64);
        }
        // add rsi, rdi
        self.code.emit_u8(0x48);
        self.code.emit_u8(0x01);
        self.code.emit_u8(0xFE);
        Ok(())
    }

    /// Lower the eight-operation memory `BT`.
    ///
    /// The addressed element is read through the ordinary MMU helper with the
    /// bit-offset term folded into its address; the architectural CF is then
    /// produced by a native `BT` on the staged value, merging exactly CF into
    /// the incoming flag image so the operation's architecturally undefined
    /// outputs keep the interpreter's deterministic values.
    #[cfg(feature = "smir-jit")]
    pub(crate) fn try_lower_jit_mem_bit_offset_test(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_mem_bit_offset_test_sequence(
            block,
            idx,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let index_register = self.get_reg(sequence.index_register)?;

        // Caller-frame layout after the flag-neutral reservation:
        //   [rsp+0]  zero-extended bit-string element from the load helper
        //   [rsp+8]  staged architectural bit offset
        //   [rsp+16] complete architectural RDX
        //   [rsp+24] complete architectural RAX
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -32);
            emitter.emit_mov_mr(PhysReg::Rsp, 24, PhysReg::Rax, OpWidth::W64);
            emitter.emit_mov_mr(PhysReg::Rsp, 16, PhysReg::Rdx, OpWidth::W64);
            emitter.emit_mov_mr(PhysReg::Rsp, 8, index_register, OpWidth::W64);
        }
        self.emit_jit_mem_op_bit_offset(
            sequence.guest_pc,
            true,
            None,
            Some(16),
            None,
            None,
            None,
            sequence.addr,
            sequence.mem_width,
            SignExtend::Zero,
            32,
            sequence.term,
        )?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rax, PhysReg::Rsp, 0, OpWidth::W64);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rsp, 8, OpWidth::W64);
        }
        self.code.emit_u8(0x9C); // pushfq: preserve architecturally undefined flags
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_bit_test_rr(
                BitTestRegOp::Test,
                PhysReg::Rax,
                PhysReg::Rdx,
                sequence.width,
            );
        }
        self.finish_bmi_flags(PhysReg::Rax, Some(1 << 0));
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rax, PhysReg::Rsp, 24, OpWidth::W64);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rsp, 16, OpWidth::W64);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 32);
        }
        Ok(Some(8))
    }
}
