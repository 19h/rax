//! Two-byte opcode instruction implementation for x86_64 emulator.

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::execute;
use crate::isa::x86_64::execute::crypto::aes;
use crate::isa::x86_64::execute::crypto::sha;

#[inline]
fn is_legacy_0f38_simd_opcode(opcode: u8) -> bool {
    matches!(
        opcode,
        0x00..=0x0B
            | 0x10
            | 0x14
            | 0x15
            | 0x17
            | 0x1C..=0x1E
            | 0x20..=0x2B
            | 0x30..=0x35
            | 0x37..=0x41
            | 0xC8..=0xCF
            | 0xDB..=0xDF
    )
}

impl X86_64Vcpu {
    #[inline(always)]
    pub(in crate::isa::x86_64) fn execute_0f38(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        let opcode3 = ctx.consume_u8()?;
        if is_legacy_0f38_simd_opcode(opcode3) && self.reject_rex2_for_legacy_simd(ctx)? {
            return Ok(None);
        }

        // Record precise opcode key for profiling
        #[cfg(feature = "profiling")]
        crate::observability::profiling::set_current_opcode_key(
            crate::observability::profiling::OpcodeKey::ThreeByte38(opcode3),
        );

        match opcode3 {
            // ===== SSSE3 Instructions (0x00-0x0B, 0x1C-0x1E) =====
            0x00 => execute::simd::pshufb(self, ctx),
            0x01 => execute::simd::phaddw(self, ctx),
            0x02 => execute::simd::phaddd(self, ctx),
            0x03 => execute::simd::phaddsw(self, ctx),
            0x04 => execute::simd::pmaddubsw(self, ctx),
            0x05 => execute::simd::phsubw(self, ctx),
            0x06 => execute::simd::phsubd(self, ctx),
            0x07 => execute::simd::phsubsw(self, ctx),
            0x08 => execute::simd::psignb(self, ctx),
            0x09 => execute::simd::psignw(self, ctx),
            0x0A => execute::simd::psignd(self, ctx),
            0x0B => execute::simd::pmulhrsw(self, ctx),
            0x1C => execute::simd::pabsb(self, ctx),
            0x1D => execute::simd::pabsw(self, ctx),
            0x1E => execute::simd::pabsd(self, ctx),

            // ===== SSE4.1 Instructions =====
            0x10 => execute::simd::pblendvb(self, ctx),
            0x14 => execute::simd::blendvps(self, ctx),
            0x15 => execute::simd::blendvpd(self, ctx),
            0x17 => execute::simd::ptest(self, ctx),
            0x20 => execute::simd::pmovsxbw(self, ctx),
            0x21 => execute::simd::pmovsxbd(self, ctx),
            0x22 => execute::simd::pmovsxbq(self, ctx),
            0x23 => execute::simd::pmovsxwd(self, ctx),
            0x24 => execute::simd::pmovsxwq(self, ctx),
            0x25 => execute::simd::pmovsxdq(self, ctx),
            0x28 => execute::simd::pmuldq(self, ctx),
            0x29 => execute::simd::pcmpeqq(self, ctx),
            0x2A => execute::simd::movntdqa(self, ctx),
            0x2B => execute::simd::packusdw(self, ctx),
            0x30 => execute::simd::pmovzxbw(self, ctx),
            0x31 => execute::simd::pmovzxbd(self, ctx),
            0x32 => execute::simd::pmovzxbq(self, ctx),
            0x33 => execute::simd::pmovzxwd(self, ctx),
            0x34 => execute::simd::pmovzxwq(self, ctx),
            0x35 => execute::simd::pmovzxdq(self, ctx),
            0x37 => execute::simd::pcmpgtq(self, ctx),
            0x38 => execute::simd::pminsb(self, ctx),
            0x39 => execute::simd::pminsd(self, ctx),
            0x3A => execute::simd::pminuw(self, ctx),
            0x3B => execute::simd::pminud(self, ctx),
            0x3C => execute::simd::pmaxsb(self, ctx),
            0x3D => execute::simd::pmaxsd(self, ctx),
            0x3E => execute::simd::pmaxuw(self, ctx),
            0x3F => execute::simd::pmaxud(self, ctx),
            0x40 => execute::simd::pmulld(self, ctx),
            0x41 => execute::simd::phminposuw(self, ctx),

            // INVPCID r64, m128 (66 0F 38 82 /r in 64-bit mode).
            0x82 => {
                if !ctx.operand_size_override {
                    return self.inject_undefined_instruction();
                }

                let (reg, _rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                if !is_memory {
                    return self.inject_undefined_instruction();
                }

                let invpcid_type = self.get_reg(reg, if self.sregs.cs.l { 8 } else { 4 });
                let descriptor_low = self.read_mem(addr, 8)?;
                let descriptor_linear = self.read_mem(addr + 8, 8)?;
                let descriptor_pcid = descriptor_low & 0x0FFF;
                let descriptor_reserved = descriptor_low & !0x0FFF;
                let cr4_pcide = self.sregs.cr4 & (1 << 17) != 0;

                let invalid = invpcid_type > 3
                    || descriptor_reserved != 0
                    || (!cr4_pcide && invpcid_type <= 1 && descriptor_pcid != 0)
                    || (invpcid_type == 0 && !is_canonical_48(descriptor_linear));
                if invalid {
                    self.inject_exception(13, Some(0))?;
                    return Ok(None);
                }

                // No TLB model is observable through the current emulator state.
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // GFNI: GF2P8MULB xmm1, xmm2/m128 (66 0F 38 CF)
            0xCF => execute::simd::gf2p8mulb(self, ctx),

            // ===== AES-NI Instructions (0xDB-0xDF) =====

            // AESIMC - AES Inverse Mix Columns (0xDB)
            // DEST := InvMixColumns(SRC)
            0xDB => {
                if !ctx.operand_size_override {
                    return self.inject_undefined_instruction();
                }
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let (src_lo, src_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let (result_lo, result_hi) = aes::aesimc(src_lo, src_hi);
                self.regs.xmm[xmm_dst][0] = result_lo;
                self.regs.xmm[xmm_dst][1] = result_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // AESENC - AES Encrypt Round (0xDC)
            // STATE := ShiftRows(SubBytes(STATE)); STATE := MixColumns(STATE); DEST := STATE XOR RoundKey
            0xDC => {
                if !ctx.operand_size_override {
                    return self.inject_undefined_instruction();
                }
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let (key_lo, key_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let state_lo = self.regs.xmm[xmm_dst][0];
                let state_hi = self.regs.xmm[xmm_dst][1];
                let (result_lo, result_hi) = aes::aesenc(state_lo, state_hi, key_lo, key_hi);
                self.regs.xmm[xmm_dst][0] = result_lo;
                self.regs.xmm[xmm_dst][1] = result_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // AESENCLAST - AES Encrypt Last Round (0xDD)
            // STATE := ShiftRows(SubBytes(STATE)); DEST := STATE XOR RoundKey (no MixColumns)
            0xDD => {
                if !ctx.operand_size_override {
                    return self.inject_undefined_instruction();
                }
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let (key_lo, key_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let state_lo = self.regs.xmm[xmm_dst][0];
                let state_hi = self.regs.xmm[xmm_dst][1];
                let (result_lo, result_hi) = aes::aesenclast(state_lo, state_hi, key_lo, key_hi);
                self.regs.xmm[xmm_dst][0] = result_lo;
                self.regs.xmm[xmm_dst][1] = result_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // AESDEC - AES Decrypt Round (0xDE)
            // STATE := InvShiftRows(InvSubBytes(STATE)); STATE := InvMixColumns(STATE); DEST := STATE XOR RoundKey
            0xDE => {
                if !ctx.operand_size_override {
                    return self.inject_undefined_instruction();
                }
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let (key_lo, key_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let state_lo = self.regs.xmm[xmm_dst][0];
                let state_hi = self.regs.xmm[xmm_dst][1];
                let (result_lo, result_hi) = aes::aesdec(state_lo, state_hi, key_lo, key_hi);
                self.regs.xmm[xmm_dst][0] = result_lo;
                self.regs.xmm[xmm_dst][1] = result_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // AESDECLAST - AES Decrypt Last Round (0xDF)
            // STATE := InvShiftRows(InvSubBytes(STATE)); DEST := STATE XOR RoundKey (no InvMixColumns)
            0xDF => {
                if !ctx.operand_size_override {
                    return self.inject_undefined_instruction();
                }
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let (key_lo, key_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let state_lo = self.regs.xmm[xmm_dst][0];
                let state_hi = self.regs.xmm[xmm_dst][1];
                let (result_lo, result_hi) = aes::aesdeclast(state_lo, state_hi, key_lo, key_hi);
                self.regs.xmm[xmm_dst][0] = result_lo;
                self.regs.xmm[xmm_dst][1] = result_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // ===== SHA-NI Instructions (0xC8-0xCD) =====

            // SHA1NEXTE - Calculate SHA1 state variable E after four rounds (0xC8)
            0xC8 => {
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let Some((src2_lo, src2_hi)) = sha::read_xmm_m128(self, rm, is_memory, addr)?
                else {
                    return Ok(None);
                };
                let src1_lo = self.regs.xmm[xmm_dst][0];
                let src1_hi = self.regs.xmm[xmm_dst][1];
                let (result_lo, result_hi) = sha::sha1nexte(src1_lo, src1_hi, src2_lo, src2_hi);
                self.regs.xmm[xmm_dst][0] = result_lo;
                self.regs.xmm[xmm_dst][1] = result_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // SHA1MSG1 - SHA1 message schedule update 1 (0xC9)
            0xC9 => {
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let Some((src2_lo, src2_hi)) = sha::read_xmm_m128(self, rm, is_memory, addr)?
                else {
                    return Ok(None);
                };
                let src1_lo = self.regs.xmm[xmm_dst][0];
                let src1_hi = self.regs.xmm[xmm_dst][1];
                let (result_lo, result_hi) = sha::sha1msg1(src1_lo, src1_hi, src2_lo, src2_hi);
                self.regs.xmm[xmm_dst][0] = result_lo;
                self.regs.xmm[xmm_dst][1] = result_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // SHA1MSG2 - SHA1 message schedule update 2 (0xCA)
            0xCA => {
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let Some((src2_lo, src2_hi)) = sha::read_xmm_m128(self, rm, is_memory, addr)?
                else {
                    return Ok(None);
                };
                let src1_lo = self.regs.xmm[xmm_dst][0];
                let src1_hi = self.regs.xmm[xmm_dst][1];
                let (result_lo, result_hi) = sha::sha1msg2(src1_lo, src1_hi, src2_lo, src2_hi);
                self.regs.xmm[xmm_dst][0] = result_lo;
                self.regs.xmm[xmm_dst][1] = result_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // SHA256RNDS2 - Perform two rounds of SHA256 (0xCB)
            // Uses XMM0 implicitly as the third operand
            0xCB => {
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let Some((src2_lo, src2_hi)) = sha::read_xmm_m128(self, rm, is_memory, addr)?
                else {
                    return Ok(None);
                };
                let src1_lo = self.regs.xmm[xmm_dst][0];
                let src1_hi = self.regs.xmm[xmm_dst][1];
                let xmm0_lo = self.regs.xmm[0][0]; // Implicit XMM0 operand
                let (result_lo, result_hi) =
                    sha::sha256rnds2(src1_lo, src1_hi, src2_lo, src2_hi, xmm0_lo);
                self.regs.xmm[xmm_dst][0] = result_lo;
                self.regs.xmm[xmm_dst][1] = result_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // SHA256MSG1 - SHA256 message schedule update 1 (0xCC)
            0xCC => {
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let Some((src2_lo, src2_hi)) = sha::read_xmm_m128(self, rm, is_memory, addr)?
                else {
                    return Ok(None);
                };
                let src1_lo = self.regs.xmm[xmm_dst][0];
                let src1_hi = self.regs.xmm[xmm_dst][1];
                let (result_lo, result_hi) = sha::sha256msg1(src1_lo, src1_hi, src2_lo, src2_hi);
                self.regs.xmm[xmm_dst][0] = result_lo;
                self.regs.xmm[xmm_dst][1] = result_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // SHA256MSG2 - SHA256 message schedule update 2 (0xCD)
            0xCD => {
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let Some((src2_lo, src2_hi)) = sha::read_xmm_m128(self, rm, is_memory, addr)?
                else {
                    return Ok(None);
                };
                let src1_lo = self.regs.xmm[xmm_dst][0];
                let src1_hi = self.regs.xmm[xmm_dst][1];
                let (result_lo, result_hi) = sha::sha256msg2(src1_lo, src1_hi, src2_lo, src2_hi);
                self.regs.xmm[xmm_dst][0] = result_lo;
                self.regs.xmm[xmm_dst][1] = result_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // ===== CRC32 / MOVBE Instructions =====
            // CRC32 uses F2 prefix, MOVBE doesn't

            // CRC32 r32, r/m8 (F2 0F 38 F0) or MOVBE r, m16/32/64 (0F 38 F0)
            0xF0 => {
                if ctx.rep_prefix == Some(0xF2) {
                    // CRC32 r32/r64, r/m8
                    let has_rex = ctx.rex.is_some();
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let src = if is_memory {
                        self.read_mem(addr, 1)? as u8
                    } else {
                        self.get_reg8(rm, has_rex) as u8
                    };
                    let crc_in = self.get_reg(reg, 4) as u32;
                    let crc_out = execute::crc32c(crc_in, u64::from(src), 1);
                    if ctx.rex_w() {
                        self.set_reg(reg, crc_out as u64, 8);
                    } else {
                        self.set_reg(reg, crc_out as u64, 4);
                    }
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                } else {
                    // MOVBE r, m16/32/64 (load with byte swap)
                    let (reg, _rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    if !is_memory {
                        return self.inject_undefined_instruction();
                    }
                    let size = ctx.op_size;
                    let value = self.read_mem(addr, size)?;
                    let swapped = match size {
                        2 => (value as u16).swap_bytes() as u64,
                        4 => (value as u32).swap_bytes() as u64,
                        8 => value.swap_bytes(),
                        _ => value,
                    };
                    self.set_reg(reg, swapped, size);
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
            }
            // CRC32 r32, r/m16/32/64 (F2 0F 38 F1) or MOVBE m16/32/64, r (0F 38 F1)
            0xF1 => {
                if ctx.rep_prefix == Some(0xF2) {
                    // CRC32 r32/r64, r/m16/32/64
                    let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    let crc_in = self.get_reg(reg, 4) as u32;

                    let (src, data_width) = if ctx.rex_w() {
                        // 64-bit source
                        let src = if is_memory {
                            self.read_mem(addr, 8)?
                        } else {
                            self.get_reg(rm, 8)
                        };
                        (src, 8)
                    } else if ctx.op_size == 2 {
                        // 16-bit source. This follows the operand-size attribute,
                        // not the raw 66 prefix: in a 16-bit compat segment 66
                        // selects the 32-bit CRC32 source form.
                        let src = if is_memory {
                            self.read_mem(addr, 2)? as u16
                        } else {
                            self.get_reg(rm, 2) as u16
                        };
                        (u64::from(src), 2)
                    } else {
                        // 32-bit source
                        let src = if is_memory {
                            self.read_mem(addr, 4)? as u32
                        } else {
                            self.get_reg(rm, 4) as u32
                        };
                        (u64::from(src), 4)
                    };
                    let crc_out = execute::crc32c(crc_in, src, data_width);

                    if ctx.rex_w() {
                        self.set_reg(reg, crc_out as u64, 8);
                    } else {
                        self.set_reg(reg, crc_out as u64, 4);
                    }
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                } else {
                    // MOVBE m16/32/64, r (store with byte swap)
                    let (reg, _rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                    if !is_memory {
                        return self.inject_undefined_instruction();
                    }
                    let size = ctx.op_size;
                    let value = self.get_reg(reg, size);
                    let swapped = match size {
                        2 => (value as u16).swap_bytes() as u64,
                        4 => (value as u32).swap_bytes() as u64,
                        8 => value.swap_bytes(),
                        _ => value,
                    };
                    self.write_mem(addr, swapped, size)?;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
            }
            // ADCX/ADOX (0xF6) - ADX instructions with mandatory prefixes
            0xF6 => {
                if ctx.rep_prefix == Some(0xF3) {
                    execute::arith::adox_r_rm(self, ctx)
                } else if ctx.operand_size_override {
                    execute::arith::adcx_r_rm(self, ctx)
                } else {
                    self.inject_undefined_instruction()
                }
            }
            // MOVDIR64B (0xF8)
            0xF8 => execute::data::movdir64b(self, ctx),
            // MOVDIRI (0xF9)
            0xF9 => execute::data::movdiri(self, ctx),

            _ => self.inject_undefined_instruction(),
        }
    }
}

fn is_canonical_48(addr: u64) -> bool {
    ((addr as i64) << 16 >> 16) as u64 == addr
}
