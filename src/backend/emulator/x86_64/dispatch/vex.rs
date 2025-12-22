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

                // AVX2 packed integer unpack instructions (0x60-0x6D)
                (1, 0x60) | (1, 0x61) | (1, 0x62) | (1, 0x63) |
                (1, 0x64) | (1, 0x65) | (1, 0x66) | (1, 0x67) |
                (1, 0x68) | (1, 0x69) | (1, 0x6A) | (1, 0x6B) |
                (1, 0x6C) | (1, 0x6D) => {
                    return self.execute_vex_punpck(ctx, vex_l, vvvv, opcode);
                }

                // AVX2 packed integer compare (0x74-0x76)
                (1, 0x74) | (1, 0x75) | (1, 0x76) => {
                    return self.execute_vex_pcmpeq(ctx, vex_l, vvvv, opcode);
                }

                // AVX2 packed integer arithmetic (0xD4-0xFE)
                (1, 0xD4) | (1, 0xD5) | // VPADDQ, VPMULLW
                (1, 0xD8) | (1, 0xD9) | // VPSUBUSB, VPSUBUSW
                (1, 0xDA) | (1, 0xDB) | // VPMINUB, VPAND
                (1, 0xDC) | (1, 0xDD) | // VPADDUSB, VPADDUSW
                (1, 0xDE) | (1, 0xDF) | // VPMAXUB, VPANDN
                (1, 0xE0) | (1, 0xE3) | // VPAVGB, VPAVGW
                (1, 0xE4) | (1, 0xE5) | // VPMULHUW, VPMULHW
                (1, 0xE8) | (1, 0xE9) | // VPSUBSB, VPSUBSW
                (1, 0xEA) | (1, 0xEB) | // VPMINSW, VPOR
                (1, 0xEC) | (1, 0xED) | // VPADDSB, VPADDSW
                (1, 0xEE) | (1, 0xEF) | // VPMAXSW, VPXOR
                (1, 0xF4) | (1, 0xF5) | // VPMULUDQ, VPMADDWD
                (1, 0xF6) | // VPSADBW
                (1, 0xF8) | (1, 0xF9) | (1, 0xFA) | (1, 0xFB) | // VPSUBB/W/D/Q
                (1, 0xFC) | (1, 0xFD) | (1, 0xFE) => { // VPADDB/W/D
                    return self.execute_vex_packed_int_arith(ctx, vex_l, vvvv, opcode);
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

        // VEX.0F38 SIMD instructions (m_mmmm=2, pp=1 for 66 prefix)
        if m_mmmm == 0x2 && vex_pp == 1 {
            match opcode {
                // VPSHUFB: shuffle bytes
                0x00 => {
                    return self.execute_vex_pshufb(ctx, vex_l, vvvv);
                }
                // VPHADDW: horizontal add words
                0x01 => {
                    return self.execute_vex_phadd(ctx, vex_l, vvvv, 16, false);
                }
                // VPHADDD: horizontal add dwords
                0x02 => {
                    return self.execute_vex_phadd(ctx, vex_l, vvvv, 32, false);
                }
                // VPHADDSW: horizontal add signed words with saturation
                0x03 => {
                    return self.execute_vex_phadd(ctx, vex_l, vvvv, 16, true);
                }
                // VPMADDUBSW: multiply and add unsigned/signed bytes to words
                0x04 => {
                    return self.execute_vex_pmaddubsw(ctx, vex_l, vvvv);
                }
                // VPHSUBW: horizontal subtract words
                0x05 => {
                    return self.execute_vex_phsub(ctx, vex_l, vvvv, 16, false);
                }
                // VPHSUBD: horizontal subtract dwords
                0x06 => {
                    return self.execute_vex_phsub(ctx, vex_l, vvvv, 32, false);
                }
                // VPHSUBSW: horizontal subtract signed words with saturation
                0x07 => {
                    return self.execute_vex_phsub(ctx, vex_l, vvvv, 16, true);
                }
                // VPSIGNB/W/D: negate/zero/copy based on sign
                0x08 | 0x09 | 0x0A => {
                    return self.execute_vex_psign(ctx, vex_l, vvvv, opcode);
                }
                // VPMULHRSW: multiply high with rounding and scale
                0x0B => {
                    return self.execute_vex_pmulhrsw(ctx, vex_l, vvvv);
                }
                // VPERMILPS: permute single-precision floating-point
                0x0C => {
                    return self.execute_vex_permilps_reg(ctx, vex_l, vvvv);
                }
                // VPERMILPD: permute double-precision floating-point
                0x0D => {
                    return self.execute_vex_permilpd_reg(ctx, vex_l, vvvv);
                }
                // VPBLENDVB: variable blend bytes
                0x4C => {
                    return self.execute_vex_pblendvb(ctx, vex_l, vvvv);
                }
                // VPBROADCASTB/W/D/Q: broadcast
                0x78 | 0x79 | 0x58 | 0x59 => {
                    return self.execute_vex_broadcast(ctx, vex_l, opcode);
                }
                // VPMOVSXBW/BD/BQ/WD/WQ/DQ: packed move with sign extension
                0x20 | 0x21 | 0x22 | 0x23 | 0x24 | 0x25 => {
                    return self.execute_vex_pmovsx(ctx, vex_l, opcode);
                }
                // VPMOVZXBW/BD/BQ/WD/WQ/DQ: packed move with zero extension
                0x30 | 0x31 | 0x32 | 0x33 | 0x34 | 0x35 => {
                    return self.execute_vex_pmovzx(ctx, vex_l, opcode);
                }
                // VPMULDQ: multiply packed signed dword integers
                0x28 => {
                    return self.execute_vex_pmuldq(ctx, vex_l, vvvv);
                }
                // VPCMPEQQ: compare packed quadwords for equal
                0x29 => {
                    return self.execute_vex_pcmpeqq(ctx, vex_l, vvvv);
                }
                // VPACKUSDW: pack dwords to unsigned words with saturation
                0x2B => {
                    return self.execute_vex_packusdw(ctx, vex_l, vvvv);
                }
                // VPMINSB: minimum of signed bytes
                0x38 => {
                    return self.execute_vex_pminmax_sb(ctx, vex_l, vvvv, false);
                }
                // VPMINSD: minimum of signed dwords
                0x39 => {
                    return self.execute_vex_pminmax_sd(ctx, vex_l, vvvv, false);
                }
                // VPMINUW: minimum of unsigned words
                0x3A => {
                    return self.execute_vex_pminmax_uw(ctx, vex_l, vvvv, false);
                }
                // VPMINUD: minimum of unsigned dwords
                0x3B => {
                    return self.execute_vex_pminmax_ud(ctx, vex_l, vvvv, false);
                }
                // VPMAXSB: maximum of signed bytes
                0x3C => {
                    return self.execute_vex_pminmax_sb(ctx, vex_l, vvvv, true);
                }
                // VPMAXSD: maximum of signed dwords
                0x3D => {
                    return self.execute_vex_pminmax_sd(ctx, vex_l, vvvv, true);
                }
                // VPMAXUW: maximum of unsigned words
                0x3E => {
                    return self.execute_vex_pminmax_uw(ctx, vex_l, vvvv, true);
                }
                // VPMAXUD: maximum of unsigned dwords
                0x3F => {
                    return self.execute_vex_pminmax_ud(ctx, vex_l, vvvv, true);
                }
                // VPMULLD: multiply packed dword integers
                0x40 => {
                    return self.execute_vex_pmulld(ctx, vex_l, vvvv);
                }
                // VPCMPGTQ: compare packed qwords for greater than
                0x37 => {
                    return self.execute_vex_pcmpgtq(ctx, vex_l, vvvv);
                }
                // VPABSB/W/D: packed absolute value
                0x1C | 0x1D | 0x1E => {
                    return self.execute_vex_pabs(ctx, vex_l, opcode);
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

    /// VEX PUNPCK*, PACK*, PCMPGT* instructions (0x60-0x6D)
    fn execute_vex_punpck(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        // Get source 2 from rm
        let (src2_lo, src2_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };

        // Get source 1 from vvvv
        let src1_lo = self.regs.xmm[xmm_src1][0];
        let src1_hi = self.regs.xmm[xmm_src1][1];

        match opcode {
            // PUNPCKLBW: interleave low bytes
            0x60 => {
                self.regs.xmm[xmm_dst][0] = self.unpack_low_bytes(src1_lo, src2_lo);
                self.regs.xmm[xmm_dst][1] = self.unpack_high_bytes(src1_lo, src2_lo);
            }
            // PUNPCKLWD: interleave low words
            0x61 => {
                self.regs.xmm[xmm_dst][0] = self.unpack_low_words(src1_lo, src2_lo);
                self.regs.xmm[xmm_dst][1] = self.unpack_high_words(src1_lo, src2_lo);
            }
            // PUNPCKLDQ: interleave low dwords
            0x62 => {
                // Low dwords from each qword interleaved
                let d0_src1 = src1_lo as u32;
                let d0_src2 = src2_lo as u32;
                let d1_src1 = (src1_lo >> 32) as u32;
                let d1_src2 = (src2_lo >> 32) as u32;
                self.regs.xmm[xmm_dst][0] = (d0_src1 as u64) | ((d0_src2 as u64) << 32);
                self.regs.xmm[xmm_dst][1] = (d1_src1 as u64) | ((d1_src2 as u64) << 32);
            }
            // PACKSSWB: pack words to bytes with signed saturation
            0x63 => {
                self.regs.xmm[xmm_dst][0] = self.pack_sswb(src1_lo, src1_hi);
                self.regs.xmm[xmm_dst][1] = self.pack_sswb(src2_lo, src2_hi);
            }
            // PCMPGTB: compare bytes for greater than
            0x64 => {
                self.regs.xmm[xmm_dst][0] = self.cmp_gt_bytes(src1_lo, src2_lo);
                self.regs.xmm[xmm_dst][1] = self.cmp_gt_bytes(src1_hi, src2_hi);
            }
            // PCMPGTW: compare words for greater than
            0x65 => {
                self.regs.xmm[xmm_dst][0] = self.cmp_gt_words(src1_lo, src2_lo);
                self.regs.xmm[xmm_dst][1] = self.cmp_gt_words(src1_hi, src2_hi);
            }
            // PCMPGTD: compare dwords for greater than
            0x66 => {
                self.regs.xmm[xmm_dst][0] = self.cmp_gt_dwords(src1_lo, src2_lo);
                self.regs.xmm[xmm_dst][1] = self.cmp_gt_dwords(src1_hi, src2_hi);
            }
            // PACKUSWB: pack words to unsigned bytes with saturation
            0x67 => {
                self.regs.xmm[xmm_dst][0] = self.pack_uswb(src1_lo, src1_hi);
                self.regs.xmm[xmm_dst][1] = self.pack_uswb(src2_lo, src2_hi);
            }
            // PUNPCKHBW: interleave high bytes
            0x68 => {
                self.regs.xmm[xmm_dst][0] = self.unpack_low_bytes(src1_hi, src2_hi);
                self.regs.xmm[xmm_dst][1] = self.unpack_high_bytes(src1_hi, src2_hi);
            }
            // PUNPCKHWD: interleave high words
            0x69 => {
                self.regs.xmm[xmm_dst][0] = self.unpack_low_words(src1_hi, src2_hi);
                self.regs.xmm[xmm_dst][1] = self.unpack_high_words(src1_hi, src2_hi);
            }
            // PUNPCKHDQ: interleave high dwords
            0x6A => {
                let d0_src1 = src1_hi as u32;
                let d0_src2 = src2_hi as u32;
                let d1_src1 = (src1_hi >> 32) as u32;
                let d1_src2 = (src2_hi >> 32) as u32;
                self.regs.xmm[xmm_dst][0] = (d0_src1 as u64) | ((d0_src2 as u64) << 32);
                self.regs.xmm[xmm_dst][1] = (d1_src1 as u64) | ((d1_src2 as u64) << 32);
            }
            // PACKSSDW: pack dwords to words with signed saturation
            0x6B => {
                self.regs.xmm[xmm_dst][0] = self.pack_ssdw(src1_lo, src1_hi);
                self.regs.xmm[xmm_dst][1] = self.pack_ssdw(src2_lo, src2_hi);
            }
            // PUNPCKLQDQ: interleave low qwords
            0x6C => {
                self.regs.xmm[xmm_dst][0] = src1_lo;
                self.regs.xmm[xmm_dst][1] = src2_lo;
            }
            // PUNPCKHQDQ: interleave high qwords
            0x6D => {
                self.regs.xmm[xmm_dst][0] = src1_hi;
                self.regs.xmm[xmm_dst][1] = src2_hi;
            }
            _ => unreachable!(),
        }

        // Handle YMM (256-bit)
        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];

            match opcode {
                0x60 => {
                    self.regs.ymm_high[xmm_dst][0] = self.unpack_low_bytes(src1_hi2, src2_hi2);
                    self.regs.ymm_high[xmm_dst][1] = self.unpack_high_bytes(src1_hi2, src2_hi2);
                }
                0x61 => {
                    self.regs.ymm_high[xmm_dst][0] = self.unpack_low_words(src1_hi2, src2_hi2);
                    self.regs.ymm_high[xmm_dst][1] = self.unpack_high_words(src1_hi2, src2_hi2);
                }
                0x62 => {
                    let d0_src1 = src1_hi2 as u32;
                    let d0_src2 = src2_hi2 as u32;
                    let d1_src1 = (src1_hi2 >> 32) as u32;
                    let d1_src2 = (src2_hi2 >> 32) as u32;
                    self.regs.ymm_high[xmm_dst][0] = (d0_src1 as u64) | ((d0_src2 as u64) << 32);
                    self.regs.ymm_high[xmm_dst][1] = (d1_src1 as u64) | ((d1_src2 as u64) << 32);
                }
                0x63 => {
                    self.regs.ymm_high[xmm_dst][0] = self.pack_sswb(src1_hi2, src1_hi3);
                    self.regs.ymm_high[xmm_dst][1] = self.pack_sswb(src2_hi2, src2_hi3);
                }
                0x64 => {
                    self.regs.ymm_high[xmm_dst][0] = self.cmp_gt_bytes(src1_hi2, src2_hi2);
                    self.regs.ymm_high[xmm_dst][1] = self.cmp_gt_bytes(src1_hi3, src2_hi3);
                }
                0x65 => {
                    self.regs.ymm_high[xmm_dst][0] = self.cmp_gt_words(src1_hi2, src2_hi2);
                    self.regs.ymm_high[xmm_dst][1] = self.cmp_gt_words(src1_hi3, src2_hi3);
                }
                0x66 => {
                    self.regs.ymm_high[xmm_dst][0] = self.cmp_gt_dwords(src1_hi2, src2_hi2);
                    self.regs.ymm_high[xmm_dst][1] = self.cmp_gt_dwords(src1_hi3, src2_hi3);
                }
                0x67 => {
                    self.regs.ymm_high[xmm_dst][0] = self.pack_uswb(src1_hi2, src1_hi3);
                    self.regs.ymm_high[xmm_dst][1] = self.pack_uswb(src2_hi2, src2_hi3);
                }
                0x68 => {
                    self.regs.ymm_high[xmm_dst][0] = self.unpack_low_bytes(src1_hi3, src2_hi3);
                    self.regs.ymm_high[xmm_dst][1] = self.unpack_high_bytes(src1_hi3, src2_hi3);
                }
                0x69 => {
                    self.regs.ymm_high[xmm_dst][0] = self.unpack_low_words(src1_hi3, src2_hi3);
                    self.regs.ymm_high[xmm_dst][1] = self.unpack_high_words(src1_hi3, src2_hi3);
                }
                0x6A => {
                    let d0_src1 = src1_hi3 as u32;
                    let d0_src2 = src2_hi3 as u32;
                    let d1_src1 = (src1_hi3 >> 32) as u32;
                    let d1_src2 = (src2_hi3 >> 32) as u32;
                    self.regs.ymm_high[xmm_dst][0] = (d0_src1 as u64) | ((d0_src2 as u64) << 32);
                    self.regs.ymm_high[xmm_dst][1] = (d1_src1 as u64) | ((d1_src2 as u64) << 32);
                }
                0x6B => {
                    self.regs.ymm_high[xmm_dst][0] = self.pack_ssdw(src1_hi2, src1_hi3);
                    self.regs.ymm_high[xmm_dst][1] = self.pack_ssdw(src2_hi2, src2_hi3);
                }
                0x6C => {
                    self.regs.ymm_high[xmm_dst][0] = src1_hi2;
                    self.regs.ymm_high[xmm_dst][1] = src2_hi2;
                }
                0x6D => {
                    self.regs.ymm_high[xmm_dst][0] = src1_hi3;
                    self.regs.ymm_high[xmm_dst][1] = src2_hi3;
                }
                _ => {}
            }
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    // Helper: unpack low 4 bytes from two qwords interleaved
    fn unpack_low_bytes(&self, a: u64, b: u64) -> u64 {
        let a0 = (a >> 0) & 0xFF;
        let b0 = (b >> 0) & 0xFF;
        let a1 = (a >> 8) & 0xFF;
        let b1 = (b >> 8) & 0xFF;
        let a2 = (a >> 16) & 0xFF;
        let b2 = (b >> 16) & 0xFF;
        let a3 = (a >> 24) & 0xFF;
        let b3 = (b >> 24) & 0xFF;
        a0 | (b0 << 8) | (a1 << 16) | (b1 << 24) | (a2 << 32) | (b2 << 40) | (a3 << 48) | (b3 << 56)
    }

    // Helper: unpack high 4 bytes from two qwords interleaved
    fn unpack_high_bytes(&self, a: u64, b: u64) -> u64 {
        let a4 = (a >> 32) & 0xFF;
        let b4 = (b >> 32) & 0xFF;
        let a5 = (a >> 40) & 0xFF;
        let b5 = (b >> 40) & 0xFF;
        let a6 = (a >> 48) & 0xFF;
        let b6 = (b >> 48) & 0xFF;
        let a7 = (a >> 56) & 0xFF;
        let b7 = (b >> 56) & 0xFF;
        a4 | (b4 << 8) | (a5 << 16) | (b5 << 24) | (a6 << 32) | (b6 << 40) | (a7 << 48) | (b7 << 56)
    }

    // Helper: unpack low 2 words from two qwords interleaved
    fn unpack_low_words(&self, a: u64, b: u64) -> u64 {
        let a0 = a & 0xFFFF;
        let b0 = b & 0xFFFF;
        let a1 = (a >> 16) & 0xFFFF;
        let b1 = (b >> 16) & 0xFFFF;
        a0 | (b0 << 16) | (a1 << 32) | (b1 << 48)
    }

    // Helper: unpack high 2 words from two qwords interleaved
    fn unpack_high_words(&self, a: u64, b: u64) -> u64 {
        let a2 = (a >> 32) & 0xFFFF;
        let b2 = (b >> 32) & 0xFFFF;
        let a3 = (a >> 48) & 0xFFFF;
        let b3 = (b >> 48) & 0xFFFF;
        a2 | (b2 << 16) | (a3 << 32) | (b3 << 48)
    }

    // Helper: pack words to bytes with signed saturation
    fn pack_sswb(&self, lo: u64, hi: u64) -> u64 {
        let saturate = |v: i16| -> u8 {
            if v < -128 { 0x80u8 }
            else if v > 127 { 0x7Fu8 }
            else { v as i8 as u8 }
        };
        let b0 = saturate(lo as i16) as u64;
        let b1 = saturate((lo >> 16) as i16) as u64;
        let b2 = saturate((lo >> 32) as i16) as u64;
        let b3 = saturate((lo >> 48) as i16) as u64;
        let b4 = saturate(hi as i16) as u64;
        let b5 = saturate((hi >> 16) as i16) as u64;
        let b6 = saturate((hi >> 32) as i16) as u64;
        let b7 = saturate((hi >> 48) as i16) as u64;
        b0 | (b1 << 8) | (b2 << 16) | (b3 << 24) | (b4 << 32) | (b5 << 40) | (b6 << 48) | (b7 << 56)
    }

    // Helper: pack words to unsigned bytes with saturation
    fn pack_uswb(&self, lo: u64, hi: u64) -> u64 {
        let saturate = |v: i16| -> u8 {
            if v < 0 { 0u8 }
            else if v > 255 { 0xFFu8 }
            else { v as u8 }
        };
        let b0 = saturate(lo as i16) as u64;
        let b1 = saturate((lo >> 16) as i16) as u64;
        let b2 = saturate((lo >> 32) as i16) as u64;
        let b3 = saturate((lo >> 48) as i16) as u64;
        let b4 = saturate(hi as i16) as u64;
        let b5 = saturate((hi >> 16) as i16) as u64;
        let b6 = saturate((hi >> 32) as i16) as u64;
        let b7 = saturate((hi >> 48) as i16) as u64;
        b0 | (b1 << 8) | (b2 << 16) | (b3 << 24) | (b4 << 32) | (b5 << 40) | (b6 << 48) | (b7 << 56)
    }

    // Helper: pack dwords to words with signed saturation
    fn pack_ssdw(&self, lo: u64, hi: u64) -> u64 {
        let saturate = |v: i32| -> u16 {
            if v < -32768 { 0x8000u16 }
            else if v > 32767 { 0x7FFFu16 }
            else { v as i16 as u16 }
        };
        let w0 = saturate(lo as i32) as u64;
        let w1 = saturate((lo >> 32) as i32) as u64;
        let w2 = saturate(hi as i32) as u64;
        let w3 = saturate((hi >> 32) as i32) as u64;
        w0 | (w1 << 16) | (w2 << 32) | (w3 << 48)
    }

    // Helper: compare bytes for greater than
    fn cmp_gt_bytes(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let va = ((a >> (i * 8)) & 0xFF) as i8;
            let vb = ((b >> (i * 8)) & 0xFF) as i8;
            if va > vb {
                result |= 0xFF << (i * 8);
            }
        }
        result
    }

    // Helper: compare words for greater than
    fn cmp_gt_words(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as i16;
            let vb = ((b >> (i * 16)) & 0xFFFF) as i16;
            if va > vb {
                result |= 0xFFFF << (i * 16);
            }
        }
        result
    }

    // Helper: compare dwords for greater than
    fn cmp_gt_dwords(&self, a: u64, b: u64) -> u64 {
        let lo_a = a as i32;
        let lo_b = b as i32;
        let hi_a = (a >> 32) as i32;
        let hi_b = (b >> 32) as i32;
        let lo_res = if lo_a > lo_b { 0xFFFFFFFFu64 } else { 0 };
        let hi_res = if hi_a > hi_b { 0xFFFFFFFFu64 } else { 0 };
        lo_res | (hi_res << 32)
    }

    /// VEX PCMPEQ* instructions (0x74-0x76)
    fn execute_vex_pcmpeq(
        &mut self,
        ctx: &mut InsnContext,
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

        match opcode {
            // PCMPEQB
            0x74 => {
                self.regs.xmm[xmm_dst][0] = self.cmp_eq_bytes(src1_lo, src2_lo);
                self.regs.xmm[xmm_dst][1] = self.cmp_eq_bytes(src1_hi, src2_hi);
            }
            // PCMPEQW
            0x75 => {
                self.regs.xmm[xmm_dst][0] = self.cmp_eq_words(src1_lo, src2_lo);
                self.regs.xmm[xmm_dst][1] = self.cmp_eq_words(src1_hi, src2_hi);
            }
            // PCMPEQD
            0x76 => {
                self.regs.xmm[xmm_dst][0] = self.cmp_eq_dwords(src1_lo, src2_lo);
                self.regs.xmm[xmm_dst][1] = self.cmp_eq_dwords(src1_hi, src2_hi);
            }
            _ => unreachable!(),
        }

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];

            match opcode {
                0x74 => {
                    self.regs.ymm_high[xmm_dst][0] = self.cmp_eq_bytes(src1_hi2, src2_hi2);
                    self.regs.ymm_high[xmm_dst][1] = self.cmp_eq_bytes(src1_hi3, src2_hi3);
                }
                0x75 => {
                    self.regs.ymm_high[xmm_dst][0] = self.cmp_eq_words(src1_hi2, src2_hi2);
                    self.regs.ymm_high[xmm_dst][1] = self.cmp_eq_words(src1_hi3, src2_hi3);
                }
                0x76 => {
                    self.regs.ymm_high[xmm_dst][0] = self.cmp_eq_dwords(src1_hi2, src2_hi2);
                    self.regs.ymm_high[xmm_dst][1] = self.cmp_eq_dwords(src1_hi3, src2_hi3);
                }
                _ => {}
            }
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    // Helper: compare bytes for equality
    fn cmp_eq_bytes(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let va = (a >> (i * 8)) & 0xFF;
            let vb = (b >> (i * 8)) & 0xFF;
            if va == vb {
                result |= 0xFF << (i * 8);
            }
        }
        result
    }

    // Helper: compare words for equality
    fn cmp_eq_words(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = (a >> (i * 16)) & 0xFFFF;
            let vb = (b >> (i * 16)) & 0xFFFF;
            if va == vb {
                result |= 0xFFFF << (i * 16);
            }
        }
        result
    }

    // Helper: compare dwords for equality
    fn cmp_eq_dwords(&self, a: u64, b: u64) -> u64 {
        let lo_a = a as u32;
        let lo_b = b as u32;
        let hi_a = (a >> 32) as u32;
        let hi_b = (b >> 32) as u32;
        let lo_res = if lo_a == lo_b { 0xFFFFFFFFu64 } else { 0 };
        let hi_res = if hi_a == hi_b { 0xFFFFFFFFu64 } else { 0 };
        lo_res | (hi_res << 32)
    }

    /// VEX packed integer arithmetic instructions
    fn execute_vex_packed_int_arith(
        &mut self,
        ctx: &mut InsnContext,
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

        // Process low 128 bits
        let (dst_lo, dst_hi) = self.packed_int_op(src1_lo, src1_hi, src2_lo, src2_hi, opcode);
        self.regs.xmm[xmm_dst][0] = dst_lo;
        self.regs.xmm[xmm_dst][1] = dst_hi;

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];

            let (dst_hi2, dst_hi3) = self.packed_int_op(src1_hi2, src1_hi3, src2_hi2, src2_hi3, opcode);
            self.regs.ymm_high[xmm_dst][0] = dst_hi2;
            self.regs.ymm_high[xmm_dst][1] = dst_hi3;
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    // Helper: perform packed integer operation
    fn packed_int_op(&self, a_lo: u64, a_hi: u64, b_lo: u64, b_hi: u64, opcode: u8) -> (u64, u64) {
        match opcode {
            // PADDQ: add packed qwords
            0xD4 => (a_lo.wrapping_add(b_lo), a_hi.wrapping_add(b_hi)),
            // PMULLW: multiply packed words, low result
            0xD5 => (self.mul_words_low(a_lo, b_lo), self.mul_words_low(a_hi, b_hi)),
            // PSUBUSB: subtract packed unsigned bytes with saturation
            0xD8 => (self.sub_usb(a_lo, b_lo), self.sub_usb(a_hi, b_hi)),
            // PSUBUSW: subtract packed unsigned words with saturation
            0xD9 => (self.sub_usw(a_lo, b_lo), self.sub_usw(a_hi, b_hi)),
            // PMINUB: minimum of packed unsigned bytes
            0xDA => (self.min_ub(a_lo, b_lo), self.min_ub(a_hi, b_hi)),
            // PAND: bitwise AND
            0xDB => (a_lo & b_lo, a_hi & b_hi),
            // PADDUSB: add packed unsigned bytes with saturation
            0xDC => (self.add_usb(a_lo, b_lo), self.add_usb(a_hi, b_hi)),
            // PADDUSW: add packed unsigned words with saturation
            0xDD => (self.add_usw(a_lo, b_lo), self.add_usw(a_hi, b_hi)),
            // PMAXUB: maximum of packed unsigned bytes
            0xDE => (self.max_ub(a_lo, b_lo), self.max_ub(a_hi, b_hi)),
            // PANDN: bitwise AND NOT
            0xDF => (!a_lo & b_lo, !a_hi & b_hi),
            // PAVGB: average packed unsigned bytes
            0xE0 => (self.avg_ub(a_lo, b_lo), self.avg_ub(a_hi, b_hi)),
            // PAVGW: average packed unsigned words
            0xE3 => (self.avg_uw(a_lo, b_lo), self.avg_uw(a_hi, b_hi)),
            // PMULHUW: multiply packed unsigned words, high result
            0xE4 => (self.mul_words_high_unsigned(a_lo, b_lo), self.mul_words_high_unsigned(a_hi, b_hi)),
            // PMULHW: multiply packed signed words, high result
            0xE5 => (self.mul_words_high_signed(a_lo, b_lo), self.mul_words_high_signed(a_hi, b_hi)),
            // PSUBSB: subtract packed signed bytes with saturation
            0xE8 => (self.sub_sb(a_lo, b_lo), self.sub_sb(a_hi, b_hi)),
            // PSUBSW: subtract packed signed words with saturation
            0xE9 => (self.sub_sw(a_lo, b_lo), self.sub_sw(a_hi, b_hi)),
            // PMINSW: minimum of packed signed words
            0xEA => (self.min_sw(a_lo, b_lo), self.min_sw(a_hi, b_hi)),
            // POR: bitwise OR
            0xEB => (a_lo | b_lo, a_hi | b_hi),
            // PADDSB: add packed signed bytes with saturation
            0xEC => (self.add_sb(a_lo, b_lo), self.add_sb(a_hi, b_hi)),
            // PADDSW: add packed signed words with saturation
            0xED => (self.add_sw(a_lo, b_lo), self.add_sw(a_hi, b_hi)),
            // PMAXSW: maximum of packed signed words
            0xEE => (self.max_sw(a_lo, b_lo), self.max_sw(a_hi, b_hi)),
            // PXOR: bitwise XOR
            0xEF => (a_lo ^ b_lo, a_hi ^ b_hi),
            // PMULUDQ: multiply unsigned dwords, produce qword results
            0xF4 => (self.mul_udq(a_lo, b_lo), self.mul_udq(a_hi, b_hi)),
            // PMADDWD: multiply and add packed words
            0xF5 => (self.madd_wd(a_lo, b_lo), self.madd_wd(a_hi, b_hi)),
            // PSADBW: sum of absolute differences
            0xF6 => (self.sad_bw(a_lo, b_lo), self.sad_bw(a_hi, b_hi)),
            // PSUBB: subtract packed bytes
            0xF8 => (self.sub_bytes(a_lo, b_lo), self.sub_bytes(a_hi, b_hi)),
            // PSUBW: subtract packed words
            0xF9 => (self.sub_words(a_lo, b_lo), self.sub_words(a_hi, b_hi)),
            // PSUBD: subtract packed dwords
            0xFA => (self.sub_dwords(a_lo, b_lo), self.sub_dwords(a_hi, b_hi)),
            // PSUBQ: subtract packed qwords
            0xFB => (a_lo.wrapping_sub(b_lo), a_hi.wrapping_sub(b_hi)),
            // PADDB: add packed bytes
            0xFC => (self.add_bytes(a_lo, b_lo), self.add_bytes(a_hi, b_hi)),
            // PADDW: add packed words
            0xFD => (self.add_words(a_lo, b_lo), self.add_words(a_hi, b_hi)),
            // PADDD: add packed dwords
            0xFE => (self.add_dwords(a_lo, b_lo), self.add_dwords(a_hi, b_hi)),
            _ => (0, 0), // Should not happen
        }
    }

    // Helper: multiply packed words, return low 16 bits of each product
    fn mul_words_low(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as i16;
            let vb = ((b >> (i * 16)) & 0xFFFF) as i16;
            let prod = (va as i32) * (vb as i32);
            result |= ((prod as u16) as u64) << (i * 16);
        }
        result
    }

    // Helper: multiply packed unsigned words, return high 16 bits
    fn mul_words_high_unsigned(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as u32;
            let vb = ((b >> (i * 16)) & 0xFFFF) as u32;
            let prod = va * vb;
            result |= ((prod >> 16) as u64) << (i * 16);
        }
        result
    }

    // Helper: multiply packed signed words, return high 16 bits
    fn mul_words_high_signed(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as i16;
            let vb = ((b >> (i * 16)) & 0xFFFF) as i16;
            let prod = (va as i32) * (vb as i32);
            result |= (((prod >> 16) as u16) as u64) << (i * 16);
        }
        result
    }

    // Helper: subtract unsigned bytes with saturation
    fn sub_usb(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let va = ((a >> (i * 8)) & 0xFF) as u8;
            let vb = ((b >> (i * 8)) & 0xFF) as u8;
            let diff = va.saturating_sub(vb);
            result |= (diff as u64) << (i * 8);
        }
        result
    }

    // Helper: subtract unsigned words with saturation
    fn sub_usw(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as u16;
            let vb = ((b >> (i * 16)) & 0xFFFF) as u16;
            let diff = va.saturating_sub(vb);
            result |= (diff as u64) << (i * 16);
        }
        result
    }

    // Helper: add unsigned bytes with saturation
    fn add_usb(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let va = ((a >> (i * 8)) & 0xFF) as u8;
            let vb = ((b >> (i * 8)) & 0xFF) as u8;
            let sum = va.saturating_add(vb);
            result |= (sum as u64) << (i * 8);
        }
        result
    }

    // Helper: add unsigned words with saturation
    fn add_usw(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as u16;
            let vb = ((b >> (i * 16)) & 0xFFFF) as u16;
            let sum = va.saturating_add(vb);
            result |= (sum as u64) << (i * 16);
        }
        result
    }

    // Helper: subtract signed bytes with saturation
    fn sub_sb(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let va = ((a >> (i * 8)) & 0xFF) as i8;
            let vb = ((b >> (i * 8)) & 0xFF) as i8;
            let diff = va.saturating_sub(vb) as u8;
            result |= (diff as u64) << (i * 8);
        }
        result
    }

    // Helper: subtract signed words with saturation
    fn sub_sw(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as i16;
            let vb = ((b >> (i * 16)) & 0xFFFF) as i16;
            let diff = va.saturating_sub(vb) as u16;
            result |= (diff as u64) << (i * 16);
        }
        result
    }

    // Helper: add signed bytes with saturation
    fn add_sb(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let va = ((a >> (i * 8)) & 0xFF) as i8;
            let vb = ((b >> (i * 8)) & 0xFF) as i8;
            let sum = va.saturating_add(vb) as u8;
            result |= (sum as u64) << (i * 8);
        }
        result
    }

    // Helper: add signed words with saturation
    fn add_sw(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as i16;
            let vb = ((b >> (i * 16)) & 0xFFFF) as i16;
            let sum = va.saturating_add(vb) as u16;
            result |= (sum as u64) << (i * 16);
        }
        result
    }

    // Helper: minimum unsigned bytes
    fn min_ub(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let va = ((a >> (i * 8)) & 0xFF) as u8;
            let vb = ((b >> (i * 8)) & 0xFF) as u8;
            result |= (va.min(vb) as u64) << (i * 8);
        }
        result
    }

    // Helper: maximum unsigned bytes
    fn max_ub(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let va = ((a >> (i * 8)) & 0xFF) as u8;
            let vb = ((b >> (i * 8)) & 0xFF) as u8;
            result |= (va.max(vb) as u64) << (i * 8);
        }
        result
    }

    // Helper: minimum signed words
    fn min_sw(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as i16;
            let vb = ((b >> (i * 16)) & 0xFFFF) as i16;
            result |= (va.min(vb) as u16 as u64) << (i * 16);
        }
        result
    }

    // Helper: maximum signed words
    fn max_sw(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as i16;
            let vb = ((b >> (i * 16)) & 0xFFFF) as i16;
            result |= (va.max(vb) as u16 as u64) << (i * 16);
        }
        result
    }

    // Helper: average unsigned bytes
    fn avg_ub(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let va = ((a >> (i * 8)) & 0xFF) as u16;
            let vb = ((b >> (i * 8)) & 0xFF) as u16;
            let avg = ((va + vb + 1) >> 1) as u8;
            result |= (avg as u64) << (i * 8);
        }
        result
    }

    // Helper: average unsigned words
    fn avg_uw(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as u32;
            let vb = ((b >> (i * 16)) & 0xFFFF) as u32;
            let avg = ((va + vb + 1) >> 1) as u16;
            result |= (avg as u64) << (i * 16);
        }
        result
    }

    // Helper: multiply unsigned dwords to produce qwords
    fn mul_udq(&self, a: u64, b: u64) -> u64 {
        // Only uses the low dword of each qword
        let va = a as u32;
        let vb = b as u32;
        (va as u64) * (vb as u64)
    }

    // Helper: multiply and add packed words
    fn madd_wd(&self, a: u64, b: u64) -> u64 {
        let a0 = (a & 0xFFFF) as i16;
        let a1 = ((a >> 16) & 0xFFFF) as i16;
        let a2 = ((a >> 32) & 0xFFFF) as i16;
        let a3 = ((a >> 48) & 0xFFFF) as i16;
        let b0 = (b & 0xFFFF) as i16;
        let b1 = ((b >> 16) & 0xFFFF) as i16;
        let b2 = ((b >> 32) & 0xFFFF) as i16;
        let b3 = ((b >> 48) & 0xFFFF) as i16;
        let d0 = ((a0 as i32) * (b0 as i32) + (a1 as i32) * (b1 as i32)) as u32;
        let d1 = ((a2 as i32) * (b2 as i32) + (a3 as i32) * (b3 as i32)) as u32;
        (d0 as u64) | ((d1 as u64) << 32)
    }

    // Helper: sum of absolute differences
    fn sad_bw(&self, a: u64, b: u64) -> u64 {
        let mut sum = 0u64;
        for i in 0..8 {
            let va = ((a >> (i * 8)) & 0xFF) as i16;
            let vb = ((b >> (i * 8)) & 0xFF) as i16;
            sum += (va - vb).unsigned_abs() as u64;
        }
        sum
    }

    // Helper: add packed bytes
    fn add_bytes(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let va = ((a >> (i * 8)) & 0xFF) as u8;
            let vb = ((b >> (i * 8)) & 0xFF) as u8;
            result |= (va.wrapping_add(vb) as u64) << (i * 8);
        }
        result
    }

    // Helper: subtract packed bytes
    fn sub_bytes(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let va = ((a >> (i * 8)) & 0xFF) as u8;
            let vb = ((b >> (i * 8)) & 0xFF) as u8;
            result |= (va.wrapping_sub(vb) as u64) << (i * 8);
        }
        result
    }

    // Helper: add packed words
    fn add_words(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as u16;
            let vb = ((b >> (i * 16)) & 0xFFFF) as u16;
            result |= (va.wrapping_add(vb) as u64) << (i * 16);
        }
        result
    }

    // Helper: subtract packed words
    fn sub_words(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as u16;
            let vb = ((b >> (i * 16)) & 0xFFFF) as u16;
            result |= (va.wrapping_sub(vb) as u64) << (i * 16);
        }
        result
    }

    // Helper: add packed dwords
    fn add_dwords(&self, a: u64, b: u64) -> u64 {
        let lo = (a as u32).wrapping_add(b as u32);
        let hi = ((a >> 32) as u32).wrapping_add((b >> 32) as u32);
        (lo as u64) | ((hi as u64) << 32)
    }

    // Helper: subtract packed dwords
    fn sub_dwords(&self, a: u64, b: u64) -> u64 {
        let lo = (a as u32).wrapping_sub(b as u32);
        let hi = ((a >> 32) as u32).wrapping_sub((b >> 32) as u32);
        (lo as u64) | ((hi as u64) << 32)
    }

    /// VEX.0F38.00 VPSHUFB: shuffle bytes
    fn execute_vex_pshufb(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        let (mask_lo, mask_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };

        let src_lo = self.regs.xmm[xmm_src1][0];
        let src_hi = self.regs.xmm[xmm_src1][1];

        // Create 16-byte array from source
        let mut src = [0u8; 16];
        for i in 0..8 {
            src[i] = ((src_lo >> (i * 8)) & 0xFF) as u8;
            src[i + 8] = ((src_hi >> (i * 8)) & 0xFF) as u8;
        }

        // Shuffle based on mask
        let mut dst_lo = 0u64;
        let mut dst_hi = 0u64;
        for i in 0..8 {
            let idx = ((mask_lo >> (i * 8)) & 0xFF) as u8;
            let val = if idx & 0x80 != 0 { 0 } else { src[(idx & 0x0F) as usize] };
            dst_lo |= (val as u64) << (i * 8);
        }
        for i in 0..8 {
            let idx = ((mask_hi >> (i * 8)) & 0xFF) as u8;
            let val = if idx & 0x80 != 0 { 0 } else { src[(idx & 0x0F) as usize] };
            dst_hi |= (val as u64) << (i * 8);
        }

        self.regs.xmm[xmm_dst][0] = dst_lo;
        self.regs.xmm[xmm_dst][1] = dst_hi;

        if vex_l == 1 {
            let (mask_hi2, mask_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src_hi3 = self.regs.ymm_high[xmm_src1][1];

            let mut src2 = [0u8; 16];
            for i in 0..8 {
                src2[i] = ((src_hi2 >> (i * 8)) & 0xFF) as u8;
                src2[i + 8] = ((src_hi3 >> (i * 8)) & 0xFF) as u8;
            }

            let mut dst_hi2 = 0u64;
            let mut dst_hi3 = 0u64;
            for i in 0..8 {
                let idx = ((mask_hi2 >> (i * 8)) & 0xFF) as u8;
                let val = if idx & 0x80 != 0 { 0 } else { src2[(idx & 0x0F) as usize] };
                dst_hi2 |= (val as u64) << (i * 8);
            }
            for i in 0..8 {
                let idx = ((mask_hi3 >> (i * 8)) & 0xFF) as u8;
                let val = if idx & 0x80 != 0 { 0 } else { src2[(idx & 0x0F) as usize] };
                dst_hi3 |= (val as u64) << (i * 8);
            }

            self.regs.ymm_high[xmm_dst][0] = dst_hi2;
            self.regs.ymm_high[xmm_dst][1] = dst_hi3;
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX.0F38.01-03 VPHADDW/D/SW: horizontal add
    fn execute_vex_phadd(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
        elem_bits: u32,
        saturate: bool,
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

        let (dst_lo, dst_hi) = self.hadd_128(src1_lo, src1_hi, src2_lo, src2_hi, elem_bits, saturate);
        self.regs.xmm[xmm_dst][0] = dst_lo;
        self.regs.xmm[xmm_dst][1] = dst_hi;

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];

            let (dst_hi2, dst_hi3) = self.hadd_128(src1_hi2, src1_hi3, src2_hi2, src2_hi3, elem_bits, saturate);
            self.regs.ymm_high[xmm_dst][0] = dst_hi2;
            self.regs.ymm_high[xmm_dst][1] = dst_hi3;
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    // Helper: horizontal add for 128-bit lane
    fn hadd_128(&self, a_lo: u64, a_hi: u64, b_lo: u64, b_hi: u64, elem_bits: u32, saturate: bool) -> (u64, u64) {
        if elem_bits == 16 {
            // Words
            let a = [a_lo as u16, (a_lo >> 16) as u16, (a_lo >> 32) as u16, (a_lo >> 48) as u16,
                     a_hi as u16, (a_hi >> 16) as u16, (a_hi >> 32) as u16, (a_hi >> 48) as u16];
            let b = [b_lo as u16, (b_lo >> 16) as u16, (b_lo >> 32) as u16, (b_lo >> 48) as u16,
                     b_hi as u16, (b_hi >> 16) as u16, (b_hi >> 32) as u16, (b_hi >> 48) as u16];
            let mut r = [0u16; 8];
            for i in 0..4 {
                if saturate {
                    r[i] = (a[i*2] as i16).saturating_add(a[i*2+1] as i16) as u16;
                    r[i+4] = (b[i*2] as i16).saturating_add(b[i*2+1] as i16) as u16;
                } else {
                    r[i] = (a[i*2] as i16).wrapping_add(a[i*2+1] as i16) as u16;
                    r[i+4] = (b[i*2] as i16).wrapping_add(b[i*2+1] as i16) as u16;
                }
            }
            let lo = (r[0] as u64) | ((r[1] as u64) << 16) | ((r[2] as u64) << 32) | ((r[3] as u64) << 48);
            let hi = (r[4] as u64) | ((r[5] as u64) << 16) | ((r[6] as u64) << 32) | ((r[7] as u64) << 48);
            (lo, hi)
        } else {
            // Dwords
            let a = [a_lo as u32, (a_lo >> 32) as u32, a_hi as u32, (a_hi >> 32) as u32];
            let b = [b_lo as u32, (b_lo >> 32) as u32, b_hi as u32, (b_hi >> 32) as u32];
            let r0 = (a[0] as i32).wrapping_add(a[1] as i32) as u32;
            let r1 = (a[2] as i32).wrapping_add(a[3] as i32) as u32;
            let r2 = (b[0] as i32).wrapping_add(b[1] as i32) as u32;
            let r3 = (b[2] as i32).wrapping_add(b[3] as i32) as u32;
            let lo = (r0 as u64) | ((r1 as u64) << 32);
            let hi = (r2 as u64) | ((r3 as u64) << 32);
            (lo, hi)
        }
    }

    /// VEX.0F38.05-07 VPHSUBW/D/SW: horizontal subtract
    fn execute_vex_phsub(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
        elem_bits: u32,
        saturate: bool,
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

        let (dst_lo, dst_hi) = self.hsub_128(src1_lo, src1_hi, src2_lo, src2_hi, elem_bits, saturate);
        self.regs.xmm[xmm_dst][0] = dst_lo;
        self.regs.xmm[xmm_dst][1] = dst_hi;

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];

            let (dst_hi2, dst_hi3) = self.hsub_128(src1_hi2, src1_hi3, src2_hi2, src2_hi3, elem_bits, saturate);
            self.regs.ymm_high[xmm_dst][0] = dst_hi2;
            self.regs.ymm_high[xmm_dst][1] = dst_hi3;
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    // Helper: horizontal subtract for 128-bit lane
    fn hsub_128(&self, a_lo: u64, a_hi: u64, b_lo: u64, b_hi: u64, elem_bits: u32, saturate: bool) -> (u64, u64) {
        if elem_bits == 16 {
            let a = [a_lo as u16, (a_lo >> 16) as u16, (a_lo >> 32) as u16, (a_lo >> 48) as u16,
                     a_hi as u16, (a_hi >> 16) as u16, (a_hi >> 32) as u16, (a_hi >> 48) as u16];
            let b = [b_lo as u16, (b_lo >> 16) as u16, (b_lo >> 32) as u16, (b_lo >> 48) as u16,
                     b_hi as u16, (b_hi >> 16) as u16, (b_hi >> 32) as u16, (b_hi >> 48) as u16];
            let mut r = [0u16; 8];
            for i in 0..4 {
                if saturate {
                    r[i] = (a[i*2] as i16).saturating_sub(a[i*2+1] as i16) as u16;
                    r[i+4] = (b[i*2] as i16).saturating_sub(b[i*2+1] as i16) as u16;
                } else {
                    r[i] = (a[i*2] as i16).wrapping_sub(a[i*2+1] as i16) as u16;
                    r[i+4] = (b[i*2] as i16).wrapping_sub(b[i*2+1] as i16) as u16;
                }
            }
            let lo = (r[0] as u64) | ((r[1] as u64) << 16) | ((r[2] as u64) << 32) | ((r[3] as u64) << 48);
            let hi = (r[4] as u64) | ((r[5] as u64) << 16) | ((r[6] as u64) << 32) | ((r[7] as u64) << 48);
            (lo, hi)
        } else {
            let a = [a_lo as u32, (a_lo >> 32) as u32, a_hi as u32, (a_hi >> 32) as u32];
            let b = [b_lo as u32, (b_lo >> 32) as u32, b_hi as u32, (b_hi >> 32) as u32];
            let r0 = (a[0] as i32).wrapping_sub(a[1] as i32) as u32;
            let r1 = (a[2] as i32).wrapping_sub(a[3] as i32) as u32;
            let r2 = (b[0] as i32).wrapping_sub(b[1] as i32) as u32;
            let r3 = (b[2] as i32).wrapping_sub(b[3] as i32) as u32;
            let lo = (r0 as u64) | ((r1 as u64) << 32);
            let hi = (r2 as u64) | ((r3 as u64) << 32);
            (lo, hi)
        }
    }

    /// VEX.0F38.04 VPMADDUBSW: multiply unsigned/signed bytes, add pairs to words with saturation
    fn execute_vex_pmaddubsw(
        &mut self,
        ctx: &mut InsnContext,
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

        self.regs.xmm[xmm_dst][0] = self.pmaddubsw_64(src1_lo, src2_lo);
        self.regs.xmm[xmm_dst][1] = self.pmaddubsw_64(src1_hi, src2_hi);

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = self.pmaddubsw_64(src1_hi2, src2_hi2);
            self.regs.ymm_high[xmm_dst][1] = self.pmaddubsw_64(src1_hi3, src2_hi3);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    // Helper: pmaddubsw for 64 bits (8 bytes -> 4 words)
    fn pmaddubsw_64(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let a0 = ((a >> (i * 16)) & 0xFF) as u8 as i16;
            let a1 = ((a >> (i * 16 + 8)) & 0xFF) as u8 as i16;
            let b0 = ((b >> (i * 16)) & 0xFF) as i8 as i16;
            let b1 = ((b >> (i * 16 + 8)) & 0xFF) as i8 as i16;
            let prod = (a0 * b0 + a1 * b1).clamp(-32768, 32767) as u16;
            result |= (prod as u64) << (i * 16);
        }
        result
    }

    /// VEX.0F38.08-0A VPSIGNB/W/D: negate/zero/copy based on sign
    fn execute_vex_psign(
        &mut self,
        ctx: &mut InsnContext,
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

        let elem_bits = match opcode {
            0x08 => 8,  // PSIGNB
            0x09 => 16, // PSIGNW
            0x0A => 32, // PSIGND
            _ => unreachable!(),
        };

        self.regs.xmm[xmm_dst][0] = self.psign_64(src1_lo, src2_lo, elem_bits);
        self.regs.xmm[xmm_dst][1] = self.psign_64(src1_hi, src2_hi, elem_bits);

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = self.psign_64(src1_hi2, src2_hi2, elem_bits);
            self.regs.ymm_high[xmm_dst][1] = self.psign_64(src1_hi3, src2_hi3, elem_bits);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn psign_64(&self, a: u64, b: u64, elem_bits: u32) -> u64 {
        let elem_count = 64 / elem_bits;
        let mask = (1u64 << elem_bits) - 1;
        let sign_bit = 1u64 << (elem_bits - 1);
        let mut result = 0u64;
        for i in 0..elem_count {
            let shift = i * elem_bits;
            let av = (a >> shift) & mask;
            let bv = (b >> shift) & mask;
            let rv = if bv == 0 {
                0
            } else if bv & sign_bit != 0 {
                // b is negative, negate a
                ((!av).wrapping_add(1)) & mask
            } else {
                av
            };
            result |= rv << shift;
        }
        result
    }

    /// VEX.0F38.0B VPMULHRSW: multiply high with rounding and scale
    fn execute_vex_pmulhrsw(
        &mut self,
        ctx: &mut InsnContext,
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

        self.regs.xmm[xmm_dst][0] = self.pmulhrsw_64(src1_lo, src2_lo);
        self.regs.xmm[xmm_dst][1] = self.pmulhrsw_64(src1_hi, src2_hi);

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = self.pmulhrsw_64(src1_hi2, src2_hi2);
            self.regs.ymm_high[xmm_dst][1] = self.pmulhrsw_64(src1_hi3, src2_hi3);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn pmulhrsw_64(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let av = ((a >> (i * 16)) & 0xFFFF) as i16 as i32;
            let bv = ((b >> (i * 16)) & 0xFFFF) as i16 as i32;
            // temp = (a * b + 0x4000) >> 15
            let temp = ((av * bv + 0x4000) >> 15) as u16;
            result |= (temp as u64) << (i * 16);
        }
        result
    }

    /// VEX.0F38.0C VPERMILPS (register form)
    fn execute_vex_permilps_reg(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src = vvvv as usize;

        let (ctrl_lo, ctrl_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let src_lo = self.regs.xmm[xmm_src][0];
        let src_hi = self.regs.xmm[xmm_src][1];

        let (dst_lo, dst_hi) = self.permilps_128(src_lo, src_hi, ctrl_lo, ctrl_hi);
        self.regs.xmm[xmm_dst][0] = dst_lo;
        self.regs.xmm[xmm_dst][1] = dst_hi;

        if vex_l == 1 {
            let (ctrl_hi2, ctrl_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src_hi2 = self.regs.ymm_high[xmm_src][0];
            let src_hi3 = self.regs.ymm_high[xmm_src][1];
            let (dst_hi2, dst_hi3) = self.permilps_128(src_hi2, src_hi3, ctrl_hi2, ctrl_hi3);
            self.regs.ymm_high[xmm_dst][0] = dst_hi2;
            self.regs.ymm_high[xmm_dst][1] = dst_hi3;
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn permilps_128(&self, src_lo: u64, src_hi: u64, ctrl_lo: u64, ctrl_hi: u64) -> (u64, u64) {
        let floats = [
            src_lo as u32,
            (src_lo >> 32) as u32,
            src_hi as u32,
            (src_hi >> 32) as u32,
        ];
        let r0 = floats[(ctrl_lo & 3) as usize];
        let r1 = floats[((ctrl_lo >> 32) & 3) as usize];
        let r2 = floats[(ctrl_hi & 3) as usize];
        let r3 = floats[((ctrl_hi >> 32) & 3) as usize];
        ((r0 as u64) | ((r1 as u64) << 32), (r2 as u64) | ((r3 as u64) << 32))
    }

    /// VEX.0F38.0D VPERMILPD (register form)
    fn execute_vex_permilpd_reg(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src = vvvv as usize;

        let (ctrl_lo, ctrl_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let src_lo = self.regs.xmm[xmm_src][0];
        let src_hi = self.regs.xmm[xmm_src][1];

        // Select based on bit 1 of each control qword
        let dst_lo = if (ctrl_lo >> 1) & 1 == 0 { src_lo } else { src_hi };
        let dst_hi = if (ctrl_hi >> 1) & 1 == 0 { src_lo } else { src_hi };
        self.regs.xmm[xmm_dst][0] = dst_lo;
        self.regs.xmm[xmm_dst][1] = dst_hi;

        if vex_l == 1 {
            let (ctrl_hi2, ctrl_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src_hi2 = self.regs.ymm_high[xmm_src][0];
            let src_hi3 = self.regs.ymm_high[xmm_src][1];
            let dst_hi2 = if (ctrl_hi2 >> 1) & 1 == 0 { src_hi2 } else { src_hi3 };
            let dst_hi3 = if (ctrl_hi3 >> 1) & 1 == 0 { src_hi2 } else { src_hi3 };
            self.regs.ymm_high[xmm_dst][0] = dst_hi2;
            self.regs.ymm_high[xmm_dst][1] = dst_hi3;
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX.0F38.4C VPBLENDVB: variable blend bytes
    fn execute_vex_pblendvb(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm = ctx.consume_u8()?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;
        let xmm_mask = ((imm >> 4) & 0xF) as usize;

        let (src2_lo, src2_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let src1_lo = self.regs.xmm[xmm_src1][0];
        let src1_hi = self.regs.xmm[xmm_src1][1];
        let mask_lo = self.regs.xmm[xmm_mask][0];
        let mask_hi = self.regs.xmm[xmm_mask][1];

        self.regs.xmm[xmm_dst][0] = self.blend_bytes(src1_lo, src2_lo, mask_lo);
        self.regs.xmm[xmm_dst][1] = self.blend_bytes(src1_hi, src2_hi, mask_hi);

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            let mask_hi2 = self.regs.ymm_high[xmm_mask][0];
            let mask_hi3 = self.regs.ymm_high[xmm_mask][1];
            self.regs.ymm_high[xmm_dst][0] = self.blend_bytes(src1_hi2, src2_hi2, mask_hi2);
            self.regs.ymm_high[xmm_dst][1] = self.blend_bytes(src1_hi3, src2_hi3, mask_hi3);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn blend_bytes(&self, a: u64, b: u64, mask: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let m = (mask >> (i * 8 + 7)) & 1;
            let v = if m != 0 {
                (b >> (i * 8)) & 0xFF
            } else {
                (a >> (i * 8)) & 0xFF
            };
            result |= v << (i * 8);
        }
        result
    }

    /// VEX.0F38.78/79/58/59 VPBROADCAST: broadcast
    fn execute_vex_broadcast(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        let elem_size = match opcode {
            0x78 => 1, // VPBROADCASTB
            0x79 => 2, // VPBROADCASTW
            0x58 => 4, // VPBROADCASTD
            0x59 => 8, // VPBROADCASTQ
            _ => unreachable!(),
        };

        let val = if is_memory {
            self.read_mem(addr, elem_size)?
        } else {
            self.regs.xmm[rm as usize][0] & ((1u64 << (elem_size * 8)) - 1)
        };

        let broadcast = match elem_size {
            1 => {
                let b = val as u8;
                let q = (b as u64) | ((b as u64) << 8) | ((b as u64) << 16) | ((b as u64) << 24)
                      | ((b as u64) << 32) | ((b as u64) << 40) | ((b as u64) << 48) | ((b as u64) << 56);
                (q, q)
            }
            2 => {
                let w = val as u16;
                let q = (w as u64) | ((w as u64) << 16) | ((w as u64) << 32) | ((w as u64) << 48);
                (q, q)
            }
            4 => {
                let d = val as u32;
                let q = (d as u64) | ((d as u64) << 32);
                (q, q)
            }
            8 => (val, val),
            _ => unreachable!(),
        };

        self.regs.xmm[xmm_dst][0] = broadcast.0;
        self.regs.xmm[xmm_dst][1] = broadcast.1;

        if vex_l == 1 {
            self.regs.ymm_high[xmm_dst][0] = broadcast.0;
            self.regs.ymm_high[xmm_dst][1] = broadcast.1;
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX.0F38.20-25 VPMOVSXBW/BD/BQ/WD/WQ/DQ: packed move with sign extension
    fn execute_vex_pmovsx(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        // Read source bytes needed
        let src = if is_memory {
            // Read appropriate bytes based on operation
            let bytes_needed = match opcode {
                0x20 => if vex_l == 0 { 8 } else { 16 },  // BW
                0x21 => if vex_l == 0 { 4 } else { 8 },   // BD
                0x22 => if vex_l == 0 { 2 } else { 4 },   // BQ
                0x23 => if vex_l == 0 { 8 } else { 16 },  // WD
                0x24 => if vex_l == 0 { 4 } else { 8 },   // WQ
                0x25 => if vex_l == 0 { 8 } else { 16 },  // DQ
                _ => 16,
            };
            let mut data = [0u8; 16];
            for i in 0..bytes_needed.min(16) {
                data[i] = self.read_mem(addr + i as u64, 1)? as u8;
            }
            data
        } else {
            let lo = self.regs.xmm[rm as usize][0];
            let hi = self.regs.xmm[rm as usize][1];
            let mut data = [0u8; 16];
            for i in 0..8 { data[i] = ((lo >> (i * 8)) & 0xFF) as u8; }
            for i in 0..8 { data[i + 8] = ((hi >> (i * 8)) & 0xFF) as u8; }
            data
        };

        match opcode {
            0x20 => {
                // PMOVSXBW: bytes to words
                let mut dst = [0u16; 16];
                for i in 0..8 { dst[i] = src[i] as i8 as i16 as u16; }
                self.regs.xmm[xmm_dst][0] = (dst[0] as u64) | ((dst[1] as u64) << 16) | ((dst[2] as u64) << 32) | ((dst[3] as u64) << 48);
                self.regs.xmm[xmm_dst][1] = (dst[4] as u64) | ((dst[5] as u64) << 16) | ((dst[6] as u64) << 32) | ((dst[7] as u64) << 48);
                if vex_l == 1 {
                    for i in 0..8 { dst[i + 8] = src[i + 8] as i8 as i16 as u16; }
                    self.regs.ymm_high[xmm_dst][0] = (dst[8] as u64) | ((dst[9] as u64) << 16) | ((dst[10] as u64) << 32) | ((dst[11] as u64) << 48);
                    self.regs.ymm_high[xmm_dst][1] = (dst[12] as u64) | ((dst[13] as u64) << 16) | ((dst[14] as u64) << 32) | ((dst[15] as u64) << 48);
                }
            }
            0x21 => {
                // PMOVSXBD: bytes to dwords
                let mut dst = [0u32; 8];
                for i in 0..4 { dst[i] = src[i] as i8 as i32 as u32; }
                self.regs.xmm[xmm_dst][0] = (dst[0] as u64) | ((dst[1] as u64) << 32);
                self.regs.xmm[xmm_dst][1] = (dst[2] as u64) | ((dst[3] as u64) << 32);
                if vex_l == 1 {
                    for i in 0..4 { dst[i + 4] = src[i + 4] as i8 as i32 as u32; }
                    self.regs.ymm_high[xmm_dst][0] = (dst[4] as u64) | ((dst[5] as u64) << 32);
                    self.regs.ymm_high[xmm_dst][1] = (dst[6] as u64) | ((dst[7] as u64) << 32);
                }
            }
            0x22 => {
                // PMOVSXBQ: bytes to qwords
                let d0 = src[0] as i8 as i64 as u64;
                let d1 = src[1] as i8 as i64 as u64;
                self.regs.xmm[xmm_dst][0] = d0;
                self.regs.xmm[xmm_dst][1] = d1;
                if vex_l == 1 {
                    let d2 = src[2] as i8 as i64 as u64;
                    let d3 = src[3] as i8 as i64 as u64;
                    self.regs.ymm_high[xmm_dst][0] = d2;
                    self.regs.ymm_high[xmm_dst][1] = d3;
                }
            }
            0x23 => {
                // PMOVSXWD: words to dwords
                let mut dst = [0u32; 8];
                for i in 0..4 {
                    let w = (src[i*2] as u16) | ((src[i*2+1] as u16) << 8);
                    dst[i] = w as i16 as i32 as u32;
                }
                self.regs.xmm[xmm_dst][0] = (dst[0] as u64) | ((dst[1] as u64) << 32);
                self.regs.xmm[xmm_dst][1] = (dst[2] as u64) | ((dst[3] as u64) << 32);
                if vex_l == 1 {
                    for i in 0..4 {
                        let w = (src[8 + i*2] as u16) | ((src[8 + i*2+1] as u16) << 8);
                        dst[i + 4] = w as i16 as i32 as u32;
                    }
                    self.regs.ymm_high[xmm_dst][0] = (dst[4] as u64) | ((dst[5] as u64) << 32);
                    self.regs.ymm_high[xmm_dst][1] = (dst[6] as u64) | ((dst[7] as u64) << 32);
                }
            }
            0x24 => {
                // PMOVSXWQ: words to qwords
                let w0 = (src[0] as u16) | ((src[1] as u16) << 8);
                let w1 = (src[2] as u16) | ((src[3] as u16) << 8);
                self.regs.xmm[xmm_dst][0] = w0 as i16 as i64 as u64;
                self.regs.xmm[xmm_dst][1] = w1 as i16 as i64 as u64;
                if vex_l == 1 {
                    let w2 = (src[4] as u16) | ((src[5] as u16) << 8);
                    let w3 = (src[6] as u16) | ((src[7] as u16) << 8);
                    self.regs.ymm_high[xmm_dst][0] = w2 as i16 as i64 as u64;
                    self.regs.ymm_high[xmm_dst][1] = w3 as i16 as i64 as u64;
                }
            }
            0x25 => {
                // PMOVSXDQ: dwords to qwords
                let d0 = (src[0] as u32) | ((src[1] as u32) << 8) | ((src[2] as u32) << 16) | ((src[3] as u32) << 24);
                let d1 = (src[4] as u32) | ((src[5] as u32) << 8) | ((src[6] as u32) << 16) | ((src[7] as u32) << 24);
                self.regs.xmm[xmm_dst][0] = d0 as i32 as i64 as u64;
                self.regs.xmm[xmm_dst][1] = d1 as i32 as i64 as u64;
                if vex_l == 1 {
                    let d2 = (src[8] as u32) | ((src[9] as u32) << 8) | ((src[10] as u32) << 16) | ((src[11] as u32) << 24);
                    let d3 = (src[12] as u32) | ((src[13] as u32) << 8) | ((src[14] as u32) << 16) | ((src[15] as u32) << 24);
                    self.regs.ymm_high[xmm_dst][0] = d2 as i32 as i64 as u64;
                    self.regs.ymm_high[xmm_dst][1] = d3 as i32 as i64 as u64;
                }
            }
            _ => unreachable!(),
        }

        if vex_l == 0 {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX.0F38.30-35 VPMOVZXBW/BD/BQ/WD/WQ/DQ: packed move with zero extension
    fn execute_vex_pmovzx(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        let src = if is_memory {
            let bytes_needed = match opcode {
                0x30 => if vex_l == 0 { 8 } else { 16 },
                0x31 => if vex_l == 0 { 4 } else { 8 },
                0x32 => if vex_l == 0 { 2 } else { 4 },
                0x33 => if vex_l == 0 { 8 } else { 16 },
                0x34 => if vex_l == 0 { 4 } else { 8 },
                0x35 => if vex_l == 0 { 8 } else { 16 },
                _ => 16,
            };
            let mut data = [0u8; 16];
            for i in 0..bytes_needed.min(16) {
                data[i] = self.read_mem(addr + i as u64, 1)? as u8;
            }
            data
        } else {
            let lo = self.regs.xmm[rm as usize][0];
            let hi = self.regs.xmm[rm as usize][1];
            let mut data = [0u8; 16];
            for i in 0..8 { data[i] = ((lo >> (i * 8)) & 0xFF) as u8; }
            for i in 0..8 { data[i + 8] = ((hi >> (i * 8)) & 0xFF) as u8; }
            data
        };

        match opcode {
            0x30 => {
                // PMOVZXBW
                let mut dst = [0u16; 16];
                for i in 0..8 { dst[i] = src[i] as u16; }
                self.regs.xmm[xmm_dst][0] = (dst[0] as u64) | ((dst[1] as u64) << 16) | ((dst[2] as u64) << 32) | ((dst[3] as u64) << 48);
                self.regs.xmm[xmm_dst][1] = (dst[4] as u64) | ((dst[5] as u64) << 16) | ((dst[6] as u64) << 32) | ((dst[7] as u64) << 48);
                if vex_l == 1 {
                    for i in 0..8 { dst[i + 8] = src[i + 8] as u16; }
                    self.regs.ymm_high[xmm_dst][0] = (dst[8] as u64) | ((dst[9] as u64) << 16) | ((dst[10] as u64) << 32) | ((dst[11] as u64) << 48);
                    self.regs.ymm_high[xmm_dst][1] = (dst[12] as u64) | ((dst[13] as u64) << 16) | ((dst[14] as u64) << 32) | ((dst[15] as u64) << 48);
                }
            }
            0x31 => {
                // PMOVZXBD
                let mut dst = [0u32; 8];
                for i in 0..4 { dst[i] = src[i] as u32; }
                self.regs.xmm[xmm_dst][0] = (dst[0] as u64) | ((dst[1] as u64) << 32);
                self.regs.xmm[xmm_dst][1] = (dst[2] as u64) | ((dst[3] as u64) << 32);
                if vex_l == 1 {
                    for i in 0..4 { dst[i + 4] = src[i + 4] as u32; }
                    self.regs.ymm_high[xmm_dst][0] = (dst[4] as u64) | ((dst[5] as u64) << 32);
                    self.regs.ymm_high[xmm_dst][1] = (dst[6] as u64) | ((dst[7] as u64) << 32);
                }
            }
            0x32 => {
                // PMOVZXBQ
                self.regs.xmm[xmm_dst][0] = src[0] as u64;
                self.regs.xmm[xmm_dst][1] = src[1] as u64;
                if vex_l == 1 {
                    self.regs.ymm_high[xmm_dst][0] = src[2] as u64;
                    self.regs.ymm_high[xmm_dst][1] = src[3] as u64;
                }
            }
            0x33 => {
                // PMOVZXWD
                let mut dst = [0u32; 8];
                for i in 0..4 {
                    let w = (src[i*2] as u16) | ((src[i*2+1] as u16) << 8);
                    dst[i] = w as u32;
                }
                self.regs.xmm[xmm_dst][0] = (dst[0] as u64) | ((dst[1] as u64) << 32);
                self.regs.xmm[xmm_dst][1] = (dst[2] as u64) | ((dst[3] as u64) << 32);
                if vex_l == 1 {
                    for i in 0..4 {
                        let w = (src[8 + i*2] as u16) | ((src[8 + i*2+1] as u16) << 8);
                        dst[i + 4] = w as u32;
                    }
                    self.regs.ymm_high[xmm_dst][0] = (dst[4] as u64) | ((dst[5] as u64) << 32);
                    self.regs.ymm_high[xmm_dst][1] = (dst[6] as u64) | ((dst[7] as u64) << 32);
                }
            }
            0x34 => {
                // PMOVZXWQ
                let w0 = (src[0] as u16) | ((src[1] as u16) << 8);
                let w1 = (src[2] as u16) | ((src[3] as u16) << 8);
                self.regs.xmm[xmm_dst][0] = w0 as u64;
                self.regs.xmm[xmm_dst][1] = w1 as u64;
                if vex_l == 1 {
                    let w2 = (src[4] as u16) | ((src[5] as u16) << 8);
                    let w3 = (src[6] as u16) | ((src[7] as u16) << 8);
                    self.regs.ymm_high[xmm_dst][0] = w2 as u64;
                    self.regs.ymm_high[xmm_dst][1] = w3 as u64;
                }
            }
            0x35 => {
                // PMOVZXDQ
                let d0 = (src[0] as u32) | ((src[1] as u32) << 8) | ((src[2] as u32) << 16) | ((src[3] as u32) << 24);
                let d1 = (src[4] as u32) | ((src[5] as u32) << 8) | ((src[6] as u32) << 16) | ((src[7] as u32) << 24);
                self.regs.xmm[xmm_dst][0] = d0 as u64;
                self.regs.xmm[xmm_dst][1] = d1 as u64;
                if vex_l == 1 {
                    let d2 = (src[8] as u32) | ((src[9] as u32) << 8) | ((src[10] as u32) << 16) | ((src[11] as u32) << 24);
                    let d3 = (src[12] as u32) | ((src[13] as u32) << 8) | ((src[14] as u32) << 16) | ((src[15] as u32) << 24);
                    self.regs.ymm_high[xmm_dst][0] = d2 as u64;
                    self.regs.ymm_high[xmm_dst][1] = d3 as u64;
                }
            }
            _ => unreachable!(),
        }

        if vex_l == 0 {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX.0F38.28 VPMULDQ: multiply packed signed dwords to qwords
    fn execute_vex_pmuldq(
        &mut self,
        ctx: &mut InsnContext,
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

        // Use low dword of each qword
        let a0 = src1_lo as i32 as i64;
        let b0 = src2_lo as i32 as i64;
        let a1 = src1_hi as i32 as i64;
        let b1 = src2_hi as i32 as i64;

        self.regs.xmm[xmm_dst][0] = (a0 * b0) as u64;
        self.regs.xmm[xmm_dst][1] = (a1 * b1) as u64;

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            let a2 = src1_hi2 as i32 as i64;
            let b2 = src2_hi2 as i32 as i64;
            let a3 = src1_hi3 as i32 as i64;
            let b3 = src2_hi3 as i32 as i64;
            self.regs.ymm_high[xmm_dst][0] = (a2 * b2) as u64;
            self.regs.ymm_high[xmm_dst][1] = (a3 * b3) as u64;
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX.0F38.29 VPCMPEQQ: compare packed qwords for equal
    fn execute_vex_pcmpeqq(
        &mut self,
        ctx: &mut InsnContext,
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

        self.regs.xmm[xmm_dst][0] = if src1_lo == src2_lo { !0u64 } else { 0 };
        self.regs.xmm[xmm_dst][1] = if src1_hi == src2_hi { !0u64 } else { 0 };

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = if src1_hi2 == src2_hi2 { !0u64 } else { 0 };
            self.regs.ymm_high[xmm_dst][1] = if src1_hi3 == src2_hi3 { !0u64 } else { 0 };
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX.0F38.2B VPACKUSDW: pack dwords to unsigned words with saturation
    fn execute_vex_packusdw(
        &mut self,
        ctx: &mut InsnContext,
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

        self.regs.xmm[xmm_dst][0] = self.pack_usdw(src1_lo, src1_hi);
        self.regs.xmm[xmm_dst][1] = self.pack_usdw(src2_lo, src2_hi);

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = self.pack_usdw(src1_hi2, src1_hi3);
            self.regs.ymm_high[xmm_dst][1] = self.pack_usdw(src2_hi2, src2_hi3);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn pack_usdw(&self, lo: u64, hi: u64) -> u64 {
        let saturate = |v: i32| -> u16 {
            if v < 0 { 0u16 }
            else if v > 65535 { 0xFFFFu16 }
            else { v as u16 }
        };
        let w0 = saturate(lo as i32) as u64;
        let w1 = saturate((lo >> 32) as i32) as u64;
        let w2 = saturate(hi as i32) as u64;
        let w3 = saturate((hi >> 32) as i32) as u64;
        w0 | (w1 << 16) | (w2 << 32) | (w3 << 48)
    }

    /// VEX.0F38.38/3C VPMINSB/VPMAXSB: min/max of signed bytes
    fn execute_vex_pminmax_sb(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
        is_max: bool,
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

        self.regs.xmm[xmm_dst][0] = self.minmax_sb_64(src1_lo, src2_lo, is_max);
        self.regs.xmm[xmm_dst][1] = self.minmax_sb_64(src1_hi, src2_hi, is_max);

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = self.minmax_sb_64(src1_hi2, src2_hi2, is_max);
            self.regs.ymm_high[xmm_dst][1] = self.minmax_sb_64(src1_hi3, src2_hi3, is_max);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn minmax_sb_64(&self, a: u64, b: u64, is_max: bool) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let va = ((a >> (i * 8)) & 0xFF) as i8;
            let vb = ((b >> (i * 8)) & 0xFF) as i8;
            let v = if is_max { va.max(vb) } else { va.min(vb) } as u8;
            result |= (v as u64) << (i * 8);
        }
        result
    }

    /// VEX.0F38.39/3D VPMINSD/VPMAXSD: min/max of signed dwords
    fn execute_vex_pminmax_sd(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
        is_max: bool,
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

        self.regs.xmm[xmm_dst][0] = self.minmax_sd_64(src1_lo, src2_lo, is_max);
        self.regs.xmm[xmm_dst][1] = self.minmax_sd_64(src1_hi, src2_hi, is_max);

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = self.minmax_sd_64(src1_hi2, src2_hi2, is_max);
            self.regs.ymm_high[xmm_dst][1] = self.minmax_sd_64(src1_hi3, src2_hi3, is_max);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn minmax_sd_64(&self, a: u64, b: u64, is_max: bool) -> u64 {
        let a0 = a as i32;
        let a1 = (a >> 32) as i32;
        let b0 = b as i32;
        let b1 = (b >> 32) as i32;
        let r0 = if is_max { a0.max(b0) } else { a0.min(b0) } as u32;
        let r1 = if is_max { a1.max(b1) } else { a1.min(b1) } as u32;
        (r0 as u64) | ((r1 as u64) << 32)
    }

    /// VEX.0F38.3A/3E VPMINUW/VPMAXUW: min/max of unsigned words
    fn execute_vex_pminmax_uw(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
        is_max: bool,
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

        self.regs.xmm[xmm_dst][0] = self.minmax_uw_64(src1_lo, src2_lo, is_max);
        self.regs.xmm[xmm_dst][1] = self.minmax_uw_64(src1_hi, src2_hi, is_max);

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = self.minmax_uw_64(src1_hi2, src2_hi2, is_max);
            self.regs.ymm_high[xmm_dst][1] = self.minmax_uw_64(src1_hi3, src2_hi3, is_max);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn minmax_uw_64(&self, a: u64, b: u64, is_max: bool) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = ((a >> (i * 16)) & 0xFFFF) as u16;
            let vb = ((b >> (i * 16)) & 0xFFFF) as u16;
            let v = if is_max { va.max(vb) } else { va.min(vb) };
            result |= (v as u64) << (i * 16);
        }
        result
    }

    /// VEX.0F38.3B/3F VPMINUD/VPMAXUD: min/max of unsigned dwords
    fn execute_vex_pminmax_ud(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
        is_max: bool,
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

        self.regs.xmm[xmm_dst][0] = self.minmax_ud_64(src1_lo, src2_lo, is_max);
        self.regs.xmm[xmm_dst][1] = self.minmax_ud_64(src1_hi, src2_hi, is_max);

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = self.minmax_ud_64(src1_hi2, src2_hi2, is_max);
            self.regs.ymm_high[xmm_dst][1] = self.minmax_ud_64(src1_hi3, src2_hi3, is_max);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn minmax_ud_64(&self, a: u64, b: u64, is_max: bool) -> u64 {
        let a0 = a as u32;
        let a1 = (a >> 32) as u32;
        let b0 = b as u32;
        let b1 = (b >> 32) as u32;
        let r0 = if is_max { a0.max(b0) } else { a0.min(b0) };
        let r1 = if is_max { a1.max(b1) } else { a1.min(b1) };
        (r0 as u64) | ((r1 as u64) << 32)
    }

    /// VEX.0F38.40 VPMULLD: multiply packed dword integers
    fn execute_vex_pmulld(
        &mut self,
        ctx: &mut InsnContext,
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

        self.regs.xmm[xmm_dst][0] = self.pmulld_64(src1_lo, src2_lo);
        self.regs.xmm[xmm_dst][1] = self.pmulld_64(src1_hi, src2_hi);

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = self.pmulld_64(src1_hi2, src2_hi2);
            self.regs.ymm_high[xmm_dst][1] = self.pmulld_64(src1_hi3, src2_hi3);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn pmulld_64(&self, a: u64, b: u64) -> u64 {
        let a0 = a as u32;
        let a1 = (a >> 32) as u32;
        let b0 = b as u32;
        let b1 = (b >> 32) as u32;
        let r0 = a0.wrapping_mul(b0);
        let r1 = a1.wrapping_mul(b1);
        (r0 as u64) | ((r1 as u64) << 32)
    }

    /// VEX.0F38.37 VPCMPGTQ: compare packed qwords for greater than
    fn execute_vex_pcmpgtq(
        &mut self,
        ctx: &mut InsnContext,
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

        self.regs.xmm[xmm_dst][0] = if (src1_lo as i64) > (src2_lo as i64) { !0u64 } else { 0 };
        self.regs.xmm[xmm_dst][1] = if (src1_hi as i64) > (src2_hi as i64) { !0u64 } else { 0 };

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = if (src1_hi2 as i64) > (src2_hi2 as i64) { !0u64 } else { 0 };
            self.regs.ymm_high[xmm_dst][1] = if (src1_hi3 as i64) > (src2_hi3 as i64) { !0u64 } else { 0 };
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX.0F38.1C-1E VPABSB/W/D: packed absolute value
    fn execute_vex_pabs(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        let (src_lo, src_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };

        let elem_bits = match opcode {
            0x1C => 8,  // PABSB
            0x1D => 16, // PABSW
            0x1E => 32, // PABSD
            _ => unreachable!(),
        };

        self.regs.xmm[xmm_dst][0] = self.pabs_64(src_lo, elem_bits);
        self.regs.xmm[xmm_dst][1] = self.pabs_64(src_hi, elem_bits);

        if vex_l == 1 {
            let (src_hi2, src_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            self.regs.ymm_high[xmm_dst][0] = self.pabs_64(src_hi2, elem_bits);
            self.regs.ymm_high[xmm_dst][1] = self.pabs_64(src_hi3, elem_bits);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn pabs_64(&self, v: u64, elem_bits: u32) -> u64 {
        let elem_count = 64 / elem_bits;
        let mask = (1u64 << elem_bits) - 1;
        let mut result = 0u64;
        for i in 0..elem_count {
            let shift = i * elem_bits;
            let val = (v >> shift) & mask;
            let abs_val = match elem_bits {
                8 => (val as i8).abs() as u8 as u64,
                16 => (val as i16).abs() as u16 as u64,
                32 => (val as i32).abs() as u32 as u64,
                _ => val,
            };
            result |= (abs_val & mask) << shift;
        }
        result
    }
}
