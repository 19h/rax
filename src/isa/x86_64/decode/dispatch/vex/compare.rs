//! VEX compare instruction implementations.

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::flags;

impl X86_64Vcpu {
    fn execute_vex_comis_common(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        let (unordered, greater, less) = if vex_pp == 1 {
            let a = f64::from_bits(self.regs.xmm[xmm_dst][0]);
            let b = if is_memory {
                f64::from_bits(self.read_mem(addr, 8)?)
            } else {
                f64::from_bits(self.regs.xmm[rm as usize][0])
            };
            (a.is_nan() || b.is_nan(), a > b, a < b)
        } else {
            let a = f32::from_bits(self.regs.xmm[xmm_dst][0] as u32);
            let b = if is_memory {
                f32::from_bits(self.read_mem(addr, 4)? as u32)
            } else {
                f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
            };
            (a.is_nan() || b.is_nan(), a > b, a < b)
        };

        // Clear lazy flags before setting flags directly
        self.clear_lazy_flags();

        let clear_mask = flags::bits::ZF
            | flags::bits::PF
            | flags::bits::CF
            | flags::bits::OF
            | flags::bits::AF
            | flags::bits::SF;
        self.regs.rflags &= !clear_mask;

        if unordered {
            self.regs.rflags |= flags::bits::ZF | flags::bits::PF | flags::bits::CF;
        } else if greater {
            // ZF=PF=CF=0
        } else if less {
            self.regs.rflags |= flags::bits::CF;
        } else {
            self.regs.rflags |= flags::bits::ZF;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::isa::x86_64) fn execute_vex_comis(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
    ) -> Result<Option<VcpuExit>> {
        self.execute_vex_comis_common(ctx, vex_pp)
    }

    pub(in crate::isa::x86_64) fn execute_vex_ucomis(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
    ) -> Result<Option<VcpuExit>> {
        self.execute_vex_comis_common(ctx, vex_pp)
    }

    pub(in crate::isa::x86_64) fn execute_vex_vtest(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        if vvvv != 0 {
            return self.inject_undefined_instruction();
        }
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_src1 = reg as usize;
        let lane_bits = if opcode == 0x0E { 32 } else { 64 };
        let lane_count = match (lane_bits, vex_l) {
            (32, 1) => 8,
            (32, _) => 4,
            (64, 1) => 4,
            _ => 2,
        };
        let reg_sign_mask = |vcpu: &X86_64Vcpu, reg: usize| -> u32 {
            let mut mask = 0u32;
            for lane in 0..lane_count {
                let qword = match lane {
                    0 | 1 => vcpu.regs.xmm[reg][0],
                    2 | 3 => {
                        if lane_bits == 32 {
                            vcpu.regs.xmm[reg][1]
                        } else {
                            vcpu.regs.ymm_high[reg][lane - 2]
                        }
                    }
                    4 | 5 => vcpu.regs.ymm_high[reg][0],
                    _ => vcpu.regs.ymm_high[reg][1],
                };
                let sign_set = if lane_bits == 32 {
                    ((qword >> (31 + 32 * (lane & 1))) & 1) != 0
                } else {
                    ((qword >> 63) & 1) != 0
                };
                if sign_set {
                    mask |= 1u32 << lane;
                }
            }
            mask
        };
        let mask1 = reg_sign_mask(self, xmm_src1);
        let mask2 = if is_memory {
            let mut mask = 0u32;
            for lane in 0..lane_count {
                let val = if lane_bits == 32 {
                    self.read_mem(addr + (lane * 4) as u64, 4)?
                } else {
                    self.read_mem(addr + (lane * 8) as u64, 8)?
                };
                let sign_set = if lane_bits == 32 {
                    (val & 0x8000_0000) != 0
                } else {
                    ((val >> 63) & 1) != 0
                };
                if sign_set {
                    mask |= 1u32 << lane;
                }
            }
            mask
        } else {
            reg_sign_mask(self, rm as usize)
        };
        let and_result = mask1 & mask2;
        let andn_result = mask2 & !mask1;

        // Clear lazy flags before setting flags directly
        self.clear_lazy_flags();

        self.regs.rflags &= !(flags::bits::AF
            | flags::bits::OF
            | flags::bits::PF
            | flags::bits::SF
            | flags::bits::ZF
            | flags::bits::CF);
        if and_result == 0 {
            self.regs.rflags |= flags::bits::ZF;
        }
        if andn_result == 0 {
            self.regs.rflags |= flags::bits::CF;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }
}
