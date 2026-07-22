//! Direct execution for APX-promoted POPCNT, TZCNT, and LZCNT.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::flags;
use crate::vm::vcpu::VcpuExit;

impl X86_64Vcpu {
    /// Execute `EVEX.LLZ.{NP,66}.MAP4.SCALABLE {88,F4,F5} /r`.
    pub(crate) fn execute_apx_count(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx.evex.expect("APX count requires EVEX context");

        // APX-EVEX-INT requires ND=0 and an encoded-zero V field for this
        // two-operand family. NF occupies P2 bit 2, so only the low two bits of
        // the decoded `aaa` payload are reserved. U/X4 must retain its encoded
        // one value for a register source; memory forms may use it as X4.
        let modrm = ctx.peek_u8()?;
        let register_source = modrm >> 6 == 3;
        if !matches!(evex.pp, 0 | 1)
            || evex.nd
            || evex.z
            || evex.ll != 0
            || evex.aaa & 0x03 != 0
            || evex.vvvv != 0x0F
            || !evex.v_prime
            || (register_source && !evex.x4)
        {
            return self.inject_invalid_opcode();
        }

        let op_size = Self::apx_scalar_op_size(ctx);
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let destination = reg | ctx.evex_dest_reg();
        let source = if is_memory {
            self.read_mem(addr, op_size)?
        } else {
            self.get_reg(rm | ctx.evex_rm_reg(), op_size)
        };

        let bits = u32::from(op_size) * 8;
        let result = match opcode {
            0x88 => match op_size {
                2 => u64::from((source as u16).count_ones()),
                4 => u64::from((source as u32).count_ones()),
                8 => u64::from(source.count_ones()),
                _ => unreachable!(),
            },
            0xF4 => u64::from(if source == 0 {
                bits
            } else {
                source.trailing_zeros()
            }),
            0xF5 => u64::from(if source == 0 {
                bits
            } else {
                match op_size {
                    2 => (source as u16).leading_zeros(),
                    4 => (source as u32).leading_zeros(),
                    8 => source.leading_zeros(),
                    _ => unreachable!(),
                }
            }),
            _ => unreachable!("MAP4 count dispatch admits only 88/F4/F5"),
        };

        // TZCNT/LZCNT define only CF/ZF. Resolve preceding lazy flags after a
        // potentially faulting source read and before retaining PF/AF/SF/OF.
        // NF suppresses both this materialization and every count flag write.
        if !evex.nf && matches!(opcode, 0xF4 | 0xF5) {
            self.materialize_flags();
        }
        self.set_reg(destination, result, op_size);

        if !evex.nf {
            match opcode {
                0x88 => {
                    self.regs.rflags &= !(flags::bits::OF
                        | flags::bits::SF
                        | flags::bits::ZF
                        | flags::bits::AF
                        | flags::bits::CF
                        | flags::bits::PF);
                    if source == 0 {
                        self.regs.rflags |= flags::bits::ZF;
                    }
                }
                0xF4 | 0xF5 => {
                    self.regs.rflags &= !(flags::bits::CF | flags::bits::ZF);
                    if source == 0 {
                        self.regs.rflags |= flags::bits::CF;
                    }
                    if result == 0 {
                        self.regs.rflags |= flags::bits::ZF;
                    }
                }
                _ => unreachable!(),
            }
            self.clear_lazy_flags();
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }
}
