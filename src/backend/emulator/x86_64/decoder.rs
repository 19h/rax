//! x86_64 instruction decoder with LUT-based prefix detection.

use super::cpu::{InsnContext, X86_64Vcpu};
use crate::error::{Error, Result};

/// Lookup table for prefix detection (256 bytes, index = byte value).
/// Value is 1 if the byte is a prefix, 0 otherwise.
static PREFIX_LUT: [u8; 256] = {
    let mut lut = [0u8; 256];
    // Segment overrides
    lut[0x26] = 1; lut[0x2E] = 1; lut[0x36] = 1;
    lut[0x3E] = 1; lut[0x64] = 1; lut[0x65] = 1;
    // Operand/address size
    lut[0x66] = 1; lut[0x67] = 1;
    // LOCK, REP
    lut[0xF0] = 1; lut[0xF2] = 1; lut[0xF3] = 1;
    // REX (0x40-0x4F)
    lut[0x40] = 1; lut[0x41] = 1; lut[0x42] = 1; lut[0x43] = 1;
    lut[0x44] = 1; lut[0x45] = 1; lut[0x46] = 1; lut[0x47] = 1;
    lut[0x48] = 1; lut[0x49] = 1; lut[0x4A] = 1; lut[0x4B] = 1;
    lut[0x4C] = 1; lut[0x4D] = 1; lut[0x4E] = 1; lut[0x4F] = 1;
    lut
};

pub struct Decoder;

impl Decoder {
    /// Check if byte is a prefix using LUT (faster than match).
    #[inline(always)]
    fn is_prefix(b: u8) -> bool {
        PREFIX_LUT[b as usize] != 0
    }

    /// Decode instruction prefixes and return context.
    #[inline]
    pub fn decode_prefixes(bytes: [u8; super::cpu::MAX_INSN_LEN], bytes_len: usize) -> Result<InsnContext> {
        if bytes_len == 0 {
            return Err(Error::Emulator("instruction too short".to_string()));
        }

        // Fast path: most instructions have no prefixes
        let first = bytes[0];

        if !Self::is_prefix(first) {
            return Ok(InsnContext {
                bytes,
                bytes_len,
                cursor: 0,
                rex: None,
                operand_size_override: false,
                address_size_override: false,
                rep_prefix: None,
                op_size: 4,
                rip_relative_offset: 0,
                segment_override: None,
                evex: None,
            });
        }

        // Has prefix(es) - parse them
        let mut ctx = InsnContext {
            bytes,
            bytes_len,
            cursor: 0,
            rex: None,
            operand_size_override: false,
            address_size_override: false,
            rep_prefix: None,
            op_size: 4,
            rip_relative_offset: 0,
            segment_override: None,
            evex: None,
        };

        loop {
            if ctx.cursor >= ctx.bytes_len {
                return Err(Error::Emulator("instruction too short".to_string()));
            }
            let b = ctx.bytes[ctx.cursor];
            match b {
                0x66 => ctx.operand_size_override = true,
                0x67 => ctx.address_size_override = true,
                0x40..=0x4F => ctx.rex = Some(b),
                0xF0 => {} // LOCK - ignore for now
                0xF2 | 0xF3 => ctx.rep_prefix = Some(b),
                0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 => {
                    ctx.segment_override = Some(b);
                }
                _ => break,
            }
            ctx.cursor += 1;
        }

        Ok(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_prefix() {
        // Segment overrides
        assert!(Decoder::is_prefix(0x26));
        assert!(Decoder::is_prefix(0x2E));
        assert!(Decoder::is_prefix(0x36));
        assert!(Decoder::is_prefix(0x3E));
        assert!(Decoder::is_prefix(0x64));
        assert!(Decoder::is_prefix(0x65));

        // Operand/address size
        assert!(Decoder::is_prefix(0x66));
        assert!(Decoder::is_prefix(0x67));

        // LOCK, REP
        assert!(Decoder::is_prefix(0xF0));
        assert!(Decoder::is_prefix(0xF2));
        assert!(Decoder::is_prefix(0xF3));

        // REX
        for i in 0x40..=0x4F {
            assert!(Decoder::is_prefix(i), "REX 0x{:02X} not detected", i);
        }

        // Non-prefixes
        assert!(!Decoder::is_prefix(0x00));
        assert!(!Decoder::is_prefix(0x90)); // NOP
        assert!(!Decoder::is_prefix(0xB8)); // MOV
        assert!(!Decoder::is_prefix(0xFF));
    }
}

impl X86_64Vcpu {
    /// Get the segment base address for a segment override prefix.
    /// In 64-bit mode, only FS and GS have non-zero bases.
    #[inline]
    pub(super) fn get_segment_base(&self, segment_override: Option<u8>) -> u64 {
        match segment_override {
            Some(0x64) => self.sregs.fs.base, // FS segment
            Some(0x65) => self.sregs.gs.base, // GS segment
            // In 64-bit mode, ES/CS/SS/DS bases are treated as 0
            _ => 0,
        }
    }

    /// Decode ModR/M byte to get effective address.
    /// Returns (address, bytes_consumed_after_modrm).
    #[inline]
    pub(super) fn decode_modrm_addr(
        &self,
        ctx: &InsnContext,
        modrm_offset: usize,
    ) -> Result<(u64, usize)> {
        let bytes = &ctx.bytes[modrm_offset..];
        if bytes.is_empty() {
            return Err(Error::Emulator("ModR/M: no bytes".to_string()));
        }

        let modrm = bytes[0];
        let mod_bits = modrm >> 6;
        let rm_field = modrm & 0x07; // Raw r/m field without REX.B
        let rm = rm_field | ctx.rex_b(); // r/m with REX.B applied (for register selection)
        let mut extra = 0;

        // mod == 3 means register direct, shouldn't call this function
        if mod_bits == 3 {
            return Err(Error::Emulator(
                "ModR/M: mod=3 is register, not memory".to_string(),
            ));
        }

        let mut addr: u64;

        if rm_field == 4 {
            // SIB byte follows
            if bytes.len() < 2 {
                return Err(Error::Emulator("ModR/M: missing SIB byte".to_string()));
            }
            let sib = bytes[1];
            extra += 1;
            let scale = 1u64 << (sib >> 6);
            let index = ((sib >> 3) & 0x07) | (ctx.rex.map_or(0, |r| (r & 0x02) << 2));
            let base_reg = (sib & 0x07) | ctx.rex_b();

            // Calculate base
            addr = if base_reg == 5 && mod_bits == 0 {
                // No base, disp32 follows
                0
            } else {
                self.get_reg(base_reg, 8)
            };

            // Add scaled index (index=4 means no index)
            if index != 4 {
                addr = addr.wrapping_add(self.get_reg(index, 8).wrapping_mul(scale));
            }

            // Handle displacement for base=5, mod=0 case
            if base_reg == 5 && mod_bits == 0 {
                if bytes.len() < 2 + 4 {
                    return Err(Error::Emulator(
                        "ModR/M: missing disp32 for SIB".to_string(),
                    ));
                }
                let disp = i32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]) as i64;
                extra += 4;
                addr = (addr as i64).wrapping_add(disp) as u64;
            }
        } else if rm_field == 5 && mod_bits == 0 {
            // In 64-bit mode (CS.L = 1): RIP-relative addressing [RIP+disp32]
            // In compatibility/legacy mode (CS.L = 0): absolute disp32 [disp32]
            if bytes.len() < 5 {
                return Err(Error::Emulator(
                    "ModR/M: missing disp32 for RIP-relative/disp32".to_string(),
                ));
            }
            let disp = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as i64;
            extra += 4;

            if self.sregs.cs.l {
                // 64-bit mode: RIP-relative addressing
                // RIP points to the next instruction
                let rip_after = self.regs.rip as i64
                    + modrm_offset as i64
                    + 1
                    + 4
                    + ctx.rip_relative_offset as i64;
                addr = rip_after.wrapping_add(disp) as u64;
            } else {
                // Compatibility/legacy mode: absolute [disp32]
                addr = disp as u64;
            }
        } else {
            // Regular register indirect
            addr = self.get_reg(rm, 8);
        }

        // Handle displacement
        match mod_bits {
            0 => {} // No displacement (except special cases handled above)
            1 => {
                // disp8
                if bytes.len() < extra + 2 {
                    return Err(Error::Emulator("ModR/M: missing disp8".to_string()));
                }
                let disp = bytes[extra + 1] as i8 as i64;
                extra += 1;
                addr = (addr as i64).wrapping_add(disp) as u64;
            }
            2 => {
                // disp32
                if bytes.len() < extra + 5 {
                    return Err(Error::Emulator("ModR/M: missing disp32".to_string()));
                }
                let disp = i32::from_le_bytes([
                    bytes[extra + 1],
                    bytes[extra + 2],
                    bytes[extra + 3],
                    bytes[extra + 4],
                ]) as i64;
                extra += 4;
                addr = (addr as i64).wrapping_add(disp) as u64;
            }
            _ => {}
        }

        // Apply segment override (in 64-bit mode, only FS and GS have non-zero bases)
        let seg_base = self.get_segment_base(ctx.segment_override);
        let final_addr = addr.wrapping_add(seg_base);

        Ok((final_addr, extra))
    }

    /// Decode ModR/M and return (reg, rm, is_memory, address_if_memory, extra_bytes).
    /// This is a convenience function that handles both register and memory operands.
    #[inline]
    pub(super) fn decode_modrm(
        &self,
        ctx: &mut InsnContext,
    ) -> Result<(u8, u8, bool, u64, usize)> {
        let modrm_start = ctx.cursor;
        let modrm = ctx.consume_u8()?;
        let reg = ((modrm >> 3) & 0x07) | ctx.rex_r();
        let rm = (modrm & 0x07) | ctx.rex_b();
        let mod_bits = modrm >> 6;

        if mod_bits == 3 {
            // Register operand
            Ok((reg, rm, false, 0, 0))
        } else {
            // Memory operand
            let (addr, extra) = self.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;
            Ok((reg, rm, true, addr, extra))
        }
    }

    /// Read operand from ModR/M - handles both register and memory.
    #[allow(dead_code)]
    pub(super) fn read_rm(&mut self, ctx: &mut InsnContext, size: u8) -> Result<(u64, bool, u64)> {
        let modrm_start = ctx.cursor;
        let modrm = ctx.consume_u8()?;
        let rm = (modrm & 0x07) | ctx.rex_b();
        let mod_bits = modrm >> 6;

        if mod_bits == 3 {
            // Register operand
            Ok((self.get_reg(rm, size), false, 0))
        } else {
            // Memory operand
            let (addr, extra) = self.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;
            let value = self.read_mem(addr, size)?;
            Ok((value, true, addr))
        }
    }

    /// Write to operand from ModR/M - handles both register and memory.
    #[allow(dead_code)]
    pub(super) fn write_rm(
        &mut self,
        rm: u8,
        is_memory: bool,
        addr: u64,
        value: u64,
        size: u8,
    ) -> Result<()> {
        if is_memory {
            self.write_mem(addr, value, size)
        } else {
            self.set_reg(rm, value, size);
            Ok(())
        }
    }

    /// Decode ModR/M address when modrm byte has already been consumed.
    /// Used by FPU instructions where the modrm byte determines the operation.
    /// This reads any SIB/displacement bytes from ctx and updates cursor.
    pub(super) fn decode_fpu_modrm_addr(
        &self,
        ctx: &mut InsnContext,
        modrm: u8,
    ) -> Result<u64> {
        let mod_bits = modrm >> 6;
        let rm_field = modrm & 0x07;
        let rm = rm_field | ctx.rex_b();

        if mod_bits == 3 {
            return Err(Error::Emulator(
                "ModR/M: mod=3 is register, not memory".to_string(),
            ));
        }

        let mut addr: u64;

        if rm_field == 4 {
            // SIB byte follows
            let sib = ctx.consume_u8()?;
            let scale = 1u64 << (sib >> 6);
            let index = ((sib >> 3) & 0x07) | (ctx.rex.map_or(0, |r| (r & 0x02) << 2));
            let base_reg = (sib & 0x07) | ctx.rex_b();

            // Calculate base
            addr = if base_reg == 5 && mod_bits == 0 {
                // No base, disp32 follows
                let disp = ctx.consume_u32()? as i32 as i64;
                disp as u64
            } else {
                self.get_reg(base_reg, 8)
            };

            // Add scaled index (index=4 means no index)
            if index != 4 {
                addr = addr.wrapping_add(self.get_reg(index, 8).wrapping_mul(scale));
            }

            // Handle displacement for mod != 0
            match mod_bits {
                1 => {
                    let disp = ctx.consume_u8()? as i8 as i64;
                    addr = (addr as i64).wrapping_add(disp) as u64;
                }
                2 => {
                    let disp = ctx.consume_u32()? as i32 as i64;
                    addr = (addr as i64).wrapping_add(disp) as u64;
                }
                _ => {}
            }
        } else if rm_field == 5 && mod_bits == 0 {
            // RIP-relative addressing (64-bit mode)
            let disp = ctx.consume_u32()? as i32 as i64;
            let rip_after = self.regs.rip as i64 + ctx.cursor as i64;
            addr = rip_after.wrapping_add(disp) as u64;
        } else {
            // Regular register indirect
            addr = self.get_reg(rm, 8);

            // Handle displacement
            match mod_bits {
                1 => {
                    let disp = ctx.consume_u8()? as i8 as i64;
                    addr = (addr as i64).wrapping_add(disp) as u64;
                }
                2 => {
                    let disp = ctx.consume_u32()? as i32 as i64;
                    addr = (addr as i64).wrapping_add(disp) as u64;
                }
                _ => {}
            }
        }

        // Apply segment override (in 64-bit mode, only FS and GS have non-zero bases)
        let seg_base = self.get_segment_base(ctx.segment_override);
        addr = addr.wrapping_add(seg_base);

        Ok(addr)
    }
}
