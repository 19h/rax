//! Intel APX CCMP/CTEST direct execution.

use super::*;

impl X86_64Vcpu {
    fn apx_conditional_opcode_fields_valid(ctx: &InsnContext, opcode: u8) -> Result<bool> {
        let evex = ctx
            .evex
            .ok_or_else(|| Error::Emulator("EVEX context missing".to_string()))?;
        let pp_valid = match opcode {
            0x38 | 0x3A | 0x80 | 0x84 | 0xF6 => evex.pp == 0,
            0x39 | 0x3B | 0x81 | 0x83 | 0x85 | 0xF7 => evex.pp <= 1,
            _ => false,
        };
        Ok(pp_valid && !evex.z && evex.ll == 0 && !evex.nd)
    }

    fn apx_conditional_modrm_fields_valid(ctx: &InsnContext, modrm: u8) -> Result<bool> {
        let evex = ctx
            .evex
            .ok_or_else(|| Error::Emulator("EVEX context missing".to_string()))?;
        Ok(modrm >> 6 != 3 || evex.x4)
    }

    fn validate_apx_conditional_encoding(
        &mut self,
        ctx: &InsnContext,
        opcode: u8,
        modrm: Option<u8>,
    ) -> Result<bool> {
        if !Self::apx_conditional_opcode_fields_valid(ctx, opcode)? {
            let _ = self.inject_invalid_opcode()?;
            return Ok(false);
        }
        if let Some(modrm) = modrm {
            if !Self::apx_conditional_modrm_fields_valid(ctx, modrm)? {
                let _ = self.inject_invalid_opcode()?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn apx_ccmp_condition_and_default_flags(ctx: &InsnContext) -> Result<(u8, u8)> {
        let evex = ctx
            .evex
            .ok_or_else(|| Error::Emulator("EVEX context missing".to_string()))?;
        let scc = ((evex.v_prime as u8) << 3) | evex.aaa;
        Ok((scc, evex.vvvv))
    }

    pub(crate) fn apply_apx_ccmp_default_flags(&mut self, dfv: u8) {
        let mut rflags = self.regs.rflags & !0x8D5; // CF, PF, AF, ZF, SF, OF
        if dfv & 0x1 != 0 {
            rflags |= 0x005; // CF and PF
        }
        if dfv & 0x2 != 0 {
            rflags |= 0x040;
        }
        if dfv & 0x4 != 0 {
            rflags |= 0x080;
        }
        if dfv & 0x8 != 0 {
            rflags |= 0x800;
        }
        self.regs.rflags = rflags;
        self.clear_lazy_flags();
    }

    fn check_apx_scc(&mut self, scc: u8) -> bool {
        match scc & 0x0F {
            0x0A => true,
            0x0B => false,
            cc => self.check_condition(cc),
        }
    }

    fn apply_apx_ccmp_result_flags(&mut self, result: u64, src1: u64, src2: u64, op_size: u8) {
        flags::update_flags_sub(&mut self.regs.rflags, src1, src2, result, op_size);
        self.clear_lazy_flags();
    }

    /// APX CCMP register/memory operation.
    pub(crate) fn execute_apx_ccmp(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        if !self.validate_apx_conditional_encoding(ctx, opcode, None)? {
            return Ok(None);
        }
        let modrm_byte = ctx.peek_u8()?;
        if !self.validate_apx_conditional_encoding(ctx, opcode, Some(modrm_byte))? {
            return Ok(None);
        }

        let op_size = if opcode & 1 == 0 {
            1
        } else {
            Self::apx_scalar_op_size(ctx)
        };
        let reg_is_src = opcode & 2 == 0;
        let (scc, dfv) = Self::apx_ccmp_condition_and_default_flags(ctx)?;
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let reg = reg | ctx.evex_dest_reg();
        let rm = if is_memory {
            rm
        } else {
            rm | ctx.evex_rm_reg()
        };

        // A false SCC suppresses the comparison, not operand faults.
        let rm_value = if is_memory {
            self.read_mem(addr, op_size)?
        } else {
            self.get_reg(rm, op_size)
        };
        let reg_value = self.get_reg(reg, op_size);
        let (src1, src2) = if reg_is_src {
            (rm_value, reg_value)
        } else {
            (reg_value, rm_value)
        };

        if self.check_apx_scc(scc) {
            let result = src1.wrapping_sub(src2);
            self.apply_apx_ccmp_result_flags(result, src1, src2, op_size);
        } else {
            self.apply_apx_ccmp_default_flags(dfv);
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// APX CTEST register/memory operation.
    pub(crate) fn execute_apx_ctest(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        if !self.validate_apx_conditional_encoding(ctx, opcode, None)? {
            return Ok(None);
        }
        let modrm_byte = ctx.peek_u8()?;
        if !self.validate_apx_conditional_encoding(ctx, opcode, Some(modrm_byte))? {
            return Ok(None);
        }

        let op_size = if opcode == 0x84 {
            1
        } else {
            Self::apx_scalar_op_size(ctx)
        };
        let (scc, dfv) = Self::apx_ccmp_condition_and_default_flags(ctx)?;
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let reg = reg | ctx.evex_dest_reg();
        let rm = if is_memory {
            rm
        } else {
            rm | ctx.evex_rm_reg()
        };

        let src1 = if is_memory {
            self.read_mem(addr, op_size)?
        } else {
            self.get_reg(rm, op_size)
        };
        let src2 = self.get_reg(reg, op_size);
        if self.check_apx_scc(scc) {
            let result = src1 & src2;
            self.update_flags_alu(result, src1, src2, op_size, ApxAluOp::And);
        } else {
            self.apply_apx_ccmp_default_flags(dfv);
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(crate) fn execute_apx_ccmp_imm(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        self.execute_apx_conditional_imm(ctx, opcode, true)
    }

    pub(crate) fn execute_apx_ctest_imm(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        self.execute_apx_conditional_imm(ctx, opcode, false)
    }

    fn execute_apx_conditional_imm(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
        is_compare: bool,
    ) -> Result<Option<VcpuExit>> {
        let modrm_byte = ctx.peek_u8()?;
        if !self.validate_apx_conditional_encoding(ctx, opcode, Some(modrm_byte))? {
            return Ok(None);
        }

        let op_size = if matches!(opcode, 0x80 | 0xF6) {
            1
        } else {
            Self::apx_scalar_op_size(ctx)
        };
        let (_, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let rm = if is_memory {
            rm
        } else {
            rm | ctx.evex_rm_reg()
        };
        // As for /r forms, the source access is unconditional on SCC.
        let src = if is_memory {
            self.read_mem(addr, op_size)?
        } else {
            self.get_reg(rm, op_size)
        };
        let imm = match opcode {
            0x80 | 0xF6 => ctx.consume_u8()? as u64,
            0x81 | 0xF7 if op_size == 2 => ctx.consume_u16()? as u64,
            0x81 | 0xF7 if op_size == 8 => ctx.consume_u32()? as i32 as i64 as u64,
            0x81 | 0xF7 => ctx.consume_u32()? as u64,
            0x83 => ctx.consume_u8()? as i8 as i64 as u64,
            _ => unreachable!("APX conditional immediate dispatch opcode"),
        };
        let (scc, dfv) = Self::apx_ccmp_condition_and_default_flags(ctx)?;
        if self.check_apx_scc(scc) {
            if is_compare {
                let result = src.wrapping_sub(imm);
                self.apply_apx_ccmp_result_flags(result, src, imm, op_size);
            } else {
                let result = src & imm;
                self.update_flags_alu(result, src, imm, op_size, ApxAluOp::And);
            }
        } else {
            self.apply_apx_ccmp_default_flags(dfv);
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }
}
