//! MMX state preservation and helper-backed memory transfers.

#[cfg(feature = "smir-jit")]
use std::collections::HashMap;

use super::{X86_64Lowerer, X86Cond, X86Emitter};
#[cfg(feature = "smir-jit")]
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{X86OpHint, X86SsePrefix, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, DispSize, MemWidth, SignExtend, VReg, VecWidth, X86Reg,
};
use crate::smir::lower::regalloc::PhysReg;
#[cfg(feature = "smir-jit")]
use crate::smir::lower::runtime::X86MmxScalarMemoryTransferEncoding;
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
            // Every caller saves guest RFLAGS before publishing helper state.
            // Clear host DF only after that snapshot: the platform ABI requires
            // DF=0 at Rust call boundaries, and the caller restores guest DF.
            self.code.emit_u8(0xFC);
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
        let exact_movq = matches!(
            hint,
            Some(X86OpHint::SseMov {
                prefix: X86SsePrefix::None,
                opcode,
            }) if opcode == expected_opcode
        );
        let exact_movntq =
            !is_load && matches!(hint, Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)));
        (width == VecWidth::V64 && (exact_movq || exact_movntq)).then_some(index)
    }

    fn emit_mmx_stack_move(&mut self, reg: PhysReg, offset: i32, store: bool) {
        self.code.emit_u8(0x0F);
        self.code.emit_u8(if store { 0x7F } else { 0x6F });
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_modrm_mem_disp(reg, PhysReg::Rsp, offset, DispSize::Auto);
    }

    #[cfg(feature = "smir-jit")]
    fn emit_mmx_scalar_stack_transfer(&mut self, encoding: X86MmxScalarMemoryTransferEncoding) {
        if encoding.rex_w {
            self.code.emit_u8(0x48);
        }
        self.code.emit_u8(0x0F);
        self.code.emit_u8(encoding.opcode);
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_modrm_mem_disp(
            PhysReg::Mm(encoding.mm_index),
            PhysReg::Rsp,
            0,
            DispSize::Auto,
        );
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
                operand: "expected exact legacy MMX MOVQ or MOVNTQ memory form".to_string(),
            }
        })?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        if !is_load {
            self.emit_mmx_stack_move(PhysReg::Mm(index), 0, true);
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
            self.emit_mmx_stack_move(PhysReg::Mm(index), 0, false);
        }
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        Ok(())
    }

    /// Fuse one exact `MASKMOVQ mm, mm` expansion into eight ordered,
    /// conditionally executed 1-byte MMU-helper stores. The data and mask MMX
    /// registers are snapshotted in a 16-byte caller slot. Each mask test saves
    /// and restores guest flags before either skipping the lane or crossing the
    /// Rust ABI boundary. A lane fault releases the caller slot and returns at
    /// the original guest PC; earlier active lanes remain committed and the
    /// trailing `EnterMmx` marker is not executed.
    #[cfg(feature = "smir-jit")]
    pub(crate) fn try_lower_jit_mmx_maskmovq(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_mmx_maskmovq_sequence(
            block,
            idx,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        self.emit_mmx_stack_move(PhysReg::Mm(sequence.data_index), 0, true);
        self.emit_mmx_stack_move(PhysReg::Mm(sequence.mask_index), 8, true);

        for lane in 0..8u8 {
            let store_offset = if sequence.address_size_32 {
                4 + usize::from(lane) * 5
            } else {
                3 + usize::from(lane) * 4
            };
            let store = &block.ops[idx + store_offset];
            let lifted_addr = match &store.kind {
                crate::smir::ir::ops::OpKind::PredStore {
                    addr,
                    width: MemWidth::B1,
                    ..
                } => addr,
                _ => {
                    return Err(LowerError::InvalidOperand {
                        op: "MMX MASKMOVQ".to_string(),
                        operand: "validated lane must end in its exact byte store".to_string(),
                    });
                }
            };
            let helper_addr = if sequence.address_size_32 {
                Some(match lifted_addr {
                    Address::BaseOffset { disp_size, .. } => Address::BaseOffset {
                        base: VReg::Arch(ArchReg::X86(X86Reg::Rdi)),
                        offset: i64::from(lane),
                        disp_size: *disp_size,
                    },
                    Address::SegmentRel {
                        segment,
                        index: None,
                        scale: 1,
                        ..
                    } => Address::SegmentRel {
                        segment: *segment,
                        base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rdi))),
                        index: None,
                        scale: 1,
                        disp: i64::from(lane),
                    },
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "MMX MASKMOVQ addr32".to_string(),
                            operand: "validated lane must use EDI with optional FS/GS".to_string(),
                        });
                    }
                })
            } else {
                None
            };
            let addr = helper_addr.as_ref().unwrap_or(lifted_addr);

            self.code.emit_u8(0x9C); // pushfq
            // test byte ptr [rsp + saved-flags + mask-slot + lane], 0x80
            self.code.emit_u8(0xF6);
            self.code.emit_u8(0x44);
            self.code.emit_u8(0x24);
            self.code.emit_u8(16 + lane);
            self.code.emit_u8(0x80);
            let inactive = self.emit_jcc_placeholder(X86Cond::E);
            self.code.emit_u8(0x9D); // popfq before a helper call

            let emit = if sequence.address_size_32 {
                Self::emit_jit_mem_op_addr32
            } else {
                Self::emit_jit_mem_op
            };
            emit(
                self,
                store.guest_pc,
                false,
                None,
                None,
                None,
                None,
                Some(16 + i32::from(lane)),
                addr,
                MemWidth::B1,
                SignExtend::Zero,
                16,
            )?;
            self.code.emit_u8(0xE9);
            let done = self.code.position();
            self.code.emit_u32(0);

            self.patch_rel32_to_current(inactive)?;
            self.code.emit_u8(0x9D); // popfq on the inactive path
            self.patch_rel32_to_current(done)?;
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        self.lower_op(&block.ops[idx + sequence.marker_offset])?;
        Ok(Some(sequence.consumed))
    }

    /// Fuse one exact MMX MOVD/MOVQ scalar-memory transfer. A 16-byte host
    /// stack slot holds the architectural 4- or 8-byte payload across the MMU
    /// helper boundary; the MMX-state marker is committed only after success.
    #[cfg(feature = "smir-jit")]
    pub(crate) fn try_lower_jit_mmx_scalar_memory_transfer(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_mmx_scalar_memory_transfer_sequence(
                block,
                idx,
                true,
                virtual_definitions,
                virtual_uses,
            )
        else {
            return Ok(None);
        };
        let memory = &block.ops[idx + sequence.memory_offset];
        let addr = match (&memory.kind, sequence.encoding.is_load) {
            (
                crate::smir::ir::ops::OpKind::Load {
                    addr,
                    width,
                    sign: SignExtend::Zero,
                    ..
                },
                true,
            ) if *width == sequence.encoding.mem_width => addr,
            (crate::smir::ir::ops::OpKind::Store { addr, width, .. }, false)
                if *width == sequence.encoding.mem_width =>
            {
                addr
            }
            _ => {
                return Err(LowerError::InvalidOperand {
                    op: "MMX MOVD/MOVQ memory transfer".to_string(),
                    operand: "validated sequence must contain its exact architectural access"
                        .to_string(),
                });
            }
        };

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        if !sequence.encoding.is_load {
            self.emit_mmx_scalar_stack_transfer(sequence.encoding);
        }
        self.emit_jit_mem_op(
            memory.guest_pc,
            sequence.encoding.is_load,
            None,
            sequence.encoding.is_load.then_some(16),
            None,
            None,
            (!sequence.encoding.is_load).then_some(16),
            addr,
            sequence.encoding.mem_width,
            SignExtend::Zero,
            16,
        )?;
        if sequence.encoding.is_load {
            self.emit_mmx_scalar_stack_transfer(sequence.encoding);
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        self.lower_op(&block.ops[idx + sequence.marker_offset])?;
        Ok(Some(sequence.consumed))
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
