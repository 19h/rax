//! Helper-backed VEX masked-memory load/store lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{Address, DispSize, OpWidth, SignExtend, VReg, VecWidth};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;

const MASK_OFFSET: i32 = 0;
const VALUE_OFFSET: i32 = 32;
const FRAME_SIZE: i32 = 96;

impl X86_64Lowerer {
    fn emit_vex_masked_stack_vector(
        &mut self,
        register: PhysReg,
        width: VecWidth,
        displacement: i32,
        load: bool,
    ) {
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_vex_prefix(
            X86VecMap::Map0F,
            X86SsePrefix::Rep,
            width,
            false,
            register.vec_ext(),
            0,
            PhysReg::Rsp.vec_ext(),
            0,
        );
        emitter.code.emit_u8(if load { 0x6F } else { 0x7F });
        emitter.emit_modrm_mem_disp(register, PhysReg::Rsp, displacement, DispSize::Auto);
    }

    fn emit_vex_masked_operand_snapshot(
        &mut self,
        mask: u8,
        vector: u8,
        width: VecWidth,
        load: bool,
    ) {
        let mask = match width {
            VecWidth::V128 => PhysReg::Xmm(mask),
            VecWidth::V256 => PhysReg::Ymm(mask),
            _ => unreachable!("validated VEX masked-memory width"),
        };
        self.emit_vex_masked_stack_vector(mask, width, MASK_OFFSET, false);
        if load {
            let mut emitter = X86Emitter::new(&mut self.code);
            for offset in (VALUE_OFFSET..VALUE_OFFSET + 32).step_by(8) {
                emitter.emit_mov_mi_disp(PhysReg::Rsp, offset, DispSize::Auto, 0, OpWidth::W64);
            }
        } else {
            let data = match width {
                VecWidth::V128 => PhysReg::Xmm(vector),
                VecWidth::V256 => PhysReg::Ymm(vector),
                _ => unreachable!("validated VEX masked-memory width"),
            };
            self.emit_vex_masked_stack_vector(data, width, VALUE_OFFSET, false);
        }
    }

    fn emit_vex_masked_lane_helper(
        &mut self,
        guest_pc: u64,
        address: &Address,
        lane: usize,
        element_bytes: i32,
        memory_width: crate::smir::ir::types::MemWidth,
        load: bool,
    ) -> Result<(), LowerError> {
        self.code.emit_u8(0x9C); // pushfq
        {
            // Test only the element MSB. The outer frame moved by 8 bytes
            // after PUSHFQ; TEST is bracketed by POPFQ on both paths.
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                PhysReg::Rsp,
                8 + MASK_OFFSET + lane as i32 * element_bytes + element_bytes - 1,
                DispSize::Auto,
                0x80,
                OpWidth::W8,
            );
        }
        let inactive = self.emit_jcc_placeholder(X86Cond::E);
        self.code.emit_u8(0x9D); // popfq before the helper

        let outer_value_offset = VALUE_OFFSET + lane as i32 * element_bytes;
        self.emit_jit_mem_op_linear_offset(
            guest_pc,
            load,
            None,
            load.then_some(16 + outer_value_offset),
            None,
            None,
            (!load).then_some(16 + outer_value_offset),
            address,
            memory_width,
            SignExtend::Zero,
            FRAME_SIZE,
            lane as i32 * element_bytes,
        )?;
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        self.patch_rel32_to_current(inactive)?;
        self.code.emit_u8(0x9D); // popfq on the inactive path
        self.patch_rel32_to_current(done)?;
        Ok(())
    }

    /// Fuse one exact `VMASKMOVPS/PD` or `VPMASKMOVD/Q` expansion.
    ///
    /// Mask and store data are snapshotted before the first possible access.
    /// A load accumulates into nonarchitectural stack state and commits its
    /// vector destination only after every active lane succeeds. Stores call
    /// helpers in ascending lane order, retaining completed active lanes if a
    /// later active lane faults. Inactive lanes invoke no memory helper.
    pub(crate) fn try_lower_jit_vex_masked_memory(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_masked_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let address = match &block.ops[index].kind {
            OpKind::Lea { addr, .. } => addr,
            _ => unreachable!("validated VEX masked-memory sequence starts with LEA"),
        };
        let encoding = sequence.encoding;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -FRAME_SIZE);
        }
        self.emit_vex_masked_operand_snapshot(
            encoding.mask,
            encoding.vector,
            encoding.width,
            encoding.load,
        );

        let lanes = encoding.width.lanes(encoding.elem) as usize;
        let element_bytes = encoding.elem.bytes() as i32;
        for lane in 0..lanes {
            self.emit_vex_masked_lane_helper(
                block.ops[index].guest_pc,
                address,
                lane,
                element_bytes,
                encoding.memory_width,
                encoding.load,
            )?;
        }

        if encoding.load {
            let destination = match encoding.width {
                VecWidth::V128 => PhysReg::Xmm(encoding.vector),
                VecWidth::V256 => PhysReg::Ymm(encoding.vector),
                _ => unreachable!("validated VEX masked-memory width"),
            };
            self.emit_vex_masked_stack_vector(destination, encoding.width, VALUE_OFFSET, true);
            if self.avx_ymm16_vector_state {
                self.emit_avx_ymm16_state_backed_upper_clear(encoding.vector);
            }
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, FRAME_SIZE);
        }
        Ok(Some(sequence.consumed))
    }
}
