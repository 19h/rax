//! permute.rs

use crate::isa::arm::aarch64::cpu::*;
use crate::isa::arm::aarch64::cpu::*;
use std::collections::HashSet;
use std::fmt::Debug;

use crate::isa::arm::aarch64::exceptions::{
    ExceptionType, SyndromeRegister, build_spsr, exception_target_el, parse_spsr, vector_offset,
};
use crate::isa::arm::aarch64::gic::{Gic, GicConfig};
use crate::isa::arm::aarch64::mmu::{Mmu, MmuConfig, TranslationFault, TranslationGranule};
use crate::isa::arm::aarch64::sysregs::SystemRegisters;
use crate::isa::arm::aarch64::{NUM_ELS, NUM_GPRS, NUM_SIMD_REGS, sctlr};

use crate::isa::arm::common::cpu::{
    ArmCpu, ArmError, ArmException, ArmProfile, ArmVersion, CpuExit, MemoryFaultInfo,
    MemoryFaultType, ProcessorState, WatchpointKind,
};
use crate::isa::arm::common::features::ArmFeatures;
use crate::isa::arm::common::memory::ArmMemory;
use crate::isa::arm::common::sysreg::Aarch64SysRegEncoding;
use crate::vm::vcpu::Aarch64SystemRegisters;

impl AArch64Cpu {


    /// Execute SVE LASTA/LASTB/CLASTA/CLASTB to a GPR. `B` (bit16) takes the
    /// last active element; `A` takes the element after it (wrapping). The
    /// conditional (C) forms keep Rdn when no element is active.
    pub(crate) fn exec_sve_lastx(&mut self, insn: u32, esize: usize) -> Result<CpuExit, ArmError> {
        let before = (insn >> 16) & 1 == 1; // B = take the last active element
        let conditional = (insn >> 20) & 1 == 1; // CLAST
        let pg = ((insn >> 10) & 0x7) as usize;
        let zn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as u8;
        let mask = self.sve_p[pg];
        let n = 16 / esize;
        let op = self.v[zn].to_le_bytes();
        let em = elem_mask((esize * 8) as u32);
        let mut last: i32 = -1;
        for e in (0..n).rev() {
            if (mask >> (e * esize)) & 1 == 1 {
                last = e as i32;
                break;
            }
        }
        if conditional && last < 0 {
            self.set_x(rd, self.get_x(rd) & em);
            return Ok(CpuExit::Continue);
        }
        let idx = if before {
            if last < 0 { n - 1 } else { last as usize }
        } else {
            let i = (last + 1) as usize;
            if i >= n { 0 } else { i }
        };
        let res = read_elem(&op, idx * esize, esize) & em;
        self.set_x(rd, res);
        Ok(CpuExit::Continue)
    }



    /// Execute SVE LASTA/LASTB to a SIMD&FP scalar. `B` (bit16) takes the last
    /// active element; `A` takes the element after it, wrapping.
    pub(crate) fn exec_sve_last_scalar(&mut self, insn: u32, esize: usize) -> Result<CpuExit, ArmError> {
        let before = (insn >> 16) & 1 == 1;
        let pg = ((insn >> 10) & 0x7) as usize;
        let zn = ((insn >> 5) & 0x1F) as usize;
        let vd = (insn & 0x1F) as usize;
        let mask = self.sve_p[pg];
        let n = 16 / esize;
        let op = self.v[zn].to_le_bytes();
        let em = elem_mask((esize * 8) as u32);
        let mut last: i32 = -1;
        for e in (0..n).rev() {
            if (mask >> (e * esize)) & 1 == 1 {
                last = e as i32;
                break;
            }
        }
        let idx = if before {
            if last < 0 { n - 1 } else { last as usize }
        } else {
            let i = (last + 1) as usize;
            if i >= n { 0 } else { i }
        };
        let res = read_elem(&op, idx * esize, esize) & em;
        self.v[vd] = res as u128;
        Ok(CpuExit::Continue)
    }



    /// Execute SVE CPY/MOV (predicated copy). `mode`: 0=immediate (Pg=4-bit,
    /// merging or zeroing), 1=scalar GPR (Rn, SP if 31, merging), 2=SIMD scalar
    /// Vn (merging). Pg is byte-granular.
    pub(crate) fn exec_sve_cpy(&mut self, insn: u32, esize: usize, mode: u32) -> Result<CpuExit, ArmError> {
        let zd = (insn & 0x1F) as usize;
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let elements = 16 / esize;
        let (pg, merging, elem_val) = match mode {
            0 => {
                // LSL #8 (sh=1) is undefined for byte elements.
                if esize == 1 && (insn >> 13) & 1 == 1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let pg = ((insn >> 16) & 0xF) as usize; // 4-bit predicate
                let merging = (insn >> 14) & 1 == 1;
                let imm8 = ((insn >> 5) & 0xFF) as u8 as i8 as i64;
                let imm = if (insn >> 13) & 1 == 1 {
                    imm8 << 8
                } else {
                    imm8
                };
                (pg, merging, (imm as u64) & mask)
            }
            1 => {
                let pg = ((insn >> 10) & 0x7) as usize;
                let rn = ((insn >> 5) & 0x1F) as u8;
                let v = if rn == 31 {
                    self.current_sp()
                } else {
                    self.get_x(rn)
                };
                (pg, true, v & mask)
            }
            _ => {
                let pg = ((insn >> 10) & 0x7) as usize;
                let vn = ((insn >> 5) & 0x1F) as usize;
                (pg, true, (self.v[vn] as u64) & mask)
            }
        };
        let pred = self.sve_p[pg];
        let mut dst = if merging {
            self.v[zd].to_le_bytes()
        } else {
            [0u8; 16]
        };
        for e in 0..elements {
            if (pred >> (e * esize)) & 1 == 1 {
                write_elem(&mut dst, e * esize, esize, elem_val);
            }
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }



    /// Execute SVE INDEX: Zd[e] = base + e*step, with base/step from either a
    /// signed 5-bit immediate or an X register. bits[11:10]: bit10 picks the
    /// base source (0=imm5 at [9:5], 1=Xn), bit11 the step source (0=imm5 at
    /// [20:16], 1=Xm).
    pub(crate) fn exec_sve_index(&mut self, insn: u32, zd: usize, esize: usize) -> Result<CpuExit, ArmError> {
        let sext5 = |v: u32| -> i64 { (((v & 0x1F) as i32) << 27 >> 27) as i64 };
        let base: i64 = if (insn >> 10) & 1 == 1 {
            self.get_x(((insn >> 5) & 0x1F) as u8) as i64
        } else {
            sext5((insn >> 5) & 0x1F)
        };
        let step: i64 = if (insn >> 11) & 1 == 1 {
            self.get_x(((insn >> 16) & 0x1F) as u8) as i64
        } else {
            sext5((insn >> 16) & 0x1F)
        };
        let bits = (esize * 8) as u32;
        let m = elem_mask(bits) as u128;
        let elements = 16 / esize;
        let mut dst = 0u128;
        for e in 0..elements {
            let v = base.wrapping_add((e as i64).wrapping_mul(step)) as u64 as u128 & m;
            dst |= v << (e * esize * 8);
        }
        self.v[zd] = dst;
        Ok(CpuExit::Continue)
    }



    /// Execute the SVE element-count / inc-dec-by-element-count / stack-
    /// allocation family (all 0x04): ADDVL/ADDPL/RDVL, CNTB/H/W/D, INCB/DECB...
    /// to a GPR or Z register, and the saturating SQINCB/UQINCB.../SQDECB...
    /// forms. The pattern selects how many elements of size esz the predicate
    /// would have, scaled by MUL #(imm4+1).
    pub(crate) fn exec_sve_elem_count(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let rd = (insn & 0x1F) as u8;
        // Stack-allocation forms (ADDVL/ADDPL/RDVL): bits[15:11]==01010.
        // ADDVL/ADDPL use the stack-pointer destination encoding; RDVL writes an
        // X register, so rd==31 is XZR and the write is discarded.
        if (insn >> 11) & 0x1F == 0b01010 {
            let imm6 = (((insn >> 5) & 0x3F) as i64) << 58 >> 58; // sign-extend 6
            let vl_bytes = (self.sve_vl / 8) as i64;
            let rn = ((insn >> 16) & 0x1F) as u8;
            let base = if rn == 31 {
                self.get_sp()
            } else {
                self.get_x(rn)
            };
            let op = (insn >> 21) & 0x7;
            let val = match op {
                0b001 => base.wrapping_add((imm6 * vl_bytes) as u64), // ADDVL
                0b011 => base.wrapping_add((imm6 * (vl_bytes / 8)) as u64), // ADDPL
                0b101 => (imm6 * vl_bytes) as u64,                    // RDVL (rn==31)
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            if rd == 31 && op != 0b101 {
                self.set_sp(val);
            } else {
                self.set_x(rd, val);
            }
            return Ok(CpuExit::Continue);
        }
        // Element-count forms: count = pattern_count(pat, esz) * (imm4 + 1).
        let esz = (insn >> 22) & 0x3;
        let esize_bits = 8u32 << esz; // 8/16/32/64
        let elements = (self.sve_vl as usize) / esize_bits as usize;
        let pattern = (insn >> 5) & 0x1F;
        let mul = (((insn >> 16) & 0xF) + 1) as u64;
        let count = sve_pattern_count(pattern, elements) as u64 * mul;
        match (insn >> 12) & 0xF {
            0b1110 => {
                if (insn >> 20) & 0x3 == 0b10 {
                    // CNT_r: Rd = count.
                    self.set_x(rd, count);
                } else {
                    // INCDEC_r: Rd = Xd +/- count (64-bit wrapping). d = bit10.
                    let cur = self.get_x(rd);
                    let v = if (insn >> 10) & 1 == 1 {
                        cur.wrapping_sub(count)
                    } else {
                        cur.wrapping_add(count)
                    };
                    self.set_x(rd, v);
                }
                Ok(CpuExit::Continue)
            }
            0b1111 => {
                // SINCDEC_r: saturating GPR. bits[21:20]==10 => 32-bit, 11 => 64.
                let sf64 = (insn >> 20) & 0x3 == 0b11;
                let dec = (insn >> 11) & 1 == 1;
                let uns = (insn >> 10) & 1 == 1;
                let cur = self.get_x(rd);
                let res = if sf64 {
                    sat_addsub_64(cur, count, uns, dec)
                } else {
                    sat_addsub_32(cur, count, uns, dec)
                };
                self.set_x(rd, res);
                Ok(CpuExit::Continue)
            }
            0b1100 => {
                // Vector inc/dec. Byte elements (esz==0) are unallocated.
                if esize_bits == 8 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let esize = esize_bits as usize / 8;
                let a = self.v[rd as usize].to_le_bytes();
                let mut dst = a;
                let nlanes = 16 / esize;
                let sat = (insn >> 20) & 0x3 == 0b10; // SINCDEC_v vs INCDEC_v
                let mask = elem_mask(esize_bits);
                for e in 0..nlanes {
                    let off = e * esize;
                    let v = read_elem(&a, off, esize);
                    let r = if sat {
                        let dec = (insn >> 11) & 1 == 1;
                        let uns = (insn >> 10) & 1 == 1;
                        sat_addsub_elem(v, count, esize_bits, uns, dec)
                    } else {
                        let dec = (insn >> 10) & 1 == 1;
                        (if dec {
                            v.wrapping_sub(count)
                        } else {
                            v.wrapping_add(count)
                        }) & mask
                    };
                    write_elem(&mut dst, off, esize, r);
                }
                self.v[rd as usize] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }
            _ => Ok(CpuExit::Undefined(insn)),
        }
    }



    /// Execute SVE ZIP1/ZIP2/UZP1/UZP2/TRN1/TRN2 (unpredicated vector permute).
    /// At VL=128 these match the corresponding NEON permutes over the register.
    pub(crate) fn exec_sve_zip_uzp_trn(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        zm: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        let opc = (insn >> 10) & 0x7;
        // Only six opc values are defined for the SVE unpredicated permute:
        // ZIP1/ZIP2 (000/001), UZP1/UZP2 (010/011), TRN1/TRN2 (100/101). opc
        // 0b110/0b111 are reserved — reject them as UNDEFINED rather than
        // writing a zeroed Zd and reporting success. (#167)
        if opc >= 0b110 {
            return Ok(CpuExit::Undefined(insn));
        }
        let n = 16 / esize;
        let half = n / 2;
        let a = self.v[zn].to_le_bytes();
        let b = self.v[zm].to_le_bytes();
        let mut dst = [0u8; 16];
        let get = |buf: &[u8; 16], i: usize| read_elem(buf, i * esize, esize);
        for i in 0..half {
            let (lo, hi): (u64, u64) = match opc {
                0b000 => (get(&a, i), get(&b, i)),                 // ZIP1
                0b001 => (get(&a, half + i), get(&b, half + i)),   // ZIP2
                0b100 => (get(&a, 2 * i), get(&b, 2 * i)),         // TRN1
                0b101 => (get(&a, 2 * i + 1), get(&b, 2 * i + 1)), // TRN2
                _ => (0, 0),
            };
            match opc {
                0b000 | 0b001 | 0b100 | 0b101 => {
                    write_elem(&mut dst, (2 * i) * esize, esize, lo);
                    write_elem(&mut dst, (2 * i + 1) * esize, esize, hi);
                }
                _ => {}
            }
        }
        if opc == 0b010 || opc == 0b011 {
            // UZP1 (even) / UZP2 (odd): concatenated even/odd elements of Zn:Zm.
            let off = if opc == 0b011 { 1 } else { 0 };
            for i in 0..n {
                let v = if i < half {
                    get(&a, 2 * i + off)
                } else {
                    get(&b, 2 * (i - half) + off)
                };
                write_elem(&mut dst, i * esize, esize, v);
            }
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }



    /// Execute SVE TBL (table lookup, single source table). For each element e,
    /// `Zd[e] = Zn[Zm[e]]` if the index `Zm[e]` is within range, else 0. The
    /// table Zn is indexed by the unsigned element value of Zm.
    pub(crate) fn exec_sve_tbl(
        &mut self,
        zd: usize,
        zn: usize,
        zm: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        let elements = 16 / esize;
        let table = self.v[zn].to_le_bytes();
        let idxs = self.v[zm].to_le_bytes();
        let mut dst = [0u8; 16];
        for e in 0..elements {
            let idx = read_elem(&idxs, e * esize, esize) as usize;
            if idx < elements {
                let val = read_elem(&table, idx * esize, esize);
                write_elem(&mut dst, e * esize, esize, val);
            }
            // Out-of-range index leaves the destination element as 0.
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }



    /// Execute SVE TBX (table lookup with destination preservation). Like TBL,
    /// but an out-of-range index keeps the prior value of the destination
    /// element rather than zeroing it (so Zd is both source and destination).
    pub(crate) fn exec_sve_tbx(
        &mut self,
        zd: usize,
        zn: usize,
        zm: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        let elements = 16 / esize;
        let table = self.v[zn].to_le_bytes();
        let idxs = self.v[zm].to_le_bytes();
        let mut dst = self.v[zd].to_le_bytes(); // preserve existing Zd
        for e in 0..elements {
            let idx = read_elem(&idxs, e * esize, esize) as usize;
            if idx < elements {
                let val = read_elem(&table, idx * esize, esize);
                write_elem(&mut dst, e * esize, esize, val);
            }
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }



    /// Execute SVE DUP (indexed): broadcast element `index` of Zn to every lane
    /// of Zd. The esize and index are encoded in tsz:imm2 — the lowest set bit
    /// of tsz selects esize (8<<n bits), the remaining high bits give the index.
    /// An index past the end of the register broadcasts zero.
    pub(crate) fn exec_sve_dup_indexed(
        &mut self,
        insn: u32,
        zn: usize,
        zd: usize,
    ) -> Result<CpuExit, ArmError> {
        let imm2 = (insn >> 22) & 0x3;
        let tsz = (insn >> 16) & 0x1F;
        if tsz == 0 {
            return Ok(CpuExit::Undefined(insn));
        }
        let imm = (imm2 << 5) | tsz; // 7-bit imm2:tsz
        let tz = tsz.trailing_zeros(); // 0..=4
        let esize = 1usize << tz; // bytes: 1,2,4,8,16
        let index = (imm >> (tz + 1)) as usize;
        if esize == 16 {
            // Quadword element (VL=128 -> a single element): index 0 selects the
            // whole register, anything beyond broadcasts zero.
            self.v[zd] = if index == 0 { self.v[zn] } else { 0 };
            return Ok(CpuExit::Continue);
        }
        let elements = 16 / esize;
        let src = self.v[zn].to_le_bytes();
        let element = if index >= elements {
            0u64
        } else {
            read_elem(&src, index * esize, esize)
        };
        let mut dst = [0u8; 16];
        for e in 0..elements {
            write_elem(&mut dst, e * esize, esize, element);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }



    /// Execute SVE COMPACT: pack the active (per Pg) elements of Zn contiguously
    /// into the low elements of Zd, zeroing the remaining high elements. Only
    /// 32-bit (S) and 64-bit (D) element sizes are defined (esize = 32 << sz).
    pub(crate) fn exec_sve_compact(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        pg: usize,
    ) -> Result<CpuExit, ArmError> {
        let esize = 4usize << ((insn >> 22) & 1); // bytes: 4 (S) or 8 (D)
        let elements = 16 / esize;
        let pred = self.sve_p[pg];
        let src = self.v[zn].to_le_bytes();
        let mut dst = [0u8; 16];
        let mut x = 0;
        for e in 0..elements {
            if (pred >> (e * esize)) & 1 == 1 {
                let val = read_elem(&src, e * esize, esize);
                write_elem(&mut dst, x * esize, esize, val);
                x += 1;
            }
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }



    /// Execute SVE SPLICE (destructive): copy the elements of Zdn spanning from
    /// the first to the last active element (inclusive, regardless of the
    /// predicate value of elements in between) into the low part of the result,
    /// then fill the remaining elements from the low elements of Zm. With no
    /// active element the result is Zm unchanged. `zd`=Zdn, `zn`=Zm.
    pub(crate) fn exec_sve_splice(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        pg: usize,
    ) -> Result<CpuExit, ArmError> {
        self.exec_sve_splice_sources(insn, zd, zd, zn, pg)
    }



    /// Execute SVE2 SPLICE (constructive): source 1 is Zn and source 2 is the
    /// next architectural vector register modulo 32.
    pub(crate) fn exec_sve_splice_pair(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        pg: usize,
    ) -> Result<CpuExit, ArmError> {
        self.exec_sve_splice_sources(insn, zd, zn, (zn + 1) & 31, pg)
    }



    pub(crate) fn exec_sve_splice_sources(
        &mut self,
        insn: u32,
        zd: usize,
        s1: usize,
        s2: usize,
        pg: usize,
    ) -> Result<CpuExit, ArmError> {
        let esize = 1usize << ((insn >> 22) & 0x3); // bytes
        let elements = 16 / esize;
        let pred = self.sve_p[pg];
        let op1 = self.v[s1].to_le_bytes();
        let op2 = self.v[s2].to_le_bytes();
        let mut dst = [0u8; 16];
        let mut x = 0usize;
        let mut lastnum: i32 = -1;
        for e in 0..elements {
            if (pred >> (e * esize)) & 1 == 1 {
                lastnum = e as i32;
            }
        }
        if lastnum >= 0 {
            let mut active = false;
            for e in 0..=(lastnum as usize) {
                if (pred >> (e * esize)) & 1 == 1 {
                    active = true;
                }
                if active {
                    let val = read_elem(&op1, e * esize, esize);
                    write_elem(&mut dst, x * esize, esize, val);
                    x += 1;
                }
            }
        }
        // Fill the remaining (elements - x) destination slots from Zm's low part.
        for e in 0..(elements - x) {
            let val = read_elem(&op2, e * esize, esize);
            write_elem(&mut dst, x * esize, esize, val);
            x += 1;
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }



    /// Execute SVE permute operations (DUP, INDEX, REV, etc.).
    pub(crate) fn exec_sve_permute(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        zm: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        let op1 = (insn >> 23) & 0x3;
        let op3 = (insn >> 10) & 0x3F;

        match op1 {
            // DUP (scalar register): broadcast Xn/SP to all elements.
            0b10 | 0b11 if (insn >> 16) & 0x3F == 0b100000 && op3 == 0b001110 => {
                let rn = ((insn >> 5) & 0x1F) as u8;
                let val = self.gpr_or_sp(rn);
                let elements = 16 / esize;

                let mut dst = [0u8; 16];
                for e in 0..elements {
                    let offset = e * esize;
                    match esize {
                        1 => dst[offset] = val as u8,
                        2 => {
                            let bytes = (val as u16).to_le_bytes();
                            dst[offset..offset + 2].copy_from_slice(&bytes);
                        }
                        4 => {
                            let bytes = (val as u32).to_le_bytes();
                            dst[offset..offset + 4].copy_from_slice(&bytes);
                        }
                        8 => {
                            let bytes = val.to_le_bytes();
                            dst[offset..offset + 8].copy_from_slice(&bytes);
                        }
                        _ => {}
                    }
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // INDEX
            0b11 if (insn >> 17) & 0xF == 0 => {
                let rn = ((insn >> 5) & 0x1F) as u8;
                let rm = ((insn >> 16) & 0x1F) as u8;
                let elements = 16 / esize;

                let start = self.get_x(rn) as i64;
                let incr = self.get_x(rm) as i64;

                let mut dst = [0u8; 16];
                for e in 0..elements {
                    let val = start.wrapping_add((e as i64).wrapping_mul(incr));
                    let offset = e * esize;
                    match esize {
                        1 => dst[offset] = val as u8,
                        2 => {
                            let bytes = (val as u16).to_le_bytes();
                            dst[offset..offset + 2].copy_from_slice(&bytes);
                        }
                        4 => {
                            let bytes = (val as u32).to_le_bytes();
                            dst[offset..offset + 4].copy_from_slice(&bytes);
                        }
                        8 => {
                            let bytes = (val as u64).to_le_bytes();
                            dst[offset..offset + 8].copy_from_slice(&bytes);
                        }
                        _ => {}
                    }
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // ZIP/UZP/TRN
            0b10 if (op3 & 0x30) == 0x00 => {
                let opc = (insn >> 10) & 0x7;
                let elements = 16 / esize;
                let src1 = self.v[zn].to_le_bytes();
                let src2 = self.v[zm].to_le_bytes();
                let mut dst = [0u8; 16];

                match opc {
                    // ZIP1 - interleave lower halves
                    0b000 => {
                        let half = elements / 2;
                        for e in 0..half {
                            for b in 0..esize {
                                dst[e * 2 * esize + b] = src1[e * esize + b];
                                dst[(e * 2 + 1) * esize + b] = src2[e * esize + b];
                            }
                        }
                    }
                    // ZIP2 - interleave upper halves
                    0b001 => {
                        let half = elements / 2;
                        for e in 0..half {
                            let src_off = (half + e) * esize;
                            for b in 0..esize {
                                dst[e * 2 * esize + b] = src1[src_off + b];
                                dst[(e * 2 + 1) * esize + b] = src2[src_off + b];
                            }
                        }
                    }
                    // UZP1 - even elements
                    0b010 => {
                        let half = elements / 2;
                        for e in 0..half {
                            for b in 0..esize {
                                dst[e * esize + b] = src1[e * 2 * esize + b];
                                dst[(half + e) * esize + b] = src2[e * 2 * esize + b];
                            }
                        }
                    }
                    // UZP2 - odd elements
                    0b011 => {
                        let half = elements / 2;
                        for e in 0..half {
                            for b in 0..esize {
                                dst[e * esize + b] = src1[(e * 2 + 1) * esize + b];
                                dst[(half + e) * esize + b] = src2[(e * 2 + 1) * esize + b];
                            }
                        }
                    }
                    // TRN1 - transpose even elements
                    0b100 => {
                        for e in 0..elements / 2 {
                            for b in 0..esize {
                                dst[e * 2 * esize + b] = src1[e * 2 * esize + b];
                                dst[(e * 2 + 1) * esize + b] = src2[e * 2 * esize + b];
                            }
                        }
                    }
                    // TRN2 - transpose odd elements
                    0b101 => {
                        for e in 0..elements / 2 {
                            for b in 0..esize {
                                dst[e * 2 * esize + b] = src1[(e * 2 + 1) * esize + b];
                                dst[(e * 2 + 1) * esize + b] = src2[(e * 2 + 1) * esize + b];
                            }
                        }
                    }
                    _ => {
                        return Err(ArmError::Unimplemented(format!(
                            "SVE ZIP/UZP/TRN opc={}",
                            opc
                        )));
                    }
                }

                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // REV
            0b10 if (op3 & 0x38) == 0x18 => {
                let elements = 16 / esize;
                let src = self.v[zn].to_le_bytes();
                let mut dst = [0u8; 16];

                for e in 0..elements {
                    let src_e = elements - 1 - e;
                    for b in 0..esize {
                        dst[e * esize + b] = src[src_e * esize + b];
                    }
                }

                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // RDVL - read vector length
            0b11 if (insn >> 17) & 0x1F == 0x1F && (op3 & 0x3E) == 0x10 => {
                let rd = (insn & 0x1F) as u8;
                let imm6 = ((insn >> 5) & 0x3F) as i64;
                let imm = if imm6 & 0x20 != 0 { imm6 | !0x3F } else { imm6 };
                // VL in bytes
                let vl_bytes = (self.sve_vl / 8) as i64;
                let result = (vl_bytes * imm) as u64;
                self.set_x(rd, result);
                Ok(CpuExit::Continue)
            }

            // CNTx - count elements
            0b11 if (insn >> 17) & 0x18 == 0x10 => {
                let rd = (insn & 0x1F) as u8;
                let opc = (insn >> 16) & 0x7;
                let pattern = (insn >> 5) & 0x1F;
                let imm4 = ((insn >> 16) & 0xF) as u64;

                let esize_bits = match opc {
                    0b000 => 8,  // CNTB
                    0b001 => 16, // CNTH
                    0b010 => 32, // CNTW
                    0b011 => 64, // CNTD
                    _ => 8,
                };

                let elements = (self.sve_vl as u64) / esize_bits;
                let count = match pattern {
                    0b11111 => elements, // ALL
                    _ => elements,
                };

                self.set_x(rd, count * imm4.max(1));
                Ok(CpuExit::Continue)
            }

            _ => Err(ArmError::Unimplemented(format!(
                "SVE permute op1={:02b} op3={:06b}",
                op1, op3
            ))),
        }
    }
}
