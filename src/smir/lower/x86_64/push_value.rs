//! Fused native lowering for `PUSH m16/m64`.

use super::*;

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
}
