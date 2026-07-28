//! Native AArch64 lowering for x86 BEXTR.

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::types::{ArchReg, ArmReg, OpWidth, VReg};
use crate::smir::lower::LowerError;

use super::Aarch64Lowerer;

impl Aarch64Lowerer {
    /// Materialize BEXTR's exact x86 status-flag contract on NZCV. Logical
    /// AArch64 flag production supplies Z=zero and clears C/V, while N must be
    /// retained because x86 BEXTR leaves SF undefined and the interpreter's
    /// deterministic policy preserves its incoming value.
    pub(crate) fn lower_bextr_result_flags(
        &mut self,
        dst: u8,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let scratches = Self::scratch_regs(&[dst], 2)?;
        let saved = scratches[0];
        let produced = scratches[1];
        self.emit_scratch_save(&scratches);
        self.emit_sysreg(saved, ArmReg::Nzcv, true)?;
        self.lower_bmi_result_flags(dst, width, false)?;
        self.emit_sysreg(produced, ArmReg::Nzcv, true)?;
        self.emit_merge_requested_nzcv(
            saved,
            produced,
            FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF),
        )?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lower_bextr_register_control(
        &mut self,
        dst: VReg,
        src: VReg,
        control: VReg,
        width: OpWidth,
        emit_width: OpWidth,
        bits: u32,
        set_flags: bool,
    ) -> Result<(), LowerError> {
        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        let src = Self::gpr_arm_or_x86(src)?;
        let control = Self::gpr_arm_or_x86(control)?;
        let scratches = Self::scratch_regs(&[dst, src, control], 3)?;
        let start = scratches[0];
        let len = scratches[1];
        let mask = scratches[2];
        self.emit_scratch_save(&scratches);

        self.emit_bitfield(start, control, 0b10, 0, 7, OpWidth::W32)?;
        self.emit_bitfield(len, control, 0b10, 8, 15, OpWidth::W32)?;

        let zero_len = self.code.position();
        self.emit(0xb400_0000 | u32::from(len));

        let guard_start_bit = bits.trailing_zeros();
        let mut zero_start = Vec::with_capacity((8 - guard_start_bit) as usize);
        for bit in guard_start_bit..8 {
            let offset = self.code.position();
            self.emit_test_branch(start, bit, true, 0)?;
            zero_start.push((offset, bit));
        }

        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            self.emit_bitfield(dst, src, 0b10, 0, bits - 1, OpWidth::W32)?;
            self.emit_dp2(dst, dst, start, 0b1001, OpWidth::W32)?;
        } else {
            self.emit_dp2(dst, src, start, 0b1001, emit_width)?;
        }

        let mut skip_mask = Vec::with_capacity((8 - guard_start_bit) as usize);
        for bit in guard_start_bit..8 {
            let offset = self.code.position();
            self.emit_test_branch(len, bit, true, 0)?;
            skip_mask.push((offset, bit));
        }

        self.emit_movn_zero(mask, emit_width)?;
        self.emit_dp2(mask, mask, len, 0b1000, emit_width)?;
        self.emit_logic_reg_n(dst, dst, mask, 0b00, true, emit_width)?;
        for (offset, bit) in skip_mask {
            self.patch_test_branch_to_current(offset, len, bit, true)?;
        }
        if set_flags {
            self.lower_bextr_result_flags(dst, emit_width)?;
        }
        let end_branch = self.code.position();
        self.emit(0x1400_0000);

        self.patch_compare_branch_to_current(zero_len, len, false)?;
        for (offset, bit) in zero_start {
            self.patch_test_branch_to_current(offset, start, bit, true)?;
        }
        self.emit_mov_imm(dst, 0, emit_width)?;
        if set_flags {
            self.lower_bextr_result_flags(dst, emit_width)?;
        }
        self.patch_branch_to_current(end_branch)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_bextr(
        &mut self,
        dst: VReg,
        src: VReg,
        control: VReg,
        width: OpWidth,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        let set_flags = flags.updates_any();
        let emit_width = match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
            OpWidth::W64 => OpWidth::W64,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native Bextr width {other:?}"),
                });
            }
        };
        if let VReg::Imm(value) = src {
            if (value as u64 & width.mask()) == 0 {
                let dst = Self::dst_gpr_arm_or_x86(dst)?;
                self.emit_mov_imm(dst, 0, emit_width)?;
                if set_flags {
                    self.lower_bextr_result_flags(dst, emit_width)?;
                }
                return Ok(());
            }
        }
        let bits = width.bits();
        let control = match control {
            VReg::Imm(value) => value as u64,
            other => {
                return self.lower_bextr_register_control(
                    dst, src, other, width, emit_width, bits, set_flags,
                );
            }
        };
        let start = (control & 0xff) as u32;
        let len = ((control >> 8) & 0xff) as u32;
        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        if let VReg::Imm(value) = src {
            let src = (value as u64) & width.mask();
            let result = if start >= bits || len == 0 {
                0
            } else {
                let shifted = src >> start;
                if len >= bits {
                    shifted
                } else {
                    shifted & ((1_u64 << len) - 1)
                }
            } & width.mask();
            if !self.try_emit_movn_single(dst, result, emit_width)? {
                self.emit_mov_imm(dst, result as i64, emit_width)?;
            }
            if set_flags {
                self.lower_bextr_result_flags(dst, emit_width)?;
            }
            return Ok(());
        }

        if start >= bits || len == 0 {
            self.emit_mov_imm(dst, 0, emit_width)?;
            if set_flags {
                self.lower_bextr_result_flags(dst, emit_width)?;
            }
            return Ok(());
        }

        let width_bits = len.min(bits - start) as u8;
        self.lower_bfx(
            VReg::Arch(ArchReg::Arm(ArmReg::X(dst))),
            src,
            start as u8,
            width_bits,
            false,
            emit_width,
        )?;
        if set_flags {
            self.lower_bextr_result_flags(dst, emit_width)?;
        }
        Ok(())
    }
}
