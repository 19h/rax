//! AArch64 guest-address construction for the x86-64 cross lowerer.

use super::*;

impl Aarch64X86_64Lowerer {
    pub(super) fn load_addr_to(&mut self, addr: &Address, dst: PhysReg) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => self.load_vreg_to(*base, dst, OpWidth::W64)?,
            Address::BaseOffset { base, offset, .. } => {
                self.load_vreg_to(*base, dst, OpWidth::W64)?;
                self.emit_add_i64_to_reg(dst, *offset);
            }
            Address::BaseIndexScale {
                base,
                index,
                scale,
                disp,
                ..
            } => {
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    if let Some(base) = base {
                        drop(emitter);
                        self.load_vreg_to(*base, dst, OpWidth::W64)?;
                    } else {
                        emitter.emit_xor_rr(dst, dst, OpWidth::W64);
                    }
                }
                self.load_vreg_to(*index, B2, OpWidth::W64)?;
                match scale {
                    1 => {}
                    2 | 4 | 8 => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_shl_ri(B2, scale.trailing_zeros() as u8, OpWidth::W64);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 memory scale {scale}"),
                        });
                    }
                }
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_add_rr(dst, B2, OpWidth::W64);
                }
                self.emit_add_i64_to_reg(dst, i64::from(*disp));
            }
            Address::Absolute(addr) => self.emit_mov_imm(dst, *addr as i64, OpWidth::W64),
            Address::PcRel { offset, base, .. } => {
                let addr = base.unwrap_or(0).wrapping_add(*offset as u64);
                self.emit_mov_imm(dst, addr as i64, OpWidth::W64);
            }
            Address::X86Addr32(_) | Address::GpRel { .. } | Address::SegmentRel { .. } => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 memory address {addr:?}"),
                });
            }
        }
        Ok(())
    }
}
