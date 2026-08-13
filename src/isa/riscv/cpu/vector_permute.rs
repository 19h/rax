//! RVV register permutation operations with whole-group snapshot semantics.

use super::{Insn, RiscVCpu, Trap, VLENB};

impl RiscVCpu {
    pub(super) fn exec_whole_register_move(&mut self, insn: &Insn, vm: bool) -> Result<(), Trap> {
        let nreg = match insn.rs1 {
            0 => 1u8,
            1 => 2,
            3 => 4,
            7 => 8,
            _ => return Err(Trap::illegal(insn.raw)),
        };
        if !vm || insn.rd % nreg != 0 || insn.rs2 % nreg != 0 {
            return Err(Trap::illegal(insn.raw));
        }

        let total_bytes = usize::from(nreg) * VLENB as usize;
        let sew_bytes = self.sew_bytes();
        let effective_length = total_bytes / sew_bytes;
        let first_byte = (self.vstart as usize).min(effective_length) * sew_bytes;

        // Source and destination groups may overlap because they have the same
        // EEW. Snapshot the complete source group so the result is independent
        // of copy direction, then preserve every prestart element.
        let mut source = [0u8; 8 * VLENB as usize];
        for (offset, byte) in source[..total_bytes].iter_mut().enumerate() {
            *byte = self.velem(insn.rs2, offset, 1) as u8;
        }
        for (offset, byte) in source[first_byte..total_bytes].iter().enumerate() {
            self.set_velem(insn.rd, first_byte + offset, 1, u64::from(*byte));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::riscv::{FlatMemory, Isa, RiscVConfig, RiscVExit, Xlen, decode};

    fn vmvr(nreg_encoding: u32, vd: u32, vs2: u32) -> u32 {
        (0b100111 << 26)
            | (1 << 25)
            | (vs2 << 20)
            | (nreg_encoding << 15)
            | (0b011 << 12)
            | (vd << 7)
            | 0x57
    }

    fn cpu(vtype: u64, vstart: u64) -> RiscVCpu {
        let mut cpu = RiscVCpu::new(RiscVConfig::rv64gc(), Box::new(FlatMemory::new(0, 0x1000)));
        cpu.set_vl_vtype(0, vtype);
        cpu.set_vstart(vstart);
        cpu
    }

    fn execute(cpu: &mut RiscVCpu, raw: u32) {
        let insn = decode(raw, Xlen::Rv64, &Isa::rv64gc());
        assert_eq!(cpu.execute_insn(&insn, 0x1000), Ok(RiscVExit::Continue));
    }

    #[test]
    fn whole_register_move_resumes_in_sew_sized_elements() {
        let mut cpu = cpu(0x10, 2); // e32,m1; resume after eight bytes
        cpu.set_vreg(2, &[0x22; 16]);
        cpu.set_vreg(3, &[0x33; 16]);
        cpu.set_vreg(4, &[0xaa; 16]);
        cpu.set_vreg(5, &[0xbb; 16]);

        execute(&mut cpu, vmvr(1, 4, 2));

        assert_eq!(&cpu.vreg(4)[..8], &[0xaa; 8]);
        assert_eq!(&cpu.vreg(4)[8..], &[0x22; 8]);
        assert_eq!(cpu.vreg(5), [0x33; 16]);
        assert_eq!(cpu.vstart(), 0);
    }

    #[test]
    fn whole_register_move_snapshots_overlapping_source_group() {
        let mut cpu = cpu(0x00, 0); // e8,m1
        for register in 0..6u8 {
            cpu.set_vreg(register, &[register; 16]);
        }

        execute(&mut cpu, vmvr(3, 4, 0));

        for register in 4..8u8 {
            assert_eq!(cpu.vreg(register), [register - 4; 16]);
        }
    }
}
