//! Direct execution for APX-promoted MOVBE encodings.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

impl X86_64Vcpu {
    /// Execute `EVEX.LLZ.{NP,66}.MAP4.SCALABLE {60,61} /r`.
    pub(crate) fn execute_apx_movbe(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx.evex.expect("APX MOVBE requires EVEX context");
        let modrm = ctx.peek_u8()?;
        let register_form = modrm >> 6 == 3;

        // MOVBE is a two-operand APX-EVEX-INT family. VVVVV and all payload
        // fields are fixed zero, ND/NF are unavailable, and U/X4 retains its
        // encoded-one value for register operands. Memory operands may use X4
        // as the high SIB-index extension.
        if !matches!(evex.pp, 0 | 1)
            || evex.nd
            || evex.nf
            || evex.z
            || evex.ll != 0
            || evex.aaa != 0
            || evex.vvvv != 0x0F
            || !evex.v_prime
            || (register_form && !evex.x4)
        {
            return self.inject_invalid_opcode();
        }

        let op_size = Self::apx_scalar_op_size(ctx);
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let reg = reg | ctx.evex_dest_reg();
        let rm = rm | ctx.evex_rm_reg();
        let swap = |value: u64| match op_size {
            2 => u64::from((value as u16).swap_bytes()),
            4 => u64::from((value as u32).swap_bytes()),
            8 => value.swap_bytes(),
            _ => unreachable!("APX MOVBE has only 16-, 32-, and 64-bit forms"),
        };

        match opcode {
            0x60 => {
                let source = if is_memory {
                    self.read_mem(addr, op_size)?
                } else {
                    self.get_reg(rm, op_size)
                };
                self.set_reg(reg, swap(source), op_size);
            }
            0x61 => {
                let value = swap(self.get_reg(reg, op_size));
                if is_memory {
                    self.write_mem(addr, value, op_size)?;
                } else {
                    self.set_reg(rm, value, op_size);
                }
            }
            _ => unreachable!("MAP4 MOVBE dispatch admits only opcodes 60/61"),
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }
}
