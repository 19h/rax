//! Fused native lowering for memory-destination `CMPXCHG`.

use super::*;
use crate::smir::lower::runtime::X86JitScalarValue;

impl X86_64Lowerer {
    /// Lower the fused memory `CMPXCHG`.
    ///
    /// Layout of the flag-neutral caller frame:
    ///   `[rsp+0]`  zero-extended memory operand written by the load helper
    ///   `[rsp+8]`  staged replacement value
    ///   `[rsp+24]` complete architectural RAX
    ///
    /// The architectural flags come from a single `CMP` against the staged
    /// memory operand; the helper call on the matching path preserves them, and
    /// every other instruction in the sequence is `MOV`/`LEA`/`Jcc`. The
    /// accumulator write-back is a branch rather than `CMOVcc` because SMIR's
    /// `CMove` writes only when the condition holds, whereas a 32-bit host
    /// `CMOVcc` would zero-extend the destination unconditionally.
    #[cfg(feature = "smir-jit")]
    pub(crate) fn try_lower_jit_cmpxchg(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_cmpxchg_sequence(
            block,
            idx,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let width = sequence.width;

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -32);
            emitter.emit_mov_mr(PhysReg::Rsp, 24, PhysReg::Rax, OpWidth::W64);
        }
        // Stage the replacement value while every guest register is still live.
        match sequence.source {
            X86JitScalarValue::Register(source) => {
                let source = self.get_reg(source)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rsp, 8, source, OpWidth::W64);
            }
            X86JitScalarValue::Immediate(value) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_ri(PhysReg::Rax, value, OpWidth::W64);
                emitter.emit_mov_mr(PhysReg::Rsp, 8, PhysReg::Rax, OpWidth::W64);
                emitter.emit_mov_rm(PhysReg::Rax, PhysReg::Rsp, 24, OpWidth::W64);
            }
        }

        self.emit_jit_mem_op(
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
        )?;

        // Publish the architectural comparison. RAX carries the accumulator
        // value; its guest content is restored below without touching flags.
        match sequence.accumulator {
            X86JitScalarValue::Register(accumulator) => {
                let accumulator = self.get_reg(accumulator)?;
                if accumulator != PhysReg::Rax {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(PhysReg::Rax, accumulator, OpWidth::W64);
                }
            }
            X86JitScalarValue::Immediate(value) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_ri(PhysReg::Rax, value, OpWidth::W64);
            }
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_alu_mem_disp(
                0x38,
                PhysReg::Rax,
                PhysReg::Rsp,
                0,
                DispSize::Auto,
                width,
                X86AluEncoding::RegRm,
            );
            // Flag-neutral restore of the architectural accumulator.
            emitter.emit_mov_rm(PhysReg::Rax, PhysReg::Rsp, 24, OpWidth::W64);
        }

        let mismatch = self.emit_jcc_placeholder(X86Cond::Ne);
        self.emit_jit_mem_op(
            sequence.guest_pc,
            false,
            None,
            None,
            None,
            None,
            Some(24),
            sequence.addr,
            sequence.mem_width,
            SignExtend::Zero,
            32,
        )?;
        self.code.emit_u8(0xE9); // jmp .done
        let done = self.code.position();
        self.code.emit_u32(0);

        self.patch_rel32_to_current(mismatch)?;
        if sequence.writes_accumulator {
            // Architecturally the accumulator takes the memory operand only on
            // a mismatch, with ordinary partial-register write semantics.
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rax, PhysReg::Rsp, 0, width);
        }
        self.patch_rel32_to_current(done)?;

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 32);
        }
        Ok(Some(sequence.consumed))
    }
}
