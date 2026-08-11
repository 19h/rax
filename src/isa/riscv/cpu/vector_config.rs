//! RVV vector-length and vector-type configuration.

use super::{Avl, RiscVCpu, VLEN};

impl RiscVCpu {
    /// Apply a `vtype` and an application vector length, returning the new `vl`
    /// and updating the `vl`/`vtype` CSRs. An illegal `vtype` sets `vill` and
    /// zeroes `vl`.
    pub(super) fn set_vtype(&mut self, vtype: u64, avl: Avl) -> u64 {
        // Every successfully executed vector configuration instruction resets
        // vstart, including one that records an unsupported vtype through vill.
        self.vstart = 0;

        let vsew = (vtype >> 3) & 0x7;
        let vlmul = vtype & 0x7;
        // Bits above [7:0] (vma/vta/vsew/vlmul) are reserved; vlmul=4 reserved;
        // SEW must be <= ELEN (64).
        let mut vill = (vtype >> 8) != 0 || vlmul == 4 || vsew > 3;
        let sew = 8u64 << vsew;
        let vlmax = if vill {
            0
        } else {
            match vlmul {
                0 => VLEN / sew,
                1 => VLEN * 2 / sew,
                2 => VLEN * 4 / sew,
                3 => VLEN * 8 / sew,
                5 => VLEN / 8 / sew,
                6 => VLEN / 4 / sew,
                7 => VLEN / 2 / sew,
                _ => 0,
            }
        };
        if vlmax == 0 {
            vill = true;
        }
        if vill {
            self.vtype = 1u64 << (self.xbits() - 1);
            self.vl = 0;
            return 0;
        }
        let avl = match avl {
            Avl::Keep => self.vl,
            Avl::Max => vlmax,
            Avl::Reg(v) => v,
        };
        let vl = avl.min(vlmax);
        self.vtype = vtype;
        self.vl = vl;
        vl
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::riscv::{FlatMemory, Isa, RiscVConfig, RiscVExit, Xlen, decode};

    fn cpu() -> RiscVCpu {
        RiscVCpu::new(RiscVConfig::rv64gc(), Box::new(FlatMemory::new(0, 0x1000)))
    }

    fn execute(cpu: &mut RiscVCpu, raw: u32) {
        let insn = decode(raw, Xlen::Rv64, &Isa::rv64gc());
        assert_eq!(cpu.execute_insn(&insn, 0x1000), Ok(RiscVExit::Continue));
        assert_eq!(cpu.vstart(), 0, "{insn:?} did not reset vstart");
    }

    #[test]
    fn every_vector_configuration_form_resets_vstart() {
        let vsetvli = (7 << 12) | (1 << 7) | 0x57;
        let vsetivli = (0b11 << 30) | ((3u32 << 3) << 20) | (3 << 15) | (7 << 12) | (6 << 7) | 0x57;
        let vsetvl = (1 << 31) | (2 << 20) | (3 << 15) | (7 << 12) | (4 << 7) | 0x57;

        for raw in [vsetvli, vsetivli, vsetvl] {
            let mut cpu = cpu();
            cpu.set_x(2, 0); // valid e8,m1 vtype for vsetvl
            cpu.set_x(3, 4);
            cpu.set_vstart(7);
            execute(&mut cpu, raw);
        }
    }

    #[test]
    fn unsupported_vtype_still_resets_vstart_when_vill_is_recorded() {
        let mut cpu = cpu();
        cpu.set_x(2, 1u64 << 63);
        cpu.set_x(3, 4);
        cpu.set_vstart(5);
        let vsetvl = (1 << 31) | (2 << 20) | (3 << 15) | (7 << 12) | (4 << 7) | 0x57;

        execute(&mut cpu, vsetvl);

        assert_eq!(cpu.vl(), 0);
        assert_eq!(cpu.vtype(), 1u64 << 63);
    }
}
