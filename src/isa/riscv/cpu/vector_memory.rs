//! RVV memory operations and precise `vstart` publication.

use super::{Insn, Op, RiscVCpu, Trap, VLENB, acc_fault};

#[inline]
fn encoded_width(insn: &Insn) -> Result<usize, Trap> {
    match insn.funct3 {
        0 => Ok(1),
        5 => Ok(2),
        6 => Ok(4),
        7 => Ok(8),
        _ => Err(Trap::illegal(insn.raw)),
    }
}

impl RiscVCpu {
    fn vector_read(&mut self, element: usize, addr: u64, buf: &mut [u8]) -> Result<(), Trap> {
        if self.mem.read(addr, buf).is_err() {
            self.vstart = element as u64;
            return Err(acc_fault(false, addr));
        }
        Ok(())
    }

    fn vector_write(&mut self, element: usize, addr: u64, data: &[u8]) -> Result<(), Trap> {
        if self.mem.write(addr, data).is_err() {
            self.vstart = element as u64;
            return Err(acc_fault(true, addr));
        }
        Ok(())
    }

    pub(super) fn exec_vector_memory(
        &mut self,
        insn: &Insn,
        vm: bool,
        vd: u8,
        vstart: usize,
        vl: usize,
    ) -> Result<(), Trap> {
        match insn.op {
            Op::Vle | Op::Vse => {
                let eb = encoded_width(insn)?;
                let base = self.x(insn.rs1) & self.xmask();
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let addr = base.wrapping_add((e * eb) as u64) & self.xmask();
                    if insn.op == Op::Vle {
                        let mut buf = [0u8; 8];
                        self.vector_read(e, addr, &mut buf[..eb])?;
                        self.set_velem(vd, e, eb, u64::from_le_bytes(buf));
                    } else {
                        let val = self.velem(vd, e, eb);
                        self.vector_write(e, addr, &val.to_le_bytes()[..eb])?;
                    }
                }
            }
            Op::Vlse | Op::Vsse => {
                let eb = encoded_width(insn)?;
                let base = self.x(insn.rs1) & self.xmask();
                let stride = self.x(insn.rs2) as i64;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let addr =
                        base.wrapping_add((e as i64).wrapping_mul(stride) as u64) & self.xmask();
                    if insn.op == Op::Vlse {
                        let mut buf = [0u8; 8];
                        self.vector_read(e, addr, &mut buf[..eb])?;
                        self.set_velem(vd, e, eb, u64::from_le_bytes(buf));
                    } else {
                        let val = self.velem(vd, e, eb);
                        self.vector_write(e, addr, &val.to_le_bytes()[..eb])?;
                    }
                }
            }
            Op::Vlxei | Op::Vsxei => {
                let ieb = encoded_width(insn)?;
                let eb = self.sew_bytes();
                let base = self.x(insn.rs1) & self.xmask();
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let addr = base.wrapping_add(self.velem(insn.rs2, e, ieb)) & self.xmask();
                    if insn.op == Op::Vlxei {
                        let mut buf = [0u8; 8];
                        self.vector_read(e, addr, &mut buf[..eb])?;
                        self.set_velem(vd, e, eb, u64::from_le_bytes(buf));
                    } else {
                        let val = self.velem(vd, e, eb);
                        self.vector_write(e, addr, &val.to_le_bytes()[..eb])?;
                    }
                }
            }
            Op::Vleff => {
                let eb = encoded_width(insn)?;
                let base = self.x(insn.rs1) & self.xmask();
                let mut new_vl = vl;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let addr = base.wrapping_add((e * eb) as u64) & self.xmask();
                    let mut buf = [0u8; 8];
                    match self.mem.read(addr, &mut buf[..eb]) {
                        Ok(()) => self.set_velem(vd, e, eb, u64::from_le_bytes(buf)),
                        Err(_) if e == 0 => {
                            self.vstart = 0;
                            return Err(acc_fault(false, addr));
                        }
                        Err(_) => {
                            new_vl = e;
                            break;
                        }
                    }
                }
                self.vl = new_vl as u64;
            }
            Op::Vlseg | Op::Vsseg => {
                let nf = ((insn.raw >> 29) & 7) as usize + 1;
                let mop = (insn.raw >> 26) & 3;
                let lumop = (insn.raw >> 20) & 0x1f;
                let is_load = insn.op == Op::Vlseg;
                let is_ff = is_load && lumop == 0b10000;
                let width = encoded_width(insn)?;
                let indexed = mop == 0b01 || mop == 0b11;
                let eb = if indexed { self.sew_bytes() } else { width };

                let sew_bits = 8u32 << ((self.vtype >> 3) & 0x7);
                let eew_bits = if indexed {
                    sew_bits
                } else {
                    (width as u32) * 8
                };
                let (lmul_n, lmul_d): (u32, u32) = match self.vtype & 0x7 {
                    0 => (1, 1),
                    1 => (2, 1),
                    2 => (4, 1),
                    3 => (8, 1),
                    5 => (1, 8),
                    6 => (1, 4),
                    7 => (1, 2),
                    _ => (1, 1),
                };
                let emul_regs = ((eew_bits * lmul_n) / (sew_bits * lmul_d)).max(1) as usize;
                if emul_regs > 8 || nf * emul_regs > 8 || vd as usize + nf * emul_regs > 32 {
                    return Err(Trap::illegal(insn.raw));
                }

                let base = self.x(insn.rs1) & self.xmask();
                let stride = self.x(insn.rs2) as i64;
                let mut new_vl = vl;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let elem_base = match mop {
                        0b00 => base.wrapping_add((e * nf * eb) as u64),
                        0b10 => base.wrapping_add((e as i64).wrapping_mul(stride) as u64),
                        _ => base.wrapping_add(self.velem(insn.rs2, e, width)),
                    } & self.xmask();
                    for f in 0..nf {
                        let addr = elem_base.wrapping_add((f * eb) as u64) & self.xmask();
                        let reg = (vd as usize + f * emul_regs) as u8;
                        if is_load {
                            let mut buf = [0u8; 8];
                            match self.mem.read(addr, &mut buf[..eb]) {
                                Ok(()) => self.set_velem(reg, e, eb, u64::from_le_bytes(buf)),
                                Err(_) if is_ff && e > 0 => {
                                    new_vl = e;
                                    break;
                                }
                                Err(_) => {
                                    self.vstart = e as u64;
                                    return Err(acc_fault(false, addr));
                                }
                            }
                        } else {
                            let val = self.velem(reg, e, eb);
                            self.vector_write(e, addr, &val.to_le_bytes()[..eb])?;
                        }
                    }
                }
                if is_ff {
                    self.vl = new_vl as u64;
                    self.vstart = 0;
                }
            }
            Op::Vlm | Op::Vsm => {
                // Mask transfers use byte-sized elements and vstart is a byte
                // index into their effective length ceil(vl/8).
                if !vm {
                    return Err(Trap::illegal(insn.raw));
                }
                let base = self.x(insn.rs1) & self.xmask();
                let evl = vl.div_ceil(8);
                for e in vstart..evl {
                    let addr = base.wrapping_add(e as u64) & self.xmask();
                    if insn.op == Op::Vlm {
                        let mut buf = [0u8; 1];
                        self.vector_read(e, addr, &mut buf)?;
                        self.set_velem(vd, e, 1, u64::from(buf[0]));
                    } else {
                        let val = self.velem(vd, e, 1);
                        self.vector_write(e, addr, &[val as u8])?;
                    }
                }
            }
            Op::Vlre | Op::Vsre => {
                // Whole-register transfers ignore vtype/vl. vstart indexes
                // encoded-EEW elements in the independent effective length.
                let eb = encoded_width(insn)?;
                let nreg = ((insn.raw >> 29) & 7) as usize + 1;
                if !vm
                    || !matches!(nreg, 1 | 2 | 4 | 8)
                    || vd as usize % nreg != 0
                    || vd as usize + nreg > 32
                {
                    return Err(Trap::illegal(insn.raw));
                }
                let base = self.x(insn.rs1) & self.xmask();
                let evl = nreg * VLENB as usize / eb;
                for e in vstart..evl {
                    let addr = base.wrapping_add((e * eb) as u64) & self.xmask();
                    if insn.op == Op::Vlre {
                        let mut buf = [0u8; 8];
                        self.vector_read(e, addr, &mut buf[..eb])?;
                        self.set_velem(vd, e, eb, u64::from_le_bytes(buf));
                    } else {
                        let val = self.velem(vd, e, eb);
                        self.vector_write(e, addr, &val.to_le_bytes()[..eb])?;
                    }
                }
            }
            _ => return Err(Trap::illegal(insn.raw)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::riscv::{FlatMemory, Isa, RiscVConfig, RiscVExit, Xlen, decode};

    fn cpu(memory: FlatMemory, vl: u64, vtype: u64) -> RiscVCpu {
        let mut cpu = RiscVCpu::new(RiscVConfig::rv64gc(), Box::new(memory));
        cpu.set_vl_vtype(vl, vtype);
        cpu
    }

    fn execute(cpu: &mut RiscVCpu, raw: u32) -> Result<RiscVExit, Trap> {
        let insn = decode(raw, Xlen::Rv64, &Isa::rv64gc());
        assert_ne!(insn.op, Op::Illegal, "test encoding {raw:08x} is illegal");
        cpu.execute_insn(&insn, 0x1000)
    }

    fn load(vm: u32, nf: u32, lumop: u32, rs1: u32, width: u32, vd: u32) -> u32 {
        (nf << 29) | (vm << 25) | (lumop << 20) | (rs1 << 15) | (width << 12) | (vd << 7) | 0x07
    }

    fn store(vm: u32, nf: u32, sumop: u32, rs1: u32, width: u32, vs3: u32) -> u32 {
        (nf << 29) | (vm << 25) | (sumop << 20) | (rs1 << 15) | (width << 12) | (vs3 << 7) | 0x27
    }

    #[test]
    fn mask_load_and_store_use_byte_indexed_vstart() {
        let mut load_cpu = cpu(FlatMemory::with_data(0x100, vec![0x11, 0x22, 0x33]), 24, 0);
        load_cpu.set_x(10, 0x100);
        load_cpu.set_vreg(1, &[0xaa; 16]);
        load_cpu.set_vstart(1);
        assert_eq!(
            execute(&mut load_cpu, load(1, 0, 0b01011, 10, 0, 1)),
            Ok(RiscVExit::Continue)
        );
        assert_eq!(&load_cpu.vreg(1)[..3], &[0xaa, 0x22, 0x33]);
        assert_eq!(load_cpu.vstart(), 0);

        let mut store_cpu = cpu(FlatMemory::with_data(0x100, vec![0xee; 3]), 24, 0);
        store_cpu.set_x(10, 0x100);
        let mut source = [0u8; 16];
        source[..3].copy_from_slice(&[0x11, 0x22, 0x33]);
        store_cpu.set_vreg(1, &source);
        store_cpu.set_vstart(1);
        assert_eq!(
            execute(&mut store_cpu, store(1, 0, 0b01011, 10, 0, 1)),
            Ok(RiscVExit::Continue)
        );
        let mut stored = [0u8; 3];
        store_cpu.read_memory(0x100, &mut stored).unwrap();
        assert_eq!(stored, [0xee, 0x22, 0x33]);
        assert_eq!(store_cpu.vstart(), 0);
    }

    #[test]
    fn whole_register_transfer_uses_encoded_eew_and_ignores_vill() {
        let memory: Vec<u8> = (0..32).collect();
        let mut load_cpu = cpu(FlatMemory::with_data(0x100, memory.clone()), 0, 1u64 << 63);
        load_cpu.set_x(10, 0x100);
        load_cpu.set_vreg(2, &[0xaa; 16]);
        load_cpu.set_vreg(3, &[0xaa; 16]);
        load_cpu.set_vstart(2); // two e32 elements = eight bytes

        assert_eq!(
            execute(&mut load_cpu, load(1, 1, 0b01000, 10, 6, 2)),
            Ok(RiscVExit::Continue)
        );

        let mut actual = [0u8; 32];
        actual[..16].copy_from_slice(&load_cpu.vreg(2));
        actual[16..].copy_from_slice(&load_cpu.vreg(3));
        assert_eq!(&actual[..8], &[0xaa; 8]);
        assert_eq!(&actual[8..], &memory[8..]);
        assert_eq!(load_cpu.vstart(), 0);

        let mut store_cpu = cpu(FlatMemory::with_data(0x100, vec![0xee; 32]), 0, 1u64 << 63);
        store_cpu.set_x(10, 0x100);
        let source2: [u8; 16] = memory[..16].try_into().unwrap();
        let source3: [u8; 16] = memory[16..].try_into().unwrap();
        store_cpu.set_vreg(2, &source2);
        store_cpu.set_vreg(3, &source3);
        store_cpu.set_vstart(2);
        assert_eq!(
            execute(&mut store_cpu, store(1, 1, 0b01000, 10, 6, 2)),
            Ok(RiscVExit::Continue)
        );
        let mut stored = [0u8; 32];
        store_cpu.read_memory(0x100, &mut stored).unwrap();
        assert_eq!(&stored[..8], &[0xee; 8]);
        assert_eq!(&stored[8..], &memory[8..]);
        assert_eq!(store_cpu.vstart(), 0);
    }

    #[test]
    fn mask_transfer_respects_vill_and_whole_register_groups_are_validated() {
        let mut vill_cpu = cpu(FlatMemory::new(0x100, 64), 8, 1u64 << 63);
        vill_cpu.set_x(10, 0x100);
        vill_cpu.set_vstart(3);
        let raw = load(1, 0, 0b01011, 10, 0, 1);
        assert_eq!(execute(&mut vill_cpu, raw), Err(Trap::illegal(raw)));
        assert_eq!(vill_cpu.vstart(), 3);

        let mut group_cpu = cpu(FlatMemory::new(0x100, 64), 0, 0);
        group_cpu.set_x(10, 0x100);
        let misaligned = load(1, 1, 0b01000, 10, 0, 3);
        assert_eq!(
            execute(&mut group_cpu, misaligned),
            Err(Trap::illegal(misaligned))
        );
    }

    #[test]
    fn vector_memory_faults_publish_the_faulting_element() {
        let mut cpu = cpu(
            FlatMemory::with_data(0x100, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
            4,
            0x10,
        );
        cpu.set_x(10, 0x100);
        cpu.set_vreg(1, &[0xaa; 16]);

        let trap = execute(&mut cpu, load(1, 0, 0, 10, 6, 1)).unwrap_err();

        assert_eq!(trap, acc_fault(false, 0x108));
        assert_eq!(cpu.vstart(), 2);
        assert_eq!(&cpu.vreg(1)[..8], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&cpu.vreg(1)[8..], &[0xaa; 8]);
    }

    #[test]
    fn mask_and_whole_register_fault_indices_use_their_effective_elements() {
        let mut mask_cpu = cpu(FlatMemory::with_data(0x100, vec![0; 2]), 24, 0);
        mask_cpu.set_x(10, 0x100);
        let mask_trap = execute(&mut mask_cpu, load(1, 0, 0b01011, 10, 0, 1)).unwrap_err();
        assert_eq!(mask_trap, acc_fault(false, 0x102));
        assert_eq!(mask_cpu.vstart(), 2);

        let mut whole_cpu = cpu(FlatMemory::with_data(0x100, vec![0; 10]), 0, 0);
        whole_cpu.set_x(10, 0x100);
        let whole_trap = execute(&mut whole_cpu, load(1, 0, 0b01000, 10, 6, 2)).unwrap_err();
        assert_eq!(whole_trap, acc_fault(false, 0x108));
        assert_eq!(whole_cpu.vstart(), 2);
    }

    #[test]
    fn vlsegff_is_fault_only_first() {
        // vlseg<nf>e<eew>ff (lumop=0b10000, nf!=0) is the segment
        // fault-only-first form; it must decode to Vlseg (not Illegal)
        // and stop at the first faulting element with vl trimmed.
        // 16 bytes of memory at 0x100; SEW=8, LMUL=1, e16 -> EMUL=2.
        // vlseg2e16ff.v v2, (x10): 2 fields x 2 regs, vl=8 elements
        // needs 32 bytes; memory holds 16 -> element 4 faults.
        let mut c = cpu(FlatMemory::with_data(0x100, vec![0u8; 16]), 8, 0);
        c.set_x(10, 0x100);
        let raw = load(1, 1, 0b10000, 10, 5, 2); // vlseg2e16ff.v v2
        let insn = decode(raw, Xlen::Rv64, &Isa::rv64gc());
        assert_eq!(insn.op, Op::Vlseg, "vlsegff must decode as Vlseg");
        assert_eq!(
            execute(&mut c, raw),
            Ok(RiscVExit::Continue),
            "fault-only-first load must not trap on late fault"
        );
        assert!(c.vl() < 8, "vl must be trimmed to the faulting element");
        assert_eq!(c.vstart(), 0, "fault-only-first resets vstart");
    }
}
