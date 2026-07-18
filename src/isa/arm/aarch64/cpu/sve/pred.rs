//! pred.rs

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

    /// Write SVE predicate register `i`. Exposed for the differential harness.
    pub fn set_sve_pred(&mut self, i: usize, v: u32) {
        self.sve_p[i] = v;
    }



    /// Execute SVE integer predicated operations.
    pub(crate) fn exec_sve_int_pred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        _zm: usize,
        pg: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        // SVE predicated integer ALU (destructive): Zdn = op(Zdn, Zm) for active
        // elements, Zdn unchanged for inactive. The governing predicate Pg is
        // BYTE-granular (element e of `esize` bytes is active iff bit e*esize is
        // set). The op is (group=bits[21:19], opc=bits[18:16]):
        //   000: 000 ADD,  001 SUB,  011 SUBR
        //   001: 000 SMAX, 001 UMAX, 010 SMIN, 011 UMIN, 100 SABD, 101 UABD
        //   010: 000 MUL,  010 SMULH,011 UMULH,100 SDIV, 101 UDIV, 110 SDIVR, 111 UDIVR
        //   011: 000 ORR,  001 EOR,  010 AND,  011 BIC
        // The predicated ALU group has bits[15:13]==000; other values (shifts
        // =100, etc.) are handled by dedicated dispatch arms.
        if (insn >> 13) & 0x7 != 0b000 {
            return Ok(CpuExit::Undefined(insn));
        }
        let group = (insn >> 19) & 0x7;
        // SDIV/UDIV/SDIVR/UDIVR only exist for word and doubleword elements;
        // byte/halfword encodings are unallocated regardless of the predicate.
        if group == 0b010 && (insn >> 18) & 1 == 1 && esize < 4 {
            return Ok(CpuExit::Undefined(insn));
        }
        let opc = (insn >> 16) & 0x7;
        if !matches!(
            (group, opc),
            (0b000, 0b000 | 0b001 | 0b011)
                | (0b001, 0b000..=0b101)
                | (0b010, 0b000 | 0b010 | 0b011 | 0b100 | 0b101 | 0b110 | 0b111)
                | (0b011, 0b000..=0b011)
        ) {
            return Ok(CpuExit::Undefined(insn));
        }
        let pred = self.sve_p[pg];
        let elements = 16 / esize;
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let a_reg = self.v[zd].to_le_bytes(); // Zdn (first source, also dest)
        let b_reg = self.v[zn].to_le_bytes(); // Zm (second source)
        let mut dst = a_reg;
        // Signed divide over the (sign-extended) element values. Division by
        // zero yields 0; the MIN/-1 case never overflows i128 for esize<=64 and
        // the subsequent element mask wraps it to the architectural result.
        let sdiv = |n: i128, d: i128| -> i128 { if d == 0 { 0 } else { n / d } };
        for e in 0..elements {
            if (pred >> (e * esize)) & 1 == 0 {
                continue;
            }
            let off = e * esize;
            let a = read_elem(&a_reg, off, esize);
            let b = read_elem(&b_reg, off, esize);
            let sa = sext_elem(a, bits);
            let sb = sext_elem(b, bits);
            let ua = uext_elem(a, bits);
            let ub = uext_elem(b, bits);
            let r = match (group, opc) {
                (0b000, 0b000) => a.wrapping_add(b),
                (0b000, 0b001) => a.wrapping_sub(b),
                (0b000, 0b011) => b.wrapping_sub(a),
                (0b001, 0b000) => {
                    if sa > sb {
                        a
                    } else {
                        b
                    }
                }
                (0b001, 0b001) => {
                    if ua > ub {
                        a
                    } else {
                        b
                    }
                }
                (0b001, 0b010) => {
                    if sa < sb {
                        a
                    } else {
                        b
                    }
                }
                (0b001, 0b011) => {
                    if ua < ub {
                        a
                    } else {
                        b
                    }
                }
                (0b001, 0b100) => (sa - sb).unsigned_abs() as u64,
                (0b001, 0b101) => (if ua > ub { ua - ub } else { ub - ua }) as u64,
                (0b010, 0b000) => a.wrapping_mul(b),
                (0b010, 0b010) => ((sa * sb) >> bits) as u64,
                (0b010, 0b011) => ((ua * ub) >> bits) as u64,
                (0b010, 0b100) if esize >= 4 => sdiv(sa, sb) as u64,
                (0b010, 0b101) if esize >= 4 => (if ub == 0 { 0 } else { ua / ub }) as u64,
                (0b010, 0b110) if esize >= 4 => sdiv(sb, sa) as u64,
                (0b010, 0b111) if esize >= 4 => (if ua == 0 { 0 } else { ub / ua }) as u64,
                (0b011, 0b000) => a | b,
                (0b011, 0b001) => a ^ b,
                (0b011, 0b010) => a & b,
                (0b011, 0b011) => a & !b,
                _ => return Ok(CpuExit::Undefined(insn)),
            } & mask;
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }



    /// Execute SVE predicate-generating operations (PTRUE/PTRUES, PFALSE, the
    /// WHILE family). Predicates are stored BYTE-granular: element `e` (size
    /// `esize` bytes) is governed by bit `e*esize`, matching the architecture
    /// and the differential oracle. The dispatch keys on the real opcode bits
    /// (NOT on op1=bits[24:23], which folds the size field's high bit).
    pub(crate) fn exec_sve_pred_ops(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let size = (insn >> 22) & 0x3;
        let esize = 1usize << size;
        let elements = 16 / esize;
        let pd = (insn & 0xF) as usize;
        let b15_10 = (insn >> 10) & 0x3F;

        // First-fault register (FFR) manipulation — handled first since these
        // are fully-fixed encodings. SETFFR sets every PL bit (16 at VL=128).
        if insn == 0x252C_9000 {
            self.sve_ffr = 0xFFFF;
            return Ok(CpuExit::Continue);
        }
        // WRFFR Pn: FFR = P[Pn].
        if insn & 0xFFFF_FE1F == 0x2528_9000 {
            self.sve_ffr = self.sve_p[((insn >> 5) & 0xF) as usize];
            return Ok(CpuExit::Continue);
        }
        // RDFFR Pd (unpredicated): P[Pd] = FFR. Requires top byte 0x25 (bit24==1);
        // the 0x24 family is a distinct (unallocated here) encoding space.
        if (insn >> 24) & 1 == 1 && (insn >> 10) & 0x3FFF == 0x67C && (insn >> 4) & 0x3F == 0 {
            self.sve_p[pd] = self.sve_ffr;
            return Ok(CpuExit::Continue);
        }
        // RDFFR/RDFFRS Pd, Pg/Z (predicated): P[Pd] = FFR & P[Pg] (zeroing). The
        // S-bit form (bit22, RDFFRS) also sets NZCV = PredTest(Pg, result). The
        // mask ignores bit22 (==bit12 of the shifted field). Requires bit24==1.
        if (insn >> 24) & 1 == 1
            && (insn >> 10) & 0x2FFF == 0x63C
            && (insn >> 9) & 1 == 0
            && (insn >> 4) & 1 == 0
        {
            let pgl = ((insn >> 5) & 0xF) as usize;
            let r = self.sve_ffr & self.sve_p[pgl];
            self.sve_p[pd] = r;
            if (insn >> 22) & 1 == 1 {
                let (n, z, c, v) = pred_test(self.sve_p[pgl], r, 16, 1);
                self.set_n(n);
                self.set_z(z);
                self.set_c(c);
                self.set_v(v);
            }
            return Ok(CpuExit::Continue);
        }

        // PTEST Pg, Pn.B: NZCV = PredTest(Pg, Pn) at byte granularity (the
        // predicate result is unchanged; only flags are written). Encoding
        // 00100101 01 010000 11 pg 0 rn 0 0000.
        if insn & 0xFFFF_C21F == 0x2550_C000 {
            let pgl = ((insn >> 10) & 0xF) as usize;
            let pn = ((insn >> 5) & 0xF) as usize;
            let (n, z, c, v) = pred_test(self.sve_p[pgl], self.sve_p[pn], 16, 1);
            self.set_n(n);
            self.set_z(z);
            self.set_c(c);
            self.set_v(v);
            return Ok(CpuExit::Continue);
        }

        // FDUP: broadcast an FP modified-immediate to all lanes. 0x25,
        // bits[21:13]==111001110. Unpredicated; size 0 reserved.
        if (insn >> 24) & 0xFF == 0b00100101 && (insn >> 13) & 0x1FF == 0b111001110 {
            if esize < 2 {
                return Ok(CpuExit::Undefined(insn));
            }
            let zd = (insn & 0x1F) as usize;
            let val = vfp_expand_imm(((insn >> 5) & 0xFF) as u8, esize);
            let mut out = 0u128;
            for e in 0..elements {
                out |= (val as u128) << (e * esize * 8);
            }
            self.v[zd] = out;
            return Ok(CpuExit::Continue);
        }

        // SVE integer compare with signed immediate -> predicate: 0x25,
        // bit21==0, condition (bits[15:13], bit4): GE/GT/LT/LE/EQ/NE. imm5 is
        // signed (bits[20:16]). Zeroing under Pg; sets NZCV via PredTest(Pg).
        if (insn >> 24) & 0xFF == 0b00100101
            && (insn >> 21) & 1 == 0
            && matches!(
                ((insn >> 13) & 0x7, (insn >> 4) & 1),
                (0b000, 0) | (0b000, 1) | (0b001, 0) | (0b001, 1) | (0b100, 0) | (0b100, 1)
            )
        {
            let cc = ((insn >> 13) & 0x7, (insn >> 4) & 1);
            let imm = (((insn >> 16) & 0x1F) as i64) << 59 >> 59; // sign-extend imm5
            let pgi = ((insn >> 10) & 0x7) as usize;
            let bits = (esize * 8) as u32;
            let pred = self.sve_p[pgi];
            let n = self.v[((insn >> 5) & 0x1F) as usize].to_le_bytes();
            let mut result = 0u32;
            for e in 0..elements {
                let off = e * esize;
                if (pred >> off) & 1 == 0 {
                    continue;
                }
                let a = sext_elem(read_elem(&n, off, esize), bits) as i64;
                let r = match cc {
                    (0b000, 0) => a >= imm,
                    (0b000, 1) => a > imm,
                    (0b001, 0) => a < imm,
                    (0b001, 1) => a <= imm,
                    (0b100, 0) => a == imm,
                    _ => a != imm,
                };
                if r {
                    result |= 1 << off;
                }
            }
            self.sve_p[pd] = result;
            let (nf, zf, cf, vf) = pred_test(pred, result, elements, esize);
            self.set_n(nf);
            self.set_z(zf);
            self.set_c(cf);
            self.set_v(vf);
            return Ok(CpuExit::Continue);
        }

        // SVE integer compare with unsigned immediate -> predicate: 0x24,
        // bit21==1. imm7=bits[20:14] (unsigned); condition (bit13, bit4):
        // HS/HI/LO/LS. Zeroing under Pg; sets NZCV via PredTest(Pg).
        if (insn >> 24) & 0xFF == 0b00100100 && (insn >> 21) & 1 == 1 {
            let imm = ((insn >> 14) & 0x7F) as u64;
            let lohi = ((insn >> 13) & 1, (insn >> 4) & 1);
            let pgi = ((insn >> 10) & 0x7) as usize;
            let bits = (esize * 8) as u32;
            let pred = self.sve_p[pgi];
            let n = self.v[((insn >> 5) & 0x1F) as usize].to_le_bytes();
            let mut result = 0u32;
            for e in 0..elements {
                let off = e * esize;
                if (pred >> off) & 1 == 0 {
                    continue;
                }
                let a = uext_elem(read_elem(&n, off, esize), bits) as u64;
                let r = match lohi {
                    (0, 0) => a >= imm,
                    (0, 1) => a > imm,
                    (1, 0) => a < imm,
                    _ => a <= imm,
                };
                if r {
                    result |= 1 << off;
                }
            }
            self.sve_p[pd] = result;
            let (nf, zf, cf, vf) = pred_test(pred, result, elements, esize);
            self.set_n(nf);
            self.set_z(zf);
            self.set_c(cf);
            self.set_v(vf);
            return Ok(CpuExit::Continue);
        }

        // ADD/SUB/SUBR Zd.T, Zd.T, #imm{,LSL #8}: unpredicated destructive
        // integer arithmetic with an unsigned 8-bit immediate. op=2 is
        // reserved; byte elements cannot use the shifted form.
        if (insn >> 24) & 0xFF == 0b00100101
            && (insn >> 18) & 0xF == 0b1000
            && (insn >> 14) & 0x3 == 0b11
        {
            let op = (insn >> 16) & 0x3;
            if op == 0b10 || (esize == 1 && (insn >> 13) & 1 == 1) {
                return Ok(CpuExit::Undefined(insn));
            }
            let zd = (insn & 0x1F) as usize;
            let imm8 = ((insn >> 5) & 0xFF) as u64;
            let imm = if (insn >> 13) & 1 == 1 {
                imm8 << 8
            } else {
                imm8
            };
            let bits = (esize * 8) as u32;
            let mask = elem_mask(bits);
            let src = self.v[zd].to_le_bytes();
            let mut dst = [0u8; 16];
            for e in 0..elements {
                let off = e * esize;
                let a = read_elem(&src, off, esize) & mask;
                let r = match op {
                    0b00 => a.wrapping_add(imm),
                    0b01 => a.wrapping_sub(imm),
                    _ => imm.wrapping_sub(a),
                } & mask;
                write_elem(&mut dst, off, esize, r);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // SQADD/UQADD/SQSUB/UQSUB Zd.T, Zd.T, #imm{,LSL #8}: unpredicated
        // destructive saturating integer arithmetic with an unsigned immediate.
        if (insn >> 24) & 0xFF == 0b00100101
            && matches!(
                (insn >> 16) & 0x3F,
                0b100100 | 0b100101 | 0b100110 | 0b100111
            )
            && (insn >> 14) & 0x3 == 0b11
        {
            if esize == 1 && (insn >> 13) & 1 == 1 {
                return Ok(CpuExit::Undefined(insn));
            }
            let op = (insn >> 16) & 0x3F;
            let zd = (insn & 0x1F) as usize;
            let bits = (esize * 8) as u32;
            let imm8 = ((insn >> 5) & 0xFF) as u64;
            let imm = if (insn >> 13) & 1 == 1 {
                imm8 << 8
            } else {
                imm8
            };
            let src = self.v[zd].to_le_bytes();
            let mut dst = [0u8; 16];
            let unsigned = matches!(op, 0b100101 | 0b100111);
            let subtract = matches!(op, 0b100110 | 0b100111);
            for e in 0..elements {
                let off = e * esize;
                let a = read_elem(&src, off, esize);
                let r = sat_addsub_elem(a, imm, bits, unsigned, subtract);
                write_elem(&mut dst, off, esize, r);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // SMAX/UMAX/SMIN/UMIN Zd.T, Zd.T, #imm: unpredicated destructive
        // min/max against an 8-bit signed or unsigned immediate.
        if (insn >> 24) & 0xFF == 0b00100101
            && matches!(
                (insn >> 16) & 0x3F,
                0b101000 | 0b101001 | 0b101010 | 0b101011
            )
            && (insn >> 13) & 0x7 == 0b110
        {
            let op = (insn >> 16) & 0x3F;
            let zd = (insn & 0x1F) as usize;
            let bits = (esize * 8) as u32;
            let mask = elem_mask(bits);
            let imm8 = ((insn >> 5) & 0xFF) as u8;
            let src = self.v[zd].to_le_bytes();
            let mut dst = [0u8; 16];
            for e in 0..elements {
                let off = e * esize;
                let a = read_elem(&src, off, esize);
                let r = match op {
                    0b101000 => (sext_elem(a, bits).max(imm8 as i8 as i128) as u64) & mask,
                    0b101001 => (uext_elem(a, bits).max(imm8 as u128) as u64) & mask,
                    0b101010 => (sext_elem(a, bits).min(imm8 as i8 as i128) as u64) & mask,
                    0b101011 => (uext_elem(a, bits).min(imm8 as u128) as u64) & mask,
                    _ => unreachable!(),
                };
                write_elem(&mut dst, off, esize, r);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // MUL Zd.T, Zd.T, #imm: unpredicated destructive integer multiply with
        // a signed 8-bit immediate, sign-extended to the element width.
        if (insn >> 24) & 0xFF == 0b00100101
            && (insn >> 16) & 0x3F == 0b110000
            && (insn >> 13) & 0x7 == 0b110
        {
            let zd = (insn & 0x1F) as usize;
            let bits = (esize * 8) as u32;
            let mask = elem_mask(bits);
            let imm = (((insn >> 5) & 0xFF) as u8 as i8 as i64 as u64) & mask;
            let src = self.v[zd].to_le_bytes();
            let mut dst = [0u8; 16];
            for e in 0..elements {
                let off = e * esize;
                let a = read_elem(&src, off, esize) & mask;
                write_elem(&mut dst, off, esize, a.wrapping_mul(imm) & mask);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // DUP Zd.T, #imm{,LSL #8} (unpredicated immediate broadcast): bits[21:16]
        // ==111000, bits[15:14]==11. (Distinct from PTRUE by bit21==1.)
        if (insn >> 16) & 0x3F == 0b111000 && (insn >> 14) & 0x3 == 0b11 {
            // LSL #8 (sh=1) is undefined for byte elements.
            if esize == 1 && (insn >> 13) & 1 == 1 {
                return Ok(CpuExit::Undefined(insn));
            }
            let zd = (insn & 0x1F) as usize;
            let imm8 = ((insn >> 5) & 0xFF) as u8 as i8 as i64;
            let imm = if (insn >> 13) & 1 == 1 {
                imm8 << 8
            } else {
                imm8
            };
            let elem_val = (imm as u64) & elem_mask((esize * 8) as u32);
            let mut dst = [0u8; 16];
            for e in 0..(16 / esize) {
                write_elem(&mut dst, e * esize, esize, elem_val);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // CNTP Rd, Pg, Pn.T: count active Pn elements under Pg -> 64-bit GPR.
        // bits[21:16]==100000, bits[15:14]==10.
        if (insn >> 24) & 0xFF == 0b00100101
            && (insn >> 16) & 0x3F == 0b100000
            && (insn >> 14) & 0x3 == 0b10
        {
            let pgl = ((insn >> 10) & 0xF) as usize;
            let pn = ((insn >> 5) & 0xF) as usize;
            let rd = (insn & 0x1F) as u8;
            let (mask, op) = (self.sve_p[pgl], self.sve_p[pn]);
            let mut sum = 0u64;
            for e in 0..(16 / esize) {
                let b = e * esize;
                if (mask >> b) & 1 == 1 && (op >> b) & 1 == 1 {
                    sum += 1;
                }
            }
            self.set_x(rd, sum);
            return Ok(CpuExit::Continue);
        }

        // INCP/DECP: increment/decrement a GPR (R form, bit11==1) or each Z
        // element (Z form, bit11==0) by the active-element count of Pg.
        // bits[21:17]==10110 (bit16: 0=INC, 1=DEC), bits[15:12]==1000.
        if (insn >> 24) & 0xFF == 0b00100101
            && (insn >> 17) & 0x1F == 0b10110
            && (insn >> 12) & 0xF == 0b1000
        {
            let dec = (insn >> 16) & 1 == 1;
            let is_z = (insn >> 11) & 1 == 0;
            let pgl = ((insn >> 5) & 0xF) as usize;
            let dn = (insn & 0x1F) as usize;
            let mask = self.sve_p[pgl];
            let mut count = 0u64;
            for e in 0..(16 / esize) {
                if (mask >> (e * esize)) & 1 == 1 {
                    count += 1;
                }
            }
            if is_z {
                if esize == 1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let a = self.v[dn].to_le_bytes();
                let mut dst = a;
                let em = elem_mask((esize * 8) as u32);
                for e in 0..(16 / esize) {
                    let off = e * esize;
                    let v = read_elem(&a, off, esize);
                    let r = if dec {
                        v.wrapping_sub(count)
                    } else {
                        v.wrapping_add(count)
                    } & em;
                    write_elem(&mut dst, off, esize, r);
                }
                self.v[dn] = u128::from_le_bytes(dst);
            } else {
                let cur = self.get_x(dn as u8);
                self.set_x(
                    dn as u8,
                    if dec {
                        cur.wrapping_sub(count)
                    } else {
                        cur.wrapping_add(count)
                    },
                );
            }
            return Ok(CpuExit::Continue);
        }

        // SQINCP/UQINCP/SQDECP/UQDECP (saturating INCP/DECP): bits[21:18]==1010,
        // d=bit17 (0=inc, 1=dec), u=bit16 (0=signed SQ, 1=unsigned UQ),
        // bits[15:11]==10001 (GPR r-form) / 10000 (vector z-form). For the GPR
        // form bit10 selects 64-bit (1) vs 32-bit (0) saturation width. The
        // count of active Pg elements (at this esz) is the saturating delta.
        if (insn >> 24) & 0xFF == 0b00100101
            && (insn >> 18) & 0xF == 0b1010
            && (insn >> 12) & 0xF == 0b1000
        {
            let dec = (insn >> 17) & 1 == 1;
            let uns = (insn >> 16) & 1 == 1;
            let is_z = (insn >> 11) & 1 == 0;
            let pgl = ((insn >> 5) & 0xF) as usize;
            let dn = (insn & 0x1F) as usize;
            let mask = self.sve_p[pgl];
            let mut count = 0u64;
            for e in 0..(16 / esize) {
                if (mask >> (e * esize)) & 1 == 1 {
                    count += 1;
                }
            }
            if is_z {
                // Vector form: per-element saturating add/sub. esz==0 (byte) is
                // unallocated for the z-form.
                if esize == 1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let a = self.v[dn].to_le_bytes();
                let mut dst = a;
                let bits = (esize * 8) as u32;
                for e in 0..(16 / esize) {
                    let off = e * esize;
                    let v = read_elem(&a, off, esize);
                    let r = sat_addsub_elem(v, count, bits, uns, dec);
                    write_elem(&mut dst, off, esize, r);
                }
                self.v[dn] = u128::from_le_bytes(dst);
            } else {
                // GPR form: bit10 picks 64-bit vs 32-bit saturation width.
                let sf64 = (insn >> 10) & 1 == 1;
                let cur = self.get_x(dn as u8);
                let res = if sf64 {
                    sat_addsub_64(cur, count, uns, dec)
                } else {
                    sat_addsub_32(cur, count, uns, dec)
                };
                self.set_x(dn as u8, res);
            }
            return Ok(CpuExit::Continue);
        }

        // CMP<cc>_P.P.ZZ (bits[31:24]==0x24): predicated vector compare producing
        // a zeroing predicate Pd, then NZCV = PredTest(Pg, result). The compare
        // is (bits[15:13], bit4): (000,0)HS (000,1)HI (100,0)GE (100,1)GT
        // (101,0)EQ (101,1)NE.
        if (insn >> 24) & 0xFF == 0b00100100 && (insn >> 21) & 1 == 0 {
            let cmp_hi = (insn >> 13) & 0x7;
            let cmp_lo = (insn >> 4) & 1;
            let wide_rhs = matches!(cmp_hi, 0b001 | 0b010 | 0b011 | 0b110 | 0b111);
            if wide_rhs && esize == 8 {
                return Ok(CpuExit::Undefined(insn));
            }
            let pg = ((insn >> 10) & 0x7) as usize;
            let zn = ((insn >> 5) & 0x1F) as usize;
            let zm = ((insn >> 16) & 0x1F) as usize;
            let n_reg = self.v[zn].to_le_bytes();
            let m_reg = self.v[zm].to_le_bytes();
            let gov = self.sve_p[pg];
            let bits = (esize * 8) as u32;
            let mut result = 0u32;
            for e in 0..elements {
                let b = e * esize;
                if (gov >> b) & 1 == 0 {
                    continue; // inactive -> 0 (zeroing predicate)
                }
                let a = read_elem(&n_reg, b, esize);
                let cond = if wide_rhs {
                    let c = read_elem(&m_reg, (b / 8) * 8, 8);
                    let sa = sext_elem(a, bits);
                    let sc = c as i64 as i128;
                    let ua = uext_elem(a, bits);
                    let uc = c as u128;
                    match (cmp_hi, cmp_lo) {
                        (0b001, 0) => sa == sc,
                        (0b001, 1) => sa != sc,
                        (0b010, 0) => sa >= sc,
                        (0b010, 1) => sa > sc,
                        (0b011, 0) => sa < sc,
                        (0b011, 1) => sa <= sc,
                        (0b110, 0) => ua >= uc,
                        (0b110, 1) => ua > uc,
                        (0b111, 0) => ua < uc,
                        (0b111, 1) => ua <= uc,
                        _ => return Ok(CpuExit::Undefined(insn)),
                    }
                } else {
                    let c = read_elem(&m_reg, b, esize);
                    match (cmp_hi, cmp_lo) {
                        (0b000, 0) => uext_elem(a, bits) >= uext_elem(c, bits),
                        (0b000, 1) => uext_elem(a, bits) > uext_elem(c, bits),
                        (0b100, 0) => sext_elem(a, bits) >= sext_elem(c, bits),
                        (0b100, 1) => sext_elem(a, bits) > sext_elem(c, bits),
                        (0b101, 0) => a == c,
                        (0b101, 1) => a != c,
                        _ => return Ok(CpuExit::Undefined(insn)),
                    }
                };
                if cond {
                    result |= 1 << b;
                }
            }
            self.sve_p[pd] = result;
            let (n, z, cf, v) = pred_test(gov, result, elements, esize);
            self.set_n(n);
            self.set_z(z);
            self.set_c(cf);
            self.set_v(v);
            return Ok(CpuExit::Continue);
        }

        // Predicate-on-predicate logical ops (Pd = Pg & op(Pn, Pm), zeroing):
        // 0x25, bits[21:20]==00, bits[15:14]==01. Op selected by (bit23, bit9,
        // bit4). These work on the raw VL/8-bit (16 at VL=128) predicate values,
        // no element size. bits[21:20] MUST be 00 — the BRKA/BRKB/BRKN family
        // shares bits[15:14]==01 but has bits[21:20]==01.
        if (insn >> 24) & 0xFF == 0b00100101
            && (insn >> 20) & 0x3 == 0b00
            && (insn >> 14) & 0x3 == 0b01
        {
            let pm = ((insn >> 16) & 0xF) as usize;
            let pgl = ((insn >> 10) & 0xF) as usize;
            let pn = ((insn >> 5) & 0xF) as usize;
            let s = (insn >> 22) & 1;
            let vg = self.sve_p[pgl];
            let vn = self.sve_p[pn];
            let vm = self.sve_p[pm];
            let sel = (insn >> 23) & 1 == 0 && (insn >> 9) & 1 == 1 && (insn >> 4) & 1 == 1;
            if sel && s == 1 {
                return Ok(CpuExit::Undefined(insn));
            }
            let r = if sel {
                // SEL Pd = Pg ? Pn : Pm (per bit). Not zeroing, never sets flags.
                ((vg & vn) | (!vg & vm)) & 0xFFFF
            } else {
                (match ((insn >> 23) & 1, (insn >> 9) & 1, (insn >> 4) & 1) {
                    (0, 0, 0) => vg & vn & vm,    // AND(S)
                    (0, 0, 1) => vg & vn & !vm,   // BIC(S)
                    (0, 1, 0) => vg & (vn ^ vm),  // EOR(S)
                    (1, 0, 0) => vg & (vn | vm),  // ORR(S)
                    (1, 0, 1) => vg & (vn | !vm), // ORN(S)
                    (1, 1, 0) => vg & !(vn | vm), // NOR(S)
                    (1, 1, 1) => vg & !(vn & vm), // NAND(S)
                    _ => return Ok(CpuExit::Undefined(insn)),
                }) & 0xFFFF
            };
            self.sve_p[pd] = r;
            // The S-bit forms (ANDS/BICS/.../MOVS) set NZCV = PredTest(Pg, Pd);
            // SEL never sets flags.
            if s == 1 && !sel {
                let (n, z, c, v) = pred_test(vg, r, 16, 1);
                self.set_n(n);
                self.set_z(z);
                self.set_c(c);
                self.set_v(v);
            }
            return Ok(CpuExit::Continue);
        }

        // PFALSE Pd: writes an all-false predicate.
        if insn & 0xFFFF_FFF0 == 0x2518_E400 {
            self.sve_p[pd] = 0;
            return Ok(CpuExit::Continue);
        }

        // PTRUE / PTRUES Pd.T, pattern: bits[15:10]==111000, S=bit16. PTRUES
        // sets NZCV = PredTest(result, result) — i.e. the result governs itself,
        // so C = !LastActive collapses to (result == 0).
        if (insn >> 24) & 0xFF == 0x25
            && (insn >> 17) & 0x1F == 0b01100
            && b15_10 == 0b111000
            && (insn >> 4) & 1 == 0
        {
            let s = (insn >> 16) & 1;
            let pattern = (insn >> 5) & 0x1F;
            let count = sve_pattern_count(pattern, elements);
            let mut pred = 0u32;
            for e in 0..count {
                pred |= 1 << (e * esize);
            }
            self.sve_p[pd] = pred;
            if s == 1 {
                let empty = pred == 0;
                self.set_n(!empty);
                self.set_z(empty);
                self.set_c(empty);
                self.set_v(false);
            }
            return Ok(CpuExit::Continue);
        }

        // CTERMEQ/CTERMNE: 0x25, bit23==1, bit21==1, bits[15:10]==001000.
        // Compares two GP registers (sf=bit22 -> 64/32-bit); sets N to the
        // comparison result and V=!N&!C, leaving Z and C unchanged. bit4 = NE.
        if (insn >> 23) & 1 == 1 && (insn >> 21) & 1 == 1 && (insn >> 10) & 0x3F == 0b001000 {
            let sf = (insn >> 22) & 1 == 1;
            let ne = (insn >> 4) & 1 == 1;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rm = ((insn >> 16) & 0x1F) as u8;
            let (a, b) = if sf {
                (self.get_x(rn), self.get_x(rm))
            } else {
                (self.get_w(rn) as u64, self.get_w(rm) as u64)
            };
            let cmp = if ne { a != b } else { a == b };
            self.set_n(cmp);
            let c = self.get_c();
            self.set_v(!cmp & !c);
            return Ok(CpuExit::Continue);
        }

        // WHILE family (RR): bit21==1, bits[15:13]==000, bit10==1. Compares a
        // running index against a limit; bits[11:10]: 01=signed, 11=unsigned;
        // bit4: 0=strict (<), 1=inclusive (<=). The result is a contiguous run
        // of active elements from element 0, and NZCV is set from the result.
        if (insn >> 21) & 1 == 1 && (insn >> 13) & 0x7 == 0 && (insn >> 10) & 1 == 1 {
            let sf = (insn >> 12) & 1 == 1;
            let unsigned = (insn >> 10) & 0x3 == 0b11;
            let inclusive = (insn >> 4) & 1 == 1;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rm = ((insn >> 16) & 0x1F) as u8;
            let bits = if sf { 64 } else { 32 };
            let mask = elem_mask(bits);
            let mut op1 = if sf {
                self.get_x(rn)
            } else {
                self.get_w(rn) as u64
            } & mask;
            let op2 = if sf {
                self.get_x(rm)
            } else {
                self.get_w(rm) as u64
            } & mask;
            let mut pred = 0u32;
            let mut last = true;
            for e in 0..elements {
                let cond = if unsigned {
                    if inclusive { op1 <= op2 } else { op1 < op2 }
                } else {
                    let a = sext_elem(op1, bits);
                    let b = sext_elem(op2, bits);
                    if inclusive { a <= b } else { a < b }
                };
                last &= cond;
                if last {
                    pred |= 1 << (e * esize);
                }
                op1 = op1.wrapping_add(1) & mask;
            }
            self.sve_p[pd] = pred;
            let (n, z, c, v) = pred_test_flags(pred, elements, esize);
            self.set_n(n);
            self.set_z(z);
            self.set_c(c);
            self.set_v(v);
            return Ok(CpuExit::Continue);
        }

        // WHILE gt-family (RR): bit21==1, bits[15:13]==000, bit10==0. The running
        // index decreases from rn toward rm. bit11: 0=signed (GT/GE), 1=unsigned
        // (HI/HS). Per qemu do_WHILE, the "or-equal" sense is inverted vs the
        // lt-family: bit4==0 => GE/HS (inclusive), bit4==1 => GT/HI (strict).
        if (insn >> 21) & 1 == 1 && (insn >> 13) & 0x7 == 0 && (insn >> 10) & 1 == 0 {
            let sf = (insn >> 12) & 1 == 1;
            let unsigned = (insn >> 11) & 1 == 1;
            // a->eq: GT/HI have it set; the comparison "or-equal" flag is its
            // negation for the gt-family (eq = a->eq == lt, lt == false here).
            let eq = (insn >> 4) & 1 == 0;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rm = ((insn >> 16) & 0x1F) as u8;
            let tmax = elements as u64;
            let count: u64 = if unsigned {
                let a = if sf {
                    self.get_x(rn)
                } else {
                    self.get_w(rn) as u64
                };
                let b = if sf {
                    self.get_x(rm)
                } else {
                    self.get_w(rm) as u64
                };
                let cond = if eq { a >= b } else { a > b };
                if !cond {
                    0
                } else if eq && b == 0 {
                    // op1 == maxval(0): produce an all-true predicate.
                    tmax
                } else {
                    let t0 = (a - b) as u128 + if eq { 1 } else { 0 };
                    t0.min(tmax as u128) as u64
                }
            } else {
                let a = if sf {
                    self.get_x(rn) as i64
                } else {
                    self.get_w(rn) as i32 as i64
                };
                let b = if sf {
                    self.get_x(rm) as i64
                } else {
                    self.get_w(rm) as i32 as i64
                };
                let cond = if eq { a >= b } else { a > b };
                let maxval = if sf { i64::MIN } else { i32::MIN as i64 };
                if !cond {
                    0
                } else if eq && b == maxval {
                    tmax
                } else {
                    let t0 = (a as i128 - b as i128) + if eq { 1 } else { 0 };
                    t0.clamp(0, tmax as i128) as u64
                }
            };
            // The gt-family anchors the contiguous active run at the TOP of the
            // predicate (high-numbered elements), per qemu do_whileg.
            let start = elements - count.min(elements as u64) as usize;
            let mut pred = 0u32;
            for e in start..elements {
                pred |= 1 << (e * esize);
            }
            self.sve_p[pd] = pred;
            let (n, z, c, v) = pred_test_flags(pred, elements, esize);
            self.set_n(n);
            self.set_z(z);
            self.set_c(c);
            self.set_v(v);
            return Ok(CpuExit::Continue);
        }

        // WHILERW / WHILEWR (memory-hazard predicate): 0x25, bit21==1,
        // bits[15:10]==001100, bit4 picks WHILERW(1)/WHILEWR(0). Both produce a
        // monotone prefix of active elements, then set NZCV like the WHILE
        // family. A sub-element distance and a distance spanning at least one
        // whole vector both produce a full predicate.
        if (insn >> 21) & 1 == 1 && b15_10 == 0b001100 {
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rm = ((insn >> 16) & 0x1F) as u8;
            let rw = (insn >> 4) & 1 == 1;
            let xn = self.get_x(rn);
            let xm = self.get_x(rm);
            let full_count = elements as u64;
            let raw_count = if rw {
                xn.abs_diff(xm) >> size
            } else if xn >= xm {
                full_count
            } else {
                (xm - xn) >> size
            };
            let count = if raw_count == 0 || raw_count >= full_count {
                full_count
            } else {
                raw_count
            };
            let mut pred = 0u32;
            for e in 0..count as usize {
                pred |= 1 << (e * esize);
            }
            self.sve_p[pd] = pred;
            let (n, z, c, v) = pred_test_flags(pred, elements, esize);
            self.set_n(n);
            self.set_z(z);
            self.set_c(c);
            self.set_v(v);
            return Ok(CpuExit::Continue);
        }

        // BRKA / BRKB (break after / before the first true element of Pn,
        // single source): 0x25, bits[21:16]==010000, bits[15:14]==01, bit9==0.
        // bit23 picks BRKA(0)/BRKB(1); bit22 the flag-setting S form; bit4 the
        // merging(1)/zeroing(0) of Pg-inactive elements. esize is always 1 byte.
        if (insn >> 24) & 1 == 1
            && (insn >> 16) & 0x3F == 0b010000
            && (insn >> 14) & 0x3 == 0b01
            && (insn >> 9) & 1 == 0
        {
            let before = (insn >> 23) & 1 == 1; // BRKB
            let setflags = (insn >> 22) & 1 == 1;
            let merging = (insn >> 4) & 1 == 1;
            // The flag-setting form (BRKAS/BRKBS) is always zeroing: M (bit4)
            // must be 0, so S=1 with M=1 is an unallocated encoding.
            if setflags && merging {
                return Ok(CpuExit::Undefined(insn));
            }
            let pg = ((insn >> 10) & 0xF) as usize;
            let pn = ((insn >> 5) & 0xF) as usize;
            let mask = self.sve_p[pg];
            let operand = self.sve_p[pn];
            let prior = self.sve_p[pd];
            let mut result = 0u32;
            let mut brk = false;
            for e in 0..16 {
                let elem = (operand >> e) & 1 == 1;
                if (mask >> e) & 1 == 1 {
                    if before {
                        brk = brk || elem;
                        if !brk {
                            result |= 1 << e;
                        }
                    } else {
                        if !brk {
                            result |= 1 << e;
                        }
                        brk = brk || elem;
                    }
                } else if merging && (prior >> e) & 1 == 1 {
                    result |= 1 << e;
                }
            }
            if setflags {
                let (n, z, c, v) = pred_test(mask, result, 16, 1);
                self.set_n(n);
                self.set_z(z);
                self.set_c(c);
                self.set_v(v);
            }
            self.sve_p[pd] = result;
            return Ok(CpuExit::Continue);
        }

        // BRKN: 0x25, bit23==0, bits[21:16]==011000, bits[15:14]==01. If the
        // last Pg-active element of Pn is true, the result is Pdm unchanged,
        // else all-false. BRKNS (bit22==1) sets NZCV via PredTest(Ones,result).
        if (insn >> 24) & 1 == 1
            && (insn >> 23) & 1 == 0
            && (insn >> 16) & 0x3F == 0b011000
            && (insn >> 14) & 0x3 == 0b01
            && (insn >> 9) & 1 == 0 // fixed 0 between Pg and Pn
            && (insn >> 4) & 1 == 0
        // fixed 0 between Pn and Pdm
        {
            let setflags = (insn >> 22) & 1 == 1;
            let pg = ((insn >> 10) & 0xF) as usize;
            let pn = ((insn >> 5) & 0xF) as usize;
            let mask = self.sve_p[pg];
            let operand1 = self.sve_p[pn];
            let operand2 = self.sve_p[pd]; // Pdm (source + dest)
            let result = if last_active(mask, operand1, 16, 1) {
                operand2
            } else {
                0
            };
            if setflags {
                let (n, z, c, v) = pred_test(0xFFFF, result, 16, 1);
                self.set_n(n);
                self.set_z(z);
                self.set_c(c);
                self.set_v(v);
            }
            self.sve_p[pd] = result;
            return Ok(CpuExit::Continue);
        }

        // BRKPA / BRKPB (propagating partition break): 0x25, bit23==0,
        // bits[21:20]==00, bits[15:14]==11, bit9==0. The carry-in is whether
        // the last Pg-active element of Pn is set; within Pg-active elements the
        // result stays true until the Pm break (after for BRKPA, before BRKPB).
        if (insn >> 24) & 1 == 1
            && (insn >> 23) & 1 == 0
            && (insn >> 20) & 0x3 == 0b00
            && (insn >> 14) & 0x3 == 0b11
            && (insn >> 9) & 1 == 0
        {
            let before = (insn >> 4) & 1 == 1; // BRKPB
            let setflags = (insn >> 22) & 1 == 1;
            let pm = ((insn >> 16) & 0xF) as usize;
            let pg = ((insn >> 10) & 0xF) as usize;
            let pn = ((insn >> 5) & 0xF) as usize;
            let mask = self.sve_p[pg];
            let operand1 = self.sve_p[pn];
            let operand2 = self.sve_p[pm];
            let mut last = last_active(mask, operand1, 16, 1);
            let mut result = 0u32;
            for e in 0..16 {
                if (mask >> e) & 1 == 1 {
                    if before {
                        last = last && (operand2 >> e) & 1 == 0;
                        if last {
                            result |= 1 << e;
                        }
                    } else {
                        if last {
                            result |= 1 << e;
                        }
                        last = last && (operand2 >> e) & 1 == 0;
                    }
                }
            }
            if setflags {
                let (n, z, c, v) = pred_test(mask, result, 16, 1);
                self.set_n(n);
                self.set_z(z);
                self.set_c(c);
                self.set_v(v);
            }
            self.sve_p[pd] = result;
            return Ok(CpuExit::Continue);
        }

        // PFIRST Pdn.B, Pg, Pdn.B: bits[23:16]==01011000, bits[15:9]==1100000,
        // bit4==0. Sets the FIRST Pg-active element true in the (unchanged) Pdn.
        // Always operates on byte elements (esize=8 bits), independent of the
        // bits[23:22] field which is fixed to 01 in the opcode.
        if (insn >> 16) & 0xFF == 0b01011000
            && (insn >> 9) & 0x7F == 0b1100000
            && (insn >> 4) & 1 == 0
        {
            let pg = ((insn >> 5) & 0xF) as usize;
            let mask = self.sve_p[pg];
            let mut result = self.sve_p[pd];
            for e in 0..16 {
                if (mask >> e) & 1 == 1 {
                    result |= 1 << e;
                    break;
                }
            }
            let (n, z, c, v) = pred_test(mask, result, 16, 1);
            self.set_n(n);
            self.set_z(z);
            self.set_c(c);
            self.set_v(v);
            self.sve_p[pd] = result;
            return Ok(CpuExit::Continue);
        }

        // PNEXT Pdn.T, Pg, Pdn.T: bits[21:16]==011001, bits[15:9]==1100010,
        // bit4==0. Finds the next Pg-active element strictly after the last
        // active element of the current Pdn, leaving only that element active.
        if (insn >> 16) & 0x3F == 0b011001
            && (insn >> 9) & 0x7F == 0b1100010
            && (insn >> 4) & 1 == 0
        {
            let pg = ((insn >> 5) & 0xF) as usize;
            let mask = self.sve_p[pg];
            let operand = self.sve_p[pd];
            let mut last: i32 = -1;
            for e in 0..elements {
                if (operand >> (e * esize)) & 1 == 1 {
                    last = e as i32;
                }
            }
            let mut next = (last + 1) as usize;
            while next < elements && (mask >> (next * esize)) & 1 == 0 {
                next += 1;
            }
            let mut result = 0u32;
            if next < elements {
                result |= 1 << (next * esize);
            }
            let (n, z, c, v) = pred_test(mask, result, elements, esize);
            self.set_n(n);
            self.set_z(z);
            self.set_c(c);
            self.set_v(v);
            self.sve_p[pd] = result;
            return Ok(CpuExit::Continue);
        }

        Err(ArmError::Unimplemented(format!(
            "SVE predicate op bits[15:10]={:06b}",
            b15_10
        )))
    }
}
