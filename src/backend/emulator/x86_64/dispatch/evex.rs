//! EVEX-encoded (AVX-512) instruction dispatch.
//!
//! EVEX prefix format (after 0x62):
//! - P0: R X B R' 0 m m m
//! - P1: W v v v v 1 p p
//! - P2: z L' L b V' a a a
//!
//! mm field (opcode map):
//! - 1: 0F (two-byte opcode)
//! - 2: 0F 38 (three-byte opcode)
//! - 3: 0F 3A (three-byte opcode with immediate)

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::insn;

impl X86_64Vcpu {
    /// Execute EVEX-encoded instruction.
    /// mm: opcode map (1=0F, 2=0F38, 3=0F3A)
    pub(in crate::backend::emulator::x86_64) fn execute_evex(
        &mut self,
        ctx: &mut InsnContext,
        mm: u8,
    ) -> Result<Option<VcpuExit>> {
        let opcode = ctx.consume_u8()?;

        match mm {
            1 => self.execute_evex_0f(ctx, opcode),
            2 => self.execute_evex_0f38(ctx, opcode),
            3 => self.execute_evex_0f3a(ctx, opcode),
            _ => Err(Error::Emulator(format!(
                "Invalid EVEX mm field {} at RIP={:#x}",
                mm, self.regs.rip
            ))),
        }
    }

    /// EVEX 0F opcode map (mm=1)
    fn execute_evex_0f(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        Err(Error::Emulator(format!(
            "Unimplemented EVEX.0F opcode {:#04x} at RIP={:#x}",
            opcode, self.regs.rip
        )))
    }

    /// EVEX 0F38 opcode map (mm=2)
    fn execute_evex_0f38(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx.evex.ok_or_else(|| {
            Error::Emulator("EVEX context missing".to_string())
        })?;

        match opcode {
            // VPMULLD/VPMULLQ (0x40)
            // W=0: VPMULLD (32-bit elements)
            // W=1: VPMULLQ (64-bit elements)
            0x40 if evex.pp == 1 => {
                if evex.w {
                    insn::simd::vpmullq(self, ctx)
                } else {
                    insn::simd::vpmulld_evex(self, ctx)
                }
            }

            _ => Err(Error::Emulator(format!(
                "Unimplemented EVEX.0F38 opcode {:#04x} (W={}) at RIP={:#x}",
                opcode, evex.w as u8, self.regs.rip
            ))),
        }
    }

    /// EVEX 0F3A opcode map (mm=3)
    fn execute_evex_0f3a(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        Err(Error::Emulator(format!(
            "Unimplemented EVEX.0F3A opcode {:#04x} at RIP={:#x}",
            opcode, self.regs.rip
        )))
    }
}
