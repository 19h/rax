//! Control and status register behavior.

use super::*;

impl RiscVCpu {
    pub(super) fn exec_csr(&mut self, insn: &Insn) -> Result<(), Trap> {
        let addr = insn.csr;
        let is_imm = matches!(insn.op, Op::Csrrwi | Op::Csrrsi | Op::Csrrci);
        let src = if is_imm {
            insn.rs1 as u64
        } else {
            self.x(insn.rs1)
        };
        let is_write = matches!(insn.op, Op::Csrrw | Op::Csrrwi);
        let writes = is_write || insn.rs1 != 0;

        if writes && Csr::is_read_only(addr) {
            return Err(Trap::illegal(insn.raw));
        }

        // CSRRW with rd=x0 suppresses the CSR read and any read side effects.
        let old = if is_write && insn.rd == 0 {
            0
        } else {
            self.csr_read(addr)?
        };

        if writes {
            let new = match insn.op {
                Op::Csrrw | Op::Csrrwi => src,
                Op::Csrrs | Op::Csrrsi => old | src,
                Op::Csrrc | Op::Csrrci => old & !src,
                _ => unreachable!(),
            };
            self.csr_write(addr, new)?;
        }
        self.set_x(insn.rd, old);
        Ok(())
    }

    /// Read a CSR value (XLEN-wide).
    pub fn csr_read(&self, addr: u16) -> Result<u64, Trap> {
        let csr = match Csr::from_addr(addr) {
            Some(c) => c,
            None => {
                if self.cfg.isa.xsoteria {
                    return Ok(self.ext_csr.get(&addr).copied().unwrap_or(0) & self.xmask());
                }
                return Err(Trap::illegal(0));
            }
        };
        if !self.csr_available(csr) {
            return Err(Trap::illegal(0));
        }
        let v = match csr {
            Csr::Fflags => (self.fcsr & 0x1f) as u64,
            Csr::Frm => ((self.fcsr >> 5) & 0x7) as u64,
            Csr::Fcsr => (self.fcsr & 0xff) as u64,
            Csr::Jvt => self.jvt,
            Csr::Cycle => self.cycle & self.xmask(),
            Csr::Time => self.time & self.xmask(),
            Csr::Instret => self.instret & self.xmask(),
            Csr::CycleH => (self.cycle >> 32) & 0xffff_ffff,
            Csr::TimeH => (self.time >> 32) & 0xffff_ffff,
            Csr::InstretH => (self.instret >> 32) & 0xffff_ffff,
            Csr::Mstatus => self.mstatus,
            Csr::Sstatus => self.mstatus & self.sstatus_mask(),
            Csr::Misa => self.misa(),
            Csr::Medeleg => self.medeleg,
            Csr::Mideleg => self.mideleg,
            Csr::Mie => self.mie,
            Csr::Sie => self.mie & self.supervisor_interrupt_mask(),
            Csr::Mtvec => self.mtvec,
            Csr::Mcounteren => self.mcounteren,
            Csr::Mscratch => self.mscratch,
            Csr::Mepc => self.mepc_read_value(),
            Csr::Mcause => self.mcause,
            Csr::Mtval => self.mtval,
            Csr::Mip => self.mip,
            Csr::Sip => self.mip & self.supervisor_interrupt_mask(),
            Csr::Mvendorid | Csr::Marchid | Csr::Mimpid => 0,
            Csr::Mhartid => self.mhartid,
            Csr::Vl => self.vl,
            Csr::Vtype => self.vtype,
            Csr::Vlenb => VLEN / 8,
            Csr::Vstart => self.vstart,
            Csr::Vxsat => self.vxsat,
            Csr::Vxrm => self.vxrm,
            Csr::Vcsr => (self.vxrm << 1) | self.vxsat,
        };
        Ok(v & self.xmask())
    }

    /// Write a CSR value (XLEN-wide).
    pub fn csr_write(&mut self, addr: u16, value: u64) -> Result<(), Trap> {
        let csr = match Csr::from_addr(addr) {
            Some(c) => c,
            None => {
                if self.cfg.isa.xsoteria {
                    self.ext_csr.insert(addr, value & self.xmask());
                    return Ok(());
                }
                return Err(Trap::illegal(0));
            }
        };
        if !self.csr_available(csr) {
            return Err(Trap::illegal(0));
        }
        match csr {
            Csr::Fflags => self.fcsr = (self.fcsr & !0x1f) | (value as u32 & 0x1f),
            Csr::Frm => self.fcsr = (self.fcsr & !0xe0) | (((value as u32) & 0x7) << 5),
            Csr::Fcsr => self.fcsr = value as u32 & 0xff,
            // Jump-table mode zero is the only currently defined/implemented
            // WARL mode; BASE is consequently always 64-byte aligned.
            Csr::Jvt => self.jvt = value & !0x3f & self.xmask(),
            Csr::Mstatus => self.mstatus = value,
            Csr::Sstatus => {
                let mask = self.sstatus_mask();
                self.mstatus = (self.mstatus & !mask) | (value & mask);
            }
            Csr::Medeleg => self.medeleg = value,
            Csr::Mideleg => self.mideleg = value,
            Csr::Mie => self.mie = value,
            Csr::Sie => {
                let mask = self.supervisor_interrupt_mask();
                self.mie = (self.mie & !mask) | (value & mask);
            }
            Csr::Mtvec => {
                let base = value & !0b11 & self.xmask();
                let mode = u64::from(value & 0b11 == 1);
                self.mtvec = base | mode;
            }
            Csr::Mcounteren => self.mcounteren = value,
            Csr::Mscratch => self.mscratch = value,
            Csr::Mepc => self.mepc = value & self.mepc_alignment_mask() & self.xmask(),
            Csr::Mcause => self.mcause = value,
            Csr::Mtval => self.mtval = value,
            Csr::Mip => self.mip = value,
            Csr::Sip => {
                // norm:sip: SSIP (bit 1) is writable by software regardless
                // of mideleg; mideleg only controls interrupt delegation.
                let mask = (1 << 1) & self.xmask();
                self.mip = (self.mip & !mask) | (value & mask);
            }
            Csr::Vstart => self.vstart = value,
            Csr::Vxsat => self.vxsat = value & 1,
            Csr::Vxrm => self.vxrm = value & 3,
            Csr::Vcsr => {
                self.vxsat = value & 1;
                self.vxrm = (value >> 1) & 3;
            }
            _ => {}
        }
        Ok(())
    }

    /// Whether a recognized CSR exists in the configured architectural
    /// profile. Zicsr controls the CSR instructions themselves; this gate
    /// covers the XLEN- and extension-dependent register families.
    fn csr_available(&self, csr: Csr) -> bool {
        match csr {
            Csr::Fflags | Csr::Frm | Csr::Fcsr => self.cfg.isa.f,
            Csr::Jvt => self.cfg.isa.zcmt,
            Csr::CycleH | Csr::TimeH | Csr::InstretH => self.rv32(),
            Csr::Vstart
            | Csr::Vxsat
            | Csr::Vxrm
            | Csr::Vcsr
            | Csr::Vl
            | Csr::Vtype
            | Csr::Vlenb => self.cfg.isa.v,
            _ => true,
        }
    }

    fn sstatus_mask(&self) -> u64 {
        let sd = 1u64 << (self.xbits() - 1);
        let uxl = if self.rv32() { 0 } else { 0b11 << 32 };
        (SSTATUS_BASE_MASK | uxl | sd) & self.xmask()
    }

    fn supervisor_interrupt_mask(&self) -> u64 {
        self.mideleg & S_INTERRUPT_MASK & self.xmask()
    }

    fn supervisor_software_interrupt_mask(&self) -> u64 {
        self.mideleg & (1 << 1) & self.xmask()
    }

    fn misa(&self) -> u64 {
        let mxl: u64 = if self.rv32() { 1 } else { 2 };
        let shift = self.xbits() as u64 - 2;
        let mut bits: u64 = 1 << 8; // I
        let isa = &self.cfg.isa;
        for (enabled, bit) in [
            (isa.a, 0),
            (isa.c, 2),
            (isa.d, 3),
            (isa.f, 5),
            (isa.h, 7),
            (isa.m, 12),
            (isa.v, 21),
        ] {
            if enabled {
                bits |= 1 << bit;
            }
        }
        (mxl << shift) | bits
    }

    #[inline]
    fn mepc_alignment_mask(&self) -> u64 {
        if self.cfg.isa.c { !1 } else { !3 }
    }

    #[inline]
    fn mepc_read_value(&self) -> u64 {
        self.mepc & self.mepc_alignment_mask() & self.xmask()
    }

    pub(super) fn mret(&mut self) {
        // pc <- mepc; MIE <- MPIE; MPIE <- 1; priv <- MPP; MPP <- U.
        self.pc = self.mepc_read_value();
        let mpie = (self.mstatus >> 7) & 1;
        self.mstatus &= !(1 << 3);
        self.mstatus |= mpie << 3;
        self.mstatus |= 1 << 7;
        let mpp = (self.mstatus >> 11) & 0b11;
        self.priv_ = match mpp {
            3 => Priv::Machine,
            1 => Priv::Supervisor,
            _ => Priv::User,
        };
        self.mstatus &= !(0b11 << 11);
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::isa::riscv::FlatMemory;

    fn cpu_with_xlen(xlen: Xlen, isa: Isa) -> RiscVCpu {
        RiscVCpu::new(
            RiscVConfig { xlen, isa },
            Box::new(FlatMemory::new(0, 0x2000)),
        )
    }

    fn cpu(isa: Isa) -> RiscVCpu {
        cpu_with_xlen(Xlen::Rv64, isa)
    }

    #[test]
    fn mepc_masks_the_configured_ialign_on_reads_and_mret() {
        let mut no_c = cpu(Isa::rv_i());
        no_c.csr_write(0x341, 0x1003).unwrap();
        assert_eq!(no_c.csr_read(0x341), Ok(0x1000));
        no_c.mret();
        assert_eq!(no_c.pc(), 0x1000);

        let mut with_c = cpu(Isa {
            c: true,
            ..Isa::rv_i()
        });
        with_c.csr_write(0x341, 0x1003).unwrap();
        assert_eq!(with_c.csr_read(0x341), Ok(0x1002));
        with_c.mret();
        assert_eq!(with_c.pc(), 0x1002);
    }

    #[test]
    fn misa_reports_enabled_h_and_v_independently() {
        for (h, v) in [(false, false), (true, false), (false, true), (true, true)] {
            let cpu = cpu(Isa {
                h,
                v,
                ..Isa::rv_i()
            });
            let misa = cpu.csr_read(0x301).unwrap();
            assert_eq!((misa >> 7) & 1, h as u64, "H projection");
            assert_eq!((misa >> 21) & 1, v as u64, "V projection");
        }
    }

    #[test]
    fn mtvec_warl_readback_never_exposes_a_reserved_mode() {
        let mut cpu = cpu(Isa::rv_i());
        for mode in 0..=3 {
            cpu.csr_write(0x305, 0x1000 | mode).unwrap();
            let expected_mode = u64::from(mode == 1);
            assert_eq!(
                cpu.csr_read(0x305),
                Ok(0x1000 | expected_mode),
                "MODE={mode}"
            );
        }
    }

    #[test]
    fn sip_ssip_writable_without_mideleg() {
        let mut cpu = cpu(Isa::rv_i());
        // mideleg=0: SSIP is still software-writable through sip.
        cpu.csr_write(0x303, 0).unwrap();
        cpu.csr_write(0x144, 1 << 1).unwrap();
        assert_eq!(
            cpu.csr_read(0x344).unwrap() & (1 << 1),
            1 << 1,
            "sip SSIP write must set mip.SSIP even when mideleg=0"
        );
        // Clearing through sip also works.
        cpu.csr_write(0x144, 0).unwrap();
        assert_eq!(cpu.csr_read(0x344).unwrap() & (1 << 1), 0);
        // Non-SSIP bits of sip are not stored.
        cpu.csr_write(0x144, 1 << 5).unwrap();
        assert_eq!(cpu.csr_read(0x344).unwrap() & (1 << 5), 0);
    }

    #[test]
    fn csr_availability_tracks_xlen_and_declaring_extensions() {
        let mut unavailable = cpu(Isa::rv_i());
        for addr in [
            0x001, 0x002, 0x003, // F
            0x017, // Zcmt
            0x008, 0x009, 0x00A, 0x00F, 0xC20, 0xC21, 0xC22, // V
            0xC80, 0xC81, 0xC82, // RV32-only counter high halves
        ] {
            assert_eq!(
                unavailable.csr_read(addr),
                Err(Trap::illegal(0)),
                "read {addr:#05x}"
            );
            assert_eq!(
                unavailable.csr_write(addr, u64::MAX),
                Err(Trap::illegal(0)),
                "write {addr:#05x}"
            );
        }

        let mut available = cpu(Isa {
            f: true,
            v: true,
            zcmt: true,
            ..Isa::rv_i()
        });
        for addr in [0x001, 0x002, 0x003, 0x017, 0x008, 0x009, 0x00A, 0x00F] {
            assert!(available.csr_write(addr, 0).is_ok(), "write {addr:#05x}");
        }
        for addr in [
            0x001, 0x002, 0x003, 0x017, 0x008, 0x009, 0x00A, 0x00F, 0xC20, 0xC21, 0xC22,
        ] {
            assert!(available.csr_read(addr).is_ok(), "read {addr:#05x}");
        }

        let rv32 = cpu_with_xlen(Xlen::Rv32, Isa::rv_i());
        for addr in [0xC80, 0xC81, 0xC82] {
            assert!(rv32.csr_read(addr).is_ok(), "RV32 read {addr:#05x}");
        }
    }
}
