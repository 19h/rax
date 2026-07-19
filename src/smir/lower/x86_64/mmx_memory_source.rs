//! Helper-backed lowering for exact MMX memory-source operations.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{DispSize, MemWidth, SignExtend, VReg, VecWidth};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::runtime::X86MmxMemorySourceEncoding;

impl X86_64Lowerer {
    fn emit_mmx_memory_stack_source(
        &mut self,
        encoding: X86MmxMemorySourceEncoding,
    ) -> Result<(), LowerError> {
        use crate::smir::ir::ops::X86VecMap;

        self.code.emit_u8(0x0F);
        match encoding.map {
            X86VecMap::Map0F => {}
            X86VecMap::Map0F38 => self.code.emit_u8(0x38),
            X86VecMap::Map0F3A => self.code.emit_u8(0x3A),
            X86VecMap::Map5 | X86VecMap::Map6 => {
                return Err(LowerError::InvalidOperand {
                    op: "MMX memory source".to_string(),
                    operand: "classic MMX cannot use EVEX map 5/6".to_string(),
                });
            }
        }
        self.code.emit_u8(encoding.opcode);
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_modrm_mem_disp(
            PhysReg::Mm(encoding.dst_index),
            PhysReg::Rsp,
            0,
            DispSize::Auto,
        );
        if let Some(immediate) = encoding.immediate {
            self.code.emit_u8(immediate);
        }
        Ok(())
    }

    /// Fuse one exact helper-backed MMX memory-source sequence.
    /// The MMU helper deposits the source in a 16-byte caller slot, after which
    /// the original MMX opcode consumes `[rsp]` directly. No virtual register is
    /// allocated onto the identity-mapped guest GPR file.
    #[cfg(feature = "smir-jit")]
    pub(crate) fn try_lower_jit_mmx_memory_source(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_mmx_memory_source_sequence(
            block,
            idx,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let load = &block.ops[idx];
        let addr = match (&load.kind, sequence.encoding.mem_width) {
            (
                OpKind::VLoad {
                    addr,
                    width: VecWidth::V64,
                    ..
                },
                MemWidth::B8,
            ) => addr,
            (
                OpKind::Load {
                    addr,
                    width,
                    sign: SignExtend::Zero,
                    ..
                },
                expected @ (MemWidth::B2 | MemWidth::B4),
            ) if *width == expected => addr,
            _ => {
                return Err(LowerError::InvalidOperand {
                    op: "MMX memory source".to_string(),
                    operand: "validated sequence must start with its exact architectural load"
                        .to_string(),
                });
            }
        };

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        self.emit_jit_mem_op(
            load.guest_pc,
            true,
            None,
            Some(16),
            None,
            None,
            None,
            addr,
            sequence.encoding.mem_width,
            SignExtend::Zero,
            16,
        )?;
        self.emit_mmx_memory_stack_source(sequence.encoding)?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        self.lower_op(&block.ops[idx + sequence.marker_offset])?;
        Ok(Some(sequence.consumed))
    }
}
