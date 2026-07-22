//! Direct execution for APX-promoted CRC32 encodings.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::execute;
use crate::vm::vcpu::VcpuExit;

impl X86_64Vcpu {
    /// Execute `EVEX.LLZ.{NP,66}.MAP4.SCALABLE F0/F1 /r` CRC32.
    pub(crate) fn execute_apx_crc32(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx.evex.expect("APX CRC32 requires EVEX context");
        let pp_valid = match opcode {
            0xF0 => evex.pp == 0,
            0xF1 => matches!(evex.pp, 0 | 1),
            _ => false,
        };

        // APX-EVEX-INT requires the encoded V field to name architectural
        // register zero when ND=0. CRC32 supports neither ND nor NF, masking,
        // nor nonzero LL. For ModR/M register forms EVEX.U/X4 must retain its
        // encoded-one value; memory forms may use it as the EGPR index bit.
        let modrm = ctx.peek_u8()?;
        let register_source = modrm >> 6 == 3;
        if !pp_valid
            || evex.nd
            || evex.nf
            || evex.z
            || evex.ll != 0
            || evex.aaa != 0
            || evex.vvvv != 0x0F
            || !evex.v_prime
            || (register_source && !evex.x4)
        {
            return self.inject_invalid_opcode();
        }

        let data_width = if opcode == 0xF0 {
            1
        } else {
            Self::apx_scalar_op_size(ctx)
        };
        let destination_width = if evex.w { 8 } else { 4 };
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let destination = reg | ctx.evex_dest_reg();
        let source = rm | ctx.evex_rm_reg();
        let data = if is_memory {
            self.read_mem(addr, data_width)?
        } else if data_width == 1 {
            // Every extended-EVEX byte form uses SPL/BPL/SIL/DIL rather than
            // the legacy high-byte aliases AH/CH/DH/BH.
            self.get_reg8(source, true)
        } else {
            self.get_reg(source, data_width)
        };
        let crc = self.get_reg(destination, 4) as u32;
        let result = execute::crc32c(crc, data, data_width);

        self.set_reg(destination, u64::from(result), destination_width);
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }
}
