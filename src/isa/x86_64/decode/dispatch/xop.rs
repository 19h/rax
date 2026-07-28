//! AMD XOP instruction-prefix dispatch.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::execute;
use crate::isa::x86_64::execute::bmi::TbmKind;
use crate::isa::x86_64::flags;
use crate::vm::vcpu::VcpuExit;

/// XOP permits only segment overrides and 67H before its 8FH lead byte.
fn has_forbidden_xop_legacy_prefix(ctx: &InsnContext) -> bool {
    let xop_offset = ctx.cursor.saturating_sub(1).min(ctx.bytes_len);
    ctx.bytes[..xop_offset]
        .iter()
        .any(|byte| !matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67))
}

impl X86_64Vcpu {
    /// Disambiguate legacy POP (8F /0) from AMD XOP. AMD reserves XOP map
    /// selectors below 8 specifically so those byte sequences remain POP.
    pub(in crate::isa::x86_64) fn execute_pop_or_xop(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        if ctx.peek_u8()? & 0x1f < 8 {
            return execute::data::pop_rm(self, ctx);
        }
        self.execute_xop(ctx)
    }

    fn execute_xop(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        if has_forbidden_xop_legacy_prefix(ctx) {
            return self.inject_undefined_instruction();
        }

        // 8F [~R ~X ~B mmmmm] [W ~vvvv L pp] opcode
        let p0 = ctx.consume_u8()?;
        let p1 = ctx.consume_u8()?;
        let opcode = ctx.consume_u8()?;
        let map = p0 & 0x1f;
        let w = p1 & 0x80 != 0;
        let vvvv = ((p1 >> 3) & 0x0f) ^ 0x0f;
        let l = (p1 >> 2) & 1;
        let pp = p1 & 3;

        if !(8..=10).contains(&map) {
            return self.inject_undefined_instruction();
        }

        let long_mode = self.sregs.cs.l;
        // In 32-bit protected and compatibility modes XOP.R/X must encode 1,
        // decoded vvvv values 8-15 are invalid, and XOP.B is ignored. Keep all
        // three synthetic REX extensions clear so ModR/M/SIB decoding cannot
        // accidentally expose a long-mode register.
        if !long_mode && (p0 & 0xC0 != 0xC0 || vvvv >= 8) {
            return self.inject_undefined_instruction();
        }
        let (r, x, b) = if long_mode {
            (
                ((p0 >> 7) & 1) ^ 1,
                ((p0 >> 6) & 1) ^ 1,
                ((p0 >> 5) & 1) ^ 1,
            )
        } else {
            (0, 0, 0)
        };
        ctx.rex = Some(0x40 | (u8::from(long_mode && w) << 3) | (r << 2) | (x << 1) | b);
        // XOP.W selects 64 bits only in 64-bit mode. AMD specifies WIG in
        // protected/compatibility 32-bit mode.
        ctx.op_size = if long_mode && w { 8 } else { 4 };
        ctx.rip_relative_offset = 0;

        // All implemented XOP forms require pp=00. Scalar TBM and packed
        // rotate/shift additionally require L=0; VPCMOV admits both vector
        // lengths. Feature/mode checks precede ModR/M address calculation and
        // memory access, preserving #UD priority over any potential memory
        // fault.
        let scalar_tbm =
            (map == 9 && matches!(opcode, 0x01 | 0x02)) || (map == 10 && opcode == 0x10);
        let packed_immediate = map == 8 && matches!(opcode, 0xC0..=0xC3);
        let packed_variable = map == 9 && matches!(opcode, 0x90..=0x9B);
        let packed_bit = packed_immediate || packed_variable;
        let vpcmov = map == 8 && opcode == 0xA2;
        if (!scalar_tbm && !packed_bit && !vpcmov)
            || pp != 0
            || (!vpcmov && l != 0)
            || packed_immediate && (w || vvvv != 0)
        {
            return self.inject_undefined_instruction();
        }

        if packed_bit || vpcmov {
            const CR0_TS: u64 = 1 << 3;
            const CR4_OSXSAVE: u64 = 1 << 18;
            if !self.xop_enabled()
                || self.sregs.cr0 & 1 == 0
                || self.regs.rflags & flags::bits::VM != 0
                || self.sregs.cr4 & CR4_OSXSAVE == 0
                || self.xcr0 & 0b110 != 0b110
            {
                return self.inject_undefined_instruction();
            }
            if self.sregs.cr0 & CR0_TS != 0 {
                self.inject_exception(7, None)?;
                return Ok(None);
            }
            ctx.rip_relative_offset = usize::from(packed_immediate || vpcmov);
            if vpcmov {
                return execute::simd::execute_xop_vpcmov(self, ctx, vvvv, w, l);
            }
            return execute::simd::execute_xop_packed_bit(
                self,
                ctx,
                opcode,
                vvvv,
                w,
                packed_immediate,
            );
        }

        if !self.tbm_enabled() || self.sregs.cr0 & 1 == 0 || self.regs.rflags & flags::bits::VM != 0
        {
            return self.inject_undefined_instruction();
        }

        if map == 10 {
            // Immediate BEXTR reserves XOP.vvvv as encoded 1111b.
            if vvvv != 0 {
                return self.inject_undefined_instruction();
            }
            ctx.rip_relative_offset = 4;
            return execute::bmi::tbm_bextr_imm(self, ctx);
        }

        let extension = (ctx.peek_u8()? >> 3) & 7;
        let kind = match (opcode, extension) {
            (0x01, 1) => TbmKind::Blcfill,
            (0x01, 2) => TbmKind::Blsfill,
            (0x01, 3) => TbmKind::Blcs,
            (0x01, 4) => TbmKind::Tzmsk,
            (0x01, 5) => TbmKind::Blcic,
            (0x01, 6) => TbmKind::Blsic,
            (0x01, 7) => TbmKind::T1mskc,
            (0x02, 1) => TbmKind::Blcmsk,
            (0x02, 6) => TbmKind::Blci,
            _ => return self.inject_undefined_instruction(),
        };
        execute::bmi::tbm(self, ctx, vvvv, kind)
    }
}
