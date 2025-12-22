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
}
