//! Guest effective-address construction for helper-backed JIT memory operations.

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::types::{Address, ArchReg, OpWidth, VReg, X86Reg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_GUEST_FS_BASE_OFFSET, X86_GUEST_GS_BASE_OFFSET};

impl X86_64Lowerer {
    pub(crate) fn emit_jit_mem_effective_address(
        &mut self,
        addr: &Address,
        address_size_32: bool,
    ) -> Result<(), LowerError> {
        if let Address::X86Addr32(inner) = addr {
            if address_size_32 {
                return Err(LowerError::InvalidOperand {
                    op: "jit-mem addr32".to_string(),
                    operand: "nested explicit addr32 address".to_string(),
                });
            }
            return self.emit_jit_mem_effective_address(inner, true);
        }

        // --- effective guest address into RSI (enc 6), reading base/index from
        //     the struct (state ptr in RAX) ---
        if address_size_32 {
            match addr {
                Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rdi))) => {
                    self.emit_struct_mov(PhysReg::Rax, 6, 7 * 8, false);
                    self.code.emit_u8(0x89);
                    self.code.emit_u8(0xF6); // mov esi,esi: zero-extend EDI
                }
                Address::BaseOffset {
                    base: VReg::Arch(ArchReg::X86(X86Reg::Rdi)),
                    offset,
                    ..
                } => {
                    self.emit_struct_mov(PhysReg::Rax, 6, 7 * 8, false);
                    self.code.emit_u8(0x89);
                    self.code.emit_u8(0xF6); // mov esi,esi
                    if *offset != 0 {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_add_ri(PhysReg::Rsi, *offset, OpWidth::W32);
                    }
                }
                Address::SegmentRel {
                    segment,
                    base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rdi))),
                    index: None,
                    scale: 1,
                    disp,
                } => {
                    let seg_off = match segment {
                        VReg::Arch(ArchReg::X86(X86Reg::FsBase)) => X86_GUEST_FS_BASE_OFFSET,
                        VReg::Arch(ArchReg::X86(X86Reg::GsBase)) => X86_GUEST_GS_BASE_OFFSET,
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: "jit-mem addr32: non-FS/GS segment".to_string(),
                            });
                        }
                    };
                    self.emit_struct_mov(PhysReg::Rax, 6, seg_off, false); // rsi = segment base
                    self.emit_struct_mov(PhysReg::Rax, 7, 7 * 8, false); // rdi = guest RDI
                    self.code.emit_u8(0x89);
                    self.code.emit_u8(0xFF); // mov edi,edi
                    if *disp != 0 {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_add_ri(PhysReg::Rdi, *disp, OpWidth::W32);
                    }
                    self.code.emit_u8(0x48);
                    self.code.emit_u8(0x01);
                    self.code.emit_u8(0xFE); // add rsi,rdi
                }
                _ => self.emit_jit_mem_effective_address_addr32_general(addr)?,
            }
        } else {
            match addr {
                Address::Direct(b) => {
                    let b = self.jit_arch_enc(*b)?;
                    self.emit_struct_mov(PhysReg::Rax, 6, (b as i32) * 8, false);
                }
                Address::BaseOffset { base, offset, .. } => {
                    let b = self.jit_arch_enc(*base)?;
                    self.emit_struct_mov(PhysReg::Rax, 6, (b as i32) * 8, false);
                    self.emit_add_rsi_imm(*offset)?;
                }
                Address::BaseIndexScale {
                    base,
                    index,
                    scale,
                    disp,
                    ..
                } => {
                    match base {
                        Some(b) => {
                            let b = self.jit_arch_enc(*b)?;
                            self.emit_struct_mov(PhysReg::Rax, 6, (b as i32) * 8, false);
                        }
                        None => {
                            // xor rsi, rsi  (48 31 F6)
                            self.code.emit_u8(0x48);
                            self.code.emit_u8(0x31);
                            self.code.emit_u8(0xF6);
                        }
                    }
                    let i = self.jit_arch_enc(*index)?;
                    self.emit_struct_mov(PhysReg::Rax, 7, (i as i32) * 8, false); // rdi = index
                    let sh = (*scale as u32).trailing_zeros() as u8; // 1->0,2->1,4->2,8->3
                    if sh > 0 {
                        // shl rdi, sh  (48 C1 E7 ib)
                        self.code.emit_u8(0x48);
                        self.code.emit_u8(0xC1);
                        self.code.emit_u8(0xE7);
                        self.code.emit_u8(sh);
                    }
                    // add rsi, rdi  (48 01 FE)
                    self.code.emit_u8(0x48);
                    self.code.emit_u8(0x01);
                    self.code.emit_u8(0xFE);
                    self.emit_add_rsi_imm(*disp as i64)?;
                }
                Address::Absolute(a) => self.emit_movabs(6, *a),
                Address::PcRel { offset, base, .. } => {
                    let b = base.ok_or_else(|| LowerError::UnsupportedOp {
                        op: "jit-mem: pcrel without base".to_string(),
                    })?;
                    self.emit_movabs(6, b.wrapping_add(*offset as u64));
                }
                Address::SegmentRel {
                    segment,
                    base,
                    index,
                    scale,
                    disp,
                } => {
                    // [segment_base + base + index*scale + disp]. The segment base is
                    // not a GPR, so it is read from a dedicated GuestRegs slot
                    // (fs_base / gs_base) rather than a gpr[] slot.
                    let seg_off: i32 = match segment {
                        VReg::Arch(ArchReg::X86(X86Reg::FsBase)) => X86_GUEST_FS_BASE_OFFSET,
                        VReg::Arch(ArchReg::X86(X86Reg::GsBase)) => X86_GUEST_GS_BASE_OFFSET,
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: "jit-mem: SegmentRel with non-FS/GS segment".to_string(),
                            });
                        }
                    };
                    self.emit_struct_mov(PhysReg::Rax, 6, seg_off, false); // rsi = seg base
                    if let Some(b) = base {
                        let b = self.jit_arch_enc(*b)?;
                        self.emit_struct_mov(PhysReg::Rax, 7, (b as i32) * 8, false); // rdi = base
                        // add rsi, rdi  (48 01 FE)
                        self.code.emit_u8(0x48);
                        self.code.emit_u8(0x01);
                        self.code.emit_u8(0xFE);
                    }
                    if let Some(idx) = index {
                        let i = self.jit_arch_enc(*idx)?;
                        self.emit_struct_mov(PhysReg::Rax, 7, (i as i32) * 8, false); // rdi = index
                        let sh = (*scale as u32).trailing_zeros() as u8;
                        if sh > 0 {
                            // shl rdi, sh  (48 C1 E7 ib)
                            self.code.emit_u8(0x48);
                            self.code.emit_u8(0xC1);
                            self.code.emit_u8(0xE7);
                            self.code.emit_u8(sh);
                        }
                        // add rsi, rdi  (48 01 FE)
                        self.code.emit_u8(0x48);
                        self.code.emit_u8(0x01);
                        self.code.emit_u8(0xFE);
                    }
                    self.emit_add_rsi_imm(*disp)?;
                }
                _ => {
                    return Err(LowerError::UnsupportedOp {
                        op: "jit-mem: unsupported address form".to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// General state-backed x86 addr32 builder. The special RDI-only cases in
    /// `emit_jit_mem_effective_address` retain their established byte sequences;
    /// this path covers every other architectural GPR, SIB shape, absolute
    /// offset, and FS/GS-relative form.
    fn emit_jit_mem_effective_address_addr32_general(
        &mut self,
        addr: &Address,
    ) -> Result<(), LowerError> {
        match addr {
            Address::X86Addr32(_) => {
                return Err(LowerError::InvalidOperand {
                    op: "jit-mem addr32".to_string(),
                    operand: "nested explicit addr32 address".to_string(),
                });
            }
            Address::Direct(base) => {
                self.emit_jit_addr32_offset(Some(*base), None, 1, 0)?;
            }
            Address::BaseOffset { base, offset, .. } => {
                self.emit_jit_addr32_offset(Some(*base), None, 1, *offset)?;
            }
            Address::BaseIndexScale {
                base,
                index,
                scale,
                disp,
                ..
            } => {
                self.emit_jit_addr32_offset(*base, Some(*index), *scale, i64::from(*disp))?;
            }
            Address::Absolute(offset) => {
                self.code.emit_u8(0xBE); // mov esi, imm32
                self.code.emit_u32(*offset as u32);
            }
            Address::SegmentRel {
                segment,
                base,
                index,
                scale,
                disp,
            } => {
                let segment_offset = match segment {
                    VReg::Arch(ArchReg::X86(X86Reg::FsBase)) => X86_GUEST_FS_BASE_OFFSET,
                    VReg::Arch(ArchReg::X86(X86Reg::GsBase)) => X86_GUEST_GS_BASE_OFFSET,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "jit-mem addr32: non-FS/GS segment".to_string(),
                        });
                    }
                };
                self.emit_jit_addr32_offset(*base, *index, *scale, *disp)?;
                self.emit_struct_mov(PhysReg::Rax, 7, segment_offset, false);
                self.code.emit_u8(0x48);
                self.code.emit_u8(0x01);
                self.code.emit_u8(0xFE); // add rsi,rdi after offset zero-extension
            }
            Address::PcRel { .. } | Address::GpRel { .. } => {
                return Err(LowerError::UnsupportedOp {
                    op: "jit-mem addr32: unsupported address form".to_string(),
                });
            }
        }
        Ok(())
    }

    /// Evaluate `base + index*scale + displacement` modulo 2^32 into RSI.
    fn emit_jit_addr32_offset(
        &mut self,
        base: Option<VReg>,
        index: Option<VReg>,
        scale: u8,
        disp: i64,
    ) -> Result<(), LowerError> {
        if !matches!(scale, 1 | 2 | 4 | 8) {
            return Err(LowerError::UnsupportedOp {
                op: format!("jit-mem addr32: invalid scale {scale}"),
            });
        }

        if let Some(base) = base {
            let guest_index = self.jit_arch_enc(base)?;
            self.emit_struct_mov(PhysReg::Rax, 6, i32::from(guest_index) * 8, false);
            self.code.emit_u8(0x89);
            self.code.emit_u8(0xF6); // mov esi,esi
        } else {
            self.code.emit_u8(0x31);
            self.code.emit_u8(0xF6); // xor esi,esi
        }

        if let Some(index) = index {
            let guest_index = self.jit_arch_enc(index)?;
            self.emit_struct_mov(PhysReg::Rax, 7, i32::from(guest_index) * 8, false);
            self.code.emit_u8(0x89);
            self.code.emit_u8(0xFF); // mov edi,edi
            let shift = scale.trailing_zeros() as u8;
            if shift != 0 {
                self.code.emit_u8(0xC1);
                self.code.emit_u8(0xE7);
                self.code.emit_u8(shift); // shl edi, shift
            }
            self.code.emit_u8(0x01);
            self.code.emit_u8(0xFE); // add esi,edi
        }

        if disp != 0 {
            self.code.emit_u8(0x81);
            self.code.emit_u8(0xC6);
            self.code.emit_u32(disp as u32); // add esi, imm32
        }
        Ok(())
    }
}
