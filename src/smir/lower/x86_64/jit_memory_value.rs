//! Opt-in scalar value marshalling for packed JIT memory-helper stores.

use super::{Address, DispSize, MemWidth, OpWidth, PhysReg, SignExtend, X86_64Lowerer, X86Emitter};
use crate::smir::lower::LowerError;

impl X86_64Lowerer {
    /// Store one dense packed-stack element through the scalar helper.
    ///
    /// Unlike the generic stack-backed store path, this explicitly normalizes
    /// the helper's `u64` value argument to the element width. Dense elements
    /// are adjacent, so a qword load would otherwise include later lanes.
    pub(crate) fn emit_jit_mem_op_linear_offset_packed_stack_store(
        &mut self,
        guest_pc: u64,
        stack_offset: i32,
        address: &Address,
        memory_width: MemWidth,
        fault_stack_cleanup: i32,
        linear_offset: i32,
    ) -> Result<(), LowerError> {
        self.emit_jit_mem_op_inner(
            guest_pc,
            false,
            None,
            None,
            None,
            None,
            Some(stack_offset),
            address,
            memory_width,
            SignExtend::Zero,
            fault_stack_cleanup,
            false,
            None,
            linear_offset,
            true,
        )
    }

    /// Load one dense packed scalar into the store helper's `u64` value
    /// argument without exposing adjacent stack elements.
    pub(crate) fn emit_jit_stack_store_value_argument(
        &mut self,
        stack_offset: i32,
        memory_width: MemWidth,
    ) {
        let width = memory_width
            .to_op_width()
            .expect("JIT memory helper validated a scalar store width");
        let mut emitter = X86Emitter::new(&mut self.code);
        match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => emitter.emit_movzx_rm_disp(
                PhysReg::Rdx,
                PhysReg::Rsp,
                stack_offset,
                DispSize::Auto,
                width,
                OpWidth::W64,
            ),
            OpWidth::W64 => emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rsp, stack_offset, width),
            OpWidth::W128 => unreachable!("JIT memory helper validated a scalar store width"),
        }
    }
}
