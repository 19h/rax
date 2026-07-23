//! VEX integer instruction implementation for x86_64 emulator.

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};

fn xmm_byte(xmm: [u64; 2], idx: usize) -> u8 {
    let lane = idx / 8;
    let shift = (idx % 8) * 8;
    ((xmm[lane] >> shift) & 0xFF) as u8
}

fn xmm_word(xmm: [u64; 2], idx: usize) -> u16 {
    let lane = idx / 4;
    let shift = (idx % 4) * 16;
    ((xmm[lane] >> shift) & 0xFFFF) as u16
}

fn xmm_dword(xmm: [u64; 2], idx: usize) -> u32 {
    if idx < 2 {
        ((xmm[0] >> (idx * 32)) & 0xFFFF_FFFF) as u32
    } else {
        ((xmm[1] >> ((idx - 2) * 32)) & 0xFFFF_FFFF) as u32
    }
}

fn put_xmm_byte(xmm: &mut [u64; 2], idx: usize, value: u8) {
    let lane = idx / 8;
    let shift = (idx % 8) * 8;
    let mask = !(0xFFu64 << shift);
    xmm[lane] = (xmm[lane] & mask) | ((value as u64) << shift);
}

fn put_xmm_word(xmm: &mut [u64; 2], idx: usize, value: u16) {
    let lane = idx / 4;
    let shift = (idx % 4) * 16;
    let mask = !(0xFFFFu64 << shift);
    xmm[lane] = (xmm[lane] & mask) | ((value as u64) << shift);
}

fn put_xmm_dword(xmm: &mut [u64; 2], idx: usize, value: u32) {
    if idx < 2 {
        let mask = !(0xFFFF_FFFFu64 << (idx * 32));
        xmm[0] = (xmm[0] & mask) | ((value as u64) << (idx * 32));
    } else {
        let pos = idx - 2;
        let mask = !(0xFFFF_FFFFu64 << (pos * 32));
        xmm[1] = (xmm[1] & mask) | ((value as u64) << (pos * 32));
    }
}

fn put_xmm_qword(xmm: &mut [u64; 2], idx: usize, value: u64) {
    xmm[idx] = value;
}

impl X86_64Vcpu {
    pub(in crate::isa::x86_64) fn execute_vpextrb(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        _vex_w: u8,
    ) -> Result<Option<VcpuExit>> {
        if vex_l != 0 {
            return self.inject_undefined_instruction();
        }

        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let value = xmm_byte(self.regs.xmm[reg as usize], (imm8 & 0x0F) as usize);

        if is_memory {
            self.write_mem(addr, value as u64, 1)?;
        } else {
            self.set_reg(rm, value as u64, 4);
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::isa::x86_64) fn execute_vpextrw_0f(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        _vex_w: u8,
    ) -> Result<Option<VcpuExit>> {
        if vex_l != 0 {
            return self.inject_undefined_instruction();
        }

        let (reg, rm, is_memory, _addr, _) = self.decode_modrm(ctx)?;
        if is_memory {
            return self.inject_undefined_instruction();
        }
        let imm8 = ctx.consume_u8()?;
        let value = xmm_word(self.regs.xmm[rm as usize], (imm8 & 0x07) as usize);

        self.set_reg(reg, value as u64, 4);
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::isa::x86_64) fn execute_vpextrw_0f3a(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        _vex_w: u8,
    ) -> Result<Option<VcpuExit>> {
        if vex_l != 0 {
            return self.inject_undefined_instruction();
        }

        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let value = xmm_word(self.regs.xmm[reg as usize], (imm8 & 0x07) as usize);

        if is_memory {
            self.write_mem(addr, value as u64, 2)?;
        } else {
            self.set_reg(rm, value as u64, 4);
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::isa::x86_64) fn execute_vpextrd_q(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vex_w: u8,
    ) -> Result<Option<VcpuExit>> {
        if vex_l != 0 {
            return self.inject_undefined_instruction();
        }

        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let xmm = self.regs.xmm[reg as usize];

        if vex_w != 0 {
            let value = xmm[(imm8 & 0x01) as usize];
            if is_memory {
                self.write_mem(addr, value, 8)?;
            } else {
                self.set_reg(rm, value, 8);
            }
        } else {
            let value = xmm_dword(xmm, (imm8 & 0x03) as usize);
            if is_memory {
                self.write_mem(addr, value as u64, 4)?;
            } else {
                self.set_reg(rm, value as u64, 4);
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::isa::x86_64) fn execute_vextractps(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
    ) -> Result<Option<VcpuExit>> {
        if vex_l != 0 {
            return self.inject_undefined_instruction();
        }

        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let idx = (imm8 & 0x03) as usize;
        let value = xmm_dword(self.regs.xmm[reg as usize], idx);

        if is_memory {
            self.write_mem(addr, value as u64, 4)?;
        } else {
            self.set_reg(rm, value as u64, 4);
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::isa::x86_64) fn execute_vpinsrb(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        _vex_w: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        if vex_l != 0 {
            return self.inject_undefined_instruction();
        }

        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let value = if is_memory {
            self.read_mem(addr, 1)? as u8
        } else {
            self.get_reg(rm, 1) as u8
        };

        let dst = reg as usize;
        let mut result = self.regs.xmm[vvvv as usize];
        put_xmm_byte(&mut result, (imm8 & 0x0F) as usize, value);

        self.regs.xmm[dst] = result;
        self.regs.ymm_high[dst] = [0; 2];
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::isa::x86_64) fn execute_vpinsrw(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        _vex_w: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        if vex_l != 0 {
            return self.inject_undefined_instruction();
        }

        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let value = if is_memory {
            self.read_mem(addr, 2)? as u16
        } else {
            self.get_reg(rm, 2) as u16
        };

        let dst = reg as usize;
        let mut result = self.regs.xmm[vvvv as usize];
        put_xmm_word(&mut result, (imm8 & 0x07) as usize, value);

        self.regs.xmm[dst] = result;
        self.regs.ymm_high[dst] = [0; 2];
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::isa::x86_64) fn execute_vpinsrd_q(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vex_w: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        if vex_l != 0 {
            return self.inject_undefined_instruction();
        }

        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let dst = reg as usize;
        let mut result = self.regs.xmm[vvvv as usize];

        if vex_w != 0 {
            let value = if is_memory {
                self.read_mem(addr, 8)?
            } else {
                self.get_reg(rm, 8)
            };
            put_xmm_qword(&mut result, (imm8 & 0x01) as usize, value);
        } else {
            let value = if is_memory {
                self.read_mem(addr, 4)? as u32
            } else {
                self.get_reg(rm, 4) as u32
            };
            put_xmm_dword(&mut result, (imm8 & 0x03) as usize, value);
        }

        self.regs.xmm[dst] = result;
        self.regs.ymm_high[dst] = [0; 2];
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::isa::x86_64) fn execute_vinsertps(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        if vex_l != 0 {
            return self.inject_undefined_instruction();
        }

        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let dst = reg as usize;
        let src1 = vvvv as usize;
        let src_lane = ((imm8 >> 6) & 0x03) as usize;
        let dst_lane = ((imm8 >> 4) & 0x03) as usize;
        let zmask = imm8 & 0x0F;

        let src_value = if is_memory {
            self.read_mem(addr, 4)? as u32
        } else {
            xmm_dword(self.regs.xmm[rm as usize], src_lane)
        };

        let mut result = self.regs.xmm[src1];
        put_xmm_dword(&mut result, dst_lane, src_value);
        for lane in 0..4 {
            if zmask & (1 << lane) != 0 {
                put_xmm_dword(&mut result, lane, 0);
            }
        }

        self.regs.xmm[dst] = result;
        self.regs.ymm_high[dst] = [0; 2];
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::isa::x86_64) fn execute_vinsertf128(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vex_w: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        if vex_l != 1 || vex_w != 0 {
            return self.inject_undefined_instruction();
        }

        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        // Read 128-bit source from xmm or memory
        let (insert_lo, insert_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };

        // Copy src1 to dst first
        self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src1][0];
        self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
        self.regs.ymm_high[xmm_dst][0] = self.regs.ymm_high[xmm_src1][0];
        self.regs.ymm_high[xmm_dst][1] = self.regs.ymm_high[xmm_src1][1];

        // Insert into selected lane based on imm8[0]
        if (imm8 & 1) == 0 {
            // Insert into low 128 bits
            self.regs.xmm[xmm_dst][0] = insert_lo;
            self.regs.xmm[xmm_dst][1] = insert_hi;
        } else {
            // Insert into high 128 bits
            self.regs.ymm_high[xmm_dst][0] = insert_lo;
            self.regs.ymm_high[xmm_dst][1] = insert_hi;
        }
        // The VEX.256 write clears ZMM[511:256] even if every low result bit
        // was already equal to the destination's old value.
        self.regs.zmm_high[xmm_dst] = [0; 4];

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::isa::x86_64) fn execute_vextractf128(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vex_w: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        if vex_l != 1 || vex_w != 0 || vvvv != 0 {
            return self.inject_undefined_instruction();
        }

        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let xmm_src = reg as usize;

        // Select lane based on imm8[0]
        let (extract_lo, extract_hi) = if (imm8 & 1) == 0 {
            // Extract low 128 bits
            (self.regs.xmm[xmm_src][0], self.regs.xmm[xmm_src][1])
        } else {
            // Extract high 128 bits
            (
                self.regs.ymm_high[xmm_src][0],
                self.regs.ymm_high[xmm_src][1],
            )
        };

        if is_memory {
            // Preflight the complete 16-byte destination before preserving the
            // existing pair of qword writes (and their MMIO/trace semantics).
            self.mmu.preflight_write_range(addr, 16, &self.sregs)?;
            self.write_mem(addr, extract_lo, 8)?;
            self.write_mem(addr + 8, extract_hi, 8)?;
        } else {
            let xmm_dst = rm as usize;
            self.regs.xmm[xmm_dst][0] = extract_lo;
            self.regs.xmm[xmm_dst][1] = extract_hi;
            // VEX clears upper bits
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
            self.regs.zmm_high[xmm_dst] = [0; 4];
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::isa::x86_64) fn execute_vperm2f128(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        // Get all 4 source lanes (2 from src1, 2 from src2)
        let src1_lo = (self.regs.xmm[xmm_src1][0], self.regs.xmm[xmm_src1][1]);
        let src1_hi = (
            self.regs.ymm_high[xmm_src1][0],
            self.regs.ymm_high[xmm_src1][1],
        );
        let (src2_lo, src2_hi) = if is_memory {
            (
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?),
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?),
            )
        } else {
            (
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1]),
                (
                    self.regs.ymm_high[rm as usize][0],
                    self.regs.ymm_high[rm as usize][1],
                ),
            )
        };

        // Select result low 128 bits based on imm8[1:0]
        let result_lo = if (imm8 & 0x08) != 0 {
            // Zero this lane
            (0u64, 0u64)
        } else {
            match imm8 & 0x03 {
                0 => src1_lo,
                1 => src1_hi,
                2 => src2_lo,
                3 => src2_hi,
                _ => unreachable!(),
            }
        };

        // Select result high 128 bits based on imm8[5:4]
        let result_hi = if (imm8 & 0x80) != 0 {
            // Zero this lane
            (0u64, 0u64)
        } else {
            match (imm8 >> 4) & 0x03 {
                0 => src1_lo,
                1 => src1_hi,
                2 => src2_lo,
                3 => src2_hi,
                _ => unreachable!(),
            }
        };

        self.regs.xmm[xmm_dst][0] = result_lo.0;
        self.regs.xmm[xmm_dst][1] = result_lo.1;
        self.regs.ymm_high[xmm_dst][0] = result_hi.0;
        self.regs.ymm_high[xmm_dst][1] = result_hi.1;

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }
}
