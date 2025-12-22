//! VEX-encoded (AVX) instruction dispatch for the x86_64 CPU emulator.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::flags;
use super::super::insn;

impl X86_64Vcpu {
    /// Execute 2-byte VEX-encoded instructions (0xC5 prefix)
    pub(in crate::backend::emulator::x86_64) fn execute_vex2(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        // VEX 2-byte prefix: 0xC5 [R vvvv L pp]
        // Implied: m-mmmm = 1 (0F escape), X = 1, B = 1, W = 0
        let vex1 = ctx.consume_u8()?;
        let opcode = ctx.consume_u8()?;

        let vex_r = (vex1 >> 7) & 1;
        let vex_l = (vex1 >> 2) & 1;
        let vex_pp = vex1 & 0x03;
        let vvvv = ((vex1 >> 3) & 0x0F) ^ 0x0F;

        // 2-byte VEX implies m-mmmm=1 (0F), X=B=1, W=0
        let m_mmmm: u8 = 1;
        let vex_w: u8 = 0;

        // Set up REX (R is from VEX, X and B are implied 1 which means REX.X=0, REX.B=0)
        let rex_r = (vex_r ^ 1) & 1;
        let rex = 0x40 | (rex_r << 2);
        ctx.rex = Some(rex);
        ctx.op_size = 4; // W=0 implies 32-bit operand size
        ctx.rip_relative_offset = 1;

        self.execute_vex_common(ctx, m_mmmm, vex_pp, vex_l, vex_w, vvvv, opcode)
    }

    /// Execute 3-byte VEX-encoded instructions (0xC4 prefix)
    pub(in crate::backend::emulator::x86_64) fn execute_vex3(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        // VEX 3-byte prefix (0xC4)
        let vex1 = ctx.consume_u8()?;
        let vex2 = ctx.consume_u8()?;
        let opcode = ctx.consume_u8()?;

        let vex_r = (vex1 >> 7) & 1;
        let vex_x = (vex1 >> 6) & 1;
        let vex_b = (vex1 >> 5) & 1;
        let m_mmmm = vex1 & 0x1F;

        let vex_w = (vex2 >> 7) & 1;
        let vex_l = (vex2 >> 2) & 1;
        let vex_pp = vex2 & 0x03;

        // Set up REX and operand size from VEX
        let rex_r = (vex_r ^ 1) & 1;
        let rex_x = (vex_x ^ 1) & 1;
        let rex_b = (vex_b ^ 1) & 1;
        let mut rex = 0x40 | (rex_r << 2) | (rex_x << 1) | rex_b;
        if vex_w != 0 {
            rex |= 0x08;
        }
        ctx.rex = Some(rex);
        ctx.op_size = if vex_w != 0 { 8 } else { 4 };
        ctx.rip_relative_offset = 1;

        // VEX.vvvv register (inverted in VEX encoding)
        let vvvv = ((vex2 >> 3) & 0x0F) ^ 0x0F;

        self.execute_vex_common(ctx, m_mmmm, vex_pp, vex_l, vex_w, vvvv, opcode)
    }

    /// Common VEX instruction execution logic
    fn execute_vex_common(
        &mut self,
        ctx: &mut InsnContext,
        m_mmmm: u8,
        vex_pp: u8,
        vex_l: u8,
        vex_w: u8,
        vvvv: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let mask = if ctx.op_size == 8 { !0u64 } else { 0xFFFF_FFFFu64 };

        // VEX.LZ.F2.0F3A.W{0,1} F0 /r ib (RORX)
        if m_mmmm == 0x3 && vex_pp == 0x3 && vex_l == 0 && opcode == 0xF0 {
            let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
            let src = if is_memory {
                self.read_mem(addr, ctx.op_size)?
            } else {
                self.get_reg(rm, ctx.op_size)
            };
            let imm = ctx.consume_u8()?;
            let bits = if ctx.op_size == 8 { 64u32 } else { 32u32 };
            let mask = if bits == 64 { !0u64 } else { 0xFFFF_FFFFu64 };
            let count_mask = if bits == 64 { 0x3F } else { 0x1F };
            let count = (imm & count_mask) as u32;
            let src = src & mask;
            let result = if count == 0 {
                src
            } else {
                ((src >> count) | (src << (bits - count))) & mask
            };
            self.set_reg(reg, result, ctx.op_size);
            self.regs.rip += ctx.cursor as u64;
            return Ok(None);
        }

        // VEX.LZ.0F38 BMI1/BMI2 instructions
        if m_mmmm == 0x2 && vex_l == 0 {
            let mask = if ctx.op_size == 8 { !0u64 } else { 0xFFFF_FFFFu64 };

            match (vex_pp, opcode) {
                // ANDN: VEX.LZ.0F38.W{0,1} F2 /r
                // dest = src1 & ~src2, where vvvv=src1, r/m=src2
                (0, 0xF2) => {
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let src1 = self.get_reg(vvvv, ctx.op_size) & mask;
                    let src2 = if is_memory {
                        self.read_mem(addr, ctx.op_size)? & mask
                    } else {
                        self.get_reg(rm, ctx.op_size) & mask
                    };
                    let result = src1 & (!src2);
                    self.set_reg(reg, result & mask, ctx.op_size);
                    // SF and ZF based on result, OF and CF cleared
                    let sf = if ctx.op_size == 8 { (result >> 63) & 1 } else { (result >> 31) & 1 };
                    let zf = if result == 0 { 1 } else { 0 };
                    self.regs.rflags &= !(flags::bits::SF | flags::bits::ZF | flags::bits::OF | flags::bits::CF);
                    if sf != 0 { self.regs.rflags |= flags::bits::SF; }
                    if zf != 0 { self.regs.rflags |= flags::bits::ZF; }
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                // BLSI: VEX.LZ.0F38.W{0,1} F3 /3
                // BLSMSK: VEX.LZ.0F38.W{0,1} F3 /2
                // BLSR: VEX.LZ.0F38.W{0,1} F3 /1
                (0, 0xF3) => {
                    let modrm = ctx.peek_u8()?;
                    let reg_op = (modrm >> 3) & 0x07;
                    let (_, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let src = if is_memory {
                        self.read_mem(addr, ctx.op_size)? & mask
                    } else {
                        self.get_reg(rm, ctx.op_size) & mask
                    };
                    let result = match reg_op {
                        1 => src & src.wrapping_sub(1), // BLSR: src & (src - 1)
                        2 => src ^ src.wrapping_sub(1), // BLSMSK: src ^ (src - 1)
                        3 => src.wrapping_neg() & src,  // BLSI: (-src) & src
                        _ => return Err(Error::Emulator(format!("unimplemented VEX.0F38.F3 /{}", reg_op))),
                    };
                    self.set_reg(vvvv, result & mask, ctx.op_size);
                    // Set flags
                    // SF based on result sign
                    let sf = if ctx.op_size == 8 { (result >> 63) & 1 } else { (result >> 31) & 1 };
                    // ZF based on result for BLSI/BLSR, based on src for BLSMSK
                    let zf = match reg_op {
                        2 => if src == 0 { 1 } else { 0 },      // BLSMSK: ZF = (src == 0)
                        _ => if result == 0 { 1 } else { 0 },   // BLSI/BLSR: ZF = (result == 0)
                    };
                    // CF: BLSMSK sets CF if src != 0, BLSI/BLSR set CF if src == 0
                    let cf = match reg_op {
                        2 => if src != 0 { 1 } else { 0 },      // BLSMSK: CF = (src != 0)
                        _ => if src == 0 { 1 } else { 0 },      // BLSI/BLSR: CF = (src == 0)
                    };
                    self.regs.rflags &= !(flags::bits::SF | flags::bits::ZF | flags::bits::OF | flags::bits::CF);
                    if sf != 0 { self.regs.rflags |= flags::bits::SF; }
                    if zf != 0 { self.regs.rflags |= flags::bits::ZF; }
                    if cf != 0 { self.regs.rflags |= flags::bits::CF; }
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                // BZHI: VEX.LZ.0F38.W{0,1} F5 /r
                (0, 0xF5) => {
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let src = if is_memory {
                        self.read_mem(addr, ctx.op_size)? & mask
                    } else {
                        self.get_reg(rm, ctx.op_size) & mask
                    };
                    let index = (self.get_reg(vvvv, ctx.op_size) & 0xFF) as u32;
                    let bits = if ctx.op_size == 8 { 64u32 } else { 32u32 };
                    let result = if index >= bits {
                        src
                    } else {
                        src & ((1u64 << index) - 1)
                    };
                    self.set_reg(reg, result, ctx.op_size);
                    // SF and ZF based on result, CF = (index >= bits)
                    let sf = if ctx.op_size == 8 { (result >> 63) & 1 } else { (result >> 31) & 1 };
                    let zf = if result == 0 { 1 } else { 0 };
                    let cf = if index >= bits { 1 } else { 0 };
                    self.regs.rflags &= !(flags::bits::SF | flags::bits::ZF | flags::bits::OF | flags::bits::CF);
                    if sf != 0 { self.regs.rflags |= flags::bits::SF; }
                    if zf != 0 { self.regs.rflags |= flags::bits::ZF; }
                    if cf != 0 { self.regs.rflags |= flags::bits::CF; }
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                // BEXTR: VEX.LZ.0F38.W{0,1} F7 /r
                (0, 0xF7) => {
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let src = if is_memory {
                        self.read_mem(addr, ctx.op_size)? & mask
                    } else {
                        self.get_reg(rm, ctx.op_size) & mask
                    };
                    let control = self.get_reg(vvvv, ctx.op_size);
                    let start = (control & 0xFF) as u32;
                    let len = ((control >> 8) & 0xFF) as u32;
                    let bits = if ctx.op_size == 8 { 64u32 } else { 32u32 };
                    let result = if start >= bits || len == 0 {
                        0
                    } else {
                        let shifted = src >> start;
                        if len >= bits {
                            shifted
                        } else {
                            shifted & ((1u64 << len) - 1)
                        }
                    };
                    self.set_reg(reg, result, ctx.op_size);
                    let zf = if result == 0 { 1 } else { 0 };
                    self.regs.rflags &= !(flags::bits::ZF | flags::bits::OF | flags::bits::CF);
                    if zf != 0 { self.regs.rflags |= flags::bits::ZF; }
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                // MULX: VEX.LZ.F2.0F38.W{0,1} F6 /r
                (3, 0xF6) => {
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let src1 = if ctx.op_size == 8 { self.regs.rdx } else { self.regs.rdx & mask };
                    let src2 = if is_memory {
                        self.read_mem(addr, ctx.op_size)? & mask
                    } else {
                        self.get_reg(rm, ctx.op_size) & mask
                    };
                    let (hi, lo) = if ctx.op_size == 8 {
                        let prod = (src1 as u128) * (src2 as u128);
                        ((prod >> 64) as u64, prod as u64)
                    } else {
                        let prod = (src1 as u64) * (src2 as u64);
                        ((prod >> 32) as u64 & mask, prod as u64 & mask)
                    };
                    // Write low first, then high (so high wins if both destinations are the same)
                    self.set_reg(vvvv, lo, ctx.op_size);
                    self.set_reg(reg, hi, ctx.op_size);
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                // PDEP: VEX.LZ.F2.0F38.W{0,1} F5 /r
                (3, 0xF5) => {
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let src = self.get_reg(vvvv, ctx.op_size) & mask;
                    let selector = if is_memory {
                        self.read_mem(addr, ctx.op_size)? & mask
                    } else {
                        self.get_reg(rm, ctx.op_size) & mask
                    };
                    let mut result = 0u64;
                    let mut k = 0u32;
                    for i in 0..ctx.op_size * 8 {
                        if (selector >> i) & 1 != 0 {
                            if (src >> k) & 1 != 0 {
                                result |= 1 << i;
                            }
                            k += 1;
                        }
                    }
                    self.set_reg(reg, result & mask, ctx.op_size);
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                // PEXT: VEX.LZ.F3.0F38.W{0,1} F5 /r
                (2, 0xF5) => {
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let src = self.get_reg(vvvv, ctx.op_size) & mask;
                    let selector = if is_memory {
                        self.read_mem(addr, ctx.op_size)? & mask
                    } else {
                        self.get_reg(rm, ctx.op_size) & mask
                    };
                    let mut result = 0u64;
                    let mut k = 0u32;
                    for i in 0..ctx.op_size * 8 {
                        if (selector >> i) & 1 != 0 {
                            if (src >> i) & 1 != 0 {
                                result |= 1 << k;
                            }
                            k += 1;
                        }
                    }
                    self.set_reg(reg, result & mask, ctx.op_size);
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                // SARX: VEX.LZ.F3.0F38.W{0,1} F7 /r
                (2, 0xF7) => {
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let src = if is_memory {
                        self.read_mem(addr, ctx.op_size)? & mask
                    } else {
                        self.get_reg(rm, ctx.op_size) & mask
                    };
                    let count_mask = if ctx.op_size == 8 { 0x3F } else { 0x1F };
                    let count = (self.get_reg(vvvv, ctx.op_size) & count_mask) as u32;
                    let result = if ctx.op_size == 8 {
                        ((src as i64) >> count) as u64
                    } else {
                        (((src as u32 as i32) >> count) as u32) as u64
                    };
                    self.set_reg(reg, result & mask, ctx.op_size);
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                // SHRX: VEX.LZ.F2.0F38.W{0,1} F7 /r
                (3, 0xF7) => {
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let src = if is_memory {
                        self.read_mem(addr, ctx.op_size)? & mask
                    } else {
                        self.get_reg(rm, ctx.op_size) & mask
                    };
                    let count_mask = if ctx.op_size == 8 { 0x3F } else { 0x1F };
                    let count = (self.get_reg(vvvv, ctx.op_size) & count_mask) as u32;
                    let result = src >> count;
                    self.set_reg(reg, result & mask, ctx.op_size);
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                // SHLX: VEX.LZ.66.0F38.W{0,1} F7 /r
                (1, 0xF7) => {
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let src = if is_memory {
                        self.read_mem(addr, ctx.op_size)? & mask
                    } else {
                        self.get_reg(rm, ctx.op_size) & mask
                    };
                    let count_mask = if ctx.op_size == 8 { 0x3F } else { 0x1F };
                    let count = (self.get_reg(vvvv, ctx.op_size) & count_mask) as u32;
                    let result = src << count;
                    self.set_reg(reg, result & mask, ctx.op_size);
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                _ => {}
            }
        }

        // VEX.0F38 SIMD instructions (m=2) - variable shifts (supports L=0/1)
        if m_mmmm == 0x2 && vex_pp == 1 {
            match opcode {
                // VPSRLVD/VPSRLVQ (0x45), VPSRAVD (0x46), VPSLLVD/VPSLLVQ (0x47)
                0x45 | 0x46 | 0x47 => {
                    return self.execute_vex_variable_shift(ctx, vex_l, vvvv, vex_w, opcode);
                }
                _ => {}
            }
        }

        // VEX.0F encoded SSE/AVX instructions (m=1)
        if m_mmmm == 0x1 {
            match (vex_pp, opcode) {
                // VMOVDQA xmm/ymm, xmm/ymm/m (VEX.66.0F 6F /r)
                (1, 0x6F) => {
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let xmm_dst = reg as usize;

                    if vex_l == 0 {
                        // 128-bit XMM
                        if is_memory {
                            if addr & 0xF != 0 {
                                return Err(Error::Emulator(format!(
                                    "VMOVDQA: unaligned memory access at {:#x}",
                                    addr
                                )));
                            }
                            self.regs.xmm[xmm_dst][0] = self.read_mem(addr, 8)?;
                            self.regs.xmm[xmm_dst][1] = self.read_mem(addr + 8, 8)?;
                        } else {
                            let xmm_src = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                        }
                        // VEX clears upper bits
                        self.regs.ymm_high[xmm_dst][0] = 0;
                        self.regs.ymm_high[xmm_dst][1] = 0;
                    } else {
                        // 256-bit YMM
                        if is_memory {
                            if addr & 0x1F != 0 {
                                return Err(Error::Emulator(format!(
                                    "VMOVDQA: unaligned memory access at {:#x}",
                                    addr
                                )));
                            }
                            self.regs.xmm[xmm_dst][0] = self.read_mem(addr, 8)?;
                            self.regs.xmm[xmm_dst][1] = self.read_mem(addr + 8, 8)?;
                            self.regs.ymm_high[xmm_dst][0] = self.read_mem(addr + 16, 8)?;
                            self.regs.ymm_high[xmm_dst][1] = self.read_mem(addr + 24, 8)?;
                        } else {
                            let xmm_src = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                            self.regs.ymm_high[xmm_dst][0] = self.regs.ymm_high[xmm_src][0];
                            self.regs.ymm_high[xmm_dst][1] = self.regs.ymm_high[xmm_src][1];
                        }
                    }
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                // VMOVDQU xmm/ymm, xmm/ymm/m (VEX.F3.0F 6F /r)
                (2, 0x6F) => {
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let xmm_dst = reg as usize;

                    if vex_l == 0 {
                        // 128-bit XMM
                        if is_memory {
                            self.regs.xmm[xmm_dst][0] = self.read_mem(addr, 8)?;
                            self.regs.xmm[xmm_dst][1] = self.read_mem(addr + 8, 8)?;
                        } else {
                            let xmm_src = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                        }
                        self.regs.ymm_high[xmm_dst][0] = 0;
                        self.regs.ymm_high[xmm_dst][1] = 0;
                    } else {
                        // 256-bit YMM
                        if is_memory {
                            self.regs.xmm[xmm_dst][0] = self.read_mem(addr, 8)?;
                            self.regs.xmm[xmm_dst][1] = self.read_mem(addr + 8, 8)?;
                            self.regs.ymm_high[xmm_dst][0] = self.read_mem(addr + 16, 8)?;
                            self.regs.ymm_high[xmm_dst][1] = self.read_mem(addr + 24, 8)?;
                        } else {
                            let xmm_src = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                            self.regs.ymm_high[xmm_dst][0] = self.regs.ymm_high[xmm_src][0];
                            self.regs.ymm_high[xmm_dst][1] = self.regs.ymm_high[xmm_src][1];
                        }
                    }
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                // VMOVDQA xmm/ymm/m, xmm/ymm (VEX.66.0F 7F /r) - store
                (1, 0x7F) => {
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let xmm_src = reg as usize;

                    if vex_l == 0 {
                        // 128-bit XMM
                        if is_memory {
                            if addr & 0xF != 0 {
                                return Err(Error::Emulator(format!(
                                    "VMOVDQA: unaligned memory access at {:#x}",
                                    addr
                                )));
                            }
                            self.write_mem(addr, self.regs.xmm[xmm_src][0], 8)?;
                            self.write_mem(addr + 8, self.regs.xmm[xmm_src][1], 8)?;
                        } else {
                            let xmm_dst = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                            self.regs.ymm_high[xmm_dst][0] = 0;
                            self.regs.ymm_high[xmm_dst][1] = 0;
                        }
                    } else {
                        // 256-bit YMM
                        if is_memory {
                            if addr & 0x1F != 0 {
                                return Err(Error::Emulator(format!(
                                    "VMOVDQA: unaligned memory access at {:#x}",
                                    addr
                                )));
                            }
                            self.write_mem(addr, self.regs.xmm[xmm_src][0], 8)?;
                            self.write_mem(addr + 8, self.regs.xmm[xmm_src][1], 8)?;
                            self.write_mem(addr + 16, self.regs.ymm_high[xmm_src][0], 8)?;
                            self.write_mem(addr + 24, self.regs.ymm_high[xmm_src][1], 8)?;
                        } else {
                            let xmm_dst = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                            self.regs.ymm_high[xmm_dst][0] = self.regs.ymm_high[xmm_src][0];
                            self.regs.ymm_high[xmm_dst][1] = self.regs.ymm_high[xmm_src][1];
                        }
                    }
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                // VMOVDQU xmm/ymm/m, xmm/ymm (VEX.F3.0F 7F /r) - store
                (2, 0x7F) => {
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let xmm_src = reg as usize;

                    if vex_l == 0 {
                        // 128-bit XMM
                        if is_memory {
                            self.write_mem(addr, self.regs.xmm[xmm_src][0], 8)?;
                            self.write_mem(addr + 8, self.regs.xmm[xmm_src][1], 8)?;
                        } else {
                            let xmm_dst = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                            self.regs.ymm_high[xmm_dst][0] = 0;
                            self.regs.ymm_high[xmm_dst][1] = 0;
                        }
                    } else {
                        // 256-bit YMM
                        if is_memory {
                            self.write_mem(addr, self.regs.xmm[xmm_src][0], 8)?;
                            self.write_mem(addr + 8, self.regs.xmm[xmm_src][1], 8)?;
                            self.write_mem(addr + 16, self.regs.ymm_high[xmm_src][0], 8)?;
                            self.write_mem(addr + 24, self.regs.ymm_high[xmm_src][1], 8)?;
                        } else {
                            let xmm_dst = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                            self.regs.ymm_high[xmm_dst][0] = self.regs.ymm_high[xmm_src][0];
                            self.regs.ymm_high[xmm_dst][1] = self.regs.ymm_high[xmm_src][1];
                        }
                    }
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }

                // VEX arithmetic: VADDPS/PD (0x58), VMULPS/PD (0x59), VSUBPS/PD (0x5C), VDIVPS/PD (0x5E)
                // VSQRTPS/PD (0x51), VMINPS/PD (0x5D), VMAXPS/PD (0x5F)
                // pp=0: PS (packed single), pp=1: PD (packed double)
                // pp=2: SS (scalar single), pp=3: SD (scalar double)
                (_, 0x51) | (_, 0x58) | (_, 0x59) | (_, 0x5C) | (_, 0x5D) | (_, 0x5E) | (_, 0x5F) => {
                    return self.execute_vex_arith(ctx, vex_pp, vex_l, vvvv, opcode);
                }

                // VEX logical: VANDPS/PD (0x54), VANDNPS/PD (0x55), VORPS/PD (0x56), VXORPS/PD (0x57)
                (_, 0x54) | (_, 0x55) | (_, 0x56) | (_, 0x57) => {
                    return self.execute_vex_logical(ctx, vex_pp, vex_l, vvvv, opcode);
                }

                // VEX unpack: VUNPCKLPS/PD (0x14), VUNPCKHPS/PD (0x15)
                (_, 0x14) | (_, 0x15) => {
                    return self.execute_vex_unpack(ctx, vex_pp, vex_l, vvvv, opcode);
                }

                // VEX conversion 0x5A: VCVTPS2PD, VCVTPD2PS, VCVTSS2SD, VCVTSD2SS
                (_, 0x5A) => {
                    return self.execute_vex_cvt_fp(ctx, vex_pp, vex_l, vvvv);
                }

                // VEX conversion 0x5B: VCVTDQ2PS, VCVTPS2DQ, VCVTTPS2DQ
                (_, 0x5B) => {
                    return self.execute_vex_cvt_dq_ps(ctx, vex_pp, vex_l);
                }

                // VEX conversion 0xE6: VCVTTPD2DQ, VCVTDQ2PD, VCVTPD2DQ
                (_, 0xE6) => {
                    return self.execute_vex_cvt_pd_dq(ctx, vex_pp, vex_l);
                }

                // VEX scalar int-to-float: VCVTSI2SS (0x2A with F3), VCVTSI2SD (0x2A with F2)
                (2, 0x2A) | (3, 0x2A) => {
                    return self.execute_vex_cvtsi2s(ctx, vex_pp, vex_w, vvvv);
                }

                // VEX truncating scalar float-to-int: VCVTTSS2SI (0x2C with F3), VCVTTSD2SI (0x2C with F2)
                (2, 0x2C) | (3, 0x2C) => {
                    return self.execute_vex_cvtts2si(ctx, vex_pp, vex_w);
                }

                // VEX rounding scalar float-to-int: VCVTSS2SI (0x2D with F3), VCVTSD2SI (0x2D with F2)
                (2, 0x2D) | (3, 0x2D) => {
                    return self.execute_vex_cvts2si(ctx, vex_pp, vex_w);
                }

                // VSHUFPS (0xC6 with NP), VSHUFPD (0xC6 with 66)
                (0, 0xC6) | (1, 0xC6) => {
                    return self.execute_vex_shufp(ctx, vex_pp, vex_l, vvvv);
                }

                // VMOVMSKPS (0x50 with NP), VMOVMSKPD (0x50 with 66)
                (0, 0x50) | (1, 0x50) => {
                    return self.execute_vex_movmskp(ctx, vex_pp, vex_l);
                }

                // VRSQRTPS (0x52 with NP), VRSQRTSS (0x52 with F3)
                (0, 0x52) | (2, 0x52) => {
                    return self.execute_vex_rsqrt(ctx, vex_pp, vex_l, vvvv);
                }

                // VRCPPS (0x53 with NP), VRCPSS (0x53 with F3)
                (0, 0x53) | (2, 0x53) => {
                    return self.execute_vex_rcp(ctx, vex_pp, vex_l, vvvv);
                }

                // VZEROUPPER/VZEROALL (0x77)
                (0, 0x77) => {
                    return self.execute_vex_vzero(ctx, vex_l);
                }

                // VLDMXCSR/VSTMXCSR (0xAE)
                (0, 0xAE) => {
                    return self.execute_vex_ldst_mxcsr(ctx);
                }

                // VADDSUBPD (0xD0 with 66), VADDSUBPS (0xD0 with F2)
                (1, 0xD0) | (3, 0xD0) => {
                    return self.execute_vex_addsubp(ctx, vex_pp, vex_l, vvvv);
                }

                // VHADDPD (0x7C with 66), VHADDPS (0x7C with F2)
                (1, 0x7C) | (3, 0x7C) => {
                    return self.execute_vex_haddp(ctx, vex_pp, vex_l, vvvv);
                }

                // VHSUBPD (0x7D with 66), VHSUBPS (0x7D with F2)
                (1, 0x7D) | (3, 0x7D) => {
                    return self.execute_vex_hsubp(ctx, vex_pp, vex_l, vvvv);
                }

                // VEX move: VMOVAPS/VMOVUPS (0x28/0x29/0x10/0x11)
                (0, 0x10) | (0, 0x28) => {
                    // VMOVUPS (0x10) or VMOVAPS (0x28) load
                    let aligned = opcode == 0x28;
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let xmm_dst = reg as usize;

                    if vex_l == 0 {
                        if is_memory {
                            if aligned && addr & 0xF != 0 {
                                return Err(Error::Emulator(format!(
                                    "VMOVAPS: unaligned memory access at {:#x}", addr
                                )));
                            }
                            self.regs.xmm[xmm_dst][0] = self.read_mem(addr, 8)?;
                            self.regs.xmm[xmm_dst][1] = self.read_mem(addr + 8, 8)?;
                        } else {
                            let xmm_src = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                        }
                        self.regs.ymm_high[xmm_dst][0] = 0;
                        self.regs.ymm_high[xmm_dst][1] = 0;
                    } else {
                        if is_memory {
                            if aligned && addr & 0x1F != 0 {
                                return Err(Error::Emulator(format!(
                                    "VMOVAPS: unaligned memory access at {:#x}", addr
                                )));
                            }
                            self.regs.xmm[xmm_dst][0] = self.read_mem(addr, 8)?;
                            self.regs.xmm[xmm_dst][1] = self.read_mem(addr + 8, 8)?;
                            self.regs.ymm_high[xmm_dst][0] = self.read_mem(addr + 16, 8)?;
                            self.regs.ymm_high[xmm_dst][1] = self.read_mem(addr + 24, 8)?;
                        } else {
                            let xmm_src = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                            self.regs.ymm_high[xmm_dst][0] = self.regs.ymm_high[xmm_src][0];
                            self.regs.ymm_high[xmm_dst][1] = self.regs.ymm_high[xmm_src][1];
                        }
                    }
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                (0, 0x11) | (0, 0x29) => {
                    // VMOVUPS (0x11) or VMOVAPS (0x29) store
                    let aligned = opcode == 0x29;
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let xmm_src = reg as usize;

                    if vex_l == 0 {
                        if is_memory {
                            if aligned && addr & 0xF != 0 {
                                return Err(Error::Emulator(format!(
                                    "VMOVAPS: unaligned memory access at {:#x}", addr
                                )));
                            }
                            self.write_mem(addr, self.regs.xmm[xmm_src][0], 8)?;
                            self.write_mem(addr + 8, self.regs.xmm[xmm_src][1], 8)?;
                        } else {
                            let xmm_dst = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                            self.regs.ymm_high[xmm_dst][0] = 0;
                            self.regs.ymm_high[xmm_dst][1] = 0;
                        }
                    } else {
                        if is_memory {
                            if aligned && addr & 0x1F != 0 {
                                return Err(Error::Emulator(format!(
                                    "VMOVAPS: unaligned memory access at {:#x}", addr
                                )));
                            }
                            self.write_mem(addr, self.regs.xmm[xmm_src][0], 8)?;
                            self.write_mem(addr + 8, self.regs.xmm[xmm_src][1], 8)?;
                            self.write_mem(addr + 16, self.regs.ymm_high[xmm_src][0], 8)?;
                            self.write_mem(addr + 24, self.regs.ymm_high[xmm_src][1], 8)?;
                        } else {
                            let xmm_dst = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                            self.regs.ymm_high[xmm_dst][0] = self.regs.ymm_high[xmm_src][0];
                            self.regs.ymm_high[xmm_dst][1] = self.regs.ymm_high[xmm_src][1];
                        }
                    }
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                // VMOVAPD/VMOVUPD (pp=1, 66 prefix)
                (1, 0x10) | (1, 0x28) => {
                    let aligned = opcode == 0x28;
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let xmm_dst = reg as usize;

                    if vex_l == 0 {
                        if is_memory {
                            if aligned && addr & 0xF != 0 {
                                return Err(Error::Emulator(format!(
                                    "VMOVAPD: unaligned memory access at {:#x}", addr
                                )));
                            }
                            self.regs.xmm[xmm_dst][0] = self.read_mem(addr, 8)?;
                            self.regs.xmm[xmm_dst][1] = self.read_mem(addr + 8, 8)?;
                        } else {
                            let xmm_src = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                        }
                        self.regs.ymm_high[xmm_dst][0] = 0;
                        self.regs.ymm_high[xmm_dst][1] = 0;
                    } else {
                        if is_memory {
                            if aligned && addr & 0x1F != 0 {
                                return Err(Error::Emulator(format!(
                                    "VMOVAPD: unaligned memory access at {:#x}", addr
                                )));
                            }
                            self.regs.xmm[xmm_dst][0] = self.read_mem(addr, 8)?;
                            self.regs.xmm[xmm_dst][1] = self.read_mem(addr + 8, 8)?;
                            self.regs.ymm_high[xmm_dst][0] = self.read_mem(addr + 16, 8)?;
                            self.regs.ymm_high[xmm_dst][1] = self.read_mem(addr + 24, 8)?;
                        } else {
                            let xmm_src = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                            self.regs.ymm_high[xmm_dst][0] = self.regs.ymm_high[xmm_src][0];
                            self.regs.ymm_high[xmm_dst][1] = self.regs.ymm_high[xmm_src][1];
                        }
                    }
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }
                (1, 0x11) | (1, 0x29) => {
                    let aligned = opcode == 0x29;
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let xmm_src = reg as usize;

                    if vex_l == 0 {
                        if is_memory {
                            if aligned && addr & 0xF != 0 {
                                return Err(Error::Emulator(format!(
                                    "VMOVAPD: unaligned memory access at {:#x}", addr
                                )));
                            }
                            self.write_mem(addr, self.regs.xmm[xmm_src][0], 8)?;
                            self.write_mem(addr + 8, self.regs.xmm[xmm_src][1], 8)?;
                        } else {
                            let xmm_dst = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                            self.regs.ymm_high[xmm_dst][0] = 0;
                            self.regs.ymm_high[xmm_dst][1] = 0;
                        }
                    } else {
                        if is_memory {
                            if aligned && addr & 0x1F != 0 {
                                return Err(Error::Emulator(format!(
                                    "VMOVAPD: unaligned memory access at {:#x}", addr
                                )));
                            }
                            self.write_mem(addr, self.regs.xmm[xmm_src][0], 8)?;
                            self.write_mem(addr + 8, self.regs.xmm[xmm_src][1], 8)?;
                            self.write_mem(addr + 16, self.regs.ymm_high[xmm_src][0], 8)?;
                            self.write_mem(addr + 24, self.regs.ymm_high[xmm_src][1], 8)?;
                        } else {
                            let xmm_dst = rm as usize;
                            self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                            self.regs.ymm_high[xmm_dst][0] = self.regs.ymm_high[xmm_src][0];
                            self.regs.ymm_high[xmm_dst][1] = self.regs.ymm_high[xmm_src][1];
                        }
                    }
                    self.regs.rip += ctx.cursor as u64;
                    return Ok(None);
                }

                // VPSHUFD/VPSHUFHW/VPSHUFLW (0x70)
                (_, 0x70) => {
                    return self.execute_vex_shuffle(ctx, vex_pp, vex_l);
                }

                // VCMPPS/VCMPPD/VCMPSS/VCMPSD (0xC2)
                (_, 0xC2) => {
                    return self.execute_vex_cmp(ctx, vex_pp, vex_l, vvvv);
                }

                // VEX packed integer shift by immediate (Group 12/13/14)
                // VPSLLW/VPSRAW/VPSRLW (0x71), VPSLLD/VPSRAD/VPSRLD (0x72), VPSLLQ/VPSRLQ/VPSLLDQ/VPSRLDQ (0x73)
                (1, 0x71) | (1, 0x72) | (1, 0x73) => {
                    return self.execute_vex_packed_shift_imm(ctx, vex_l, vvvv, opcode);
                }

                // VEX packed integer shift by XMM count
                // VPSRLW (0xD1), VPSRLD (0xD2), VPSRLQ (0xD3) - right logical
                // VPSRAW (0xE1), VPSRAD (0xE2) - right arithmetic
                // VPSLLW (0xF1), VPSLLD (0xF2), VPSLLQ (0xF3) - left logical
                (1, 0xD1) | (1, 0xD2) | (1, 0xD3) |
                (1, 0xE1) | (1, 0xE2) |
                (1, 0xF1) | (1, 0xF2) | (1, 0xF3) => {
                    return self.execute_vex_packed_shift_xmm(ctx, vex_l, vvvv, opcode);
                }

                _ => {}
            }
        }

        // VEX.0F3A encoded instructions (m_mmmm=3)
        if m_mmmm == 0x3 && vex_pp == 1 {
            match opcode {
                // VINSERTF128 ymm1, ymm2, xmm3/m128, imm8 (VEX.66.0F3A.W0 18 /r ib)
                0x18 => {
                    return self.execute_vinsertf128(ctx, vex_l, vvvv);
                }
                // VEXTRACTF128 xmm1/m128, ymm2, imm8 (VEX.66.0F3A.W0 19 /r ib)
                0x19 => {
                    return self.execute_vextractf128(ctx, vex_l);
                }
                // VPERM2F128 ymm1, ymm2, ymm3/m256, imm8 (VEX.66.0F3A.W0 06 /r ib)
                0x06 => {
                    return self.execute_vperm2f128(ctx, vex_l, vvvv);
                }
                _ => {}
            }
        }

        Err(Error::Emulator(format!(
            "unimplemented VEX instruction m={:#x} pp={} l={} opcode={:#x}",
            m_mmmm, vex_pp, vex_l, opcode
        )))
    }

    /// VEX-encoded arithmetic: VADDPS/PD, VMULPS/PD, VSUBPS/PD, VDIVPS/PD, VSQRTPS/PD, VMINPS/PD, VMAXPS/PD
    fn execute_vex_arith(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        // Determine operation type based on opcode
        let op: fn(f32, f32) -> f32 = match opcode {
            0x58 => |a, b| a + b,  // ADD
            0x59 => |a, b| a * b,  // MUL
            0x5C => |a, b| a - b,  // SUB
            0x5D => |a, b| a.min(b), // MIN
            0x5E => |a, b| a / b,  // DIV
            0x5F => |a, b| a.max(b), // MAX
            0x51 => |_a, b| b.sqrt(), // SQRT (unary, uses only src2)
            _ => unreachable!(),
        };
        let op_d: fn(f64, f64) -> f64 = match opcode {
            0x58 => |a, b| a + b,
            0x59 => |a, b| a * b,
            0x5C => |a, b| a - b,
            0x5D => |a, b| a.min(b),
            0x5E => |a, b| a / b,
            0x5F => |a, b| a.max(b),
            0x51 => |_a, b| b.sqrt(),
            _ => unreachable!(),
        };

        match vex_pp {
            0 => {
                // VEX.0F (NP) - packed single (PS)
                let (src2_lo, src2_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let src1_lo = self.regs.xmm[xmm_src1][0];
                let src1_hi = self.regs.xmm[xmm_src1][1];

                // Process 4 floats in low 128 bits
                let r0 = op(f32::from_bits(src1_lo as u32), f32::from_bits(src2_lo as u32));
                let r1 = op(f32::from_bits((src1_lo >> 32) as u32), f32::from_bits((src2_lo >> 32) as u32));
                let r2 = op(f32::from_bits(src1_hi as u32), f32::from_bits(src2_hi as u32));
                let r3 = op(f32::from_bits((src1_hi >> 32) as u32), f32::from_bits((src2_hi >> 32) as u32));
                self.regs.xmm[xmm_dst][0] = r0.to_bits() as u64 | ((r1.to_bits() as u64) << 32);
                self.regs.xmm[xmm_dst][1] = r2.to_bits() as u64 | ((r3.to_bits() as u64) << 32);

                if vex_l == 1 {
                    // YMM - process high 128 bits too
                    let (src2_hi2, src2_hi3) = if is_memory {
                        (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                    } else {
                        (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                    };
                    let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                    let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                    let r4 = op(f32::from_bits(src1_hi2 as u32), f32::from_bits(src2_hi2 as u32));
                    let r5 = op(f32::from_bits((src1_hi2 >> 32) as u32), f32::from_bits((src2_hi2 >> 32) as u32));
                    let r6 = op(f32::from_bits(src1_hi3 as u32), f32::from_bits(src2_hi3 as u32));
                    let r7 = op(f32::from_bits((src1_hi3 >> 32) as u32), f32::from_bits((src2_hi3 >> 32) as u32));
                    self.regs.ymm_high[xmm_dst][0] = r4.to_bits() as u64 | ((r5.to_bits() as u64) << 32);
                    self.regs.ymm_high[xmm_dst][1] = r6.to_bits() as u64 | ((r7.to_bits() as u64) << 32);
                } else {
                    // VEX.128 clears upper bits
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            1 => {
                // VEX.66.0F - packed double (PD)
                let (src2_lo, src2_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let src1_lo = self.regs.xmm[xmm_src1][0];
                let src1_hi = self.regs.xmm[xmm_src1][1];

                let r0 = op_d(f64::from_bits(src1_lo), f64::from_bits(src2_lo));
                let r1 = op_d(f64::from_bits(src1_hi), f64::from_bits(src2_hi));
                self.regs.xmm[xmm_dst][0] = r0.to_bits();
                self.regs.xmm[xmm_dst][1] = r1.to_bits();

                if vex_l == 1 {
                    let (src2_hi2, src2_hi3) = if is_memory {
                        (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                    } else {
                        (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                    };
                    let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                    let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                    let r2 = op_d(f64::from_bits(src1_hi2), f64::from_bits(src2_hi2));
                    let r3 = op_d(f64::from_bits(src1_hi3), f64::from_bits(src2_hi3));
                    self.regs.ymm_high[xmm_dst][0] = r2.to_bits();
                    self.regs.ymm_high[xmm_dst][1] = r3.to_bits();
                } else {
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            2 => {
                // VEX.F3.0F - scalar single (SS)
                let src2 = if is_memory {
                    f32::from_bits(self.read_mem(addr, 4)? as u32)
                } else {
                    f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
                };
                let src1 = f32::from_bits(self.regs.xmm[xmm_src1][0] as u32);
                let result = op(src1, src2);
                // Copy src1 to dst, then overwrite low 32 bits
                self.regs.xmm[xmm_dst][0] = (self.regs.xmm[xmm_src1][0] & !0xFFFFFFFF) | result.to_bits() as u64;
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
            3 => {
                // VEX.F2.0F - scalar double (SD)
                let src2 = if is_memory {
                    f64::from_bits(self.read_mem(addr, 8)?)
                } else {
                    f64::from_bits(self.regs.xmm[rm as usize][0])
                };
                let src1 = f64::from_bits(self.regs.xmm[xmm_src1][0]);
                let result = op_d(src1, src2);
                self.regs.xmm[xmm_dst][0] = result.to_bits();
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
            _ => unreachable!(),
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX-encoded logical: VANDPS/PD, VANDNPS/PD, VORPS/PD, VXORPS/PD
    fn execute_vex_logical(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        // Logical ops are the same for PS and PD (bitwise on 128/256 bits)
        let op: fn(u64, u64) -> u64 = match opcode {
            0x54 => |a, b| a & b,      // AND
            0x55 => |a, b| (!a) & b,   // ANDN
            0x56 => |a, b| a | b,      // OR
            0x57 => |a, b| a ^ b,      // XOR
            _ => unreachable!(),
        };

        let (src2_lo, src2_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let src1_lo = self.regs.xmm[xmm_src1][0];
        let src1_hi = self.regs.xmm[xmm_src1][1];

        self.regs.xmm[xmm_dst][0] = op(src1_lo, src2_lo);
        self.regs.xmm[xmm_dst][1] = op(src1_hi, src2_hi);

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = op(src1_hi2, src2_hi2);
            self.regs.ymm_high[xmm_dst][1] = op(src1_hi3, src2_hi3);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX-encoded unpack: VUNPCKLPS/PD, VUNPCKHPS/PD
    fn execute_vex_unpack(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        let (src2_lo, src2_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let src1_lo = self.regs.xmm[xmm_src1][0];
        let src1_hi = self.regs.xmm[xmm_src1][1];

        let is_high = opcode == 0x15;
        let is_pd = vex_pp == 1; // 66 prefix = PD

        if is_pd {
            // Double precision - interleave 64-bit elements
            if is_high {
                // VUNPCKHPD: dst[0]=src1[1], dst[1]=src2[1]
                self.regs.xmm[xmm_dst][0] = src1_hi;
                self.regs.xmm[xmm_dst][1] = src2_hi;
            } else {
                // VUNPCKLPD: dst[0]=src1[0], dst[1]=src2[0]
                self.regs.xmm[xmm_dst][0] = src1_lo;
                self.regs.xmm[xmm_dst][1] = src2_lo;
            }

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];

                if is_high {
                    self.regs.ymm_high[xmm_dst][0] = src1_hi3;
                    self.regs.ymm_high[xmm_dst][1] = src2_hi3;
                } else {
                    self.regs.ymm_high[xmm_dst][0] = src1_hi2;
                    self.regs.ymm_high[xmm_dst][1] = src2_hi2;
                }
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        } else {
            // Single precision - interleave 32-bit elements
            if is_high {
                // VUNPCKHPS: interleave high singles
                let d2 = src1_hi as u32;
                let d3 = (src1_hi >> 32) as u32;
                let s2 = src2_hi as u32;
                let s3 = (src2_hi >> 32) as u32;
                self.regs.xmm[xmm_dst][0] = d2 as u64 | ((s2 as u64) << 32);
                self.regs.xmm[xmm_dst][1] = d3 as u64 | ((s3 as u64) << 32);
            } else {
                // VUNPCKLPS: interleave low singles
                let d0 = src1_lo as u32;
                let d1 = (src1_lo >> 32) as u32;
                let s0 = src2_lo as u32;
                let s1 = (src2_lo >> 32) as u32;
                self.regs.xmm[xmm_dst][0] = d0 as u64 | ((s0 as u64) << 32);
                self.regs.xmm[xmm_dst][1] = d1 as u64 | ((s1 as u64) << 32);
            }

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];

                if is_high {
                    let d6 = src1_hi3 as u32;
                    let d7 = (src1_hi3 >> 32) as u32;
                    let s6 = src2_hi3 as u32;
                    let s7 = (src2_hi3 >> 32) as u32;
                    self.regs.ymm_high[xmm_dst][0] = d6 as u64 | ((s6 as u64) << 32);
                    self.regs.ymm_high[xmm_dst][1] = d7 as u64 | ((s7 as u64) << 32);
                } else {
                    let d4 = src1_hi2 as u32;
                    let d5 = (src1_hi2 >> 32) as u32;
                    let s4 = src2_hi2 as u32;
                    let s5 = (src2_hi2 >> 32) as u32;
                    self.regs.ymm_high[xmm_dst][0] = d4 as u64 | ((s4 as u64) << 32);
                    self.regs.ymm_high[xmm_dst][1] = d5 as u64 | ((s5 as u64) << 32);
                }
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX conversion 0x5A: VCVTPS2PD, VCVTPD2PS, VCVTSS2SD, VCVTSD2SS
    fn execute_vex_cvt_fp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        match vex_pp {
            0 => {
                // VCVTPS2PD: convert packed single to packed double
                // XMM: 2 singles -> 2 doubles, YMM: 4 singles -> 4 doubles
                if vex_l == 0 {
                    // 128-bit: read 64 bits (2 singles), produce 128 bits (2 doubles)
                    let src = if is_memory {
                        self.read_mem(addr, 8)?
                    } else {
                        self.regs.xmm[rm as usize][0]
                    };
                    let s0 = f32::from_bits(src as u32);
                    let s1 = f32::from_bits((src >> 32) as u32);
                    self.regs.xmm[xmm_dst][0] = (s0 as f64).to_bits();
                    self.regs.xmm[xmm_dst][1] = (s1 as f64).to_bits();
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                } else {
                    // 256-bit: read 128 bits (4 singles), produce 256 bits (4 doubles)
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let s0 = f32::from_bits(src_lo as u32);
                    let s1 = f32::from_bits((src_lo >> 32) as u32);
                    let s2 = f32::from_bits(src_hi as u32);
                    let s3 = f32::from_bits((src_hi >> 32) as u32);
                    self.regs.xmm[xmm_dst][0] = (s0 as f64).to_bits();
                    self.regs.xmm[xmm_dst][1] = (s1 as f64).to_bits();
                    self.regs.ymm_high[xmm_dst][0] = (s2 as f64).to_bits();
                    self.regs.ymm_high[xmm_dst][1] = (s3 as f64).to_bits();
                }
            }
            1 => {
                // VCVTPD2PS: convert packed double to packed single
                // XMM: 2 doubles -> 2 singles (in low 64 bits), YMM: 4 doubles -> 4 singles
                if vex_l == 0 {
                    // 128-bit: read 128 bits (2 doubles), produce 64 bits (2 singles)
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let d0 = f64::from_bits(src_lo);
                    let d1 = f64::from_bits(src_hi);
                    let s0 = (d0 as f32).to_bits() as u64;
                    let s1 = (d1 as f32).to_bits() as u64;
                    self.regs.xmm[xmm_dst][0] = s0 | (s1 << 32);
                    self.regs.xmm[xmm_dst][1] = 0;
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                } else {
                    // 256-bit: read 256 bits (4 doubles), produce 128 bits (4 singles)
                    let (src0, src1, src2, src3) = if is_memory {
                        (
                            self.read_mem(addr, 8)?,
                            self.read_mem(addr + 8, 8)?,
                            self.read_mem(addr + 16, 8)?,
                            self.read_mem(addr + 24, 8)?,
                        )
                    } else {
                        (
                            self.regs.xmm[rm as usize][0],
                            self.regs.xmm[rm as usize][1],
                            self.regs.ymm_high[rm as usize][0],
                            self.regs.ymm_high[rm as usize][1],
                        )
                    };
                    let d0 = f64::from_bits(src0);
                    let d1 = f64::from_bits(src1);
                    let d2 = f64::from_bits(src2);
                    let d3 = f64::from_bits(src3);
                    let s0 = (d0 as f32).to_bits() as u64;
                    let s1 = (d1 as f32).to_bits() as u64;
                    let s2 = (d2 as f32).to_bits() as u64;
                    let s3 = (d3 as f32).to_bits() as u64;
                    self.regs.xmm[xmm_dst][0] = s0 | (s1 << 32);
                    self.regs.xmm[xmm_dst][1] = s2 | (s3 << 32);
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            2 => {
                // VCVTSS2SD: convert scalar single to scalar double
                let xmm_src1 = vvvv as usize;
                let src = if is_memory {
                    f32::from_bits(self.read_mem(addr, 4)? as u32)
                } else {
                    f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
                };
                let result = (src as f64).to_bits();
                self.regs.xmm[xmm_dst][0] = result;
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
            3 => {
                // VCVTSD2SS: convert scalar double to scalar single
                let xmm_src1 = vvvv as usize;
                let src = if is_memory {
                    f64::from_bits(self.read_mem(addr, 8)?)
                } else {
                    f64::from_bits(self.regs.xmm[rm as usize][0])
                };
                let result = (src as f32).to_bits() as u64;
                // Preserve upper bits from src1, write result to low 32 bits
                self.regs.xmm[xmm_dst][0] = (self.regs.xmm[xmm_src1][0] & !0xFFFFFFFF) | result;
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
            _ => unreachable!(),
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX conversion 0x5B: VCVTDQ2PS, VCVTPS2DQ, VCVTTPS2DQ
    fn execute_vex_cvt_dq_ps(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        // Helper to saturate f32 to i32
        fn f32_to_i32_saturate(f: f32) -> i32 {
            if f.is_nan() {
                i32::MIN
            } else if f >= i32::MAX as f32 {
                i32::MIN // Intel returns 0x80000000 for overflow
            } else if f <= i32::MIN as f32 {
                i32::MIN
            } else {
                f as i32
            }
        }

        fn f32_to_i32_truncate(f: f32) -> i32 {
            if f.is_nan() {
                i32::MIN
            } else if f >= i32::MAX as f32 {
                i32::MIN
            } else if f <= i32::MIN as f32 {
                i32::MIN
            } else {
                f.trunc() as i32
            }
        }

        match vex_pp {
            0 => {
                // VCVTDQ2PS: convert packed dword integers to packed singles
                if vex_l == 0 {
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let i0 = src_lo as i32;
                    let i1 = (src_lo >> 32) as i32;
                    let i2 = src_hi as i32;
                    let i3 = (src_hi >> 32) as i32;
                    let f0 = (i0 as f32).to_bits() as u64;
                    let f1 = (i1 as f32).to_bits() as u64;
                    let f2 = (i2 as f32).to_bits() as u64;
                    let f3 = (i3 as f32).to_bits() as u64;
                    self.regs.xmm[xmm_dst][0] = f0 | (f1 << 32);
                    self.regs.xmm[xmm_dst][1] = f2 | (f3 << 32);
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                } else {
                    let (src0, src1, src2, src3) = if is_memory {
                        (
                            self.read_mem(addr, 8)?,
                            self.read_mem(addr + 8, 8)?,
                            self.read_mem(addr + 16, 8)?,
                            self.read_mem(addr + 24, 8)?,
                        )
                    } else {
                        (
                            self.regs.xmm[rm as usize][0],
                            self.regs.xmm[rm as usize][1],
                            self.regs.ymm_high[rm as usize][0],
                            self.regs.ymm_high[rm as usize][1],
                        )
                    };
                    let convert = |v: u64| -> u64 {
                        let i0 = v as i32;
                        let i1 = (v >> 32) as i32;
                        let f0 = (i0 as f32).to_bits() as u64;
                        let f1 = (i1 as f32).to_bits() as u64;
                        f0 | (f1 << 32)
                    };
                    self.regs.xmm[xmm_dst][0] = convert(src0);
                    self.regs.xmm[xmm_dst][1] = convert(src1);
                    self.regs.ymm_high[xmm_dst][0] = convert(src2);
                    self.regs.ymm_high[xmm_dst][1] = convert(src3);
                }
            }
            1 => {
                // VCVTPS2DQ: convert packed singles to packed dword integers (rounding)
                if vex_l == 0 {
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let f0 = f32::from_bits(src_lo as u32);
                    let f1 = f32::from_bits((src_lo >> 32) as u32);
                    let f2 = f32::from_bits(src_hi as u32);
                    let f3 = f32::from_bits((src_hi >> 32) as u32);
                    // Round to nearest (default MXCSR rounding mode)
                    let i0 = f32_to_i32_saturate(f0.round()) as u32 as u64;
                    let i1 = f32_to_i32_saturate(f1.round()) as u32 as u64;
                    let i2 = f32_to_i32_saturate(f2.round()) as u32 as u64;
                    let i3 = f32_to_i32_saturate(f3.round()) as u32 as u64;
                    self.regs.xmm[xmm_dst][0] = i0 | (i1 << 32);
                    self.regs.xmm[xmm_dst][1] = i2 | (i3 << 32);
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                } else {
                    let (src0, src1, src2, src3) = if is_memory {
                        (
                            self.read_mem(addr, 8)?,
                            self.read_mem(addr + 8, 8)?,
                            self.read_mem(addr + 16, 8)?,
                            self.read_mem(addr + 24, 8)?,
                        )
                    } else {
                        (
                            self.regs.xmm[rm as usize][0],
                            self.regs.xmm[rm as usize][1],
                            self.regs.ymm_high[rm as usize][0],
                            self.regs.ymm_high[rm as usize][1],
                        )
                    };
                    let convert = |v: u64| -> u64 {
                        let f0 = f32::from_bits(v as u32);
                        let f1 = f32::from_bits((v >> 32) as u32);
                        let i0 = f32_to_i32_saturate(f0.round()) as u32 as u64;
                        let i1 = f32_to_i32_saturate(f1.round()) as u32 as u64;
                        i0 | (i1 << 32)
                    };
                    self.regs.xmm[xmm_dst][0] = convert(src0);
                    self.regs.xmm[xmm_dst][1] = convert(src1);
                    self.regs.ymm_high[xmm_dst][0] = convert(src2);
                    self.regs.ymm_high[xmm_dst][1] = convert(src3);
                }
            }
            2 => {
                // VCVTTPS2DQ: convert packed singles to packed dword integers (truncate)
                if vex_l == 0 {
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let f0 = f32::from_bits(src_lo as u32);
                    let f1 = f32::from_bits((src_lo >> 32) as u32);
                    let f2 = f32::from_bits(src_hi as u32);
                    let f3 = f32::from_bits((src_hi >> 32) as u32);
                    let i0 = f32_to_i32_truncate(f0) as u32 as u64;
                    let i1 = f32_to_i32_truncate(f1) as u32 as u64;
                    let i2 = f32_to_i32_truncate(f2) as u32 as u64;
                    let i3 = f32_to_i32_truncate(f3) as u32 as u64;
                    self.regs.xmm[xmm_dst][0] = i0 | (i1 << 32);
                    self.regs.xmm[xmm_dst][1] = i2 | (i3 << 32);
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                } else {
                    let (src0, src1, src2, src3) = if is_memory {
                        (
                            self.read_mem(addr, 8)?,
                            self.read_mem(addr + 8, 8)?,
                            self.read_mem(addr + 16, 8)?,
                            self.read_mem(addr + 24, 8)?,
                        )
                    } else {
                        (
                            self.regs.xmm[rm as usize][0],
                            self.regs.xmm[rm as usize][1],
                            self.regs.ymm_high[rm as usize][0],
                            self.regs.ymm_high[rm as usize][1],
                        )
                    };
                    let convert = |v: u64| -> u64 {
                        let f0 = f32::from_bits(v as u32);
                        let f1 = f32::from_bits((v >> 32) as u32);
                        let i0 = f32_to_i32_truncate(f0) as u32 as u64;
                        let i1 = f32_to_i32_truncate(f1) as u32 as u64;
                        i0 | (i1 << 32)
                    };
                    self.regs.xmm[xmm_dst][0] = convert(src0);
                    self.regs.xmm[xmm_dst][1] = convert(src1);
                    self.regs.ymm_high[xmm_dst][0] = convert(src2);
                    self.regs.ymm_high[xmm_dst][1] = convert(src3);
                }
            }
            _ => {
                return Err(Error::Emulator(format!(
                    "unimplemented VEX 0x5B with pp={}",
                    vex_pp
                )));
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX conversion 0xE6: VCVTTPD2DQ, VCVTDQ2PD, VCVTPD2DQ
    fn execute_vex_cvt_pd_dq(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        // Helper to saturate f64 to i32
        fn f64_to_i32_saturate(f: f64) -> i32 {
            if f.is_nan() {
                i32::MIN
            } else if f >= i32::MAX as f64 {
                i32::MIN
            } else if f <= i32::MIN as f64 {
                i32::MIN
            } else {
                f as i32
            }
        }

        fn f64_to_i32_truncate(f: f64) -> i32 {
            if f.is_nan() {
                i32::MIN
            } else if f >= i32::MAX as f64 {
                i32::MIN
            } else if f <= i32::MIN as f64 {
                i32::MIN
            } else {
                f.trunc() as i32
            }
        }

        match vex_pp {
            1 => {
                // VCVTTPD2DQ: convert packed double to packed dword (truncate)
                if vex_l == 0 {
                    // 128-bit: 2 doubles -> 2 dwords in low 64 bits
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let d0 = f64::from_bits(src_lo);
                    let d1 = f64::from_bits(src_hi);
                    let i0 = f64_to_i32_truncate(d0) as u32 as u64;
                    let i1 = f64_to_i32_truncate(d1) as u32 as u64;
                    self.regs.xmm[xmm_dst][0] = i0 | (i1 << 32);
                    self.regs.xmm[xmm_dst][1] = 0;
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                } else {
                    // 256-bit: 4 doubles -> 4 dwords in low 128 bits
                    let (src0, src1, src2, src3) = if is_memory {
                        (
                            self.read_mem(addr, 8)?,
                            self.read_mem(addr + 8, 8)?,
                            self.read_mem(addr + 16, 8)?,
                            self.read_mem(addr + 24, 8)?,
                        )
                    } else {
                        (
                            self.regs.xmm[rm as usize][0],
                            self.regs.xmm[rm as usize][1],
                            self.regs.ymm_high[rm as usize][0],
                            self.regs.ymm_high[rm as usize][1],
                        )
                    };
                    let d0 = f64::from_bits(src0);
                    let d1 = f64::from_bits(src1);
                    let d2 = f64::from_bits(src2);
                    let d3 = f64::from_bits(src3);
                    let i0 = f64_to_i32_truncate(d0) as u32 as u64;
                    let i1 = f64_to_i32_truncate(d1) as u32 as u64;
                    let i2 = f64_to_i32_truncate(d2) as u32 as u64;
                    let i3 = f64_to_i32_truncate(d3) as u32 as u64;
                    self.regs.xmm[xmm_dst][0] = i0 | (i1 << 32);
                    self.regs.xmm[xmm_dst][1] = i2 | (i3 << 32);
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            2 => {
                // VCVTDQ2PD: convert packed dword to packed double
                if vex_l == 0 {
                    // 128-bit: 2 dwords (64 bits) -> 2 doubles
                    let src = if is_memory {
                        self.read_mem(addr, 8)?
                    } else {
                        self.regs.xmm[rm as usize][0]
                    };
                    let i0 = src as i32;
                    let i1 = (src >> 32) as i32;
                    self.regs.xmm[xmm_dst][0] = (i0 as f64).to_bits();
                    self.regs.xmm[xmm_dst][1] = (i1 as f64).to_bits();
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                } else {
                    // 256-bit: 4 dwords (128 bits) -> 4 doubles
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let i0 = src_lo as i32;
                    let i1 = (src_lo >> 32) as i32;
                    let i2 = src_hi as i32;
                    let i3 = (src_hi >> 32) as i32;
                    self.regs.xmm[xmm_dst][0] = (i0 as f64).to_bits();
                    self.regs.xmm[xmm_dst][1] = (i1 as f64).to_bits();
                    self.regs.ymm_high[xmm_dst][0] = (i2 as f64).to_bits();
                    self.regs.ymm_high[xmm_dst][1] = (i3 as f64).to_bits();
                }
            }
            3 => {
                // VCVTPD2DQ: convert packed double to packed dword (rounding)
                if vex_l == 0 {
                    // 128-bit: 2 doubles -> 2 dwords in low 64 bits
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let d0 = f64::from_bits(src_lo);
                    let d1 = f64::from_bits(src_hi);
                    let i0 = f64_to_i32_saturate(d0.round()) as u32 as u64;
                    let i1 = f64_to_i32_saturate(d1.round()) as u32 as u64;
                    self.regs.xmm[xmm_dst][0] = i0 | (i1 << 32);
                    self.regs.xmm[xmm_dst][1] = 0;
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                } else {
                    // 256-bit: 4 doubles -> 4 dwords in low 128 bits
                    let (src0, src1, src2, src3) = if is_memory {
                        (
                            self.read_mem(addr, 8)?,
                            self.read_mem(addr + 8, 8)?,
                            self.read_mem(addr + 16, 8)?,
                            self.read_mem(addr + 24, 8)?,
                        )
                    } else {
                        (
                            self.regs.xmm[rm as usize][0],
                            self.regs.xmm[rm as usize][1],
                            self.regs.ymm_high[rm as usize][0],
                            self.regs.ymm_high[rm as usize][1],
                        )
                    };
                    let d0 = f64::from_bits(src0);
                    let d1 = f64::from_bits(src1);
                    let d2 = f64::from_bits(src2);
                    let d3 = f64::from_bits(src3);
                    let i0 = f64_to_i32_saturate(d0.round()) as u32 as u64;
                    let i1 = f64_to_i32_saturate(d1.round()) as u32 as u64;
                    let i2 = f64_to_i32_saturate(d2.round()) as u32 as u64;
                    let i3 = f64_to_i32_saturate(d3.round()) as u32 as u64;
                    self.regs.xmm[xmm_dst][0] = i0 | (i1 << 32);
                    self.regs.xmm[xmm_dst][1] = i2 | (i3 << 32);
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            _ => {
                return Err(Error::Emulator(format!(
                    "unimplemented VEX 0xE6 with pp={}",
                    vex_pp
                )));
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX scalar int-to-float: VCVTSI2SS, VCVTSI2SD
    fn execute_vex_cvtsi2s(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_w: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        // Source is 32-bit or 64-bit GPR/memory based on VEX.W
        let src = if is_memory {
            if vex_w != 0 {
                self.read_mem(addr, 8)? as i64
            } else {
                self.read_mem(addr, 4)? as i32 as i64
            }
        } else {
            if vex_w != 0 {
                self.get_reg(rm, 8) as i64
            } else {
                self.get_reg(rm, 4) as i32 as i64
            }
        };

        match vex_pp {
            2 => {
                // VCVTSI2SS: convert int to scalar single
                let result = (src as f32).to_bits() as u64;
                self.regs.xmm[xmm_dst][0] = (self.regs.xmm[xmm_src1][0] & !0xFFFFFFFF) | result;
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
            }
            3 => {
                // VCVTSI2SD: convert int to scalar double
                let result = (src as f64).to_bits();
                self.regs.xmm[xmm_dst][0] = result;
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
            }
            _ => unreachable!(),
        }
        self.regs.ymm_high[xmm_dst][0] = 0;
        self.regs.ymm_high[xmm_dst][1] = 0;

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX truncating float-to-int: VCVTTSS2SI, VCVTTSD2SI
    fn execute_vex_cvtts2si(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_w: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        let result = match vex_pp {
            2 => {
                // VCVTTSS2SI: convert scalar single to int (truncate)
                let src = if is_memory {
                    f32::from_bits(self.read_mem(addr, 4)? as u32)
                } else {
                    f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
                };
                if vex_w != 0 {
                    // 64-bit result
                    if src.is_nan() || src >= i64::MAX as f32 || src <= i64::MIN as f32 {
                        i64::MIN as u64
                    } else {
                        src.trunc() as i64 as u64
                    }
                } else {
                    // 32-bit result
                    if src.is_nan() || src >= i32::MAX as f32 || src <= i32::MIN as f32 {
                        i32::MIN as u32 as u64
                    } else {
                        src.trunc() as i32 as u32 as u64
                    }
                }
            }
            3 => {
                // VCVTTSD2SI: convert scalar double to int (truncate)
                let src = if is_memory {
                    f64::from_bits(self.read_mem(addr, 8)?)
                } else {
                    f64::from_bits(self.regs.xmm[rm as usize][0])
                };
                if vex_w != 0 {
                    if src.is_nan() || src >= i64::MAX as f64 || src <= i64::MIN as f64 {
                        i64::MIN as u64
                    } else {
                        src.trunc() as i64 as u64
                    }
                } else {
                    if src.is_nan() || src >= i32::MAX as f64 || src <= i32::MIN as f64 {
                        i32::MIN as u32 as u64
                    } else {
                        src.trunc() as i32 as u32 as u64
                    }
                }
            }
            _ => unreachable!(),
        };

        let size = if vex_w != 0 { 8 } else { 4 };
        self.set_reg(reg, result, size);

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX rounding float-to-int: VCVTSS2SI, VCVTSD2SI
    fn execute_vex_cvts2si(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_w: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        let result = match vex_pp {
            2 => {
                // VCVTSS2SI: convert scalar single to int (round)
                let src = if is_memory {
                    f32::from_bits(self.read_mem(addr, 4)? as u32)
                } else {
                    f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
                };
                if vex_w != 0 {
                    if src.is_nan() || src >= i64::MAX as f32 || src <= i64::MIN as f32 {
                        i64::MIN as u64
                    } else {
                        src.round() as i64 as u64
                    }
                } else {
                    if src.is_nan() || src >= i32::MAX as f32 || src <= i32::MIN as f32 {
                        i32::MIN as u32 as u64
                    } else {
                        src.round() as i32 as u32 as u64
                    }
                }
            }
            3 => {
                // VCVTSD2SI: convert scalar double to int (round)
                let src = if is_memory {
                    f64::from_bits(self.read_mem(addr, 8)?)
                } else {
                    f64::from_bits(self.regs.xmm[rm as usize][0])
                };
                if vex_w != 0 {
                    if src.is_nan() || src >= i64::MAX as f64 || src <= i64::MIN as f64 {
                        i64::MIN as u64
                    } else {
                        src.round() as i64 as u64
                    }
                } else {
                    if src.is_nan() || src >= i32::MAX as f64 || src <= i32::MIN as f64 {
                        i32::MIN as u32 as u64
                    } else {
                        src.round() as i32 as u32 as u64
                    }
                }
            }
            _ => unreachable!(),
        };

        let size = if vex_w != 0 { 8 } else { 4 };
        self.set_reg(reg, result, size);

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX shuffle: VSHUFPS, VSHUFPD
    fn execute_vex_shufp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        if vex_pp == 0 {
            // VSHUFPS: shuffle packed singles
            let (src2_lo, src2_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            let src1_lo = self.regs.xmm[xmm_src1][0];
            let src1_hi = self.regs.xmm[xmm_src1][1];

            // Extract 4 singles from src1 and src2
            let src1 = [
                src1_lo as u32,
                (src1_lo >> 32) as u32,
                src1_hi as u32,
                (src1_hi >> 32) as u32,
            ];
            let src2 = [
                src2_lo as u32,
                (src2_lo >> 32) as u32,
                src2_hi as u32,
                (src2_hi >> 32) as u32,
            ];

            // dst[0] = src1[imm[1:0]], dst[1] = src1[imm[3:2]]
            // dst[2] = src2[imm[5:4]], dst[3] = src2[imm[7:6]]
            let d0 = src1[(imm8 & 0x03) as usize] as u64;
            let d1 = src1[((imm8 >> 2) & 0x03) as usize] as u64;
            let d2 = src2[((imm8 >> 4) & 0x03) as usize] as u64;
            let d3 = src2[((imm8 >> 6) & 0x03) as usize] as u64;

            self.regs.xmm[xmm_dst][0] = d0 | (d1 << 32);
            self.regs.xmm[xmm_dst][1] = d2 | (d3 << 32);

            if vex_l == 1 {
                // YMM: repeat for upper 128 bits
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];

                let src1_u = [
                    src1_hi2 as u32,
                    (src1_hi2 >> 32) as u32,
                    src1_hi3 as u32,
                    (src1_hi3 >> 32) as u32,
                ];
                let src2_u = [
                    src2_hi2 as u32,
                    (src2_hi2 >> 32) as u32,
                    src2_hi3 as u32,
                    (src2_hi3 >> 32) as u32,
                ];

                let d4 = src1_u[(imm8 & 0x03) as usize] as u64;
                let d5 = src1_u[((imm8 >> 2) & 0x03) as usize] as u64;
                let d6 = src2_u[((imm8 >> 4) & 0x03) as usize] as u64;
                let d7 = src2_u[((imm8 >> 6) & 0x03) as usize] as u64;

                self.regs.ymm_high[xmm_dst][0] = d4 | (d5 << 32);
                self.regs.ymm_high[xmm_dst][1] = d6 | (d7 << 32);
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        } else {
            // VSHUFPD: shuffle packed doubles
            let (src2_lo, src2_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            let src1_lo = self.regs.xmm[xmm_src1][0];
            let src1_hi = self.regs.xmm[xmm_src1][1];

            // dst[0] = src1[imm[0]], dst[1] = src2[imm[1]]
            let d0 = if (imm8 & 0x01) == 0 { src1_lo } else { src1_hi };
            let d1 = if (imm8 & 0x02) == 0 { src2_lo } else { src2_hi };

            self.regs.xmm[xmm_dst][0] = d0;
            self.regs.xmm[xmm_dst][1] = d1;

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];

                let d2 = if (imm8 & 0x04) == 0 { src1_hi2 } else { src1_hi3 };
                let d3 = if (imm8 & 0x08) == 0 { src2_hi2 } else { src2_hi3 };

                self.regs.ymm_high[xmm_dst][0] = d2;
                self.regs.ymm_high[xmm_dst][1] = d3;
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX move mask: VMOVMSKPS, VMOVMSKPD
    fn execute_vex_movmskp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, _, _) = self.decode_modrm(ctx)?;
        if is_memory {
            return Err(Error::Emulator("VMOVMSK* requires XMM/YMM source".to_string()));
        }

        let xmm_src = rm as usize;
        let mut result = 0u64;

        if vex_pp == 0 {
            // VMOVMSKPS: extract sign bits of singles
            let lo = self.regs.xmm[xmm_src][0];
            let hi = self.regs.xmm[xmm_src][1];

            result |= ((lo >> 31) & 1) as u64;
            result |= ((lo >> 63) & 1) << 1;
            result |= ((hi >> 31) & 1) << 2;
            result |= ((hi >> 63) & 1) << 3;

            if vex_l == 1 {
                let hi2 = self.regs.ymm_high[xmm_src][0];
                let hi3 = self.regs.ymm_high[xmm_src][1];

                result |= ((hi2 >> 31) & 1) << 4;
                result |= ((hi2 >> 63) & 1) << 5;
                result |= ((hi3 >> 31) & 1) << 6;
                result |= ((hi3 >> 63) & 1) << 7;
            }
        } else {
            // VMOVMSKPD: extract sign bits of doubles
            let lo = self.regs.xmm[xmm_src][0];
            let hi = self.regs.xmm[xmm_src][1];

            result |= ((lo >> 63) & 1) as u64;
            result |= ((hi >> 63) & 1) << 1;

            if vex_l == 1 {
                let hi2 = self.regs.ymm_high[xmm_src][0];
                let hi3 = self.regs.ymm_high[xmm_src][1];

                result |= ((hi2 >> 63) & 1) << 2;
                result |= ((hi3 >> 63) & 1) << 3;
            }
        }

        self.set_reg(reg, result, 4);

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX reciprocal square root: VRSQRTPS, VRSQRTSS
    fn execute_vex_rsqrt(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        if vex_pp == 2 {
            // VRSQRTSS: scalar single
            let xmm_src1 = vvvv as usize;
            let src = if is_memory {
                f32::from_bits(self.read_mem(addr, 4)? as u32)
            } else {
                f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
            };
            let result = (1.0f32 / src.sqrt()).to_bits() as u64;
            self.regs.xmm[xmm_dst][0] = (self.regs.xmm[xmm_src1][0] & !0xFFFFFFFF) | result;
            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        } else {
            // VRSQRTPS: packed singles
            let rsqrt = |v: u64| -> u64 {
                let f0 = f32::from_bits(v as u32);
                let f1 = f32::from_bits((v >> 32) as u32);
                let r0 = (1.0f32 / f0.sqrt()).to_bits() as u64;
                let r1 = (1.0f32 / f1.sqrt()).to_bits() as u64;
                r0 | (r1 << 32)
            };

            if vex_l == 0 {
                let (src_lo, src_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                self.regs.xmm[xmm_dst][0] = rsqrt(src_lo);
                self.regs.xmm[xmm_dst][1] = rsqrt(src_hi);
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            } else {
                let (src0, src1, src2, src3) = if is_memory {
                    (
                        self.read_mem(addr, 8)?,
                        self.read_mem(addr + 8, 8)?,
                        self.read_mem(addr + 16, 8)?,
                        self.read_mem(addr + 24, 8)?,
                    )
                } else {
                    (
                        self.regs.xmm[rm as usize][0],
                        self.regs.xmm[rm as usize][1],
                        self.regs.ymm_high[rm as usize][0],
                        self.regs.ymm_high[rm as usize][1],
                    )
                };
                self.regs.xmm[xmm_dst][0] = rsqrt(src0);
                self.regs.xmm[xmm_dst][1] = rsqrt(src1);
                self.regs.ymm_high[xmm_dst][0] = rsqrt(src2);
                self.regs.ymm_high[xmm_dst][1] = rsqrt(src3);
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX reciprocal: VRCPPS, VRCPSS
    fn execute_vex_rcp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        if vex_pp == 2 {
            // VRCPSS: scalar single
            let xmm_src1 = vvvv as usize;
            let src = if is_memory {
                f32::from_bits(self.read_mem(addr, 4)? as u32)
            } else {
                f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
            };
            let result = (1.0f32 / src).to_bits() as u64;
            self.regs.xmm[xmm_dst][0] = (self.regs.xmm[xmm_src1][0] & !0xFFFFFFFF) | result;
            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        } else {
            // VRCPPS: packed singles
            let rcp = |v: u64| -> u64 {
                let f0 = f32::from_bits(v as u32);
                let f1 = f32::from_bits((v >> 32) as u32);
                let r0 = (1.0f32 / f0).to_bits() as u64;
                let r1 = (1.0f32 / f1).to_bits() as u64;
                r0 | (r1 << 32)
            };

            if vex_l == 0 {
                let (src_lo, src_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                self.regs.xmm[xmm_dst][0] = rcp(src_lo);
                self.regs.xmm[xmm_dst][1] = rcp(src_hi);
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            } else {
                let (src0, src1, src2, src3) = if is_memory {
                    (
                        self.read_mem(addr, 8)?,
                        self.read_mem(addr + 8, 8)?,
                        self.read_mem(addr + 16, 8)?,
                        self.read_mem(addr + 24, 8)?,
                    )
                } else {
                    (
                        self.regs.xmm[rm as usize][0],
                        self.regs.xmm[rm as usize][1],
                        self.regs.ymm_high[rm as usize][0],
                        self.regs.ymm_high[rm as usize][1],
                    )
                };
                self.regs.xmm[xmm_dst][0] = rcp(src0);
                self.regs.xmm[xmm_dst][1] = rcp(src1);
                self.regs.ymm_high[xmm_dst][0] = rcp(src2);
                self.regs.ymm_high[xmm_dst][1] = rcp(src3);
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX zero: VZEROUPPER, VZEROALL
    fn execute_vex_vzero(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
    ) -> Result<Option<VcpuExit>> {
        if vex_l == 0 {
            // VZEROUPPER: zero upper 128 bits of all YMM registers
            for i in 0..16 {
                self.regs.ymm_high[i][0] = 0;
                self.regs.ymm_high[i][1] = 0;
            }
        } else {
            // VZEROALL: zero all YMM registers
            for i in 0..16 {
                self.regs.xmm[i][0] = 0;
                self.regs.xmm[i][1] = 0;
                self.regs.ymm_high[i][0] = 0;
                self.regs.ymm_high[i][1] = 0;
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX MXCSR: VLDMXCSR, VSTMXCSR
    fn execute_vex_ldst_mxcsr(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        let modrm = ctx.peek_u8()?;
        let reg_op = (modrm >> 3) & 0x07;
        let (_, _, is_memory, addr, _) = self.decode_modrm(ctx)?;

        if !is_memory {
            return Err(Error::Emulator("VLDMXCSR/VSTMXCSR require memory operand".to_string()));
        }

        match reg_op {
            2 => {
                // VLDMXCSR: load MXCSR from memory
                // Treat as NOP - we don't emulate MXCSR rounding/exception behavior
                let _ = self.read_mem(addr, 4)?;
            }
            3 => {
                // VSTMXCSR: store MXCSR to memory
                // Return default MXCSR value (0x1F80)
                self.write_mem(addr, 0x1F80u64, 4)?;
            }
            _ => {
                return Err(Error::Emulator(format!("invalid VEX 0xAE /{}", reg_op)));
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX add-subtract: VADDSUBPS, VADDSUBPD
    fn execute_vex_addsubp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        let (src2_lo, src2_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let src1_lo = self.regs.xmm[xmm_src1][0];
        let src1_hi = self.regs.xmm[xmm_src1][1];

        if vex_pp == 3 {
            // VADDSUBPS: alternating sub/add for singles
            // dst[0] = src1[0] - src2[0], dst[1] = src1[1] + src2[1], etc.
            let addsub_ps = |a: u64, b: u64| -> u64 {
                let a0 = f32::from_bits(a as u32);
                let a1 = f32::from_bits((a >> 32) as u32);
                let b0 = f32::from_bits(b as u32);
                let b1 = f32::from_bits((b >> 32) as u32);
                let r0 = (a0 - b0).to_bits() as u64;
                let r1 = (a1 + b1).to_bits() as u64;
                r0 | (r1 << 32)
            };

            self.regs.xmm[xmm_dst][0] = addsub_ps(src1_lo, src2_lo);
            self.regs.xmm[xmm_dst][1] = addsub_ps(src1_hi, src2_hi);

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = addsub_ps(src1_hi2, src2_hi2);
                self.regs.ymm_high[xmm_dst][1] = addsub_ps(src1_hi3, src2_hi3);
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        } else {
            // VADDSUBPD: alternating sub/add for doubles
            // dst[0] = src1[0] - src2[0], dst[1] = src1[1] + src2[1]
            let a0 = f64::from_bits(src1_lo);
            let a1 = f64::from_bits(src1_hi);
            let b0 = f64::from_bits(src2_lo);
            let b1 = f64::from_bits(src2_hi);
            self.regs.xmm[xmm_dst][0] = (a0 - b0).to_bits();
            self.regs.xmm[xmm_dst][1] = (a1 + b1).to_bits();

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                let a2 = f64::from_bits(src1_hi2);
                let a3 = f64::from_bits(src1_hi3);
                let b2 = f64::from_bits(src2_hi2);
                let b3 = f64::from_bits(src2_hi3);
                self.regs.ymm_high[xmm_dst][0] = (a2 - b2).to_bits();
                self.regs.ymm_high[xmm_dst][1] = (a3 + b3).to_bits();
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX horizontal add: VHADDPS, VHADDPD
    fn execute_vex_haddp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        let (src2_lo, src2_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let src1_lo = self.regs.xmm[xmm_src1][0];
        let src1_hi = self.regs.xmm[xmm_src1][1];

        if vex_pp == 3 {
            // VHADDPS: horizontal add for singles
            // dst[0] = src1[0] + src1[1], dst[1] = src1[2] + src1[3]
            // dst[2] = src2[0] + src2[1], dst[3] = src2[2] + src2[3]
            let hadd_ps = |lo: u64, hi: u64| -> u64 {
                let s0 = f32::from_bits(lo as u32);
                let s1 = f32::from_bits((lo >> 32) as u32);
                let s2 = f32::from_bits(hi as u32);
                let s3 = f32::from_bits((hi >> 32) as u32);
                let r0 = (s0 + s1).to_bits() as u64;
                let r1 = (s2 + s3).to_bits() as u64;
                r0 | (r1 << 32)
            };

            self.regs.xmm[xmm_dst][0] = hadd_ps(src1_lo, src1_hi);
            self.regs.xmm[xmm_dst][1] = hadd_ps(src2_lo, src2_hi);

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = hadd_ps(src1_hi2, src1_hi3);
                self.regs.ymm_high[xmm_dst][1] = hadd_ps(src2_hi2, src2_hi3);
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        } else {
            // VHADDPD: horizontal add for doubles
            // dst[0] = src1[0] + src1[1], dst[1] = src2[0] + src2[1]
            let a0 = f64::from_bits(src1_lo);
            let a1 = f64::from_bits(src1_hi);
            let b0 = f64::from_bits(src2_lo);
            let b1 = f64::from_bits(src2_hi);
            self.regs.xmm[xmm_dst][0] = (a0 + a1).to_bits();
            self.regs.xmm[xmm_dst][1] = (b0 + b1).to_bits();

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                let a2 = f64::from_bits(src1_hi2);
                let a3 = f64::from_bits(src1_hi3);
                let b2 = f64::from_bits(src2_hi2);
                let b3 = f64::from_bits(src2_hi3);
                self.regs.ymm_high[xmm_dst][0] = (a2 + a3).to_bits();
                self.regs.ymm_high[xmm_dst][1] = (b2 + b3).to_bits();
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX horizontal subtract: VHSUBPS, VHSUBPD
    fn execute_vex_hsubp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        let (src2_lo, src2_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let src1_lo = self.regs.xmm[xmm_src1][0];
        let src1_hi = self.regs.xmm[xmm_src1][1];

        if vex_pp == 3 {
            // VHSUBPS: horizontal subtract for singles
            let hsub_ps = |lo: u64, hi: u64| -> u64 {
                let s0 = f32::from_bits(lo as u32);
                let s1 = f32::from_bits((lo >> 32) as u32);
                let s2 = f32::from_bits(hi as u32);
                let s3 = f32::from_bits((hi >> 32) as u32);
                let r0 = (s0 - s1).to_bits() as u64;
                let r1 = (s2 - s3).to_bits() as u64;
                r0 | (r1 << 32)
            };

            self.regs.xmm[xmm_dst][0] = hsub_ps(src1_lo, src1_hi);
            self.regs.xmm[xmm_dst][1] = hsub_ps(src2_lo, src2_hi);

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = hsub_ps(src1_hi2, src1_hi3);
                self.regs.ymm_high[xmm_dst][1] = hsub_ps(src2_hi2, src2_hi3);
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        } else {
            // VHSUBPD: horizontal subtract for doubles
            let a0 = f64::from_bits(src1_lo);
            let a1 = f64::from_bits(src1_hi);
            let b0 = f64::from_bits(src2_lo);
            let b1 = f64::from_bits(src2_hi);
            self.regs.xmm[xmm_dst][0] = (a0 - a1).to_bits();
            self.regs.xmm[xmm_dst][1] = (b0 - b1).to_bits();

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                let a2 = f64::from_bits(src1_hi2);
                let a3 = f64::from_bits(src1_hi3);
                let b2 = f64::from_bits(src2_hi2);
                let b3 = f64::from_bits(src2_hi3);
                self.regs.ymm_high[xmm_dst][0] = (a2 - a3).to_bits();
                self.regs.ymm_high[xmm_dst][1] = (b2 - b3).to_bits();
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX-encoded shuffle: VPSHUFD/VPSHUFHW/VPSHUFLW
    fn execute_vex_shuffle(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let xmm_dst = reg as usize;

        match vex_pp {
            1 => {
                // VPSHUFD: 66 prefix - shuffle dwords
                let (src_lo, src_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let dwords: [u32; 4] = [
                    src_lo as u32,
                    (src_lo >> 32) as u32,
                    src_hi as u32,
                    (src_hi >> 32) as u32,
                ];
                let d0 = dwords[((imm8 >> 0) & 3) as usize];
                let d1 = dwords[((imm8 >> 2) & 3) as usize];
                let d2 = dwords[((imm8 >> 4) & 3) as usize];
                let d3 = dwords[((imm8 >> 6) & 3) as usize];
                self.regs.xmm[xmm_dst][0] = (d0 as u64) | ((d1 as u64) << 32);
                self.regs.xmm[xmm_dst][1] = (d2 as u64) | ((d3 as u64) << 32);

                if vex_l == 1 {
                    // YMM: process high 128-bit lane independently
                    let (src2_lo, src2_hi) = if is_memory {
                        (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                    } else {
                        (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                    };
                    let dwords2: [u32; 4] = [
                        src2_lo as u32,
                        (src2_lo >> 32) as u32,
                        src2_hi as u32,
                        (src2_hi >> 32) as u32,
                    ];
                    let d4 = dwords2[((imm8 >> 0) & 3) as usize];
                    let d5 = dwords2[((imm8 >> 2) & 3) as usize];
                    let d6 = dwords2[((imm8 >> 4) & 3) as usize];
                    let d7 = dwords2[((imm8 >> 6) & 3) as usize];
                    self.regs.ymm_high[xmm_dst][0] = (d4 as u64) | ((d5 as u64) << 32);
                    self.regs.ymm_high[xmm_dst][1] = (d6 as u64) | ((d7 as u64) << 32);
                } else {
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            2 => {
                // VPSHUFHW: F3 prefix - shuffle high words
                let (src_lo, src_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                self.regs.xmm[xmm_dst][0] = src_lo;
                let w0 = (src_hi >> (((imm8 >> 0) & 3) * 16)) as u16;
                let w1 = (src_hi >> (((imm8 >> 2) & 3) * 16)) as u16;
                let w2 = (src_hi >> (((imm8 >> 4) & 3) * 16)) as u16;
                let w3 = (src_hi >> (((imm8 >> 6) & 3) * 16)) as u16;
                self.regs.xmm[xmm_dst][1] = (w0 as u64) | ((w1 as u64) << 16) | ((w2 as u64) << 32) | ((w3 as u64) << 48);

                if vex_l == 1 {
                    let (src2_lo, src2_hi) = if is_memory {
                        (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                    } else {
                        (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                    };
                    self.regs.ymm_high[xmm_dst][0] = src2_lo;
                    let w4 = (src2_hi >> (((imm8 >> 0) & 3) * 16)) as u16;
                    let w5 = (src2_hi >> (((imm8 >> 2) & 3) * 16)) as u16;
                    let w6 = (src2_hi >> (((imm8 >> 4) & 3) * 16)) as u16;
                    let w7 = (src2_hi >> (((imm8 >> 6) & 3) * 16)) as u16;
                    self.regs.ymm_high[xmm_dst][1] = (w4 as u64) | ((w5 as u64) << 16) | ((w6 as u64) << 32) | ((w7 as u64) << 48);
                } else {
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            3 => {
                // VPSHUFLW: F2 prefix - shuffle low words
                let (src_lo, src_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let w0 = (src_lo >> (((imm8 >> 0) & 3) * 16)) as u16;
                let w1 = (src_lo >> (((imm8 >> 2) & 3) * 16)) as u16;
                let w2 = (src_lo >> (((imm8 >> 4) & 3) * 16)) as u16;
                let w3 = (src_lo >> (((imm8 >> 6) & 3) * 16)) as u16;
                self.regs.xmm[xmm_dst][0] = (w0 as u64) | ((w1 as u64) << 16) | ((w2 as u64) << 32) | ((w3 as u64) << 48);
                self.regs.xmm[xmm_dst][1] = src_hi;

                if vex_l == 1 {
                    let (src2_lo, src2_hi) = if is_memory {
                        (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                    } else {
                        (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                    };
                    let w4 = (src2_lo >> (((imm8 >> 0) & 3) * 16)) as u16;
                    let w5 = (src2_lo >> (((imm8 >> 2) & 3) * 16)) as u16;
                    let w6 = (src2_lo >> (((imm8 >> 4) & 3) * 16)) as u16;
                    let w7 = (src2_lo >> (((imm8 >> 6) & 3) * 16)) as u16;
                    self.regs.ymm_high[xmm_dst][0] = (w4 as u64) | ((w5 as u64) << 16) | ((w6 as u64) << 32) | ((w7 as u64) << 48);
                    self.regs.ymm_high[xmm_dst][1] = src2_hi;
                } else {
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            _ => {
                return Err(Error::Emulator(format!("unimplemented VEX shuffle pp={}", vex_pp)));
            }
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX-encoded VCMPPS/VCMPPD/VCMPSS/VCMPSD
    fn execute_vex_cmp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        match vex_pp {
            0 => {
                // VCMPPS - packed single
                let (src2_lo, src2_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let src1_lo = self.regs.xmm[xmm_src1][0];
                let src1_hi = self.regs.xmm[xmm_src1][1];

                let r0 = if self.cmp_predicate_f32(f32::from_bits(src1_lo as u32), f32::from_bits(src2_lo as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                let r1 = if self.cmp_predicate_f32(f32::from_bits((src1_lo >> 32) as u32), f32::from_bits((src2_lo >> 32) as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                let r2 = if self.cmp_predicate_f32(f32::from_bits(src1_hi as u32), f32::from_bits(src2_hi as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                let r3 = if self.cmp_predicate_f32(f32::from_bits((src1_hi >> 32) as u32), f32::from_bits((src2_hi >> 32) as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                self.regs.xmm[xmm_dst][0] = r0 as u64 | ((r1 as u64) << 32);
                self.regs.xmm[xmm_dst][1] = r2 as u64 | ((r3 as u64) << 32);

                if vex_l == 1 {
                    let (src2_hi2, src2_hi3) = if is_memory {
                        (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                    } else {
                        (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                    };
                    let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                    let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                    let r4 = if self.cmp_predicate_f32(f32::from_bits(src1_hi2 as u32), f32::from_bits(src2_hi2 as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                    let r5 = if self.cmp_predicate_f32(f32::from_bits((src1_hi2 >> 32) as u32), f32::from_bits((src2_hi2 >> 32) as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                    let r6 = if self.cmp_predicate_f32(f32::from_bits(src1_hi3 as u32), f32::from_bits(src2_hi3 as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                    let r7 = if self.cmp_predicate_f32(f32::from_bits((src1_hi3 >> 32) as u32), f32::from_bits((src2_hi3 >> 32) as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                    self.regs.ymm_high[xmm_dst][0] = r4 as u64 | ((r5 as u64) << 32);
                    self.regs.ymm_high[xmm_dst][1] = r6 as u64 | ((r7 as u64) << 32);
                } else {
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            1 => {
                // VCMPPD - packed double
                let (src2_lo, src2_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let src1_lo = self.regs.xmm[xmm_src1][0];
                let src1_hi = self.regs.xmm[xmm_src1][1];

                let r0 = if self.cmp_predicate_f64(f64::from_bits(src1_lo), f64::from_bits(src2_lo), imm8) { !0u64 } else { 0 };
                let r1 = if self.cmp_predicate_f64(f64::from_bits(src1_hi), f64::from_bits(src2_hi), imm8) { !0u64 } else { 0 };
                self.regs.xmm[xmm_dst][0] = r0;
                self.regs.xmm[xmm_dst][1] = r1;

                if vex_l == 1 {
                    let (src2_hi2, src2_hi3) = if is_memory {
                        (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                    } else {
                        (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                    };
                    let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                    let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                    let r2 = if self.cmp_predicate_f64(f64::from_bits(src1_hi2), f64::from_bits(src2_hi2), imm8) { !0u64 } else { 0 };
                    let r3 = if self.cmp_predicate_f64(f64::from_bits(src1_hi3), f64::from_bits(src2_hi3), imm8) { !0u64 } else { 0 };
                    self.regs.ymm_high[xmm_dst][0] = r2;
                    self.regs.ymm_high[xmm_dst][1] = r3;
                } else {
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            2 => {
                // VCMPSS - scalar single
                let src2 = if is_memory {
                    f32::from_bits(self.read_mem(addr, 4)? as u32)
                } else {
                    f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
                };
                let src1 = f32::from_bits(self.regs.xmm[xmm_src1][0] as u32);
                let result = if self.cmp_predicate_f32(src1, src2, imm8) { 0xFFFFFFFFu32 } else { 0 };
                self.regs.xmm[xmm_dst][0] = (self.regs.xmm[xmm_src1][0] & !0xFFFFFFFF) | result as u64;
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
            3 => {
                // VCMPSD - scalar double
                let src2 = if is_memory {
                    f64::from_bits(self.read_mem(addr, 8)?)
                } else {
                    f64::from_bits(self.regs.xmm[rm as usize][0])
                };
                let src1 = f64::from_bits(self.regs.xmm[xmm_src1][0]);
                let result = if self.cmp_predicate_f64(src1, src2, imm8) { !0u64 } else { 0 };
                self.regs.xmm[xmm_dst][0] = result;
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
            _ => unreachable!(),
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VINSERTF128 ymm1, ymm2, xmm3/m128, imm8
    /// Insert 128-bit lane from xmm/m128 into ymm
    fn execute_vinsertf128(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
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

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEXTRACTF128 xmm1/m128, ymm2, imm8
    /// Extract 128-bit lane from ymm to xmm/m128
    fn execute_vextractf128(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let xmm_src = reg as usize;

        // Select lane based on imm8[0]
        let (extract_lo, extract_hi) = if (imm8 & 1) == 0 {
            // Extract low 128 bits
            (self.regs.xmm[xmm_src][0], self.regs.xmm[xmm_src][1])
        } else {
            // Extract high 128 bits
            (self.regs.ymm_high[xmm_src][0], self.regs.ymm_high[xmm_src][1])
        };

        if is_memory {
            self.write_mem(addr, extract_lo, 8)?;
            self.write_mem(addr + 8, extract_hi, 8)?;
        } else {
            let xmm_dst = rm as usize;
            self.regs.xmm[xmm_dst][0] = extract_lo;
            self.regs.xmm[xmm_dst][1] = extract_hi;
            // VEX clears upper bits
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VPERM2F128 ymm1, ymm2, ymm3/m256, imm8
    /// Permute 128-bit lanes from two 256-bit sources
    fn execute_vperm2f128(
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
        let src1_hi = (self.regs.ymm_high[xmm_src1][0], self.regs.ymm_high[xmm_src1][1]);
        let (src2_lo, src2_hi) = if is_memory {
            (
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?),
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?),
            )
        } else {
            (
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1]),
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1]),
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

    /// VEX packed integer shift by immediate (Group 12/13/14)
    fn execute_vex_packed_shift_imm(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let modrm = ctx.peek_u8()?;
        let reg_op = (modrm >> 3) & 0x07;
        let (_, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;

        let xmm_dst = vvvv as usize;
        let xmm_src = rm as usize;

        // Get source values
        let (src_lo, src_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[xmm_src][0], self.regs.xmm[xmm_src][1])
        };

        match opcode {
            0x71 => {
                // VPSRLW/VPSRAW/VPSLLW - word shifts
                match reg_op {
                    2 => {
                        // VPSRLW: logical right shift words
                        let shift = imm8 as u32;
                        self.regs.xmm[xmm_dst][0] = self.shift_words_right(src_lo, shift, false);
                        self.regs.xmm[xmm_dst][1] = self.shift_words_right(src_hi, shift, false);
                    }
                    4 => {
                        // VPSRAW: arithmetic right shift words
                        let shift = imm8 as u32;
                        self.regs.xmm[xmm_dst][0] = self.shift_words_right(src_lo, shift, true);
                        self.regs.xmm[xmm_dst][1] = self.shift_words_right(src_hi, shift, true);
                    }
                    6 => {
                        // VPSLLW: logical left shift words
                        let shift = imm8 as u32;
                        self.regs.xmm[xmm_dst][0] = self.shift_words_left(src_lo, shift);
                        self.regs.xmm[xmm_dst][1] = self.shift_words_left(src_hi, shift);
                    }
                    _ => {
                        return Err(Error::Emulator(format!(
                            "unimplemented VEX.0F 71 /{} at RIP={:#x}",
                            reg_op, self.regs.rip
                        )));
                    }
                }
            }
            0x72 => {
                // VPSRLD/VPSRAD/VPSLLD - dword shifts
                match reg_op {
                    2 => {
                        // VPSRLD: logical right shift dwords
                        let shift = imm8 as u32;
                        self.regs.xmm[xmm_dst][0] = self.shift_dwords_right(src_lo, shift, false);
                        self.regs.xmm[xmm_dst][1] = self.shift_dwords_right(src_hi, shift, false);
                    }
                    4 => {
                        // VPSRAD: arithmetic right shift dwords
                        let shift = imm8 as u32;
                        self.regs.xmm[xmm_dst][0] = self.shift_dwords_right(src_lo, shift, true);
                        self.regs.xmm[xmm_dst][1] = self.shift_dwords_right(src_hi, shift, true);
                    }
                    6 => {
                        // VPSLLD: logical left shift dwords
                        let shift = imm8 as u32;
                        self.regs.xmm[xmm_dst][0] = self.shift_dwords_left(src_lo, shift);
                        self.regs.xmm[xmm_dst][1] = self.shift_dwords_left(src_hi, shift);
                    }
                    _ => {
                        return Err(Error::Emulator(format!(
                            "unimplemented VEX.0F 72 /{} at RIP={:#x}",
                            reg_op, self.regs.rip
                        )));
                    }
                }
            }
            0x73 => {
                // VPSRLQ/VPSRLDQ/VPSLLQ/VPSLLDQ - qword/dqword shifts
                match reg_op {
                    2 => {
                        // VPSRLQ: logical right shift qwords
                        let shift = imm8 as u32;
                        self.regs.xmm[xmm_dst][0] = if shift >= 64 { 0 } else { src_lo >> shift };
                        self.regs.xmm[xmm_dst][1] = if shift >= 64 { 0 } else { src_hi >> shift };
                    }
                    3 => {
                        // VPSRLDQ: byte shift right (within each 128-bit lane)
                        let shift = (imm8 as usize).min(16);
                        let (new_lo, new_hi) = self.byte_shift_right_128(src_lo, src_hi, shift);
                        self.regs.xmm[xmm_dst][0] = new_lo;
                        self.regs.xmm[xmm_dst][1] = new_hi;
                    }
                    6 => {
                        // VPSLLQ: logical left shift qwords
                        let shift = imm8 as u32;
                        self.regs.xmm[xmm_dst][0] = if shift >= 64 { 0 } else { src_lo << shift };
                        self.regs.xmm[xmm_dst][1] = if shift >= 64 { 0 } else { src_hi << shift };
                    }
                    7 => {
                        // VPSLLDQ: byte shift left (within each 128-bit lane)
                        let shift = (imm8 as usize).min(16);
                        let (new_lo, new_hi) = self.byte_shift_left_128(src_lo, src_hi, shift);
                        self.regs.xmm[xmm_dst][0] = new_lo;
                        self.regs.xmm[xmm_dst][1] = new_hi;
                    }
                    _ => {
                        return Err(Error::Emulator(format!(
                            "unimplemented VEX.0F 73 /{} at RIP={:#x}",
                            reg_op, self.regs.rip
                        )));
                    }
                }
            }
            _ => unreachable!(),
        }

        // Handle YMM (256-bit) if vex_l == 1
        if vex_l == 1 {
            let (src_hi2, src_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[xmm_src][0], self.regs.ymm_high[xmm_src][1])
            };

            match opcode {
                0x71 => {
                    match reg_op {
                        2 => {
                            let shift = imm8 as u32;
                            self.regs.ymm_high[xmm_dst][0] = self.shift_words_right(src_hi2, shift, false);
                            self.regs.ymm_high[xmm_dst][1] = self.shift_words_right(src_hi3, shift, false);
                        }
                        4 => {
                            let shift = imm8 as u32;
                            self.regs.ymm_high[xmm_dst][0] = self.shift_words_right(src_hi2, shift, true);
                            self.regs.ymm_high[xmm_dst][1] = self.shift_words_right(src_hi3, shift, true);
                        }
                        6 => {
                            let shift = imm8 as u32;
                            self.regs.ymm_high[xmm_dst][0] = self.shift_words_left(src_hi2, shift);
                            self.regs.ymm_high[xmm_dst][1] = self.shift_words_left(src_hi3, shift);
                        }
                        _ => {}
                    }
                }
                0x72 => {
                    match reg_op {
                        2 => {
                            let shift = imm8 as u32;
                            self.regs.ymm_high[xmm_dst][0] = self.shift_dwords_right(src_hi2, shift, false);
                            self.regs.ymm_high[xmm_dst][1] = self.shift_dwords_right(src_hi3, shift, false);
                        }
                        4 => {
                            let shift = imm8 as u32;
                            self.regs.ymm_high[xmm_dst][0] = self.shift_dwords_right(src_hi2, shift, true);
                            self.regs.ymm_high[xmm_dst][1] = self.shift_dwords_right(src_hi3, shift, true);
                        }
                        6 => {
                            let shift = imm8 as u32;
                            self.regs.ymm_high[xmm_dst][0] = self.shift_dwords_left(src_hi2, shift);
                            self.regs.ymm_high[xmm_dst][1] = self.shift_dwords_left(src_hi3, shift);
                        }
                        _ => {}
                    }
                }
                0x73 => {
                    match reg_op {
                        2 => {
                            let shift = imm8 as u32;
                            self.regs.ymm_high[xmm_dst][0] = if shift >= 64 { 0 } else { src_hi2 >> shift };
                            self.regs.ymm_high[xmm_dst][1] = if shift >= 64 { 0 } else { src_hi3 >> shift };
                        }
                        3 => {
                            let shift = (imm8 as usize).min(16);
                            let (new_lo, new_hi) = self.byte_shift_right_128(src_hi2, src_hi3, shift);
                            self.regs.ymm_high[xmm_dst][0] = new_lo;
                            self.regs.ymm_high[xmm_dst][1] = new_hi;
                        }
                        6 => {
                            let shift = imm8 as u32;
                            self.regs.ymm_high[xmm_dst][0] = if shift >= 64 { 0 } else { src_hi2 << shift };
                            self.regs.ymm_high[xmm_dst][1] = if shift >= 64 { 0 } else { src_hi3 << shift };
                        }
                        7 => {
                            let shift = (imm8 as usize).min(16);
                            let (new_lo, new_hi) = self.byte_shift_left_128(src_hi2, src_hi3, shift);
                            self.regs.ymm_high[xmm_dst][0] = new_lo;
                            self.regs.ymm_high[xmm_dst][1] = new_hi;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        } else {
            // VEX.128 clears upper bits
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    // Helper: shift packed words left
    fn shift_words_left(&self, val: u64, shift: u32) -> u64 {
        if shift >= 16 {
            return 0;
        }
        let w0 = ((val as u16) << shift) as u64;
        let w1 = (((val >> 16) as u16) << shift) as u64;
        let w2 = (((val >> 32) as u16) << shift) as u64;
        let w3 = (((val >> 48) as u16) << shift) as u64;
        w0 | (w1 << 16) | (w2 << 32) | (w3 << 48)
    }

    // Helper: shift packed words right (logical or arithmetic)
    fn shift_words_right(&self, val: u64, shift: u32, arith: bool) -> u64 {
        if shift >= 16 {
            return if arith {
                let sign0 = if (val as i16) < 0 { 0xFFFFu64 } else { 0 };
                let sign1 = if ((val >> 16) as i16) < 0 { 0xFFFFu64 } else { 0 };
                let sign2 = if ((val >> 32) as i16) < 0 { 0xFFFFu64 } else { 0 };
                let sign3 = if ((val >> 48) as i16) < 0 { 0xFFFFu64 } else { 0 };
                sign0 | (sign1 << 16) | (sign2 << 32) | (sign3 << 48)
            } else {
                0
            };
        }
        if arith {
            let w0 = (((val as i16) >> shift) as u16) as u64;
            let w1 = ((((val >> 16) as i16) >> shift) as u16) as u64;
            let w2 = ((((val >> 32) as i16) >> shift) as u16) as u64;
            let w3 = ((((val >> 48) as i16) >> shift) as u16) as u64;
            w0 | (w1 << 16) | (w2 << 32) | (w3 << 48)
        } else {
            let w0 = ((val as u16) >> shift) as u64;
            let w1 = (((val >> 16) as u16) >> shift) as u64;
            let w2 = (((val >> 32) as u16) >> shift) as u64;
            let w3 = (((val >> 48) as u16) >> shift) as u64;
            w0 | (w1 << 16) | (w2 << 32) | (w3 << 48)
        }
    }

    // Helper: shift packed dwords left
    fn shift_dwords_left(&self, val: u64, shift: u32) -> u64 {
        if shift >= 32 {
            return 0;
        }
        let d0 = ((val as u32) << shift) as u64;
        let d1 = (((val >> 32) as u32) << shift) as u64;
        d0 | (d1 << 32)
    }

    // Helper: shift packed dwords right (logical or arithmetic)
    fn shift_dwords_right(&self, val: u64, shift: u32, arith: bool) -> u64 {
        if shift >= 32 {
            return if arith {
                let sign0 = if (val as i32) < 0 { 0xFFFFFFFFu64 } else { 0 };
                let sign1 = if ((val >> 32) as i32) < 0 { 0xFFFFFFFFu64 } else { 0 };
                sign0 | (sign1 << 32)
            } else {
                0
            };
        }
        if arith {
            let d0 = (((val as i32) >> shift) as u32) as u64;
            let d1 = ((((val >> 32) as i32) >> shift) as u32) as u64;
            d0 | (d1 << 32)
        } else {
            let d0 = ((val as u32) >> shift) as u64;
            let d1 = (((val >> 32) as u32) >> shift) as u64;
            d0 | (d1 << 32)
        }
    }

    // Helper: byte shift right within 128-bit value
    fn byte_shift_right_128(&self, lo: u64, hi: u64, shift: usize) -> (u64, u64) {
        if shift >= 16 {
            return (0, 0);
        }
        let shift_bits = shift * 8;
        if shift == 0 {
            (lo, hi)
        } else if shift < 8 {
            let new_lo = (lo >> shift_bits) | (hi << (64 - shift_bits));
            let new_hi = hi >> shift_bits;
            (new_lo, new_hi)
        } else {
            let shift_bits = (shift - 8) * 8;
            let new_lo = hi >> shift_bits;
            (new_lo, 0)
        }
    }

    // Helper: byte shift left within 128-bit value
    fn byte_shift_left_128(&self, lo: u64, hi: u64, shift: usize) -> (u64, u64) {
        if shift >= 16 {
            return (0, 0);
        }
        let shift_bits = shift * 8;
        if shift == 0 {
            (lo, hi)
        } else if shift < 8 {
            let new_hi = (hi << shift_bits) | (lo >> (64 - shift_bits));
            let new_lo = lo << shift_bits;
            (new_lo, new_hi)
        } else {
            let shift_bits = (shift - 8) * 8;
            let new_hi = lo << shift_bits;
            (0, new_hi)
        }
    }

    /// VEX packed integer shift by XMM count
    /// VPSRLW/D/Q, VPSRAW/D, VPSLLW/D/Q
    fn execute_vex_packed_shift_xmm(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src = vvvv as usize;

        // Get shift count from xmm/m128 (only low 64 bits used)
        let count = if is_memory {
            self.read_mem(addr, 8)?
        } else {
            self.regs.xmm[rm as usize][0]
        };

        // Get source data to be shifted
        let src_lo = self.regs.xmm[xmm_src][0];
        let src_hi = self.regs.xmm[xmm_src][1];

        // Apply shift based on opcode
        match opcode {
            // VPSRLW: logical right shift words
            0xD1 => {
                let shift = count.min(16) as u32;
                self.regs.xmm[xmm_dst][0] = self.shift_words_right(src_lo, shift, false);
                self.regs.xmm[xmm_dst][1] = self.shift_words_right(src_hi, shift, false);
            }
            // VPSRLD: logical right shift dwords
            0xD2 => {
                let shift = count.min(32) as u32;
                self.regs.xmm[xmm_dst][0] = self.shift_dwords_right(src_lo, shift, false);
                self.regs.xmm[xmm_dst][1] = self.shift_dwords_right(src_hi, shift, false);
            }
            // VPSRLQ: logical right shift qwords
            0xD3 => {
                let shift = count.min(64) as u32;
                self.regs.xmm[xmm_dst][0] = if shift >= 64 { 0 } else { src_lo >> shift };
                self.regs.xmm[xmm_dst][1] = if shift >= 64 { 0 } else { src_hi >> shift };
            }
            // VPSRAW: arithmetic right shift words
            0xE1 => {
                let shift = count.min(16) as u32;
                self.regs.xmm[xmm_dst][0] = self.shift_words_right(src_lo, shift, true);
                self.regs.xmm[xmm_dst][1] = self.shift_words_right(src_hi, shift, true);
            }
            // VPSRAD: arithmetic right shift dwords
            0xE2 => {
                let shift = count.min(32) as u32;
                self.regs.xmm[xmm_dst][0] = self.shift_dwords_right(src_lo, shift, true);
                self.regs.xmm[xmm_dst][1] = self.shift_dwords_right(src_hi, shift, true);
            }
            // VPSLLW: logical left shift words
            0xF1 => {
                let shift = count.min(16) as u32;
                self.regs.xmm[xmm_dst][0] = self.shift_words_left(src_lo, shift);
                self.regs.xmm[xmm_dst][1] = self.shift_words_left(src_hi, shift);
            }
            // VPSLLD: logical left shift dwords
            0xF2 => {
                let shift = count.min(32) as u32;
                self.regs.xmm[xmm_dst][0] = self.shift_dwords_left(src_lo, shift);
                self.regs.xmm[xmm_dst][1] = self.shift_dwords_left(src_hi, shift);
            }
            // VPSLLQ: logical left shift qwords
            0xF3 => {
                let shift = count.min(64) as u32;
                self.regs.xmm[xmm_dst][0] = if shift >= 64 { 0 } else { src_lo << shift };
                self.regs.xmm[xmm_dst][1] = if shift >= 64 { 0 } else { src_hi << shift };
            }
            _ => unreachable!(),
        }

        // Handle YMM (256-bit) if vex_l == 1
        if vex_l == 1 {
            let src_hi2 = self.regs.ymm_high[xmm_src][0];
            let src_hi3 = self.regs.ymm_high[xmm_src][1];

            match opcode {
                0xD1 => {
                    let shift = count.min(16) as u32;
                    self.regs.ymm_high[xmm_dst][0] = self.shift_words_right(src_hi2, shift, false);
                    self.regs.ymm_high[xmm_dst][1] = self.shift_words_right(src_hi3, shift, false);
                }
                0xD2 => {
                    let shift = count.min(32) as u32;
                    self.regs.ymm_high[xmm_dst][0] = self.shift_dwords_right(src_hi2, shift, false);
                    self.regs.ymm_high[xmm_dst][1] = self.shift_dwords_right(src_hi3, shift, false);
                }
                0xD3 => {
                    let shift = count.min(64) as u32;
                    self.regs.ymm_high[xmm_dst][0] = if shift >= 64 { 0 } else { src_hi2 >> shift };
                    self.regs.ymm_high[xmm_dst][1] = if shift >= 64 { 0 } else { src_hi3 >> shift };
                }
                0xE1 => {
                    let shift = count.min(16) as u32;
                    self.regs.ymm_high[xmm_dst][0] = self.shift_words_right(src_hi2, shift, true);
                    self.regs.ymm_high[xmm_dst][1] = self.shift_words_right(src_hi3, shift, true);
                }
                0xE2 => {
                    let shift = count.min(32) as u32;
                    self.regs.ymm_high[xmm_dst][0] = self.shift_dwords_right(src_hi2, shift, true);
                    self.regs.ymm_high[xmm_dst][1] = self.shift_dwords_right(src_hi3, shift, true);
                }
                0xF1 => {
                    let shift = count.min(16) as u32;
                    self.regs.ymm_high[xmm_dst][0] = self.shift_words_left(src_hi2, shift);
                    self.regs.ymm_high[xmm_dst][1] = self.shift_words_left(src_hi3, shift);
                }
                0xF2 => {
                    let shift = count.min(32) as u32;
                    self.regs.ymm_high[xmm_dst][0] = self.shift_dwords_left(src_hi2, shift);
                    self.regs.ymm_high[xmm_dst][1] = self.shift_dwords_left(src_hi3, shift);
                }
                0xF3 => {
                    let shift = count.min(64) as u32;
                    self.regs.ymm_high[xmm_dst][0] = if shift >= 64 { 0 } else { src_hi2 << shift };
                    self.regs.ymm_high[xmm_dst][1] = if shift >= 64 { 0 } else { src_hi3 << shift };
                }
                _ => {}
            }
        } else {
            // VEX.128 clears upper bits
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX variable shift instructions (per-element shift counts)
    /// VPSRLVD/Q, VPSRAVD, VPSLLVD/Q
    fn execute_vex_variable_shift(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
        vex_w: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src = vvvv as usize;

        // Get shift counts from xmm/m128 or ymm/m256
        let (count_lo, count_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };

        // Get source data
        let src_lo = self.regs.xmm[xmm_src][0];
        let src_hi = self.regs.xmm[xmm_src][1];

        if vex_w == 0 {
            // Dword operations
            match opcode {
                0x45 => {
                    // VPSRLVD: variable right logical shift dwords
                    self.regs.xmm[xmm_dst][0] = self.variable_shift_dwords(src_lo, count_lo, false, false);
                    self.regs.xmm[xmm_dst][1] = self.variable_shift_dwords(src_hi, count_hi, false, false);
                }
                0x46 => {
                    // VPSRAVD: variable right arithmetic shift dwords
                    self.regs.xmm[xmm_dst][0] = self.variable_shift_dwords(src_lo, count_lo, false, true);
                    self.regs.xmm[xmm_dst][1] = self.variable_shift_dwords(src_hi, count_hi, false, true);
                }
                0x47 => {
                    // VPSLLVD: variable left shift dwords
                    self.regs.xmm[xmm_dst][0] = self.variable_shift_dwords(src_lo, count_lo, true, false);
                    self.regs.xmm[xmm_dst][1] = self.variable_shift_dwords(src_hi, count_hi, true, false);
                }
                _ => unreachable!(),
            }
        } else {
            // Qword operations
            match opcode {
                0x45 => {
                    // VPSRLVQ: variable right logical shift qwords
                    self.regs.xmm[xmm_dst][0] = self.variable_shift_qword(src_lo, count_lo, false);
                    self.regs.xmm[xmm_dst][1] = self.variable_shift_qword(src_hi, count_hi, false);
                }
                0x47 => {
                    // VPSLLVQ: variable left shift qwords
                    self.regs.xmm[xmm_dst][0] = self.variable_shift_qword(src_lo, count_lo, true);
                    self.regs.xmm[xmm_dst][1] = self.variable_shift_qword(src_hi, count_hi, true);
                }
                _ => {
                    return Err(Error::Emulator(format!(
                        "VPSRAVQ (W1 opcode 0x46) not supported in VEX (AVX2)"
                    )));
                }
            }
        }

        // Handle YMM (256-bit)
        if vex_l == 1 {
            let (count_hi2, count_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src_hi2 = self.regs.ymm_high[xmm_src][0];
            let src_hi3 = self.regs.ymm_high[xmm_src][1];

            if vex_w == 0 {
                match opcode {
                    0x45 => {
                        self.regs.ymm_high[xmm_dst][0] = self.variable_shift_dwords(src_hi2, count_hi2, false, false);
                        self.regs.ymm_high[xmm_dst][1] = self.variable_shift_dwords(src_hi3, count_hi3, false, false);
                    }
                    0x46 => {
                        self.regs.ymm_high[xmm_dst][0] = self.variable_shift_dwords(src_hi2, count_hi2, false, true);
                        self.regs.ymm_high[xmm_dst][1] = self.variable_shift_dwords(src_hi3, count_hi3, false, true);
                    }
                    0x47 => {
                        self.regs.ymm_high[xmm_dst][0] = self.variable_shift_dwords(src_hi2, count_hi2, true, false);
                        self.regs.ymm_high[xmm_dst][1] = self.variable_shift_dwords(src_hi3, count_hi3, true, false);
                    }
                    _ => {}
                }
            } else {
                match opcode {
                    0x45 => {
                        self.regs.ymm_high[xmm_dst][0] = self.variable_shift_qword(src_hi2, count_hi2, false);
                        self.regs.ymm_high[xmm_dst][1] = self.variable_shift_qword(src_hi3, count_hi3, false);
                    }
                    0x47 => {
                        self.regs.ymm_high[xmm_dst][0] = self.variable_shift_qword(src_hi2, count_hi2, true);
                        self.regs.ymm_high[xmm_dst][1] = self.variable_shift_qword(src_hi3, count_hi3, true);
                    }
                    _ => {}
                }
            }
        } else {
            // VEX.128 clears upper bits
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    // Helper: variable shift two packed dwords
    // val = two dwords, counts = corresponding shift counts
    // left = true for left shift, arith = true for arithmetic right shift
    fn variable_shift_dwords(&self, val: u64, counts: u64, left: bool, arith: bool) -> u64 {
        let d0 = val as u32;
        let d1 = (val >> 32) as u32;
        let c0 = (counts as u32).min(32);
        let c1 = ((counts >> 32) as u32).min(32);

        let r0 = if left {
            if c0 >= 32 { 0 } else { d0 << c0 }
        } else if arith {
            if c0 >= 32 {
                if (d0 as i32) < 0 { 0xFFFFFFFF } else { 0 }
            } else {
                ((d0 as i32) >> c0) as u32
            }
        } else {
            if c0 >= 32 { 0 } else { d0 >> c0 }
        };

        let r1 = if left {
            if c1 >= 32 { 0 } else { d1 << c1 }
        } else if arith {
            if c1 >= 32 {
                if (d1 as i32) < 0 { 0xFFFFFFFF } else { 0 }
            } else {
                ((d1 as i32) >> c1) as u32
            }
        } else {
            if c1 >= 32 { 0 } else { d1 >> c1 }
        };

        (r0 as u64) | ((r1 as u64) << 32)
    }

    // Helper: variable shift a qword
    fn variable_shift_qword(&self, val: u64, count: u64, left: bool) -> u64 {
        let c = count.min(64) as u32;
        if left {
            if c >= 64 { 0 } else { val << c }
        } else {
            if c >= 64 { 0 } else { val >> c }
        }
    }
}
