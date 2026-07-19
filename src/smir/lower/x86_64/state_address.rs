//! State-backed x86 guest effective-address construction.

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::types::{Address, ArchReg, OpWidth, VReg, X86Reg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_GUEST_FS_BASE_OFFSET, X86_GUEST_GS_BASE_OFFSET};

impl X86_64Lowerer {
    /// Materialize one validated x86 guest effective address into RSI from the
    /// coherent GuestRegs snapshot at RAX. RDI is scratch. Ordinary addresses
    /// wrap modulo 2^64; explicit addr32 addresses wrap their offset modulo
    /// 2^32 and add an optional full-width FS/GS base afterwards.
    pub(crate) fn emit_x86_state_address_rsi(&mut self, addr: &Address) -> Result<(), LowerError> {
        if matches!(addr, Address::X86Addr32(_)) {
            return self.emit_jit_mem_effective_address(addr, false);
        }

        let load_gpr = |this: &mut Self, dst_enc: u8, reg: VReg| -> Result<(), LowerError> {
            let index = Self::x86_gpr_index(reg).ok_or_else(|| LowerError::UnsupportedOp {
                op: "X86CheckAlignment with non-GPR address operand".to_string(),
            })?;
            this.emit_struct_mov(PhysReg::Rax, dst_enc, i32::from(index) * 8, false);
            Ok(())
        };
        let add_rdi_to_rsi = |this: &mut Self| {
            this.code.emit_u8(0x48);
            this.code.emit_u8(0x01);
            this.code.emit_u8(0xFE); // add rsi, rdi
        };
        let shift_rdi = |this: &mut Self, scale: u8| -> Result<(), LowerError> {
            let shift = match scale {
                1 => 0,
                2 => 1,
                4 => 2,
                8 => 3,
                _ => {
                    return Err(LowerError::InvalidOperand {
                        op: "X86CheckAlignment".to_string(),
                        operand: format!("invalid address scale {scale}"),
                    });
                }
            };
            if shift != 0 {
                this.code.emit_u8(0x48);
                this.code.emit_u8(0xC1);
                this.code.emit_u8(0xE7);
                this.code.emit_u8(shift); // shl rdi, imm8
            }
            Ok(())
        };

        match addr {
            Address::X86Addr32(_) => {
                return Err(LowerError::InvalidOperand {
                    op: "X86CheckAlignment".to_string(),
                    operand: "nested explicit addr32 address".to_string(),
                });
            }
            Address::Direct(base) => load_gpr(self, 6, *base)?,
            Address::BaseOffset { base, offset, .. } => {
                load_gpr(self, 6, *base)?;
                self.emit_add_rsi_wrapping_i64(*offset);
            }
            Address::BaseIndexScale {
                base,
                index,
                scale,
                disp,
                ..
            } => {
                if let Some(base) = base {
                    load_gpr(self, 6, *base)?;
                } else {
                    self.code.emit_u8(0x48);
                    self.code.emit_u8(0x31);
                    self.code.emit_u8(0xF6); // xor rsi, rsi
                }
                load_gpr(self, 7, *index)?;
                shift_rdi(self, *scale)?;
                add_rdi_to_rsi(self);
                self.emit_add_rsi_wrapping_i64(i64::from(*disp));
            }
            Address::PcRel {
                offset,
                base: Some(base),
                ..
            } => self.emit_movabs(6, base.wrapping_add(*offset as u64)),
            Address::Absolute(address) => self.emit_movabs(6, *address),
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
                        return Err(LowerError::InvalidOperand {
                            op: "X86CheckAlignment".to_string(),
                            operand: "segment must be FS_BASE or GS_BASE".to_string(),
                        });
                    }
                };
                self.emit_struct_mov(PhysReg::Rax, 6, segment_offset, false);
                if let Some(base) = base {
                    load_gpr(self, 7, *base)?;
                    add_rdi_to_rsi(self);
                }
                if let Some(index) = index {
                    load_gpr(self, 7, *index)?;
                    shift_rdi(self, *scale)?;
                    add_rdi_to_rsi(self);
                } else if !matches!(scale, 1 | 2 | 4 | 8) {
                    return Err(LowerError::InvalidOperand {
                        op: "X86CheckAlignment".to_string(),
                        operand: format!("invalid address scale {scale}"),
                    });
                }
                self.emit_add_rsi_wrapping_i64(*disp);
            }
            Address::PcRel { base: None, .. } | Address::GpRel { .. } => {
                return Err(LowerError::UnsupportedOp {
                    op: "X86CheckAlignment with unresolved address".to_string(),
                });
            }
        }
        Ok(())
    }
}
