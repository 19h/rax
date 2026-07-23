//! apx.rs

use crate::error::{Error, Result};
use crate::isa::x86_64::decode::dispatch::evex::*;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::{execute, flags};

impl X86_64Vcpu {
    // ============================================================================
    // APX EVEX-MAP4 Instruction Implementations (GPR Instructions)
    // ============================================================================

    /// EVEX MAP4 opcode map (mm=4) - APX GPR instructions
    /// APX extends EVEX encoding to support:
    /// - EGPR (R16-R31) via B4, X4, R4 bits
    /// - NDD (New Data Destination) - 3-operand forms where vvvv is destination
    /// - NF (No Flags) - arithmetic without updating RFLAGS
    pub(crate) fn execute_evex_map4_apx(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx
            .evex
            .ok_or_else(|| Error::Emulator("EVEX context missing".to_string()))?;

        // APX uses ND (New Data Destination) for 3-operand forms
        // and NF (No Flags) for flag-suppressing variants
        let ndd = evex.nd; // 3-operand form
        let nf = evex.nf; // No flags update

        match opcode {
            // ADD variants (0x00-0x03)
            0x00 | 0x01 | 0x02 | 0x03 => self.execute_apx_alu(ctx, opcode, ndd, nf, ApxAluOp::Add),

            // OR variants (0x08-0x0B)
            0x08 | 0x09 | 0x0A | 0x0B => self.execute_apx_alu(ctx, opcode, ndd, nf, ApxAluOp::Or),

            // Intel APX revision 7.0 specifies NF=0 for ADC. Reject from the
            // opcode alone, before observing ModR/M or an apparent operand.
            0x10 | 0x11 | 0x12 | 0x13 if nf => self.inject_invalid_opcode(),

            // ADC variants (0x10-0x13)
            0x10 | 0x11 | 0x12 | 0x13 => self.execute_apx_alu(ctx, opcode, ndd, nf, ApxAluOp::Adc),

            // Intel APX revision 7.0 likewise specifies NF=0 for SBB.
            0x18 | 0x19 | 0x1A | 0x1B if nf => self.inject_invalid_opcode(),

            // SBB variants (0x18-0x1B)
            0x18 | 0x19 | 0x1A | 0x1B => self.execute_apx_alu(ctx, opcode, ndd, nf, ApxAluOp::Sbb),

            // AND variants (0x20-0x23)
            0x20 | 0x21 | 0x22 | 0x23 => self.execute_apx_alu(ctx, opcode, ndd, nf, ApxAluOp::And),

            // SUB variants (0x28-0x2B)
            0x28 | 0x29 | 0x2A | 0x2B => self.execute_apx_alu(ctx, opcode, ndd, nf, ApxAluOp::Sub),

            // XOR variants (0x30-0x33)
            0x30 | 0x31 | 0x32 | 0x33 => self.execute_apx_alu(ctx, opcode, ndd, nf, ApxAluOp::Xor),

            // CCMP variants (0x38-0x3B)
            0x38 | 0x39 | 0x3A | 0x3B => self.execute_apx_ccmp(ctx, opcode),

            // MAP4 conditionals: SETZUcc, EVEX SETcc, CMOV_ND, and CFCMOV.
            0x40..=0x4F => self.execute_apx_conditional_map4(ctx, opcode & 0x0F),

            // CTEST variants (0x84-0x85)
            0x84 | 0x85 => self.execute_apx_ctest(ctx, opcode),

            // APX-promoted MOVBE load/store directions.
            0x60 | 0x61 => self.execute_apx_movbe(ctx, opcode),

            // MAP4 65 is WRUSS. MAP4 66 is WRSS unless its prefix fields
            // select the APX ADCX/ADOX NDD forms. RAX never enumerates CET
            // shadow stacks, so every WRSS/WRUSS encoding is a static #UD.
            0x65 => self.inject_invalid_opcode(),
            0x66 if !ndd || nf || evex.z || !matches!(evex.pp, 1 | 2) => {
                self.inject_invalid_opcode()
            }

            // APX-promoted POPCNT, with optional NF.
            0x88 => self.execute_apx_count(ctx, opcode),

            // APX-promoted MOVRS memory loads.
            0x8A | 0x8B => self.execute_apx_movrs(ctx, opcode),

            // POP2 (0x8F)
            0x8F => self.execute_apx_pop2(ctx),

            // IMUL (0x69, 0x6B)
            0x69 => self.execute_apx_imul_imm(ctx, ndd, nf, true),
            0x6B => self.execute_apx_imul_imm(ctx, ndd, nf, false),
            0xAF => self.execute_apx_imul(ctx, ndd, nf),

            // SHLD/SHRD double shifts (0x24, 0x2C imm8; 0xA5, 0xAD CL)
            0x24 | 0x2C | 0xA5 | 0xAD => self.execute_apx_double_shift(ctx, opcode, ndd, nf),

            // Group 1 immediate ALU operations (0x80, 0x81, 0x83 /0..7).
            0x80 | 0x81 | 0x83 => self.execute_apx_group1_imm(ctx, opcode, ndd, nf),

            // Legacy opcode 82H is not promoted into APX map 4.
            0x82 => self.inject_invalid_opcode(),

            // Shift variants (0xC0, 0xC1, 0xD0-0xD3)
            0xC0 | 0xC1 => self.execute_apx_shift_imm(ctx, opcode, ndd, nf),
            0xD0 | 0xD1 | 0xD2 | 0xD3 => self.execute_apx_shift_cl(ctx, opcode, ndd, nf),

            // TZCNT/LZCNT with NF
            0xF4 | 0xF5 => self.execute_apx_count(ctx, opcode),

            // APX-promoted CRC32
            0xF0 | 0xF1 => self.execute_apx_crc32(ctx, opcode),

            // APX-promoted INVPCID
            0xF2 => self.execute_apx_invpcid(ctx),

            // APX-promoted direct stores
            0xF8 if evex.pp == 1 => self.execute_apx_movdir64b(ctx),
            0xF9 => self.execute_apx_movdiri(ctx),

            // Group 3 NOT/NEG (0xF6, 0xF7 /2,/3)
            0xF6 | 0xF7 => self.execute_apx_group3(ctx, opcode, ndd, nf),

            // INC/DEC (0xFE, 0xFF /0,/1) and PUSH2 (0xFF /6)
            0xFE | 0xFF => self.execute_apx_group_ff(ctx, opcode, ndd, nf),

            // NP is unassigned. F2/F3 select ENQCMD/ENQCMDS or
            // URDMSR/UWRMSR, but RAX advertises neither ENQCMD nor USER_MSR.
            0xF8 => self.inject_invalid_opcode(),

            // This assigned family does not yet have a direct executor.
            0xFC => Err(Error::Emulator(format!(
                "Unimplemented APX MAP4 opcode {:#x} at RIP={:#x}",
                opcode, self.regs.rip
            ))),

            // Intel APX revision 7.0's MAP4 assignments (section 3.1.5 plus
            // section 6.38 MOVRS) are exhaustively dispatched above. Every
            // remaining opcode byte faults before a ModR/M byte is observed.
            _ => self.inject_invalid_opcode(),
        }
    }

    /// Generic APX ALU operation with NDD and NF support
    pub(crate) fn execute_apx_alu(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
        ndd: bool,
        nf: bool,
        alu_op: ApxAluOp,
    ) -> Result<Option<VcpuExit>> {
        // Determine operand size from opcode and EVEX.W
        let is_byte = (opcode & 0x01) == 0;
        let op_size = if is_byte {
            1
        } else {
            Self::apx_scalar_op_size(ctx)
        };

        // Determine direction (reg->rm or rm->reg)
        let reg_is_src = (opcode & 0x02) == 0;

        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        // Apply EVEX register extensions for EGPR (R16-R31)
        let reg = reg | ctx.evex_dest_reg();
        let rm = if is_memory {
            rm
        } else {
            rm | ctx.evex_rm_reg()
        };

        // Get source values
        let (src1, src2) = if reg_is_src {
            let r_val = self.get_reg(reg, op_size);
            let rm_val = if is_memory {
                self.read_mem(addr, op_size)?
            } else {
                self.get_reg(rm, op_size)
            };
            (rm_val, r_val)
        } else {
            let r_val = self.get_reg(reg, op_size);
            let rm_val = if is_memory {
                self.read_mem(addr, op_size)?
            } else {
                self.get_reg(rm, op_size)
            };
            (r_val, rm_val)
        };

        if matches!(alu_op, ApxAluOp::Adc | ApxAluOp::Sbb) {
            self.materialize_flags();
        }

        // Perform ALU operation
        let cf_in = (self.regs.rflags & 0x001) != 0;
        let cf_val = u64::from(cf_in);
        let result = match alu_op {
            ApxAluOp::Add => src1.wrapping_add(src2),
            ApxAluOp::Adc => src1.wrapping_add(src2).wrapping_add(cf_val),
            ApxAluOp::Or => src1 | src2,
            ApxAluOp::And => src1 & src2,
            ApxAluOp::Sub => src1.wrapping_sub(src2),
            ApxAluOp::Sbb => src1.wrapping_sub(src2).wrapping_sub(cf_val),
            ApxAluOp::Xor => src1 ^ src2,
        };

        // Determine destination
        if ndd {
            // NDD mode: destination is from vvvv field
            let dest = ctx.evex_vvvv();
            self.set_reg(dest, result, op_size);
        } else if reg_is_src {
            // Destination is r/m
            if is_memory {
                self.write_mem(addr, result, op_size)?;
            } else {
                self.set_reg(rm, result, op_size);
            }
        } else {
            // Destination is reg
            self.set_reg(reg, result, op_size);
        }

        // Update flags unless NF is set
        if !nf {
            match alu_op {
                ApxAluOp::Adc => {
                    flags::update_flags_adc(
                        &mut self.regs.rflags,
                        src1,
                        src2,
                        cf_in,
                        result,
                        op_size,
                    );
                    self.clear_lazy_flags();
                }
                ApxAluOp::Sbb => {
                    flags::update_flags_sbb(
                        &mut self.regs.rflags,
                        src1,
                        src2,
                        cf_in,
                        result,
                        op_size,
                    );
                    self.clear_lazy_flags();
                }
                _ => self.update_flags_alu(result, src1, src2, op_size, alu_op),
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// APX SETZUcc operation.
    pub(crate) fn execute_apx_setzucc(
        &mut self,
        ctx: &mut InsnContext,
        cc: u8,
    ) -> Result<Option<VcpuExit>> {
        let (_, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let value = if self.check_condition(cc) { 1 } else { 0 };

        if is_memory {
            self.write_mem(addr, value, 1)?;
        } else {
            let rm = rm | ctx.evex_rm_reg();
            self.set_reg(rm, value, 8);
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(crate) fn execute_apx_evex_setcc(
        &mut self,
        ctx: &mut InsnContext,
        cc: u8,
    ) -> Result<Option<VcpuExit>> {
        let (_, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let value = if self.check_condition(cc) { 1 } else { 0 };

        if is_memory {
            self.write_mem(addr, value, 1)?;
        } else {
            let rm = rm | ctx.evex_rm_reg();
            self.set_reg(rm, value, 1);
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(crate) fn execute_apx_conditional_map4(
        &mut self,
        ctx: &mut InsnContext,
        cc: u8,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx
            .evex
            .ok_or_else(|| Error::Emulator("EVEX context missing".to_string()))?;

        if evex.pp == 0x02 {
            return self.inject_invalid_opcode();
        }

        if evex.pp == 0x03 && !evex.nf {
            if evex.nd {
                self.execute_apx_setzucc(ctx, cc)
            } else {
                self.execute_apx_evex_setcc(ctx, cc)
            }
        } else {
            self.execute_apx_cmovcc(ctx, cc, evex.nd, evex.nf)
        }
    }

    pub(crate) fn execute_apx_cmovcc(
        &mut self,
        ctx: &mut InsnContext,
        cc: u8,
        ndd: bool,
        nf: bool,
    ) -> Result<Option<VcpuExit>> {
        let op_size = Self::apx_scalar_op_size(ctx);
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let reg = reg | ctx.evex_dest_reg();
        let rm = if is_memory {
            rm
        } else {
            rm | ctx.evex_rm_reg()
        };

        if ndd {
            let dst = ctx.evex_vvvv();
            let src1 = self.get_reg(reg, op_size);

            if nf {
                if self.check_condition(cc) {
                    let src2 = if is_memory {
                        self.read_mem(addr, op_size)?
                    } else {
                        self.get_reg(rm, op_size)
                    };
                    self.set_reg(dst, src2, op_size);
                } else {
                    self.set_reg(dst, src1, op_size);
                }
            } else {
                let src2 = if is_memory {
                    self.read_mem(addr, op_size)?
                } else {
                    self.get_reg(rm, op_size)
                };
                let result = if self.check_condition(cc) { src2 } else { src1 };
                self.set_reg(dst, result, op_size);
            }
        } else if nf {
            let src = self.get_reg(reg, op_size);

            if is_memory {
                if self.check_condition(cc) {
                    self.write_mem(addr, src, op_size)?;
                }
            } else {
                let result = if self.check_condition(cc) { src } else { 0 };
                self.set_reg(rm, result, op_size);
            }
        } else {
            let dst = reg;

            if self.check_condition(cc) {
                let src = if is_memory {
                    self.read_mem(addr, op_size)?
                } else {
                    self.get_reg(rm, op_size)
                };
                self.set_reg(dst, src, op_size);
            } else {
                self.set_reg(dst, 0, op_size);
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(crate) fn apx_scalar_op_size(ctx: &InsnContext) -> u8 {
        if ctx.evex_w() {
            8
        } else if ctx.operand_size_override {
            2
        } else {
            4
        }
    }

    /// APX POP2 - pop two registers with one aligned 16-byte stack transfer.
    pub(crate) fn execute_apx_pop2(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let modrm = ctx.consume_u8()?;
        let evex = ctx
            .evex
            .ok_or_else(|| Error::Emulator("EVEX context missing".to_string()))?;
        if (modrm >> 6) != 3
            || ((modrm >> 3) & 0x07) != 0
            || !evex.nd
            || evex.nf
            || evex.z
            || evex.ll != 0
            || evex.aaa != 0
            || evex.pp != 0
            || !evex.x4
        {
            return self.inject_invalid_opcode();
        }

        // Intel names the ModRM operand B and the VVVVV operand V. POP2 loads V
        // from [RSP] and B from [RSP+8]. RSP is forbidden and the destinations
        // must be distinct.
        let b_reg = (modrm & 0x07) | ctx.evex_rm_reg();
        let v_reg = ctx.evex_vvvv();
        if b_reg == 4 || v_reg == 4 || b_reg == v_reg {
            return self.inject_invalid_opcode();
        }
        if self.regs.rsp & 0xF != 0 {
            self.inject_exception(13, Some(0))?;
            return Ok(None);
        }

        let (low, high) = self.read_mem_pair(self.regs.rsp)?;
        self.regs.rsp = self.regs.rsp.wrapping_add(16);
        self.set_reg(v_reg, low, 8);
        self.set_reg(b_reg, high, 8);

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// APX PUSH2 - push two registers with both-or-neither fault visibility.
    pub(crate) fn execute_apx_push2(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let modrm = ctx.consume_u8()?;
        let evex = ctx
            .evex
            .ok_or_else(|| Error::Emulator("EVEX context missing".to_string()))?;
        if (modrm >> 6) != 3
            || ((modrm >> 3) & 0x07) != 6
            || !evex.nd
            || evex.nf
            || evex.z
            || evex.ll != 0
            || evex.aaa != 0
            || evex.pp != 0
            || !evex.x4
        {
            return self.inject_invalid_opcode();
        }

        let b_reg = (modrm & 0x07) | ctx.evex_rm_reg();
        let v_reg = ctx.evex_vvvv();
        if b_reg == 4 || v_reg == 4 {
            return self.inject_invalid_opcode();
        }
        if self.regs.rsp & 0xF != 0 {
            self.inject_exception(13, Some(0))?;
            return Ok(None);
        }

        // PUSH2 is equivalent to PUSH V followed by PUSH B, hence B occupies
        // the lower-address qword in the final 16-byte stack image.
        let low = self.get_reg(b_reg, 8);
        let high = self.get_reg(v_reg, 8);
        let new_rsp = self.regs.rsp.wrapping_sub(16);

        self.write_mem_pair(new_rsp, low, high)?;
        self.regs.rsp = new_rsp;
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// APX IMUL with immediate
    pub(crate) fn execute_apx_imul_imm(
        &mut self,
        ctx: &mut InsnContext,
        ndd: bool,
        nf: bool,
        imm32: bool,
    ) -> Result<Option<VcpuExit>> {
        let op_size = Self::apx_scalar_op_size(ctx);
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let reg = reg | ctx.evex_dest_reg();

        let src = if is_memory {
            self.read_mem(addr, op_size)?
        } else {
            let rm = rm | ctx.evex_rm_reg();
            self.get_reg(rm, op_size)
        };

        let imm = if imm32 && op_size == 2 {
            ctx.consume_u16()? as i16 as i64 as u64
        } else if imm32 {
            ctx.consume_u32()? as i32 as i64 as u64
        } else {
            ctx.consume_u8()? as i8 as i64 as u64
        };

        let result = match op_size {
            2 => (src as i16).wrapping_mul(imm as i16) as u16 as u64,
            4 => (src as i32).wrapping_mul(imm as i32) as u32 as u64,
            8 => (src as i64).wrapping_mul(imm as i64) as u64,
            _ => unreachable!(),
        };

        let dest_reg = if ndd { ctx.evex_vvvv() } else { reg };
        self.set_reg(dest_reg, result, op_size);

        if !nf {
            // Set OF/CF if result overflowed
            let sign_extended = match op_size {
                2 => (result as i16) as i32 == (src as i16 as i32) * (imm as i16 as i32),
                4 => (result as i32) as i64 == (src as i32 as i64) * (imm as i32 as i64),
                8 => (result as i64) as i128 == (src as i64 as i128) * (imm as i64 as i128),
                _ => unreachable!(),
            };
            let flags = self.regs.rflags & !(0x801); // Clear OF, CF
            self.regs.rflags = if sign_extended { flags } else { flags | 0x801 };
            self.clear_lazy_flags();
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// APX IMUL with register/memory source.
    pub(crate) fn execute_apx_imul(
        &mut self,
        ctx: &mut InsnContext,
        ndd: bool,
        nf: bool,
    ) -> Result<Option<VcpuExit>> {
        let op_size = Self::apx_scalar_op_size(ctx);
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let reg = reg | ctx.evex_dest_reg();

        let src1 = self.get_reg(reg, op_size);
        let src2 = if is_memory {
            self.read_mem(addr, op_size)?
        } else {
            let rm = rm | ctx.evex_rm_reg();
            self.get_reg(rm, op_size)
        };

        let (result, overflow) = match op_size {
            2 => {
                let product = (src1 as i16 as i32) * (src2 as i16 as i32);
                let result = product as i16 as u16 as u64;
                (result, product != result as i16 as i32)
            }
            4 => {
                let product = (src1 as i32 as i64) * (src2 as i32 as i64);
                let result = product as i32 as u32 as u64;
                (result, product != result as i32 as i64)
            }
            8 => {
                let product = (src1 as i64 as i128) * (src2 as i64 as i128);
                let result = product as i64 as u64;
                (result, product != result as i64 as i128)
            }
            _ => unreachable!(),
        };

        let dest_reg = if ndd { ctx.evex_vvvv() } else { reg };
        self.set_reg(dest_reg, result, op_size);

        if !nf {
            let flags = self.regs.rflags & !0x801; // Clear OF, CF
            self.regs.rflags = if overflow { flags | 0x801 } else { flags };
            self.clear_lazy_flags();
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// APX SHLD/SHRD double shifts.
    pub(crate) fn execute_apx_double_shift(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
        ndd: bool,
        nf: bool,
    ) -> Result<Option<VcpuExit>> {
        let op_size = Self::apx_scalar_op_size(ctx);
        let width = op_size as u32 * 8;
        let mask = if op_size == 8 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        let is_shrd = matches!(opcode, 0x2C | 0xAD);
        let count_mask = if op_size == 8 { 0x3F } else { 0x1F };

        if matches!(opcode, 0x24 | 0x2C) {
            ctx.rip_relative_offset = 1;
        }
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let src1_reg = rm | ctx.evex_rm_reg();
        let src2_reg = reg | ctx.evex_dest_reg();
        let src1 = if is_memory {
            self.read_mem(addr, op_size)?
        } else {
            self.get_reg(src1_reg, op_size)
        } & mask;
        let src2 = self.get_reg(src2_reg, op_size) & mask;
        let count = if matches!(opcode, 0x24 | 0x2C) {
            ctx.consume_u8()? & count_mask
        } else {
            (self.regs.rcx as u8) & count_mask
        };

        let defined = count != 0 && u32::from(count) <= width;
        let result = if !defined {
            src1
        } else {
            let count = count as u32;
            if is_shrd {
                ((src1 >> count) | (src2 << (width - count))) & mask
            } else {
                ((src1 << count) | (src2 >> (width - count))) & mask
            }
        };

        if ndd {
            self.set_reg(ctx.evex_vvvv(), result, op_size);
        } else if is_memory {
            self.write_mem(addr, result, op_size)?;
        } else {
            self.set_reg(src1_reg, result, op_size);
        }

        if !nf && defined {
            self.update_apx_double_shift_flags(result, src1, count, op_size, is_shrd);
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// APX group 1 immediate ALU operations.
    pub(crate) fn execute_apx_group1_imm(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
        ndd: bool,
        nf: bool,
    ) -> Result<Option<VcpuExit>> {
        let modrm = ctx.peek_u8()?;
        let op = (modrm >> 3) & 0x07;
        // Group 1 does not identify ADC/SBB until ModR/M.reg. NF=1 is #UD
        // before address generation, operand access, or immediate fetch.
        if nf && matches!(op, 2 | 3) {
            return self.inject_invalid_opcode();
        }
        if op == 7 {
            return self.execute_apx_ccmp_imm(ctx, opcode);
        }

        let op_size = if opcode == 0x80 {
            1
        } else {
            Self::apx_scalar_op_size(ctx)
        };

        let (_, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let src_reg = rm | ctx.evex_rm_reg();
        let src = if is_memory {
            self.read_mem(addr, op_size)?
        } else {
            self.get_reg(src_reg, op_size)
        };

        let imm = match opcode {
            0x80 => ctx.consume_u8()? as u64,
            0x81 if op_size == 2 => ctx.consume_u16()? as u64,
            0x81 if op_size == 8 => ctx.consume_u32()? as i32 as i64 as u64,
            0x81 => ctx.consume_u32()? as u64,
            0x83 => ctx.consume_u8()? as i8 as i64 as u64,
            _ => unreachable!(),
        };

        if matches!(op, 2 | 3) {
            self.materialize_flags();
        }
        let cf_in = (self.regs.rflags & 0x001) != 0;
        let result = match op {
            0 => src.wrapping_add(imm),
            1 => src | imm,
            2 => src.wrapping_add(imm).wrapping_add(u64::from(cf_in)),
            3 => src.wrapping_sub(imm).wrapping_sub(u64::from(cf_in)),
            4 => src & imm,
            5 => src.wrapping_sub(imm),
            6 => src ^ imm,
            _ => unreachable!(),
        };

        if ndd {
            self.set_reg(ctx.evex_vvvv(), result, op_size);
        } else if is_memory {
            self.write_mem(addr, result, op_size)?;
        } else {
            self.set_reg(src_reg, result, op_size);
        }

        if !nf {
            match op {
                0 => self.update_flags_alu(result, src, imm, op_size, ApxAluOp::Add),
                1 => self.update_flags_alu(result, src, imm, op_size, ApxAluOp::Or),
                2 => {
                    flags::update_flags_adc(&mut self.regs.rflags, src, imm, cf_in, result, op_size)
                }
                3 => {
                    flags::update_flags_sbb(&mut self.regs.rflags, src, imm, cf_in, result, op_size)
                }
                4 => self.update_flags_alu(result, src, imm, op_size, ApxAluOp::And),
                5 => self.update_flags_alu(result, src, imm, op_size, ApxAluOp::Sub),
                6 => self.update_flags_alu(result, src, imm, op_size, ApxAluOp::Xor),
                _ => unreachable!(),
            }
            self.clear_lazy_flags();
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// APX shift with immediate
    pub(crate) fn execute_apx_shift_imm(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
        ndd: bool,
        nf: bool,
    ) -> Result<Option<VcpuExit>> {
        let is_byte = opcode == 0xC0;
        let op_size = if is_byte {
            1
        } else {
            Self::apx_scalar_op_size(ctx)
        };

        let modrm = ctx.peek_u8()?;
        let shift_type = (modrm >> 3) & 0x07;
        // Intel APX revision 7.0 specifies NF=0 for RCL (/2) and RCR (/3).
        // Reject after ModR/M classification and before decoding its address
        // or fetching the immediate.
        if nf && matches!(shift_type, 2 | 3) {
            return self.inject_invalid_opcode();
        }
        let (_, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let rm = rm | ctx.evex_rm_reg();
        let imm = ctx.consume_u8()?;

        let src = if is_memory {
            self.read_mem(addr, op_size)?
        } else {
            self.get_reg(rm, op_size)
        };

        let shift_mask = if op_size == 8 { 0x3F } else { 0x1F };
        let count = (imm as u64) & shift_mask;

        if shift_type <= 3 {
            self.materialize_flags();
        }
        let result = self.perform_shift(src, count, shift_type, op_size);

        let dest = if ndd { ctx.evex_vvvv() } else { rm };

        if ndd || !is_memory {
            self.set_reg(dest, result, op_size);
        } else {
            self.write_mem(addr, result, op_size)?;
        }

        if !nf && count != 0 {
            self.update_flags_shift(result, src, count, shift_type, op_size);
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// APX shift by CL
    pub(crate) fn execute_apx_shift_cl(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
        ndd: bool,
        nf: bool,
    ) -> Result<Option<VcpuExit>> {
        let is_byte = (opcode & 0x01) == 0;
        let op_size = if is_byte {
            1
        } else {
            Self::apx_scalar_op_size(ctx)
        };
        let by_one = (opcode & 0x02) == 0;

        let modrm = ctx.peek_u8()?;
        let shift_type = (modrm >> 3) & 0x07;
        if nf && matches!(shift_type, 2 | 3) {
            return self.inject_invalid_opcode();
        }
        let (_, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let rm = rm | ctx.evex_rm_reg();

        let src = if is_memory {
            self.read_mem(addr, op_size)?
        } else {
            self.get_reg(rm, op_size)
        };

        let shift_mask = if op_size == 8 { 0x3F } else { 0x1F };
        let count = if by_one {
            1
        } else {
            self.regs.rcx & shift_mask
        };

        if shift_type <= 3 {
            self.materialize_flags();
        }
        let result = self.perform_shift(src, count, shift_type, op_size);

        let dest = if ndd { ctx.evex_vvvv() } else { rm };

        if ndd || !is_memory {
            self.set_reg(dest, result, op_size);
        } else {
            self.write_mem(addr, result, op_size)?;
        }

        if !nf && count != 0 {
            self.update_flags_shift(result, src, count, shift_type, op_size);
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// APX Group 3 unary, multiply, and divide forms.
    pub(crate) fn execute_apx_group3(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
        ndd: bool,
        nf: bool,
    ) -> Result<Option<VcpuExit>> {
        let modrm = ctx.peek_u8()?;
        let op_type = (modrm >> 3) & 0x07;
        // Intel APX revision 7.0 specifies NF=0 for NOT. Group 3 does not
        // identify NOT until ModR/M.reg=/2, but no address or operand is needed
        // to establish #UD.
        if nf && op_type == 2 {
            return self.inject_invalid_opcode();
        }
        if matches!(op_type, 0 | 1) {
            return self.execute_apx_ctest_imm(ctx, opcode);
        }
        // Intel APX revision 7.0 specifies {ND=0} and optional {NF} for the
        // implicit MUL/IMUL/DIV/IDIV forms. Reject ND=1 as soon as ModR/M.reg
        // identifies /4..=/7, before address generation or operand access.
        if matches!(op_type, 4..=7) && ndd {
            return self.inject_invalid_opcode();
        }

        let op_size = if opcode == 0xF6 {
            1
        } else {
            Self::apx_scalar_op_size(ctx)
        };
        let (_, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let src_reg = rm | ctx.evex_rm_reg();
        let src = if is_memory {
            self.read_mem(addr, op_size)?
        } else {
            self.get_reg(src_reg, op_size)
        };

        if matches!(op_type, 4..=7) {
            let (completed, mul_overflow) =
                self.execute_apx_group3_implicit(op_type, src, op_size)?;
            if completed {
                if !nf {
                    if let Some(overflow) = mul_overflow {
                        flags::set_cf_of(&mut self.regs.rflags, overflow, overflow);
                        self.clear_lazy_flags();
                    }
                }
                self.regs.rip += ctx.cursor as u64;
            }
            return Ok(None);
        }

        if !matches!(op_type, 2 | 3) {
            return Err(Error::Emulator(format!(
                "Unimplemented APX group3 opcode {:#x} /{} at RIP={:#x}",
                opcode, op_type, self.regs.rip
            )));
        }

        let result = if op_type == 2 {
            !src
        } else {
            match op_size {
                1 => (src as i8).wrapping_neg() as u8 as u64,
                2 => (src as i16).wrapping_neg() as u16 as u64,
                4 => (src as i32).wrapping_neg() as u32 as u64,
                8 => (src as i64).wrapping_neg() as u64,
                _ => src,
            }
        };

        if ndd {
            self.set_reg(ctx.evex_vvvv(), result, op_size);
        } else if is_memory {
            self.write_mem(addr, result, op_size)?;
        } else {
            self.set_reg(src_reg, result, op_size);
        }

        if op_type == 3 && !nf {
            flags::update_flags_sub(&mut self.regs.rflags, 0, src, result, op_size);
            self.clear_lazy_flags();
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(crate) fn execute_apx_group3_implicit(
        &mut self,
        op_type: u8,
        src: u64,
        op_size: u8,
    ) -> Result<(bool, Option<bool>)> {
        let mut mul_overflow = None;
        match (op_type, op_size) {
            (4, 1) => {
                let result = (self.regs.rax as u8 as u16) * (src as u8 as u16);
                self.set_reg(0, result as u64, 2);
                mul_overflow = Some((result >> 8) != 0);
            }
            (4, 2) => {
                let result = (self.regs.rax as u16 as u32) * (src as u16 as u32);
                self.set_reg(0, result as u16 as u64, 2);
                self.set_reg(2, (result >> 16) as u16 as u64, 2);
                mul_overflow = Some((result >> 16) != 0);
            }
            (4, 4) => {
                let result = (self.regs.rax as u32 as u64) * (src as u32 as u64);
                self.set_reg(0, result as u32 as u64, 4);
                self.set_reg(2, (result >> 32) as u32 as u64, 4);
                mul_overflow = Some((result >> 32) != 0);
            }
            (4, 8) => {
                let result = (self.regs.rax as u128) * (src as u128);
                self.set_reg(0, result as u64, 8);
                self.set_reg(2, (result >> 64) as u64, 8);
                mul_overflow = Some((result >> 64) != 0);
            }
            (5, 1) => {
                let result = (self.regs.rax as u8 as i8 as i16) * (src as u8 as i8 as i16);
                self.set_reg(0, result as u16 as u64, 2);
                mul_overflow = Some(result != result as i8 as i16);
            }
            (5, 2) => {
                let result = (self.regs.rax as u16 as i16 as i32) * (src as u16 as i16 as i32);
                self.set_reg(0, result as u16 as u64, 2);
                self.set_reg(2, (result >> 16) as u16 as u64, 2);
                mul_overflow = Some(result != result as i16 as i32);
            }
            (5, 4) => {
                let result = (self.regs.rax as u32 as i32 as i64) * (src as u32 as i32 as i64);
                self.set_reg(0, result as u32 as u64, 4);
                self.set_reg(2, (result >> 32) as u32 as u64, 4);
                mul_overflow = Some(result != result as i32 as i64);
            }
            (5, 8) => {
                let result = (self.regs.rax as i64 as i128) * (src as i64 as i128);
                self.set_reg(0, result as u64, 8);
                self.set_reg(2, (result >> 64) as u64, 8);
                mul_overflow = Some(result != result as i64 as i128);
            }
            (6, 1) => {
                let divisor = src as u8 as u16;
                if divisor == 0 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                let dividend = self.regs.rax as u16;
                let quotient = dividend / divisor;
                let remainder = dividend % divisor;
                if quotient > u8::MAX as u16 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                self.set_reg(0, ((remainder << 8) | quotient) as u64, 2);
            }
            (6, 2) => {
                let divisor = src as u16 as u32;
                if divisor == 0 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                let dividend =
                    ((self.regs.rdx as u16 as u32) << 16) | (self.regs.rax as u16 as u32);
                let quotient = dividend / divisor;
                let remainder = dividend % divisor;
                if quotient > u16::MAX as u32 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                self.set_reg(0, quotient as u16 as u64, 2);
                self.set_reg(2, remainder as u16 as u64, 2);
            }
            (6, 4) => {
                let divisor = src as u32 as u64;
                if divisor == 0 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                let dividend =
                    ((self.regs.rdx as u32 as u64) << 32) | (self.regs.rax as u32 as u64);
                let quotient = dividend / divisor;
                let remainder = dividend % divisor;
                if quotient > u32::MAX as u64 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                self.set_reg(0, quotient as u32 as u64, 4);
                self.set_reg(2, remainder as u32 as u64, 4);
            }
            (6, 8) => {
                let divisor = src as u128;
                if divisor == 0 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                let dividend = ((self.regs.rdx as u128) << 64) | (self.regs.rax as u128);
                let quotient = dividend / divisor;
                let remainder = dividend % divisor;
                if quotient > u64::MAX as u128 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                self.set_reg(0, quotient as u64, 8);
                self.set_reg(2, remainder as u64, 8);
            }
            (7, 1) => {
                let divisor = src as u8 as i8 as i16;
                if divisor == 0 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                let dividend = self.regs.rax as u16 as i16;
                let (quotient, remainder) =
                    match (dividend.checked_div(divisor), dividend.checked_rem(divisor)) {
                        (Some(q), Some(r)) => (q, r),
                        _ => {
                            self.inject_exception(0, None)?;
                            return Ok((false, None));
                        }
                    };
                if quotient < i8::MIN as i16 || quotient > i8::MAX as i16 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                let ax = ((remainder as i8 as u8 as u16) << 8) | (quotient as i8 as u8 as u16);
                self.set_reg(0, ax as u64, 2);
            }
            (7, 2) => {
                let divisor = src as u16 as i16 as i32;
                if divisor == 0 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                let dividend =
                    (((self.regs.rdx as u16 as u32) << 16) | (self.regs.rax as u16 as u32)) as i32;
                let (quotient, remainder) =
                    match (dividend.checked_div(divisor), dividend.checked_rem(divisor)) {
                        (Some(q), Some(r)) => (q, r),
                        _ => {
                            self.inject_exception(0, None)?;
                            return Ok((false, None));
                        }
                    };
                if quotient < i16::MIN as i32 || quotient > i16::MAX as i32 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                self.set_reg(0, quotient as u16 as u64, 2);
                self.set_reg(2, remainder as u16 as u64, 2);
            }
            (7, 4) => {
                let divisor = src as u32 as i32 as i64;
                if divisor == 0 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                let dividend =
                    (((self.regs.rdx as u32 as u64) << 32) | (self.regs.rax as u32 as u64)) as i64;
                let (quotient, remainder) =
                    match (dividend.checked_div(divisor), dividend.checked_rem(divisor)) {
                        (Some(q), Some(r)) => (q, r),
                        _ => {
                            self.inject_exception(0, None)?;
                            return Ok((false, None));
                        }
                    };
                if quotient < i32::MIN as i64 || quotient > i32::MAX as i64 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                self.set_reg(0, quotient as u32 as u64, 4);
                self.set_reg(2, remainder as u32 as u64, 4);
            }
            (7, 8) => {
                let divisor = src as i64 as i128;
                if divisor == 0 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                let dividend = (((self.regs.rdx as u128) << 64) | (self.regs.rax as u128)) as i128;
                let (quotient, remainder) =
                    match (dividend.checked_div(divisor), dividend.checked_rem(divisor)) {
                        (Some(q), Some(r)) => (q, r),
                        _ => {
                            self.inject_exception(0, None)?;
                            return Ok((false, None));
                        }
                    };
                if quotient < i64::MIN as i128 || quotient > i64::MAX as i128 {
                    self.inject_exception(0, None)?;
                    return Ok((false, None));
                }
                self.set_reg(0, quotient as u64, 8);
                self.set_reg(2, remainder as u64, 8);
            }
            _ => {
                return Err(Error::Emulator(format!(
                    "Unsupported APX group3 implicit /{} size {}",
                    op_type, op_size
                )));
            }
        }
        Ok((true, mul_overflow))
    }

    /// APX INC/DEC
    pub(crate) fn execute_apx_group_ff(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
        ndd: bool,
        nf: bool,
    ) -> Result<Option<VcpuExit>> {
        let modrm = ctx.peek_u8()?;
        let op_type = (modrm >> 3) & 0x07;
        if opcode == 0xFF && op_type == 6 {
            return self.execute_apx_push2(ctx);
        }
        if op_type > 1 {
            return self.inject_invalid_opcode();
        }
        self.execute_apx_inc_dec(ctx, opcode, ndd, nf)
    }

    pub(crate) fn execute_apx_inc_dec(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
        ndd: bool,
        nf: bool,
    ) -> Result<Option<VcpuExit>> {
        let is_byte = opcode == 0xFE;
        let op_size = if is_byte {
            1
        } else {
            Self::apx_scalar_op_size(ctx)
        };

        let modrm = ctx.peek_u8()?;
        let op_type = (modrm >> 3) & 0x07;
        let is_dec = op_type == 1;

        let (_, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let rm = rm | ctx.evex_rm_reg();

        let src = if is_memory {
            self.read_mem(addr, op_size)?
        } else {
            self.get_reg(rm, op_size)
        };

        let result = if is_dec {
            src.wrapping_sub(1)
        } else {
            src.wrapping_add(1)
        };

        let dest = if ndd { ctx.evex_vvvv() } else { rm };

        if ndd || !is_memory {
            self.set_reg(dest, result, op_size);
        } else {
            self.write_mem(addr, result, op_size)?;
        }

        if !nf {
            // INC/DEC don't affect CF
            self.resolve_lazy_cf();
            let old_cf = self.regs.rflags & 0x001;
            self.update_flags_alu(
                result,
                src,
                1,
                op_size,
                if is_dec { ApxAluOp::Sub } else { ApxAluOp::Add },
            );
            self.regs.rflags = (self.regs.rflags & !0x001) | old_cf;
            self.clear_lazy_flags();
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(crate) fn update_apx_double_shift_flags(
        &mut self,
        result: u64,
        src1: u64,
        count: u8,
        op_size: u8,
        is_shrd: bool,
    ) {
        let width = op_size as u32 * 8;
        let sign_bit = 1u64 << (width - 1);
        let cf = if is_shrd {
            ((src1 >> (count - 1)) & 1) != 0
        } else {
            ((src1 >> (width - count as u32)) & 1) != 0
        };
        let of = count == 1 && (((result ^ src1) & sign_bit) != 0);

        flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
        if cf {
            self.regs.rflags |= flags::bits::CF;
        } else {
            self.regs.rflags &= !flags::bits::CF;
        }
        if of {
            self.regs.rflags |= flags::bits::OF;
        } else {
            self.regs.rflags &= !flags::bits::OF;
        }
        self.clear_lazy_flags();
    }
}
