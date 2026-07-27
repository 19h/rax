//! Fused native lowering for `PUSH m16/m64`.

use super::*;

/// The exact RFLAGS bits SMIR models: bit 1 (always set) plus
/// CF, PF, AF, ZF, SF, DF, OF and AC. `MaterializedFlags::to_rflags` produces
/// this set and nothing else, so a stored flag image must match it.
const X86_SMIR_RFLAGS_MASK: i64 = 0x0004_0CD7;

impl X86_64Lowerer {
    /// Lower `Load v,[mem]; SUB RSP,n; Store v,[RSP]` as a helper-backed read
    /// into a caller frame followed by the ordinary helper-backed push.
    ///
    /// The store targets `[guest RSP - n]`, so the architectural stack pointer
    /// is committed only after the write retires — matching the generic push
    /// fusion, and matching the architecture, which leaves RSP unchanged when
    /// the stack write faults.
    #[cfg(feature = "smir-jit")]
    pub(crate) fn try_lower_jit_push_memory(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_push_memory_sequence(
            block,
            idx,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));

        // Caller frame slot [rsp+0] stages the complete zero-extended source
        // value between the two helper calls. Both calls see the same frame, so
        // both use the same active offset of +16 past their own spill.
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -32);
        }
        self.emit_jit_mem_op(
            sequence.guest_pc,
            true,
            None,
            Some(16),
            None,
            None,
            None,
            sequence.source,
            sequence.source_width,
            SignExtend::Zero,
            32,
        )?;
        let destination = Address::BaseOffset {
            base: rsp,
            offset: -sequence.delta,
            disp_size: DispSize::Auto,
        };
        self.emit_jit_mem_op(
            sequence.guest_pc,
            false,
            None,
            None,
            None,
            None,
            Some(16),
            &destination,
            sequence.push_width,
            SignExtend::Zero,
            32,
        )?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 32);
        }
        self.lower_state_backed_stack_gpr_alu(
            true,
            rsp,
            rsp,
            &SrcOperand::Imm(sequence.delta),
            OpWidth::W64,
            FlagUpdate::None,
        )?;
        Ok(Some(3))
    }

    /// Lower `ReadFlags v; SUB RSP,n; Store v,[RSP]`.
    ///
    /// The architectural flag image (host status flags with the state-backed
    /// guest AC merged in) is materialized into scratch RAX, staged on a caller
    /// frame, and then pushed through the ordinary helper-backed store. Guest
    /// RAX is saved and restored around the materialization, and every step
    /// outside it is flag-neutral.
    #[cfg(feature = "smir-jit")]
    pub(crate) fn try_lower_jit_push_flags(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_push_flags_sequence(
            block,
            idx,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -32);
            emitter.emit_mov_mr(PhysReg::Rsp, 24, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_x86_read_flags_with_ac(PhysReg::Rax)?;
        // `emit_x86_read_flags_with_ac` yields the raw host image with guest AC
        // merged in. SMIR models exactly bit 1 plus CF/PF/AF/ZF/SF/DF/OF/AC, so
        // the stored value must drop every host-only bit (IF above all) to match
        // interpretation. AND writes flags, so it runs inside a PUSHFQ/POPFQ
        // pair; PUSHF itself must leave the architectural flags untouched.
        self.code.emit_u8(0x9C); // pushfq
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_and_ri(PhysReg::Rax, X86_SMIR_RFLAGS_MASK, OpWidth::W64);
        }
        self.code.emit_u8(0x9D); // popfq
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rsp, 0, PhysReg::Rax, OpWidth::W64);
            emitter.emit_mov_rm(PhysReg::Rax, PhysReg::Rsp, 24, OpWidth::W64);
        }
        let destination = Address::BaseOffset {
            base: rsp,
            offset: -sequence.delta,
            disp_size: DispSize::Auto,
        };
        self.emit_jit_mem_op(
            sequence.guest_pc,
            false,
            None,
            None,
            None,
            None,
            Some(16),
            &destination,
            sequence.push_width,
            SignExtend::Zero,
            32,
        )?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 32);
        }
        self.lower_state_backed_stack_gpr_alu(
            true,
            rsp,
            rsp,
            &SrcOperand::Imm(sequence.delta),
            OpWidth::W64,
            FlagUpdate::None,
        )?;
        Ok(Some(3))
    }
}
