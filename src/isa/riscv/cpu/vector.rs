//! Vector (RVV) element access and data-path execution.
//!
//! Split out of cpu.rs so the touched vector semantic group does not
//! add to the oversized legacy file (see AGENTS.md §9.1).

use super::*;
impl RiscVCpu {


    // ---------------------------------------------------------------
    // V: vector element access and the data-path execution.
    // ---------------------------------------------------------------

    /// SEW (element width) in bytes from the current `vtype`.
    #[inline]
    fn sew_bytes(&self) -> usize {
        1usize << ((self.vtype >> 3) & 0x7)
    }
    /// VLMAX (maximum element count) for the current `vtype`.
    #[inline]
    fn vlmax_elems(&self) -> usize {
        let sew = 8u64 << ((self.vtype >> 3) & 0x7);
        (match self.vtype & 0x7 {
            0 => VLEN / sew,
            1 => VLEN * 2 / sew,
            2 => VLEN * 4 / sew,
            3 => VLEN * 8 / sew,
            5 => VLEN / 8 / sew,
            6 => VLEN / 4 / sew,
            7 => VLEN / 2 / sew,
            _ => 0,
        }) as usize
    }
    /// Read element `e` (of `eb` bytes) from vector register group `vreg`.
    #[inline]
    fn velem(&self, vreg: u8, e: usize, eb: usize) -> u64 {
        let off = vreg as usize * VLENB as usize + e * eb;
        let mut buf = [0u8; 8];
        if off + eb <= self.v.len() {
            buf[..eb].copy_from_slice(&self.v[off..off + eb]);
        }
        u64::from_le_bytes(buf)
    }
    #[inline]
    fn set_velem(&mut self, vreg: u8, e: usize, eb: usize, val: u64) {
        let off = vreg as usize * VLENB as usize + e * eb;
        if off + eb <= self.v.len() {
            self.v[off..off + eb].copy_from_slice(&val.to_le_bytes()[..eb]);
        }
    }
    /// Mask bit `e` of `v0`.
    #[inline]
    fn vmask_bit(&self, e: usize) -> bool {
        (self.v[e / 8] >> (e % 8)) & 1 != 0
    }
    /// Mask bit `e` of an arbitrary vector register `vreg`.
    #[inline]
    fn vbit(&self, vreg: u8, e: usize) -> bool {
        let byte = vreg as usize * VLENB as usize + e / 8;
        byte < self.v.len() && (self.v[byte] >> (e % 8)) & 1 != 0
    }
    /// Set/clear mask bit `e` of vector register `vreg`.
    #[inline]
    fn set_vmask_bit(&mut self, vreg: u8, e: usize, val: bool) {
        let byte = vreg as usize * VLENB as usize + e / 8;
        if byte < self.v.len() {
            if val {
                self.v[byte] |= 1 << (e % 8);
            } else {
                self.v[byte] &= !(1 << (e % 8));
            }
        }
    }
    #[inline]
    fn sew_mask(eb: usize) -> u64 {
        if eb >= 8 {
            u64::MAX
        } else {
            (1u64 << (eb * 8)) - 1
        }
    }

    /// Execute a vector data-path instruction. The tail/mask policy is
    /// undisturbed (only active body elements are written).
    pub(super) fn exec_vector(&mut self, insn: &Insn) -> Result<(), Trap> {
        // vill (vtype MSB) => any vector instruction is illegal.
        if self.vtype >> (self.xbits() - 1) & 1 != 0 {
            return Err(Trap::illegal(insn.raw));
        }
        let vm = (insn.raw >> 25) & 1 != 0; // 1 = unmasked
        let vd = insn.rd;
        let vs2 = insn.rs2;
        let vstart = self.vstart as usize;
        let vl = self.vl as usize;

        match insn.op {
            Op::Vle | Op::Vse => {
                // Effective element width from the load/store funct3 field.
                let eb = match insn.funct3 {
                    0 => 1,
                    5 => 2,
                    6 => 4,
                    7 => 8,
                    _ => return Err(Trap::illegal(insn.raw)),
                };
                let base = self.x(insn.rs1) & self.xmask();
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let addr = base.wrapping_add((e * eb) as u64) & self.xmask();
                    if insn.op == Op::Vle {
                        let mut buf = [0u8; 8];
                        self.mem
                            .read(addr, &mut buf[..eb])
                            .map_err(|_| acc_fault(false, addr))?;
                        self.set_velem(vd, e, eb, u64::from_le_bytes(buf));
                    } else {
                        let val = self.velem(vd, e, eb); // vd holds the store data (vs3)
                        self.mem
                            .write(addr, &val.to_le_bytes()[..eb])
                            .map_err(|_| acc_fault(true, addr))?;
                    }
                }
            }
            Op::Vlse | Op::Vsse => {
                // Strided load/store: addr = base + e * byte-stride.
                let eb = match insn.funct3 {
                    0 => 1,
                    5 => 2,
                    6 => 4,
                    7 => 8,
                    _ => return Err(Trap::illegal(insn.raw)),
                };
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
                        self.mem
                            .read(addr, &mut buf[..eb])
                            .map_err(|_| acc_fault(false, addr))?;
                        self.set_velem(vd, e, eb, u64::from_le_bytes(buf));
                    } else {
                        let val = self.velem(vd, e, eb);
                        self.mem
                            .write(addr, &val.to_le_bytes()[..eb])
                            .map_err(|_| acc_fault(true, addr))?;
                    }
                }
            }
            Op::Vlxei | Op::Vsxei => {
                // Indexed load/store: addr = base + index[e]; index EEW = funct3,
                // data EEW = SEW.
                let ieb = match insn.funct3 {
                    0 => 1,
                    5 => 2,
                    6 => 4,
                    7 => 8,
                    _ => return Err(Trap::illegal(insn.raw)),
                };
                let eb = self.sew_bytes();
                let base = self.x(insn.rs1) & self.xmask();
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let idx = self.velem(insn.rs2, e, ieb);
                    let addr = base.wrapping_add(idx) & self.xmask();
                    if insn.op == Op::Vlxei {
                        let mut buf = [0u8; 8];
                        self.mem
                            .read(addr, &mut buf[..eb])
                            .map_err(|_| acc_fault(false, addr))?;
                        self.set_velem(vd, e, eb, u64::from_le_bytes(buf));
                    } else {
                        let val = self.velem(vd, e, eb);
                        self.mem
                            .write(addr, &val.to_le_bytes()[..eb])
                            .map_err(|_| acc_fault(true, addr))?;
                    }
                }
            }
            Op::Vleff => {
                // Fault-only-first unit-stride load: a fault past element 0 trims
                // vl instead of trapping. (Non-faulting path mirrors Vle.)
                let eb = match insn.funct3 {
                    0 => 1,
                    5 => 2,
                    6 => 4,
                    7 => 8,
                    _ => return Err(Trap::illegal(insn.raw)),
                };
                let base = self.x(insn.rs1) & self.xmask();
                let mut new_vl = vl;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let addr = base.wrapping_add((e * eb) as u64) & self.xmask();
                    let mut buf = [0u8; 8];
                    match self.mem.read(addr, &mut buf[..eb]) {
                        Ok(_) => self.set_velem(vd, e, eb, u64::from_le_bytes(buf)),
                        Err(_) => {
                            if e == 0 {
                                return Err(acc_fault(false, addr));
                            }
                            new_vl = e; // trim and suppress the trap
                            break;
                        }
                    }
                }
                self.vl = new_vl as u64;
            }
            Op::Vlseg | Op::Vsseg => {
                // Segment load/store: nf+1 fields per element, de-interleaved into
                // consecutive registers vd..vd+nf. Addressing per mop.
                let nf = ((insn.raw >> 29) & 7) as usize + 1;
                let mop = (insn.raw >> 26) & 3;
                let is_load = insn.op == Op::Vlseg;
                let indexed = mop == 0b01 || mop == 0b11;
                let width = match insn.funct3 {
                    0 => 1,
                    5 => 2,
                    6 => 4,
                    7 => 8,
                    _ => return Err(Trap::illegal(insn.raw)),
                };
                // For indexed segments data EEW = SEW, index EEW = funct3 width.
                let eb = if indexed { self.sew_bytes() } else { width };
                // Each field is a register group of EMUL = data_EEW/SEW * LMUL
                // registers, so consecutive fields are EMUL registers apart (not
                // 1). Reject encodings whose group exceeds 8 registers per field,
                // whose NFIELDS*EMUL > 8, or whose group would run past v31.
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
                            self.mem
                                .read(addr, &mut buf[..eb])
                                .map_err(|_| acc_fault(false, addr))?;
                            self.set_velem(reg, e, eb, u64::from_le_bytes(buf));
                        } else {
                            let val = self.velem(reg, e, eb);
                            self.mem
                                .write(addr, &val.to_le_bytes()[..eb])
                                .map_err(|_| acc_fault(true, addr))?;
                        }
                    }
                }
            }
            Op::Vlm | Op::Vsm => {
                // Mask load/store: ceil(vl/8) bytes, EEW=8, always unmasked.
                let base = self.x(insn.rs1) & self.xmask();
                let nbytes = vl.div_ceil(8);
                for i in 0..nbytes {
                    let addr = base.wrapping_add(i as u64) & self.xmask();
                    if insn.op == Op::Vlm {
                        let mut buf = [0u8; 1];
                        self.mem
                            .read(addr, &mut buf)
                            .map_err(|_| acc_fault(false, addr))?;
                        self.set_velem(vd, i, 1, buf[0] as u64);
                    } else {
                        let val = self.velem(vd, i, 1);
                        self.mem
                            .write(addr, &[val as u8])
                            .map_err(|_| acc_fault(true, addr))?;
                    }
                }
            }
            Op::Vlre | Op::Vsre => {
                // Whole-register load/store: (nf+1) * VLENB raw bytes, unmasked.
                let nreg = ((insn.raw >> 29) & 7) as usize + 1;
                let base = self.x(insn.rs1) & self.xmask();
                let total = nreg * VLENB as usize;
                for i in 0..total {
                    let addr = base.wrapping_add(i as u64) & self.xmask();
                    if insn.op == Op::Vlre {
                        let mut buf = [0u8; 1];
                        self.mem
                            .read(addr, &mut buf)
                            .map_err(|_| acc_fault(false, addr))?;
                        self.set_velem(vd, i, 1, buf[0] as u64);
                    } else {
                        let val = self.velem(vd, i, 1);
                        self.mem
                            .write(addr, &[val as u8])
                            .map_err(|_| acc_fault(true, addr))?;
                    }
                }
            }
            Op::Vmerge => {
                // vmerge.v*m (vm=0): per-element select via v0; vmv.v.* (vm=1):
                // splat the second operand. Both write every body element.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let scalar = match insn.funct3 {
                    0b100 => self.x(insn.rs1) & mask,
                    0b011 => sext5(insn.rs1) & mask,
                    _ => 0,
                };
                for e in vstart..vl {
                    let b = if insn.funct3 == 0b000 {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let r = if vm || self.vmask_bit(e) {
                        b
                    } else {
                        self.velem(vs2, e, eb)
                    };
                    self.set_velem(vd, e, eb, r & mask);
                }
            }
            Op::Vadd
            | Op::Vsub
            | Op::Vrsub
            | Op::Vand
            | Op::Vor
            | Op::Vxor
            | Op::Vminu
            | Op::Vmin
            | Op::Vmaxu
            | Op::Vmax
            | Op::Vsll
            | Op::Vsrl
            | Op::Vsra => {
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let bits = (eb * 8) as u32;
                // Operand form: OPIVV(0) uses vs1, OPIVX(4) a scalar, OPIVI(3) imm.
                let scalar = match insn.funct3 {
                    0b100 => self.x(insn.rs1) & mask,
                    0b011 => sext5(insn.rs1) & mask,
                    _ => 0,
                };
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let a = self.velem(vs2, e, eb);
                    let b = if insn.funct3 == 0b000 {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let sa = sext_sew(a, eb);
                    let sb = sext_sew(b, eb);
                    // Shift amount: OPIVI uses the unsigned 5-bit field, else the
                    // low bits of the operand.
                    let sh = if insn.funct3 == 0b011 {
                        insn.rs1 as u32 & (bits - 1)
                    } else {
                        (b as u32) & (bits - 1)
                    };
                    let r = match insn.op {
                        Op::Vadd => a.wrapping_add(b),
                        Op::Vsub => a.wrapping_sub(b),
                        Op::Vrsub => b.wrapping_sub(a),
                        Op::Vand => a & b,
                        Op::Vor => a | b,
                        Op::Vxor => a ^ b,
                        Op::Vminu => a.min(b),
                        Op::Vmaxu => a.max(b),
                        Op::Vmin => {
                            if sa <= sb {
                                a
                            } else {
                                b
                            }
                        }
                        Op::Vmax => {
                            if sa >= sb {
                                a
                            } else {
                                b
                            }
                        }
                        Op::Vsll => a << sh,
                        Op::Vsrl => (a & mask) >> sh,
                        Op::Vsra => (sa >> sh) as u64,
                        _ => unreachable!(),
                    };
                    self.set_velem(vd, e, eb, r & mask);
                }
            }
            Op::Vmul
            | Op::Vmulh
            | Op::Vmulhu
            | Op::Vmulhsu
            | Op::Vdivu
            | Op::Vdiv
            | Op::Vremu
            | Op::Vrem => {
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let bits = (eb * 8) as u32;
                let is_vv = insn.funct3 == 0b010; // OPMVV vs OPMVX
                let scalar = self.x(insn.rs1) & mask;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let a = self.velem(vs2, e, eb);
                    let b = if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let r = match insn.op {
                        Op::Vmul => a.wrapping_mul(b),
                        Op::Vmulhu => vmulh_u(a, b, bits),
                        Op::Vmulh => vmulh_s(a, b, eb, bits),
                        Op::Vmulhsu => vmulh_su(a, b, eb, bits),
                        Op::Vdivu => {
                            if b == 0 {
                                mask
                            } else {
                                a / b
                            }
                        }
                        Op::Vremu => {
                            if b == 0 {
                                a
                            } else {
                                a % b
                            }
                        }
                        Op::Vdiv => vdiv_sew(a, b, eb, bits, false),
                        Op::Vrem => vdiv_sew(a, b, eb, bits, true),
                        _ => unreachable!(),
                    };
                    self.set_velem(vd, e, eb, r & mask);
                }
            }
            Op::Vredsum
            | Op::Vredand
            | Op::Vredor
            | Op::Vredxor
            | Op::Vredminu
            | Op::Vredmin
            | Op::Vredmaxu
            | Op::Vredmax => {
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                // Accumulator seeds from vs1[0]; fold in active vs2 elements.
                let mut acc = self.velem(insn.rs1, 0, eb);
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let x = self.velem(vs2, e, eb);
                    acc = match insn.op {
                        Op::Vredsum => acc.wrapping_add(x),
                        Op::Vredand => acc & x,
                        Op::Vredor => acc | x,
                        Op::Vredxor => acc ^ x,
                        Op::Vredminu => acc.min(x),
                        Op::Vredmaxu => acc.max(x),
                        Op::Vredmin => {
                            if sext_sew(x, eb) < sext_sew(acc, eb) {
                                x
                            } else {
                                acc
                            }
                        }
                        Op::Vredmax => {
                            if sext_sew(x, eb) > sext_sew(acc, eb) {
                                x
                            } else {
                                acc
                            }
                        }
                        _ => unreachable!(),
                    } & mask;
                }
                // vl == 0 leaves vd[0] undisturbed; otherwise write the scalar result.
                if vl > vstart {
                    self.set_velem(vd, 0, eb, acc & mask);
                }
            }
            Op::Vmseq
            | Op::Vmsne
            | Op::Vmsltu
            | Op::Vmslt
            | Op::Vmsleu
            | Op::Vmsle
            | Op::Vmsgtu
            | Op::Vmsgt => {
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let scalar = match insn.funct3 {
                    0b100 => self.x(insn.rs1) & mask,
                    0b011 => sext5(insn.rs1) & mask,
                    _ => 0,
                };
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue; // masked-off: undisturbed
                    }
                    let a = self.velem(vs2, e, eb);
                    let b = if insn.funct3 == 0b000 {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let (sa, sb) = (sext_sew(a, eb), sext_sew(b, eb));
                    let r = match insn.op {
                        Op::Vmseq => a == b,
                        Op::Vmsne => a != b,
                        Op::Vmsltu => a < b,
                        Op::Vmslt => sa < sb,
                        Op::Vmsleu => a <= b,
                        Op::Vmsle => sa <= sb,
                        Op::Vmsgtu => a > b,
                        Op::Vmsgt => sa > sb,
                        _ => unreachable!(),
                    };
                    self.set_vmask_bit(vd, e, r);
                }
            }
            Op::Vfadd
            | Op::Vfsub
            | Op::Vfrsub
            | Op::Vfmul
            | Op::Vfdiv
            | Op::Vfrdiv
            | Op::Vfmin
            | Op::Vfmax
            | Op::Vfsgnj
            | Op::Vfsgnjn
            | Op::Vfsgnjx
            | Op::Vfsqrt => {
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let rm = RoundingMode::from_bits(self.frm()).unwrap_or(RoundingMode::Rne);
                let is_vv = insn.funct3 == 0b001; // OPFVV vs OPFVF
                let scalar = match eb {
                    2 => self.h(insn.rs1),
                    4 => self.s32(insn.rs1),
                    _ => self.f(insn.rs1),
                };
                let mut flags = 0u32;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let a = self.velem(vs2, e, eb);
                    let r = if insn.op == Op::Vfsqrt {
                        crate::isa::riscv::float::sf_sqrt(fmt_eb(eb), a, rm, &mut flags)
                    } else {
                        let b = if is_vv {
                            self.velem(insn.rs1, e, eb)
                        } else {
                            scalar
                        };
                        vfp_bin(insn.op, eb, a, b, rm, &mut flags)
                    };
                    self.set_velem(vd, e, eb, r & mask);
                }
                self.accrue(flags);
            }
            Op::Vfmacc
            | Op::Vfnmacc
            | Op::Vfmsac
            | Op::Vfnmsac
            | Op::Vfmadd
            | Op::Vfnmadd
            | Op::Vfmsub
            | Op::Vfnmsub => {
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let rm = RoundingMode::from_bits(self.frm()).unwrap_or(RoundingMode::Rne);
                let is_vv = insn.funct3 == 0b001;
                let scalar = match eb {
                    2 => self.h(insn.rs1),
                    4 => self.s32(insn.rs1),
                    _ => self.f(insn.rs1),
                };
                let mut flags = 0u32;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let src = if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let vs2e = self.velem(vs2, e, eb);
                    let vde = self.velem(vd, e, eb);
                    let r = vfp_fma(insn.op, eb, src, vs2e, vde, rm, &mut flags);
                    self.set_velem(vd, e, eb, r & mask);
                }
                self.accrue(flags);
            }
            Op::Vfredusum | Op::Vfredosum | Op::Vfredmin | Op::Vfredmax => {
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let rm = RoundingMode::from_bits(self.frm()).unwrap_or(RoundingMode::Rne);
                let mut flags = 0u32;
                let mut acc = self.velem(insn.rs1, 0, eb); // vs1[0] seed
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let x = self.velem(vs2, e, eb);
                    let sub = match insn.op {
                        Op::Vfredusum | Op::Vfredosum => Op::Vfadd,
                        Op::Vfredmin => Op::Vfmin,
                        _ => Op::Vfmax,
                    };
                    acc = vfp_bin(sub, eb, acc, x, rm, &mut flags) & mask;
                }
                if vl > vstart {
                    self.set_velem(vd, 0, eb, acc & mask);
                }
                self.accrue(flags);
            }
            Op::VfcvtXuF
            | Op::VfcvtXF
            | Op::VfcvtFXu
            | Op::VfcvtFX
            | Op::VfcvtRtzXuF
            | Op::VfcvtRtzXF => {
                // Single-width FP <-> integer conversions at SEW.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let frm = RoundingMode::from_bits(self.frm()).unwrap_or(RoundingMode::Rne);
                let to_int = matches!(
                    insn.op,
                    Op::VfcvtXuF | Op::VfcvtXF | Op::VfcvtRtzXuF | Op::VfcvtRtzXF
                );
                let signed = matches!(insn.op, Op::VfcvtXF | Op::VfcvtRtzXF | Op::VfcvtFX);
                let rm = if matches!(insn.op, Op::VfcvtRtzXuF | Op::VfcvtRtzXF) {
                    RoundingMode::Rtz
                } else {
                    frm
                };
                let mut flags = 0u32;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let a = self.velem(vs2, e, eb);
                    let r = if to_int {
                        match eb {
                            2 => crate::isa::riscv::float::ftoi(
                                crate::isa::riscv::float::h_widen(a as u16),
                                signed,
                                16,
                                rm,
                                &mut flags,
                            ),
                            4 => crate::isa::riscv::float::ftoi(
                                f32::from_bits(a as u32),
                                signed,
                                32,
                                rm,
                                &mut flags,
                            ),
                            _ => crate::isa::riscv::float::ftoi(f64::from_bits(a), signed, 64, rm, &mut flags),
                        }
                    } else {
                        let v: i128 = if signed {
                            sext_sew(a, eb) as i128
                        } else {
                            a as i128
                        };
                        crate::isa::riscv::float::itof_fmt(fmt_eb(eb), v, frm, &mut flags)
                    };
                    self.set_velem(vd, e, eb, r & mask);
                }
                self.accrue(flags);
            }
            Op::Vwredsumu | Op::Vwredsum => {
                // Widening integer sum reduction: 2*SEW accumulator seeded by vs1[0].
                let eb = self.sew_bytes();
                if eb > 4 {
                    return Err(Trap::illegal(insn.raw));
                }
                let web = eb * 2;
                let wmask = Self::sew_mask(web);
                let signed = insn.op == Op::Vwredsum;
                let mut acc = self.velem(insn.rs1, 0, web);
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let x = self.velem(vs2, e, eb);
                    let xe = if signed { sext_sew(x, eb) as u64 } else { x };
                    acc = acc.wrapping_add(xe) & wmask;
                }
                if vl > vstart {
                    self.set_velem(vd, 0, web, acc & wmask);
                }
            }
            Op::Vfwredusum | Op::Vfwredosum => {
                // Widening FP sum reduction: 2*SEW accumulator seeded by vs1[0].
                let eb = self.sew_bytes();
                if eb > 4 {
                    return Err(Trap::illegal(insn.raw));
                }
                let web = eb * 2;
                let wmask = Self::sew_mask(web);
                let frm = RoundingMode::from_bits(self.frm()).unwrap_or(RoundingMode::Rne);
                let mut flags = 0u32;
                let mut acc = self.velem(insn.rs1, 0, web);
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let x = crate::isa::riscv::float::fcvt_round(
                        fmt_eb(eb),
                        fmt_eb(web),
                        self.velem(vs2, e, eb),
                        frm,
                        &mut flags,
                    );
                    acc = vfp_bin(Op::Vfadd, web, acc, x, frm, &mut flags) & wmask;
                }
                if vl > vstart {
                    self.set_velem(vd, 0, web, acc & wmask);
                }
                self.accrue(flags);
            }
            Op::Vfwmacc | Op::Vfwnmacc | Op::Vfwmsac | Op::Vfwnmsac => {
                // Widening FP FMA: vs1/vs2 widened to 2*SEW, fused into 2*SEW vd.
                let eb = self.sew_bytes();
                if eb > 4 {
                    return Err(Trap::illegal(insn.raw));
                }
                let web = eb * 2;
                let wmask = Self::sew_mask(web);
                let frm = RoundingMode::from_bits(self.frm()).unwrap_or(RoundingMode::Rne);
                let is_vv = insn.funct3 == 0b001;
                let base = match insn.op {
                    Op::Vfwmacc => Op::Vfmacc,
                    Op::Vfwnmacc => Op::Vfnmacc,
                    Op::Vfwmsac => Op::Vfmsac,
                    _ => Op::Vfnmsac,
                };
                let scalar = match eb {
                    2 => self.h(insn.rs1),
                    _ => self.s32(insn.rs1),
                };
                let mut flags = 0u32;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let s_narrow = if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let src = crate::isa::riscv::float::fcvt_round(
                        fmt_eb(eb),
                        fmt_eb(web),
                        s_narrow,
                        frm,
                        &mut flags,
                    );
                    let v2 = crate::isa::riscv::float::fcvt_round(
                        fmt_eb(eb),
                        fmt_eb(web),
                        self.velem(vs2, e, eb),
                        frm,
                        &mut flags,
                    );
                    let vde = self.velem(vd, e, web);
                    let r = vfp_fma(base, web, src, v2, vde, frm, &mut flags);
                    self.set_velem(vd, e, web, r & wmask);
                }
                self.accrue(flags);
            }
            Op::Vfwadd | Op::Vfwsub | Op::Vfwmul | Op::VfwaddW | Op::VfwsubW => {
                // Widening FP arithmetic: operands widened to 2*SEW, op at 2*SEW.
                let eb = self.sew_bytes();
                if eb > 4 {
                    return Err(Trap::illegal(insn.raw));
                }
                let web = eb * 2;
                let wmask = Self::sew_mask(web);
                let frm = RoundingMode::from_bits(self.frm()).unwrap_or(RoundingMode::Rne);
                let is_vv = insn.funct3 == 0b001;
                let wide_vs2 = matches!(insn.op, Op::VfwaddW | Op::VfwsubW);
                let scalar = match eb {
                    2 => self.h(insn.rs1),
                    _ => self.s32(insn.rs1),
                };
                let mut flags = 0u32;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let aw = if wide_vs2 {
                        self.velem(vs2, e, web)
                    } else {
                        crate::isa::riscv::float::fcvt_round(
                            fmt_eb(eb),
                            fmt_eb(web),
                            self.velem(vs2, e, eb),
                            frm,
                            &mut flags,
                        )
                    };
                    let braw = if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let bw =
                        crate::isa::riscv::float::fcvt_round(fmt_eb(eb), fmt_eb(web), braw, frm, &mut flags);
                    let r = match insn.op {
                        Op::Vfwadd | Op::VfwaddW => {
                            vfp_bin(Op::Vfadd, web, aw, bw, frm, &mut flags)
                        }
                        Op::Vfwsub | Op::VfwsubW => {
                            vfp_bin(Op::Vfsub, web, aw, bw, frm, &mut flags)
                        }
                        _ => vfp_bin(Op::Vfmul, web, aw, bw, frm, &mut flags),
                    };
                    self.set_velem(vd, e, web, r & wmask);
                }
                self.accrue(flags);
            }
            Op::VfwcvtXuF
            | Op::VfwcvtXF
            | Op::VfwcvtFXu
            | Op::VfwcvtFX
            | Op::VfwcvtFF
            | Op::VfwcvtRtzXuF
            | Op::VfwcvtRtzXF => {
                // Widening conversions: SEW source -> 2*SEW result.
                let eb = self.sew_bytes();
                if eb > 4 {
                    return Err(Trap::illegal(insn.raw));
                }
                let web = eb * 2;
                let wmask = Self::sew_mask(web);
                let frm = RoundingMode::from_bits(self.frm()).unwrap_or(RoundingMode::Rne);
                let mut flags = 0u32;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let a = self.velem(vs2, e, eb);
                    let r = match insn.op {
                        Op::VfwcvtXuF | Op::VfwcvtXF | Op::VfwcvtRtzXuF | Op::VfwcvtRtzXF => {
                            let signed = matches!(insn.op, Op::VfwcvtXF | Op::VfwcvtRtzXF);
                            let rm = if matches!(insn.op, Op::VfwcvtRtzXuF | Op::VfwcvtRtzXF) {
                                RoundingMode::Rtz
                            } else {
                                frm
                            };
                            match eb {
                                2 => crate::isa::riscv::float::ftoi(
                                    crate::isa::riscv::float::h_widen(a as u16),
                                    signed,
                                    32,
                                    rm,
                                    &mut flags,
                                ),
                                _ => crate::isa::riscv::float::ftoi(
                                    f32::from_bits(a as u32),
                                    signed,
                                    64,
                                    rm,
                                    &mut flags,
                                ),
                            }
                        }
                        Op::VfwcvtFXu | Op::VfwcvtFX => {
                            let v: i128 = if insn.op == Op::VfwcvtFX {
                                sext_sew(a, eb) as i128
                            } else {
                                a as i128
                            };
                            crate::isa::riscv::float::itof_fmt(fmt_eb(web), v, frm, &mut flags)
                        }
                        _ => crate::isa::riscv::float::fcvt_round(fmt_eb(eb), fmt_eb(web), a, frm, &mut flags),
                    };
                    self.set_velem(vd, e, web, r & wmask);
                }
                self.accrue(flags);
            }
            Op::VfncvtXuF
            | Op::VfncvtXF
            | Op::VfncvtFXu
            | Op::VfncvtFX
            | Op::VfncvtFF
            | Op::VfncvtRodFF
            | Op::VfncvtRtzXuF
            | Op::VfncvtRtzXF => {
                // Narrowing conversions: 2*SEW source vs2 -> SEW result. Only
                // SEW in {16,32} (eb 2/4) is supported: SEW=8 would imply an
                // FP8 format / 8-bit float-to-int width that has no defined
                // conversion here, so reject eb outside {2,4}.
                let eb = self.sew_bytes();
                if !(2..=4).contains(&eb) {
                    return Err(Trap::illegal(insn.raw));
                }
                let web = eb * 2;
                let mask = Self::sew_mask(eb);
                let frm = RoundingMode::from_bits(self.frm()).unwrap_or(RoundingMode::Rne);
                let mut flags = 0u32;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let aw = self.velem(vs2, e, web);
                    let r = match insn.op {
                        Op::VfncvtXuF | Op::VfncvtXF | Op::VfncvtRtzXuF | Op::VfncvtRtzXF => {
                            let signed = matches!(insn.op, Op::VfncvtXF | Op::VfncvtRtzXF);
                            let rm = if matches!(insn.op, Op::VfncvtRtzXuF | Op::VfncvtRtzXF) {
                                RoundingMode::Rtz
                            } else {
                                frm
                            };
                            match web {
                                4 => crate::isa::riscv::float::ftoi(
                                    f32::from_bits(aw as u32),
                                    signed,
                                    (eb * 8) as u32,
                                    rm,
                                    &mut flags,
                                ),
                                _ => crate::isa::riscv::float::ftoi(
                                    f64::from_bits(aw),
                                    signed,
                                    (eb * 8) as u32,
                                    rm,
                                    &mut flags,
                                ),
                            }
                        }
                        Op::VfncvtFXu | Op::VfncvtFX => {
                            let v: i128 = if insn.op == Op::VfncvtFX {
                                sext_sew(aw, web) as i128
                            } else {
                                aw as i128
                            };
                            crate::isa::riscv::float::itof_fmt(fmt_eb(eb), v, frm, &mut flags)
                        }
                        Op::VfncvtRodFF => {
                            // Round-to-odd: truncate, then force the LSB on inexact.
                            let mut t = 0u32;
                            let r = crate::isa::riscv::float::fcvt_round(
                                fmt_eb(web),
                                fmt_eb(eb),
                                aw,
                                RoundingMode::Rtz,
                                &mut t,
                            );
                            flags |= t;
                            if t & 1 != 0 { r | 1 } else { r } // NX is fflags bit 0
                        }
                        _ => crate::isa::riscv::float::fcvt_round(fmt_eb(web), fmt_eb(eb), aw, frm, &mut flags),
                    };
                    self.set_velem(vd, e, eb, r & mask);
                }
                self.accrue(flags);
            }
            Op::Vmfeq | Op::Vmfne | Op::Vmflt | Op::Vmfle | Op::Vmfgt | Op::Vmfge => {
                let eb = self.sew_bytes();
                let is_vv = insn.funct3 == 0b001;
                let scalar = match eb {
                    2 => self.h(insn.rs1),
                    4 => self.s32(insn.rs1),
                    _ => self.f(insn.rs1),
                };
                let mut flags = 0u32;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let a = self.velem(vs2, e, eb);
                    let b = if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let r = vfp_cmp(insn.op, eb, a, b, &mut flags);
                    self.set_vmask_bit(vd, e, r);
                }
                self.accrue(flags);
            }
            Op::VzextVf2
            | Op::VsextVf2
            | Op::VzextVf4
            | Op::VsextVf4
            | Op::VzextVf8
            | Op::VsextVf8 => {
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let (factor, signed) = match insn.op {
                    Op::VzextVf2 => (2usize, false),
                    Op::VsextVf2 => (2, true),
                    Op::VzextVf4 => (4, false),
                    Op::VsextVf4 => (4, true),
                    Op::VzextVf8 => (8, false),
                    _ => (8, true),
                };
                if eb < factor {
                    return Err(Trap::illegal(insn.raw)); // SEW too narrow for the source
                }
                let neb = eb / factor; // narrow source element width
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let src = self.velem(vs2, e, neb);
                    let v = if signed {
                        sext_sew(src, neb) as u64
                    } else {
                        src
                    };
                    self.set_velem(vd, e, eb, v & mask);
                }
            }
            Op::Vmand
            | Op::Vmnand
            | Op::Vmandn
            | Op::Vmxor
            | Op::Vmor
            | Op::Vmnor
            | Op::Vmorn
            | Op::Vmxnor => {
                // Mask-register logicals: vd.bit[i] = vs2.bit[i] OP vs1.bit[i],
                // always unmasked, over the body [vstart, vl). The vm=0 form is
                // reserved and must raise an illegal-instruction trap.
                if !vm {
                    return Err(Trap::illegal(insn.raw));
                }
                for e in vstart..vl {
                    let a = self.vbit(vs2, e);
                    let b = self.vbit(insn.rs1, e);
                    let r = match insn.op {
                        Op::Vmand => a & b,
                        Op::Vmnand => !(a & b),
                        Op::Vmandn => a & !b,
                        Op::Vmxor => a ^ b,
                        Op::Vmor => a | b,
                        Op::Vmnor => !(a | b),
                        Op::Vmorn => a | !b,
                        Op::Vmxnor => !(a ^ b),
                        _ => unreachable!(),
                    };
                    self.set_vmask_bit(vd, e, r);
                }
            }
            Op::Vslideup => {
                // vd[i] = vs2[i - offset] for i >= offset; lower elements untouched.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let offset = if insn.funct3 == 0b011 {
                    insn.rs1 as u64
                } else {
                    self.x(insn.rs1)
                };
                let start = vstart.max(offset as usize);
                for e in start..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let v = self.velem(vs2, e - offset as usize, eb);
                    self.set_velem(vd, e, eb, v & mask);
                }
            }
            Op::Vslidedown => {
                // vd[i] = vs2[i + offset], or 0 when i + offset >= VLMAX.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let vlmax = self.vlmax_elems() as u64;
                let offset = if insn.funct3 == 0b011 {
                    insn.rs1 as u64
                } else {
                    self.x(insn.rs1)
                };
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    // A guest-controlled scalar offset can be huge; saturate so
                    // an overflowing i+offset stays >= VLMAX and zeroes the lane
                    // rather than wrapping back into an in-range source index.
                    let src = (e as u64).saturating_add(offset);
                    let v = if src < vlmax {
                        self.velem(vs2, src as usize, eb)
                    } else {
                        0
                    };
                    self.set_velem(vd, e, eb, v & mask);
                }
            }
            Op::Vslide1up | Op::Vfslide1up => {
                // vd[0] = scalar; vd[i] = vs2[i-1] for i >= 1.
                // The destination must not alias the vs2 source register
                // (vslide1down/vfslide1down permit the overlap).
                if vd == vs2 {
                    return Err(Trap::illegal(insn.raw));
                }
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let scalar = if insn.op == Op::Vfslide1up {
                    match eb {
                        2 => self.h(insn.rs1),
                        4 => self.s32(insn.rs1),
                        _ => self.f(insn.rs1),
                    }
                } else {
                    self.x(insn.rs1)
                } & mask;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let v = if e == 0 {
                        scalar
                    } else {
                        self.velem(vs2, e - 1, eb)
                    };
                    self.set_velem(vd, e, eb, v & mask);
                }
            }
            Op::Vslide1down | Op::Vfslide1down => {
                // vd[i] = vs2[i+1] for i < vl-1; vd[vl-1] = scalar.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let scalar = if insn.op == Op::Vfslide1down {
                    match eb {
                        2 => self.h(insn.rs1),
                        4 => self.s32(insn.rs1),
                        _ => self.f(insn.rs1),
                    }
                } else {
                    self.x(insn.rs1)
                } & mask;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let v = if e + 1 < vl {
                        self.velem(vs2, e + 1, eb)
                    } else {
                        scalar
                    };
                    self.set_velem(vd, e, eb, v & mask);
                }
            }
            Op::Vwaddu
            | Op::Vwadd
            | Op::Vwsubu
            | Op::Vwsub
            | Op::VwadduW
            | Op::VwaddW
            | Op::VwsubuW
            | Op::VwsubW => {
                // Widening add/subtract: 2*SEW result. `.w` forms read a wide vs2.
                let eb = self.sew_bytes();
                if eb > 4 {
                    return Err(Trap::illegal(insn.raw)); // 2*SEW must fit ELEN=64
                }
                let web = eb * 2;
                let wmask = Self::sew_mask(web);
                let signed = matches!(insn.op, Op::Vwadd | Op::Vwsub | Op::VwaddW | Op::VwsubW);
                let sub = matches!(insn.op, Op::Vwsubu | Op::Vwsub | Op::VwsubuW | Op::VwsubW);
                let wide_vs2 =
                    matches!(insn.op, Op::VwadduW | Op::VwaddW | Op::VwsubuW | Op::VwsubW);
                let is_vv = insn.funct3 == 0b010;
                let scalar = self.x(insn.rs1) & Self::sew_mask(eb);
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let a: i128 = if wide_vs2 {
                        let raw = self.velem(vs2, e, web);
                        if signed {
                            sext_sew(raw, web) as i128
                        } else {
                            raw as i128
                        }
                    } else {
                        let raw = self.velem(vs2, e, eb);
                        if signed {
                            sext_sew(raw, eb) as i128
                        } else {
                            raw as i128
                        }
                    };
                    let braw = if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let b: i128 = if signed {
                        sext_sew(braw, eb) as i128
                    } else {
                        braw as i128
                    };
                    let r = if sub { a - b } else { a + b };
                    self.set_velem(vd, e, web, (r as u64) & wmask);
                }
            }
            Op::Vwmulu
            | Op::Vwmulsu
            | Op::Vwmul
            | Op::Vwmaccu
            | Op::Vwmacc
            | Op::Vwmaccsu
            | Op::Vwmaccus => {
                // Widening multiply / multiply-accumulate: 2*SEW product into vd group.
                let eb = self.sew_bytes();
                if eb > 4 {
                    return Err(Trap::illegal(insn.raw));
                }
                let web = eb * 2;
                let wmask = Self::sew_mask(web);
                // Signedness of (a = vs2, b = vs1/rs1 multiplier).
                let (a_signed, b_signed) = match insn.op {
                    Op::Vwmulu | Op::Vwmaccu => (false, false),
                    Op::Vwmul | Op::Vwmacc => (true, true),
                    Op::Vwmulsu | Op::Vwmaccus => (true, false),
                    _ => (false, true), // Vwmaccsu
                };
                let is_vv = insn.funct3 == 0b010;
                let is_mac = matches!(
                    insn.op,
                    Op::Vwmaccu | Op::Vwmacc | Op::Vwmaccsu | Op::Vwmaccus
                );
                let scalar = self.x(insn.rs1) & Self::sew_mask(eb);
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let araw = self.velem(vs2, e, eb);
                    let braw = if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let av: i128 = if a_signed {
                        sext_sew(araw, eb) as i128
                    } else {
                        araw as i128
                    };
                    let bv: i128 = if b_signed {
                        sext_sew(braw, eb) as i128
                    } else {
                        braw as i128
                    };
                    let mut prod = av * bv;
                    if is_mac {
                        prod = prod.wrapping_add(self.velem(vd, e, web) as i128);
                    }
                    self.set_velem(vd, e, web, (prod as u64) & wmask);
                }
            }
            Op::ThVmaqa | Op::ThVmaqau | Op::ThVmaqasu | Op::ThVmaqaus => {
                // XTheadVdot accumulates four 8-bit products into each 32-bit
                // destination lane. `vl` counts destination lanes, while v0 mask
                // bits gate the individual 8-bit source products.
                let eb = self.sew_bytes();
                if eb != 4 {
                    return Err(Trap::illegal(insn.raw));
                }
                let scalar = ((insn.raw >> 26) & 1) != 0;
                let (src1_signed, src2_signed) = match insn.op {
                    Op::ThVmaqa => (true, true),
                    Op::ThVmaqau => (false, false),
                    Op::ThVmaqasu => (true, false),
                    Op::ThVmaqaus => (false, true),
                    _ => unreachable!(),
                };
                for e in vstart..vl {
                    let a = if scalar {
                        self.x(insn.rs1) as u32
                    } else {
                        self.velem(insn.rs1, e, eb) as u32
                    };
                    let b = self.velem(vs2, e, eb) as u32;
                    let mut sum = 0i64;
                    for byte in 0..4 {
                        if vm || self.vmask_bit(e * 4 + byte) {
                            let av = th_vdot_byte((a >> (byte * 8)) as u8, src1_signed);
                            let bv = th_vdot_byte((b >> (byte * 8)) as u8, src2_signed);
                            sum += av * bv;
                        }
                    }
                    let acc = self.velem(vd, e, eb) as u32;
                    self.set_velem(vd, e, eb, acc.wrapping_add(sum as u32) as u64);
                }
            }
            Op::Vnsrl | Op::Vnsra | Op::Vnclipu | Op::Vnclip => {
                // Narrowing shift/clip: 2*SEW source vs2 -> SEW result.
                let eb = self.sew_bytes();
                if eb > 4 {
                    return Err(Trap::illegal(insn.raw));
                }
                let web = eb * 2;
                let mask = Self::sew_mask(eb);
                let bits = (eb * 8) as u32;
                let sh_mask = (web * 8 - 1) as u32;
                let vxrm = self.vxrm;
                let smax = (1i128 << (bits - 1)) - 1;
                let smin = -(1i128 << (bits - 1));
                let is_clip = matches!(insn.op, Op::Vnclipu | Op::Vnclip);
                let signed = matches!(insn.op, Op::Vnsra | Op::Vnclip);
                let is_vv = insn.funct3 == 0b000;
                let scalar = match insn.funct3 {
                    0b100 => self.x(insn.rs1),
                    0b011 => insn.rs1 as u64,
                    _ => 0,
                };
                let mut sat = false;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let aw = self.velem(vs2, e, web);
                    let sh = (if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    }) as u32
                        & sh_mask;
                    let r = if !is_clip {
                        if signed {
                            (sext_sew(aw, web) >> sh) as u64
                        } else {
                            aw >> sh
                        }
                    } else if !signed {
                        let v = (aw >> sh) as u128 + round_incr(aw as u128, sh, vxrm);
                        if v > mask as u128 {
                            sat = true;
                            mask
                        } else {
                            v as u64
                        }
                    } else {
                        let sa = sext_sew(aw, web) as i128;
                        let v = (sa >> sh) + round_incr(sa as u128, sh, vxrm) as i128;
                        if v > smax {
                            sat = true;
                            smax as u64
                        } else if v < smin {
                            sat = true;
                            smin as u64
                        } else {
                            v as u64
                        }
                    };
                    self.set_velem(vd, e, eb, r & mask);
                }
                if sat {
                    self.vxsat = 1;
                }
            }
            Op::Vssrl | Op::Vssra => {
                // Scaling shift right by (amount & (SEW-1)), rounded per vxrm.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let bits = (eb * 8) as u32;
                let shmask = bits - 1;
                let vxrm = self.vxrm;
                let scalar = match insn.funct3 {
                    0b100 => self.x(insn.rs1),
                    0b011 => insn.rs1 as u64, // unsigned 5-bit shift immediate
                    _ => 0,
                };
                let is_vv = insn.funct3 == 0b000;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let a = self.velem(vs2, e, eb);
                    let sh = (if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    }) as u32
                        & shmask;
                    let incr = round_incr(a as u128, sh, vxrm);
                    let res = if insn.op == Op::Vssrl {
                        ((a >> sh) as u128 + incr) as u64
                    } else {
                        (sext_sew(a, eb) >> sh).wrapping_add(incr as i64) as u64
                    };
                    self.set_velem(vd, e, eb, res & mask);
                }
            }
            Op::Vsmul => {
                // Signed fractional multiply: (a*b) >> (SEW-1), rounded + saturated.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let bits = (eb * 8) as u32;
                let smax = (1i128 << (bits - 1)) - 1;
                let smin = -(1i128 << (bits - 1));
                let vxrm = self.vxrm;
                let is_vv = insn.funct3 == 0b000;
                let scalar = self.x(insn.rs1) & mask;
                let mut sat = false;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let a = self.velem(vs2, e, eb);
                    let b = if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let prod = sext_sew(a, eb) as i128 * sext_sew(b, eb) as i128;
                    let incr = round_incr(prod as u128, bits - 1, vxrm) as i128;
                    let mut r = (prod >> (bits - 1)) + incr;
                    if r > smax {
                        r = smax;
                        sat = true;
                    } else if r < smin {
                        r = smin;
                        sat = true;
                    }
                    self.set_velem(vd, e, eb, r as u64 & mask);
                }
                if sat {
                    self.vxsat = 1;
                }
            }
            Op::Vaaddu | Op::Vaadd | Op::Vasubu | Op::Vasub => {
                // Averaging add/subtract: (a +/- b) >> 1, rounded per vxrm.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let bits = (eb * 8) as u32;
                let m2: u128 = if bits >= 64 {
                    u128::MAX
                } else {
                    (1u128 << (2 * bits)) - 1
                };
                let vxrm = self.vxrm;
                let is_vv = insn.funct3 == 0b010;
                let scalar = self.x(insn.rs1) & mask;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let a = self.velem(vs2, e, eb);
                    let b = if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let res = match insn.op {
                        Op::Vaaddu => {
                            let v = a as u128 + b as u128;
                            ((v >> 1) + round_incr(v, 1, vxrm)) as u64
                        }
                        Op::Vasubu => {
                            let v = (a as u128).wrapping_sub(b as u128) & m2;
                            ((v >> 1) + round_incr(v, 1, vxrm)) as u64
                        }
                        Op::Vaadd => {
                            let v = sext_sew(a, eb) as i128 + sext_sew(b, eb) as i128;
                            ((v >> 1) + round_incr(v as u128, 1, vxrm) as i128) as u64
                        }
                        _ => {
                            let v = sext_sew(a, eb) as i128 - sext_sew(b, eb) as i128;
                            ((v >> 1) + round_incr(v as u128, 1, vxrm) as i128) as u64
                        }
                    };
                    self.set_velem(vd, e, eb, res & mask);
                }
            }
            Op::Vsaddu | Op::Vsadd | Op::Vssubu | Op::Vssub => {
                // Saturating fixed-point add/subtract; sets vxsat on clamp.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let bits = (eb * 8) as u32;
                let smax = (1i128 << (bits - 1)) - 1;
                let smin = -(1i128 << (bits - 1));
                let scalar = match insn.funct3 {
                    0b100 => self.x(insn.rs1) & mask,
                    0b011 => sext5(insn.rs1) & mask,
                    _ => 0,
                };
                let is_vv = insn.funct3 == 0b000;
                let mut sat = false;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let a = self.velem(vs2, e, eb);
                    let b = if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let (r, s) = match insn.op {
                        Op::Vsaddu => {
                            let full = a as u128 + b as u128;
                            if full > mask as u128 {
                                (mask, true)
                            } else {
                                (full as u64, false)
                            }
                        }
                        Op::Vssubu => {
                            if a < b {
                                (0, true)
                            } else {
                                (a - b, false)
                            }
                        }
                        Op::Vsadd => {
                            let sum = sext_sew(a, eb) as i128 + sext_sew(b, eb) as i128;
                            if sum > smax {
                                (smax as u64 & mask, true)
                            } else if sum < smin {
                                (smin as u64 & mask, true)
                            } else {
                                (sum as u64 & mask, false)
                            }
                        }
                        _ => {
                            let diff = sext_sew(a, eb) as i128 - sext_sew(b, eb) as i128;
                            if diff > smax {
                                (smax as u64 & mask, true)
                            } else if diff < smin {
                                (smin as u64 & mask, true)
                            } else {
                                (diff as u64 & mask, false)
                            }
                        }
                    };
                    self.set_velem(vd, e, eb, r & mask);
                    sat |= s;
                }
                if sat {
                    self.vxsat = 1;
                }
            }
            Op::Vadc | Op::Vsbc => {
                // vd[i] = vs2[i] +/- op[i] +/- v0.mask[i]; every body lane written.
                // These consume the v0 carry/borrow-in and are only defined in
                // the masked (vm=0) form; the unmasked vm=1 encoding is reserved.
                if vm {
                    return Err(Trap::illegal(insn.raw));
                }
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let scalar = match insn.funct3 {
                    0b100 => self.x(insn.rs1) & mask,
                    0b011 => sext5(insn.rs1) & mask,
                    _ => 0,
                };
                let is_vv = insn.funct3 == 0b000;
                for e in vstart..vl {
                    let a = self.velem(vs2, e, eb);
                    let b = if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    };
                    let cin = self.vmask_bit(e) as u64; // v0 carry/borrow-in
                    let r = if insn.op == Op::Vadc {
                        a.wrapping_add(b).wrapping_add(cin)
                    } else {
                        a.wrapping_sub(b).wrapping_sub(cin)
                    };
                    self.set_velem(vd, e, eb, r & mask);
                }
            }
            Op::Vmadc | Op::Vmsbc => {
                // vd.mask[i] = carry/borrow-out; carry-in from v0 only when vm == 0.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb) as u128;
                let scalar = match insn.funct3 {
                    0b100 => self.x(insn.rs1) & Self::sew_mask(eb),
                    0b011 => sext5(insn.rs1) & Self::sew_mask(eb),
                    _ => 0,
                };
                let is_vv = insn.funct3 == 0b000;
                let use_cin = !vm;
                for e in vstart..vl {
                    let a = self.velem(vs2, e, eb) as u128;
                    let b = if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar
                    } as u128;
                    let cin = if use_cin {
                        self.vmask_bit(e) as u128
                    } else {
                        0
                    };
                    let out = if insn.op == Op::Vmadc {
                        a + b + cin > mask
                    } else {
                        a < b + cin
                    };
                    self.set_vmask_bit(vd, e, out);
                }
            }
            Op::Vfrsqrt7 | Op::Vfrec7 => {
                // 7-bit reciprocal / reciprocal-sqrt estimates.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let rm = RoundingMode::from_bits(self.frm()).unwrap_or(RoundingMode::Rne);
                let mut flags = 0u32;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let a = self.velem(vs2, e, eb);
                    let r = if insn.op == Op::Vfrsqrt7 {
                        crate::isa::riscv::float::vfrsqrt7(fmt_eb(eb), a, &mut flags)
                    } else {
                        crate::isa::riscv::float::vfrec7(fmt_eb(eb), a, rm, &mut flags)
                    };
                    self.set_velem(vd, e, eb, r & mask);
                }
                self.accrue(flags);
            }
            Op::Vfclass => {
                // vd[i] = 10-bit IEEE class of vs2[i].
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let r = crate::isa::riscv::float::fclass_bits(fmt_eb(eb), self.velem(vs2, e, eb));
                    self.set_velem(vd, e, eb, r & mask);
                }
            }
            Op::Vmvr => {
                // vmv<nr>r.v whole-register move: only nr in {1,2,4,8} (simm
                // 0/1/3/7) is defined, the encoding must be unmasked, and both
                // vd and vs2 must be aligned to the nr-register group. Reserved
                // simm values, masked encodings, or misaligned groups trap.
                let nreg = match insn.rs1 {
                    0 => 1u8,
                    1 => 2,
                    3 => 4,
                    7 => 8,
                    _ => return Err(Trap::illegal(insn.raw)),
                };
                if !vm || vd % nreg != 0 || vs2 % nreg != 0 {
                    return Err(Trap::illegal(insn.raw));
                }
                let total = nreg as usize * VLENB as usize;
                for i in 0..total {
                    let b = self.velem(vs2, i, 1);
                    self.set_velem(vd, i, 1, b);
                }
            }
            Op::Vcompress => {
                // vcompress.vm is unmasked (vm=1), is not restartable (vstart
                // must be 0), and its destination group must not overlap the
                // source vs2 group or the single-register mask source vs1.
                let emul: u8 = match self.vtype & 0x7 {
                    1 => 2,
                    2 => 4,
                    3 => 8,
                    _ => 1, // LMUL=1 and all fractional LMULs occupy one register
                };
                let overlaps = |a: u8, an: u8, b: u8, bn: u8| a < b + bn && b < a + an;
                if !vm
                    || vstart != 0
                    || overlaps(vd, emul, vs2, emul)
                    || overlaps(vd, emul, insn.rs1, 1)
                {
                    return Err(Trap::illegal(insn.raw));
                }
                // Pack vs2 elements whose vs1 mask bit is set into the low lanes of vd.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let mut out = vstart;
                for e in vstart..vl {
                    if self.vbit(insn.rs1, e) {
                        let v = self.velem(vs2, e, eb);
                        self.set_velem(vd, out, eb, v & mask);
                        out += 1;
                    }
                }
            }
            Op::Vrgather | Op::Vrgatherei16 => {
                // vd[i] = vs2[index(i)], or 0 when the index is >= VLMAX.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let vlmax = self.vlmax_elems() as u64;
                let scalar_idx = match insn.funct3 {
                    0b100 => self.x(insn.rs1), // vx
                    0b011 => insn.rs1 as u64,  // vi (zero-extended imm)
                    _ => 0,
                };
                let ei16 = insn.op == Op::Vrgatherei16;
                let is_vv = insn.funct3 == 0b000;
                // The destination group must not overlap the source vs2 group,
                // nor (for vv/ei16) the index vector group; such encodings are
                // reserved and must trap rather than gather in place.
                let data_emul: u8 = match self.vtype & 0x7 {
                    1 => 2,
                    2 => 4,
                    3 => 8,
                    _ => 1,
                };
                let overlaps = |a: u8, an: u8, b: u8, bn: u8| a < b + bn && b < a + an;
                if overlaps(vd, data_emul, vs2, data_emul) {
                    return Err(Trap::illegal(insn.raw));
                }
                if is_vv || ei16 {
                    let idx_regs = if ei16 {
                        // Index EEW=16, so its EMUL (in registers) is
                        // ceil(data_emul * 16 / SEW), at least one register.
                        let sew_bits = 8u32 << ((self.vtype >> 3) & 0x7);
                        ((data_emul as u32 * 16 + sew_bits - 1) / sew_bits).max(1) as u8
                    } else {
                        data_emul
                    };
                    if overlaps(vd, data_emul, insn.rs1, idx_regs) {
                        return Err(Trap::illegal(insn.raw));
                    }
                }
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    let idx = if ei16 {
                        self.velem(insn.rs1, e, 2) // 16-bit index element
                    } else if is_vv {
                        self.velem(insn.rs1, e, eb)
                    } else {
                        scalar_idx
                    };
                    let v = if idx < vlmax {
                        self.velem(vs2, idx as usize, eb)
                    } else {
                        0
                    };
                    self.set_velem(vd, e, eb, v & mask);
                }
            }
            Op::Vcpop => {
                // x[rd] = number of active mask bits set in vs2. This reduction
                // is not restartable: a non-zero vstart is reserved and traps.
                if vstart != 0 {
                    return Err(Trap::illegal(insn.raw));
                }
                let mut count = 0u64;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    if self.vbit(vs2, e) {
                        count += 1;
                    }
                }
                self.set_x(insn.rd, count);
            }
            Op::Vfirst => {
                // x[rd] = index of first active set mask bit, or -1. Not
                // restartable: a non-zero vstart is reserved and traps.
                if vstart != 0 {
                    return Err(Trap::illegal(insn.raw));
                }
                let mut idx: i64 = -1;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    if self.vbit(vs2, e) {
                        idx = e as i64;
                        break;
                    }
                }
                self.set_x(insn.rd, idx as u64);
            }
            Op::Vmsbf | Op::Vmsif | Op::Vmsof => {
                // Set-before / set-including / set-only the first active set bit.
                // These prefix ops are not restartable: non-zero vstart traps.
                if vstart != 0 {
                    return Err(Trap::illegal(insn.raw));
                }
                let mut found = false;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue; // masked-off destination undisturbed
                    }
                    let s = self.vbit(vs2, e);
                    let out = if !found {
                        if s {
                            found = true;
                            insn.op != Op::Vmsbf // bf->0, if/of->1 at the first set
                        } else {
                            insn.op != Op::Vmsof // bf/if->1, of->0 before the first set
                        }
                    } else {
                        false
                    };
                    self.set_vmask_bit(vd, e, out);
                }
            }
            Op::Viota => {
                // vd[i] = count of active set bits in vs2 strictly before i.
                // This prefix scan is not restartable: non-zero vstart traps.
                if vstart != 0 {
                    return Err(Trap::illegal(insn.raw));
                }
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                let mut sum = 0u64;
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    self.set_velem(vd, e, eb, sum & mask);
                    if self.vbit(vs2, e) {
                        sum += 1;
                    }
                }
            }
            Op::Vid => {
                // vd[i] = i (element index); source vs2 ignored.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                for e in vstart..vl {
                    if !vm && !self.vmask_bit(e) {
                        continue;
                    }
                    self.set_velem(vd, e, eb, (e as u64) & mask);
                }
            }
            Op::VmvXS => {
                // x[rd] = sign-extended lane 0 of vs2 (ignores vl/vstart).
                let eb = self.sew_bytes();
                let v = sext_sew(self.velem(vs2, 0, eb), eb) as u64;
                self.set_x(insn.rd, v);
            }
            Op::VfmvFS => {
                // f[rd] = NaN-boxed lane 0 of vs2 (ignores vl/vstart).
                let eb = self.sew_bytes();
                let v = self.velem(vs2, 0, eb);
                match eb {
                    2 => self.wf16(insn.rd, v as u16),
                    4 => self.wf32(insn.rd, v as u32),
                    _ => self.wf64(insn.rd, v),
                }
            }
            Op::VmvSX => {
                // vd[0] = x[rs1] (low SEW); no-op when vstart >= vl.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                if vstart < vl {
                    self.set_velem(vd, 0, eb, self.x(insn.rs1) & mask);
                }
            }
            Op::VfmvSF => {
                // vd[0] = f[rs1] (low SEW); no-op when vstart >= vl.
                let eb = self.sew_bytes();
                let mask = Self::sew_mask(eb);
                if vstart < vl {
                    let s = match eb {
                        2 => self.h(insn.rs1),
                        4 => self.s32(insn.rs1),
                        _ => self.f(insn.rs1),
                    };
                    self.set_velem(vd, 0, eb, s & mask);
                }
            }
            _ => return Err(Trap::illegal(insn.raw)),
        }
        self.vstart = 0;
        Ok(())
    }

    // ---------------------------------------------------------------
    // V: vector configuration (vsetvl* compute the new vl from vtype).
    // ---------------------------------------------------------------

    /// Apply a `vtype` and an application vector length, returning the new `vl`
    /// and updating the `vl`/`vtype` CSRs. An illegal `vtype` sets `vill` and
    /// zeroes `vl`.
    pub(super) fn set_vtype(&mut self, vtype: u64, avl: Avl) -> u64 {
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
            self.vtype = 1u64 << (self.xbits() - 1); // vill bit
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
