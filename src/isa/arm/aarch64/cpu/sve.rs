//! SVE (Scalable Vector Extension) instruction execution

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


    /// Write the SVE first-fault register. Exposed for the differential harness.
    pub fn set_sve_ffr(&mut self, v: u32) {
        self.sve_ffr = v & 0xFFFF;
    }


    pub(crate) fn exec_sve(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        // Check if SVE is enabled (CPACR_EL1.ZEN)
        let cpacr = self.sysregs.el1.cpacr;
        let zen = (cpacr >> 16) & 0x3;

        if self.current_el == 0 && zen != 0x3 {
            return Ok(CpuExit::Undefined(insn));
        }
        if self.current_el == 1 && zen == 0x0 {
            return Ok(CpuExit::Undefined(insn));
        }

        // Early rejects for reserved field combinations that broader dispatch
        // arms below would otherwise accept.
        let top = (insn >> 24) & 0xFF;
        let has_sve2p1 = self.config.features.contains(ArmFeatures::SVE2P1);
        // SVE2 PMUL is byte-only. For word/doubleword encodings the size bits
        // alter op1 enough that the broad arithmetic fallback below can catch
        // them unless they are rejected here.
        if top == 0b0000_0100
            && (insn >> 21) & 1 == 1
            && (insn >> 12) & 0xF == 0b0110
            && (insn >> 10) & 0x3 == 0b01
            && (insn >> 22) & 0x3 != 0
        {
            return Ok(CpuExit::Undefined(insn));
        }
        // 0x04, bit21==1, bits[15:13]==100, bit12==0: unpredicated shift by
        // wide elements (ASR/LSR/LSL Zd, Zn, Zm.D). The wide operand is .D, so
        // doubleword element size is unallocated.
        if top == 0b0000_0100
            && (insn >> 21) & 1 == 1
            && (insn >> 13) & 0x7 == 0b100
            && (insn >> 12) & 1 == 0
            && (insn >> 22) & 0x3 == 0b11
        {
            return Ok(CpuExit::Undefined(insn));
        }
        // 0x25 integer arith with shifted immediate (bits[21:19]==100,
        // bits[15:14]==11): sh=1 with byte elements is unallocated.
        if top == 0b0010_0101
            && (insn >> 19) & 0x7 == 0b100
            && (insn >> 14) & 0x3 == 0b11
            && (insn >> 13) & 1 == 1
            && (insn >> 22) & 0x3 == 0
        {
            return Ok(CpuExit::Undefined(insn));
        }
        // 0x05 bitwise logical with immediate (AND/EOR/ORR/DUPM space,
        // bits[21:18]==x00000 family): a reserved imm13 pattern is unallocated.
        if top == 0b0000_0101 && (insn >> 18) & 0xF == 0b0000 {
            let n = (insn >> 17) & 1 == 1;
            let immr = (insn >> 11) & 0x3F;
            let imms = (insn >> 5) & 0x3F;
            if decode_bitmask(n, imms, immr, true).is_err() {
                return Ok(CpuExit::Undefined(insn));
            }
        }

        // Extract primary classification bits
        let op0 = (insn >> 29) & 0x7;
        let op1 = (insn >> 23) & 0x3;
        let op2 = (insn >> 17) & 0x1F;
        let op3 = (insn >> 10) & 0x3F;

        // Common register fields
        let zd = (insn & 0x1F) as usize;
        let zn = ((insn >> 5) & 0x1F) as usize;
        let zm = ((insn >> 16) & 0x1F) as usize;
        let pg = ((insn >> 10) & 0x7) as usize;
        let size = (insn >> 22) & 0x3;

        // Element size in bytes
        let esize = 1usize << size; // 1, 2, 4, or 8 bytes

        match op0 {
            // EXT (destructive): 0x05, bits[23:21]==001, bits[15:13]==000.
            // Zdn.B = (Zm:Zdn) extracted at byte offset imm8 (imm8h:imm8l).
            // Must precede the int_unpred arm below (which shares bit21==1 &&
            // bits[15:13]==000 but does not check the op byte). At VL=128 there
            // are 16 byte-elements; if imm8>=16 the offset wraps to 0.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 21) & 0x7 == 0b001
                    && (insn >> 13) & 0x7 == 0b000 =>
            {
                let imm8 = (((insn >> 16) & 0x1F) << 3) | ((insn >> 10) & 0x7);
                let low = self.v[zd]; // operand1 (Zdn) = low half of concat
                let high = self.v[zn]; // operand2 (Zm)  = high half of concat
                let pos = if imm8 >= 16 { 0 } else { imm8 };
                let s = pos * 8; // byte offset -> bit offset (0..=120)
                self.v[zd] = if s == 0 {
                    low
                } else {
                    (low >> s) | (high << (128 - s))
                };
                Ok(CpuExit::Continue)
            }

            // SVE2.1 DUPQ (broadcast indexed element within each 128-bit
            // segment): 0x05, bits[23:22]==00, bit21==1, bits[15:10]==001001. The
            // element size and segment index are packed into the tsz field
            // bits[20:16] (lowest set bit selects esize; the index is above it).
            // At VL=128 there is one segment, so Zn[index] fills every lane.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 22) & 0x3 == 0b00
                    && (insn >> 21) & 1 == 1
                    && (insn >> 10) & 0x3F == 0b001001 =>
            {
                if !has_sve2p1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let tsz = (insn >> 16) & 0x1F;
                if tsz == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let size = tsz.trailing_zeros() as usize;
                let esz = 1usize << size;
                let index = (tsz >> (size + 1)) as usize;
                if esz == 16 {
                    self.v[zd] = if index == 0 { self.v[zn] } else { 0 };
                    return Ok(CpuExit::Continue);
                }
                let nsegelt = 16 / esz;
                let src = self.v[zn].to_le_bytes();
                let val = if index < nsegelt {
                    read_elem(&src, index * esz, esz)
                } else {
                    0
                };
                let mut dst = [0u8; 16];
                for e in 0..nsegelt {
                    write_elem(&mut dst, e * esz, esz, val);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2.1 EXTQ (extract within each 128-bit segment): 0x05,
            // bits[23:20]==0110, bits[15:10]==001001. imm4=bits[19:16] is a byte
            // offset (0..15). Destructive: the concatenation (Zm:Zdn) of the
            // 128-bit segment is shifted right imm bytes; at VL=128 this matches
            // EXT with a 4-bit immediate (Zdn=Zd field, Zm=Zn field).
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 20) & 0xF == 0b0110
                    && (insn >> 10) & 0x3F == 0b001001 =>
            {
                if !has_sve2p1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let imm = ((insn >> 16) & 0xF) as usize; // bytes
                let low = self.v[zd]; // Zdn
                let high = self.v[zn]; // Zm
                let s = imm * 8;
                self.v[zd] = if s == 0 {
                    low
                } else {
                    (low >> s) | (high << (128 - s))
                };
                Ok(CpuExit::Continue)
            }

            // SVE2.1 REVD (reverse 64-bit doublewords within each 128-bit
            // segment, predicated/merging): 0x05, bits[23:16]==0b00101110,
            // bits[15:13]==100. Pg=bits[12:10]; the swap of a 128-bit segment is
            // governed by the predicate bit of its low doubleword. At VL=128 the
            // single segment swaps its two 64-bit halves iff predicate bit 0 set.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 16) & 0xFF == 0b00101110
                    && (insn >> 13) & 0x7 == 0b100 =>
            {
                if !has_sve2p1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let pg = ((insn >> 10) & 0x7) as usize;
                let pred = self.sve_p[pg];
                if pred & 1 == 1 {
                    let v = self.v[zn];
                    self.v[zd] = (v >> 64) | (v << 64);
                }
                // inactive low-doubleword -> merge (Zd unchanged)
                Ok(CpuExit::Continue)
            }

            // SVE2.1 PMOV (move a bit-plane between a vector and a predicate):
            // 0x05, bits[21:19]==101, bits[15:10]==001110. bit16 selects the
            // direction (0: Zn -> Pd, 1: Pn -> Zd). The element size and bit-plane
            // index are tsz-encoded in bits[23:22]:bits[18:17]. Per qemu pmov_pv/
            // pmov_vp, predicate bit e*esize maps to vector bit elements*idx+e
            // (elements = 16/esize). The vp form zeroes Zd only when idx==0 (else
            // it merges the selected plane).
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 19) & 0x7 == 0b101
                    && (insn >> 10) & 0x3F == 0b001110 =>
            {
                if !has_sve2p1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let sh = (insn >> 22) & 0x3;
                let sl = (insn >> 17) & 0x3;
                let (esz, idx): (usize, usize) = if sh == 0b00 && sl == 0b01 {
                    (0, 0)
                } else if sh == 0b00 && sl & 0b10 == 0b10 {
                    (1, (sl & 1) as usize)
                } else if sh == 0b01 {
                    (2, sl as usize)
                } else if sh & 0b10 == 0b10 {
                    (3, ((((sh & 1) << 2) | sl) as usize))
                } else {
                    return Ok(CpuExit::Undefined(insn));
                };
                let esize = 1usize << esz;
                let elements = 16 / esize;
                if (insn >> 16) & 1 == 0 {
                    // PMOV Pd.T, Zn[idx]: Zn -> Pd.
                    let pd = (insn & 0xF) as usize;
                    let z = self.v[((insn >> 5) & 0x1F) as usize];
                    let mut p = 0u32;
                    for e in 0..elements {
                        let bit = (z >> (elements * idx + e)) & 1;
                        p |= (bit as u32) << (e * esize);
                    }
                    self.sve_p[pd] = p;
                } else {
                    // PMOV Zd[idx], Pn.T: Pn -> Zd.
                    let zd = (insn & 0x1F) as usize;
                    let p = self.sve_p[((insn >> 5) & 0xF) as usize];
                    let mut z = if idx == 0 { 0u128 } else { self.v[zd] };
                    for e in 0..elements {
                        let pos = elements * idx + e;
                        let bit = ((p >> (e * esize)) & 1) as u128;
                        z = (z & !(1u128 << pos)) | (bit << pos);
                    }
                    self.v[zd] = z;
                }
                Ok(CpuExit::Continue)
            }

            // Unpredicated integer add/subtract (ADD/SUB/SQADD/UQADD/SQSUB/
            // UQSUB): bit21==1, bits[15:13]==000. Size is the full bits[23:22],
            // so this must NOT be gated on op1 (which folds size's high bit).
            0b000 if (insn >> 21) & 1 == 1 && (insn >> 13) & 0x7 == 0b000 => {
                self.exec_sve_int_unpred(insn, zd, zn, zm, esize)
            }

            // TBL (table lookup, single table): 0x05, bit21==1,
            // bits[15:10]==001100. Shares bits[15:10] with the unpredicated
            // logical arm below (0x04) so it MUST precede it and gate on 0x05.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 10) & 0x3F == 0b001100 =>
            {
                self.exec_sve_tbl(zd, zn, zm, esize)
            }

            // TBX (table lookup, keep destination for out-of-range): 0x05,
            // bit21==1, bits[15:10]==001011. Like TBL but unmatched indices
            // preserve the existing Zd element instead of zeroing it.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 10) & 0x3F == 0b001011 =>
            {
                self.exec_sve_tbx(zd, zn, zm, esize)
            }

            // SVE2.1 TBXQ (per-128-bit-segment table lookup, keep destination):
            // 0x05, bit21==1, bits[15:10]==001101. At VL=128 (one segment) this
            // is identical to TBX over the whole register.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 10) & 0x3F == 0b001101 =>
            {
                if !has_sve2p1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                self.exec_sve_tbx(zd, zn, zm, esize)
            }

            // TBL2 (two-register table lookup): 0x05, bit21==1,
            // bits[15:10]==001010. The tables are {Zn, Zn+1}; out-of-range
            // indices yield 0.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 10) & 0x3F == 0b001010 =>
            {
                let t0 = self.v[zn].to_le_bytes();
                let t1 = self.v[(zn + 1) % 32].to_le_bytes();
                let idx = self.v[zm].to_le_bytes();
                let n = 16 / esize;
                let mut dst = [0u8; 16];
                for e in 0..n {
                    let off = e * esize;
                    let i = read_elem(&idx, off, esize) as usize;
                    let val = if i < n {
                        read_elem(&t0, i * esize, esize)
                    } else if i < 2 * n {
                        read_elem(&t1, (i - n) * esize, esize)
                    } else {
                        0
                    };
                    write_elem(&mut dst, off, esize, val);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // Predicate ZIP/UZP/TRN (ZIP1_p..TRN2_p): 0x05, bits[21:20]==10,
            // bits[15:13]==010. opc=bits[12:10]. Permutes the esize-bit chunks
            // of the byte-granular predicates Pn (bits[8:5]) and Pm (bits[19:16]).
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 20) & 0x3 == 0b10
                    && (insn >> 13) & 0x7 == 0b010 =>
            {
                if (insn & ((1 << 9) | (1 << 4))) != 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let opc = (insn >> 10) & 0x7;
                let esize = 1usize << ((insn >> 22) & 0x3);
                let n = 16 / esize;
                let cmask = (1u32 << esize) - 1;
                let pn = self.sve_p[((insn >> 5) & 0xF) as usize];
                let pm = self.sve_p[((insn >> 16) & 0xF) as usize];
                let chunk = |p: u32, i: usize| (p >> (i * esize)) & cmask;
                let mut out = 0u32;
                let h = n / 2;
                for i in 0..h {
                    let (lo, hi) = match opc {
                        0b000 => (chunk(pn, i), chunk(pm, i)),                 // ZIP1
                        0b001 => (chunk(pn, h + i), chunk(pm, h + i)),         // ZIP2
                        0b100 => (chunk(pn, 2 * i), chunk(pm, 2 * i)),         // TRN1
                        0b101 => (chunk(pn, 2 * i + 1), chunk(pm, 2 * i + 1)), // TRN2
                        _ => (0, 0),
                    };
                    if matches!(opc, 0b000 | 0b001 | 0b100 | 0b101) {
                        out |= lo << (2 * i * esize);
                        out |= hi << ((2 * i + 1) * esize);
                    }
                }
                if opc == 0b010 || opc == 0b011 {
                    let odd = opc == 0b011; // UZP2 takes odd elements
                    for i in 0..n {
                        let v = if i < h {
                            chunk(pn, 2 * i + odd as usize)
                        } else {
                            chunk(pm, 2 * (i - h) + odd as usize)
                        };
                        out |= v << (i * esize);
                    }
                } else if opc > 0b101 {
                    return Ok(CpuExit::Undefined(insn));
                }
                self.sve_p[(insn & 0xF) as usize] = out;
                Ok(CpuExit::Continue)
            }

            // Predicate REV (REV_p): 0x05, bits[21:16]==110100, bits[15:10]==
            // 010000. Reverse the esize-bit chunks of the predicate Pn.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 16) & 0x3F == 0b110100
                    && (insn >> 10) & 0x3F == 0b010000 =>
            {
                if (insn & ((1 << 9) | (1 << 4))) != 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let esize = 1usize << ((insn >> 22) & 0x3);
                let n = 16 / esize;
                let cmask = (1u32 << esize) - 1;
                let pn = self.sve_p[((insn >> 5) & 0xF) as usize];
                let mut out = 0u32;
                for i in 0..n {
                    out |= ((pn >> ((n - 1 - i) * esize)) & cmask) << (i * esize);
                }
                self.sve_p[(insn & 0xF) as usize] = out;
                Ok(CpuExit::Continue)
            }

            // PUNPKLO/PUNPKHI (predicate unpack): 0x05, bits[23:20]==0011,
            // bits[19:17]==000, bits[15:10]==010000. Each of the low (lo) / high
            // (hi, bit16) 8 source predicate bits expands to bit 2i of the dest.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 22) & 0x3 == 0
                    && (insn >> 20) & 0x3 == 0b11
                    && (insn >> 17) & 0x7 == 0
                    && (insn >> 10) & 0x3F == 0b010000 =>
            {
                let hi = (insn >> 16) & 1 == 1;
                let pn = self.sve_p[((insn >> 5) & 0xF) as usize];
                let pd = (insn & 0xF) as usize;
                let base = if hi { 8 } else { 0 };
                let mut out = 0u32;
                for i in 0..8 {
                    if (pn >> (base + i)) & 1 == 1 {
                        out |= 1 << (2 * i);
                    }
                }
                self.sve_p[pd] = out;
                Ok(CpuExit::Continue)
            }

            // DUP (indexed broadcast) Zd.T, Zn.T[index]: 0x05, bit21==1,
            // bits[15:10]==001000. esize and index come from the tsz:imm2 field
            // (lowest set bit of tsz selects esize).
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 10) & 0x3F == 0b001000 =>
            {
                self.exec_sve_dup_indexed(insn, zn, zd)
            }

            // COMPACT (pack active elements down): 0x05, bit23==1,
            // bits[21:16]==100001, bits[15:13]==100. S/D elements only.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 23) & 1 == 1
                    && (insn >> 16) & 0x3F == 0b100001
                    && (insn >> 13) & 0x7 == 0b100 =>
            {
                self.exec_sve_compact(insn, zd, zn, pg)
            }

            // SVE2 SPLICE (constructive): 0x05, bits[21:16]==101101,
            // bits[15:13]==100. Sources are the consecutive pair {Zn, Zn+1}.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 16) & 0x3F == 0b101101
                    && (insn >> 13) & 0x7 == 0b100 =>
            {
                self.exec_sve_splice_pair(insn, zd, zn, pg)
            }

            // SPLICE (destructive): 0x05, bits[21:16]==101100, bits[15:13]==100.
            // Zdn's active span is packed low, the rest filled from Zm.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 16) & 0x3F == 0b101100
                    && (insn >> 13) & 0x7 == 0b100 =>
            {
                self.exec_sve_splice(insn, zd, zn, pg)
            }

            // Unpredicated bitwise logical (AND/ORR/EOR/BIC): bits[15:10]=001100.
            0b000 if (insn >> 21) & 1 == 1 && (insn >> 10) & 0x3F == 0b001100 => {
                self.exec_sve_logical_unpred(insn, zd, zn, zm)
            }

            // SVE2 unpredicated multiply: 0x04, bit21==1, bits[15:12]==0110,
            // bits[11:10] opc (00=MUL, 01=PMUL byte-only, 10=SMULH, 11=UMULH).
            // The 0x05 sibling of bits[15:12]==0110 is ZIP/UZP/TRN, so this MUST
            // gate on the op byte. PMUL is defined for byte elements only.
            0b000
                if (insn >> 24) & 0xFF == 0b00000100
                    && (insn >> 21) & 1 == 1
                    && (insn >> 12) & 0xF == 0b0110
                    && ((insn >> 10) & 0x3 != 0b01 || esize == 1) =>
            {
                let opc = (insn >> 10) & 0x3;
                let bits = (esize * 8) as u32;
                let mask = elem_mask(bits);
                let elements = 16 / esize;
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..elements {
                    let off = e * esize;
                    let x = read_elem(&a, off, esize);
                    let y = read_elem(&b, off, esize);
                    let r = match opc {
                        0b00 => x.wrapping_mul(y) & mask,
                        0b01 => poly_mul_8(x, y), // PMUL.B (carry-less)
                        0b10 => ((sext_elem(x, bits) * sext_elem(y, bits)) >> bits) as u64 & mask,
                        _ => ((uext_elem(x, bits) * uext_elem(y, bits)) >> bits) as u64 & mask,
                    };
                    write_elem(&mut dst, off, esize, r);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 bitwise ternary (whole-register): 0x04, bit21==1,
            // bits[15:11]==00111. Zdn=bits[4:0], Zk=bits[9:5], Zm=bits[20:16];
            // opc=bits[23:22], o2=bit10 select the operation.
            0b000
                if (insn >> 24) & 0xFF == 0b00000100
                    && (insn >> 21) & 1 == 1
                    && (insn >> 11) & 0x1F == 0b00111 =>
            {
                let opc = (insn >> 22) & 0x3;
                let o2 = (insn >> 10) & 1;
                let dn = self.v[zd]; // Zdn (first source + destination)
                let k = self.v[zn]; // Zk (select mask)
                let m = self.v[zm]; // Zm (second source)
                self.v[zd] = match (opc, o2) {
                    (0b00, 0) => dn ^ m ^ k,             // EOR3
                    (0b01, 0) => dn ^ (m & !k),          // BCAX
                    (0b00, 1) => (dn & k) | (m & !k),    // BSL
                    (0b01, 1) => (!dn & k) | (m & !k),   // BSL1N
                    (0b10, 1) => (dn & k) | (!m & !k),   // BSL2N
                    (0b11, 1) => !((dn & k) | (m & !k)), // NBSL
                    _ => return Ok(CpuExit::Undefined(insn)),
                };
                Ok(CpuExit::Continue)
            }

            // SVE FEXPA (exponential accelerator): 0x04, bit21==1,
            // bits[20:16]==00000, bits[15:10]==101110. Unpredicated table lookup.
            0b000
                if (insn >> 24) & 0xFF == 0b00000100
                    && (insn >> 21) & 1 == 1
                    && (insn >> 16) & 0x1F == 0b00000
                    && (insn >> 10) & 0x3F == 0b101110 =>
            {
                let size = (insn >> 22) & 0x3;
                if size == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let esz = 1usize << size;
                let n = self.v[zn].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..(16 / esz) {
                    let off = e * esz;
                    write_elem(&mut dst, off, esz, sve_fexpa(esz, read_elem(&n, off, esz)));
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE REVB/REVH/REVW/RBIT (predicated, merging): 0x05, bit21==1,
            // bits[20:18]==001, bits[15:13]==100. bits[17:16]: 00=REVB (reverse
            // bytes within each element), 01=REVH (halfwords), 10=REVW (words),
            // 11=RBIT (bits). @rd_pg_rn.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 18) & 0x7 == 0b001
                    && (insn >> 13) & 0x7 == 0b100 =>
            {
                let op = (insn >> 16) & 0x3;
                let unit = match op {
                    0b00 => 1usize,          // REVB
                    0b01 if esize >= 4 => 2, // REVH (S/D)
                    0b10 if esize == 8 => 4, // REVW (D)
                    0b11 => 0,               // RBIT
                    _ => return Ok(CpuExit::Undefined(insn)),
                };
                if op == 0b00 && esize < 2 {
                    return Ok(CpuExit::Undefined(insn)); // REVB.b is reserved
                }
                let pg = ((insn >> 10) & 0x7) as usize;
                let rn = ((insn >> 5) & 0x1F) as usize;
                let pred = self.sve_p[pg];
                let mask = elem_mask((esize * 8) as u32);
                let src = self.v[rn].to_le_bytes();
                let mut dst = self.v[zd].to_le_bytes();
                for e in 0..(16 / esize) {
                    let off = e * esize;
                    if (pred >> off) & 1 == 0 {
                        continue;
                    }
                    let v = read_elem(&src, off, esize);
                    let r = if op == 0b11 {
                        (v & mask).reverse_bits() >> (64 - esize * 8)
                    } else {
                        reverse_chunks(v, esize, unit) & mask
                    };
                    write_elem(&mut dst, off, esize, r);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE INSR (insert scalar, shifting the vector up one element): 0x05,
            // bit21==1, bits[15:10]==001110, bits[20:16]==00100 (GPR) or 10100
            // (SIMD scalar). New Zdn = [scalar, Zdn[0..N-1]].
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 10) & 0x3F == 0b001110
                    && matches!((insn >> 16) & 0x1F, 0b00100 | 0b10100) =>
            {
                let insr_f = (insn >> 20) & 1 == 1; // SIMD&FP scalar form
                let rmf = ((insn >> 5) & 0x1F) as usize;
                let esbits = esize * 8;
                let smask: u128 = (1u128 << esbits) - 1;
                let scalar = if insr_f {
                    self.v[rmf] & smask
                } else {
                    (self.get_x(rmf as u8) as u128) & smask
                };
                self.v[zd] = (self.v[zd] << esbits) | scalar;
                Ok(CpuExit::Continue)
            }

            // SVE CLASTA/CLASTB to vector or SIMD&FP scalar: 0x05,
            // bits[21:17]==10100 (vector) / 10101 (scalar), bit16=A(0)/B(1),
            // bits[15:13]==100. The element at (CLASTB) / after (CLASTA) the last
            // active lane of Zm is broadcast to Zdn (vector) or written to Vd
            // (scalar); with no active lane the destination is unchanged.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && matches!((insn >> 17) & 0x1F, 0b10100 | 0b10101)
                    && (insn >> 13) & 0x7 == 0b100 =>
            {
                let scalar_form = (insn >> 17) & 1 == 1;
                let before = (insn >> 16) & 1 == 1; // CLASTB
                let pg = ((insn >> 10) & 0x7) as usize;
                let src_reg = ((insn >> 5) & 0x1F) as usize;
                let n = 16 / esize;
                let pred = self.sve_p[pg];
                let mask = elem_mask((esize * 8) as u32);
                let src = self.v[src_reg].to_le_bytes();
                let mut last: i32 = -1;
                for e in (0..n).rev() {
                    if (pred >> (e * esize)) & 1 == 1 {
                        last = e as i32;
                        break;
                    }
                }
                let selected = if last >= 0 {
                    let idx = if before {
                        last as usize
                    } else {
                        let i = (last + 1) as usize;
                        if i >= n { 0 } else { i }
                    };
                    Some(read_elem(&src, idx * esize, esize) & mask)
                } else {
                    None
                };
                if scalar_form {
                    // Writing to a SIMD&FP scalar always zeroes the upper bits;
                    // with no active element the prior low element is preserved.
                    let val = selected.unwrap_or((self.v[zd] as u64) & mask);
                    self.v[zd] = val as u128;
                } else if let Some(val) = selected {
                    // Vector form: broadcast; unchanged if no active element.
                    let mut out = 0u128;
                    for e in 0..n {
                        out |= (val as u128) << (e * esize * 8);
                    }
                    self.v[zd] = out;
                }
                Ok(CpuExit::Continue)
            }

            // SVE FCPY (copy FP immediate into Pg-active lanes, merging): 0x05,
            // bits[21:20]==01, bits[15:13]==110. Pg is 4-bit (bits[19:16]).
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 20) & 0x3 == 0b01
                    && (insn >> 13) & 0x7 == 0b110 =>
            {
                if esize < 2 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let pg = ((insn >> 16) & 0xF) as usize;
                let imm8 = ((insn >> 5) & 0xFF) as u8;
                let val = vfp_expand_imm(imm8, esize);
                let pred = self.sve_p[pg];
                let mut dst = self.v[zd].to_le_bytes();
                for e in 0..(16 / esize) {
                    let off = e * esize;
                    if (pred >> off) & 1 == 1 {
                        write_elem(&mut dst, off, esize, val);
                    }
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE ORR/EOR/AND with a logical immediate: bits[21:18]==0000,
            // opc=bits[23:22]. The N:immr:imms field decodes a 64-bit mask
            // broadcast to every doubleword lane.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 18) & 0xF == 0
                    && (insn >> 22) & 0x3 != 0b11 =>
            {
                let n = (insn >> 17) & 1 == 1;
                let immr = (insn >> 11) & 0x3F;
                let imms = (insn >> 5) & 0x3F;
                match decode_bitmask(n, imms, immr, true) {
                    Ok(mask) => {
                        let imm = (mask as u128) | ((mask as u128) << 64);
                        self.v[zd] = match (insn >> 22) & 0x3 {
                            0b00 => self.v[zd] | imm,
                            0b01 => self.v[zd] ^ imm,
                            _ => self.v[zd] & imm,
                        };
                        Ok(CpuExit::Continue)
                    }
                    Err(_) => Ok(CpuExit::Undefined(insn)),
                }
            }

            // SVE DUPM (broadcast logical-immediate mask): 0x05, bits[23:18]==
            // 110000. The N:immr:imms field decodes a 64-bit mask broadcast to
            // every doubleword lane.
            0b000 if (insn >> 24) & 0xFF == 0b00000101 && (insn >> 18) & 0x3F == 0b110000 => {
                let n = (insn >> 17) & 1 == 1;
                let immr = (insn >> 11) & 0x3F;
                let imms = (insn >> 5) & 0x3F;
                match decode_bitmask(n, imms, immr, true) {
                    Ok(imm) => {
                        self.v[zd] = (imm as u128) | ((imm as u128) << 64);
                        Ok(CpuExit::Continue)
                    }
                    Err(_) => Ok(CpuExit::Undefined(insn)),
                }
            }

            // SVE UNPK (SUNPKHI/LO, UUNPKHI/LO): 0x05, bits[21:18]==1100,
            // bits[15:10]==001110. Unpack the low (h=0) or high (h=1) half of
            // Zn, sign- (u=0) or zero- (u=1) extending each half-width element.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 18) & 0xF == 0b1100
                    && (insn >> 10) & 0x3F == 0b001110 =>
            {
                let size = (insn >> 22) & 0x3;
                if size == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let d_esize = 1usize << size;
                let s_esize = d_esize / 2;
                let s_bits = (s_esize * 8) as u32;
                let mask = elem_mask((d_esize * 8) as u32);
                let unsigned = (insn >> 17) & 1 == 1;
                let hi = (insn >> 16) & 1 == 1;
                let src = self.v[zn].to_le_bytes();
                let n_dst = 16 / d_esize;
                let mut dst = [0u8; 16];
                for e in 0..n_dst {
                    let src_idx = (if hi { n_dst } else { 0 }) + e;
                    let sv = read_elem(&src, src_idx * s_esize, s_esize);
                    let r = if unsigned {
                        uext_elem(sv, s_bits) as u64
                    } else {
                        sext_elem(sv, s_bits) as u64
                    };
                    write_elem(&mut dst, e * d_esize, d_esize, r & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE FTSSEL (trigonometric select coefficient): 0x04, bit21==1,
            // bits[15:10]==101100. Per lane: result = Zm[e]&1 ? 1.0 : Zn[e];
            // then if Zm[e]&2 negate. Unpredicated; Zn=bits[9:5], Zm=bits[20:16].
            0b000
                if (insn >> 24) & 0xFF == 0b00000100
                    && (insn >> 21) & 1 == 1
                    && (insn >> 10) & 0x3F == 0b101100 =>
            {
                let size = (insn >> 22) & 0x3;
                if size == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let esize = 1usize << size;
                let one: u64 = match esize {
                    2 => 0x3C00,
                    4 => 0x3F80_0000,
                    _ => 0x3FF0_0000_0000_0000,
                };
                let signbit: u64 = 1 << (esize * 8 - 1);
                let n = self.v[zn].to_le_bytes();
                let m = self.v[zm].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..(16 / esize) {
                    let off = e * esize;
                    let mm = read_elem(&m, off, esize);
                    let mut nn = read_elem(&n, off, esize);
                    if mm & 1 != 0 {
                        nn = one;
                    }
                    if mm & 2 != 0 {
                        nn ^= signbit;
                    }
                    write_elem(&mut dst, off, esize, nn);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 XAR (exclusive-or and rotate right by immediate): 0x04,
            // bit21==1, bits[15:10]==001101. Zdn=bits[4:0], Zm=bits[9:5]; the
            // tsz:imm3 field gives the element size and rotate amount (1..bits).
            // Destructive: Zdn = ROR(Zdn ^ Zm, amount) per element.
            0b000
                if (insn >> 24) & 0xFF == 0b00000100
                    && (insn >> 21) & 1 == 1
                    && (insn >> 10) & 0x3F == 0b001101 =>
            {
                let tsz = (((insn >> 22) & 0x3) << 2) | ((insn >> 19) & 0x3);
                if tsz == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let bits: u32 = if tsz & 0b1000 != 0 {
                    64
                } else if tsz & 0b0100 != 0 {
                    32
                } else if tsz & 0b0010 != 0 {
                    16
                } else {
                    8
                };
                let esize = (bits / 8) as usize;
                let tszimm = (tsz << 3) | ((insn >> 16) & 0x7);
                let amount = (2 * bits - tszimm) % bits; // 1..bits, bits == identity
                let a = self.v[zd].to_le_bytes();
                let b = self.v[zn].to_le_bytes();
                let mask = elem_mask(bits);
                let mut dst = [0u8; 16];
                for e in 0..(16 / esize) {
                    let off = e * esize;
                    let x = (read_elem(&a, off, esize) ^ read_elem(&b, off, esize)) & mask;
                    let r = if amount == 0 {
                        x
                    } else {
                        ((x >> amount) | (x << (bits - amount))) & mask
                    };
                    write_elem(&mut dst, off, esize, r);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 SQDMULH/SQRDMULH (unpredicated saturating doubling multiply
            // high): 0x04, bit21==1, bits[15:11]==01110. R=bit10 adds rounding.
            0b000
                if (insn >> 24) & 0xFF == 0b00000100
                    && (insn >> 21) & 1 == 1
                    && (insn >> 11) & 0x1F == 0b01110 =>
            {
                let round = (insn >> 10) & 1 == 1;
                let bits = (esize * 8) as u32;
                let mask = elem_mask(bits);
                let elements = 16 / esize;
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = [0u8; 16];
                let hi = (1i128 << (bits - 1)) - 1;
                let lo = -(1i128 << (bits - 1));
                for e in 0..elements {
                    let off = e * esize;
                    let prod = sext_elem(read_elem(&a, off, esize), bits)
                        * sext_elem(read_elem(&b, off, esize), bits);
                    let high = if round {
                        (prod + (1i128 << (bits - 2))) >> (bits - 1)
                    } else {
                        prod >> (bits - 1)
                    };
                    write_elem(&mut dst, off, esize, high.clamp(lo, hi) as u64 & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE element count / inc-dec / stack allocation (0x04). The stack
            // forms (ADDVL/ADDPL/RDVL, bits[15:11]==01010) share bits[15:13]==010
            // with INDEX but differ at bit12; the count forms have bits[15:14]==11.
            // Routed before INDEX so the stack forms are not mis-decoded.
            0b000
                if (insn >> 24) & 0xFF == 0b00000100
                    && (insn >> 21) & 1 == 1
                    && (((insn >> 11) & 0x1F == 0b01010)
                        || matches!((insn >> 12) & 0xF, 0b1100 | 0b1110 | 0b1111)) =>
            {
                self.exec_sve_elem_count(insn)
            }

            // INDEX (immediate/scalar variants): bit21==1, bits[15:13]==010.
            0b000 if (insn >> 21) & 1 == 1 && (insn >> 13) & 0x7 == 0b010 => {
                self.exec_sve_index(insn, zd, esize)
            }

            // ZIP/UZP/TRN (unpredicated permute): 0x05, bit21==1, bits[15:13]==011.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 13) & 0x7 == 0b011 =>
            {
                self.exec_sve_zip_uzp_trn(insn, zd, zn, zm, esize)
            }

            // SEL Zd.T, Pg, Zn, Zm: 0x05, bit21==1, bits[15:14]==11. Per-element
            // merge governed by the 4-bit predicate Pg (bits[13:10]).
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 14) & 0x3 == 0b11 =>
            {
                let pg = ((insn >> 10) & 0xF) as usize;
                let pred = self.sve_p[pg];
                let elements = 16 / esize;
                let n_reg = self.v[zn].to_le_bytes();
                let m_reg = self.v[zm].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..elements {
                    let off = e * esize;
                    let src = if (pred >> (e * esize)) & 1 == 1 {
                        &n_reg
                    } else {
                        &m_reg
                    };
                    write_elem(&mut dst, off, esize, read_elem(src, off, esize));
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // REV Zd.T, Zn.T (reverse all elements): 0x05, bits[20:16]==11000,
            // bits[15:10]==001110.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 16) & 0x1F == 0b11000
                    && (insn >> 10) & 0x3F == 0b001110 =>
            {
                let n = 16 / esize;
                let a = self.v[zn].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..n {
                    write_elem(
                        &mut dst,
                        e * esize,
                        esize,
                        read_elem(&a, (n - 1 - e) * esize, esize),
                    );
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // CPY/MOV (predicated copy of immediate / scalar GPR / SIMD scalar),
            // all in the 0x05 space.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 20) & 0x3 == 0b01
                    && (insn >> 15) & 1 == 0 =>
            {
                self.exec_sve_cpy(insn, esize, 0) // CPY immediate
            }
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 16) & 0x3F == 0b101000
                    && (insn >> 13) & 0x7 == 0b101 =>
            {
                self.exec_sve_cpy(insn, esize, 1) // CPY scalar GPR
            }
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 16) & 0x3F == 0b100000
                    && (insn >> 13) & 0x7 == 0b100 =>
            {
                self.exec_sve_cpy(insn, esize, 2) // CPY SIMD scalar
            }

            // LASTA/LASTB to SIMD&FP scalar: 0x05, bits[21:17]==10001,
            // bit16: 0=A (after), 1=B (before), bits[15:13]==100.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 17) & 0x1F == 0b10001
                    && (insn >> 13) & 0x7 == 0b100 =>
            {
                self.exec_sve_last_scalar(insn, esize)
            }

            // LASTA/LASTB/CLASTA/CLASTB -> GPR: 0x05, bits[15:13]==101, bit21==1,
            // bits[19:17]==000. bit20: 0=LAST, 1=CLAST; bit16: 0=A (after), 1=B.
            0b000
                if (insn >> 24) & 0xFF == 0b00000101
                    && (insn >> 13) & 0x7 == 0b101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 17) & 0x7 == 0b000 =>
            {
                self.exec_sve_lastx(insn, esize)
            }

            // ADR (vector address generation): 0x04, bit21==1, bits[15:12]==1010.
            // Zd[e] = Zn[e] + offset(Zm[e]) * 2^msz. bits[23:22] selects the
            // form: 00=D+SXTW(Zm<31:0>), 01=D+UXTW(Zm<31:0>), 10=S packed,
            // 11=D packed. msz = bits[11:10].
            0b000
                if (insn >> 24) & 0xFF == 0b00000100
                    && (insn >> 21) & 1 == 1
                    && (insn >> 12) & 0xF == 0b1010 =>
            {
                let mode = (insn >> 22) & 0x3;
                let msz = (insn >> 10) & 0x3;
                let esize = if mode == 0b10 { 4 } else { 8 };
                let elements = 16 / esize;
                let m = elem_mask((esize * 8) as u32);
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..elements {
                    let off = e * esize;
                    let base = read_elem(&a, off, esize);
                    let zmv = read_elem(&b, off, esize);
                    let offset = match mode {
                        0b00 => (zmv as u32 as i32 as i64 as u64) << msz, // SXTW
                        0b01 => (zmv as u32 as u64) << msz,               // UXTW
                        _ => zmv << msz,                                  // packed S/D
                    };
                    write_elem(&mut dst, off, esize, base.wrapping_add(offset) & m);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // MOVPRFX Zd, Zn (unpredicated whole-register copy): 0x04,
            // bits[23:16]==00100000, bits[15:10]==101111. Standalone it is a
            // plain vector move.
            0b000
                if (insn >> 24) & 0xFF == 0b00000100
                    && (insn >> 16) & 0xFF == 0b00100000
                    && (insn >> 10) & 0x3F == 0b101111 =>
            {
                self.v[zd] = self.v[zn];
                Ok(CpuExit::Continue)
            }

            // MOVPRFX Zd.T, Pg/M-or-Z, Zn.T (predicated copy): 0x04,
            // bits[21:18]==0100, bit17==0, bits[15:13]==001. Active lanes copy
            // Zn; inactive lanes merge (M=1, keep Zd) or zero (M=0). Must precede
            // the integer-reduction arm, which shares bit21==0 && bits[15:13]==001.
            0b000
                if (insn >> 24) & 0xFF == 0b00000100
                    && (insn >> 18) & 0xF == 0b0100
                    && (insn >> 17) & 1 == 0
                    && (insn >> 13) & 0x7 == 0b001 =>
            {
                let merging = (insn >> 16) & 1 == 1;
                let pred = self.sve_p[pg];
                let elements = 16 / esize;
                let n_reg = self.v[zn].to_le_bytes();
                let mut dst = self.v[zd].to_le_bytes(); // merging base = prior Zd
                for e in 0..elements {
                    let off = e * esize;
                    if (pred >> off) & 1 == 1 {
                        write_elem(&mut dst, off, esize, read_elem(&n_reg, off, esize));
                    } else if !merging {
                        write_elem(&mut dst, off, esize, 0);
                    }
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2.1 integer quadword reductions (ADDQV/SMAXQV/UMAXQV/SMINQV/
            // UMINQV/ANDQV/ORQV/EORQV): 0x04, bit21==0, bits[15:13]==001, and
            // bit18==1 (the QV opcodes set the high bit of bits[18:16], unlike the
            // scalar reductions below). Reduces each element position across the
            // 128-bit segments into a single quadword in Vd. Must precede the
            // scalar-reduction arm, which shares bit21==0 && bits[15:13]==001.
            0b000
                if (insn >> 24) & 0xFF == 0b00000100
                    && (insn >> 21) & 1 == 0
                    && (insn >> 13) & 0x7 == 0b001
                    && (insn >> 18) & 1 == 1 =>
            {
                if !has_sve2p1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                self.exec_sve_qv_reduce_int(insn, esize)
            }

            // Integer reductions to a scalar (SADDV/UADDV/SMAXV/.../ANDV/ORV/
            // EORV): bit21==0, bits[15:13]==001.
            0b000 if (insn >> 21) & 1 == 0 && (insn >> 13) & 0x7 == 0b001 => {
                self.exec_sve_int_reduce(insn, esize)
            }

            // Predicated shift by vector (ASR/LSR/LSL Zdn, Pg/M, Zdn, Zm):
            // bits[15:13]==100, bits[21:19]==010.
            0b000
                if (insn >> 13) & 0x7 == 0b100
                    && (insn >> 19) & 0x7 == 0b010
                    && (insn >> 21) & 1 == 0 =>
            {
                self.exec_sve_shift_pred(insn, zd, zn, pg, esize)
            }

            // Predicated shift by wide elements (ASR/LSR/LSL Zdn, Pg/M, Zdn,
            // Zm.D): byte/half/word lanes use the 64-bit Zm element that covers
            // the lane as the shift amount; doubleword lanes are unallocated.
            0b000
                if top == 0b0000_0100
                    && (insn >> 13) & 0x7 == 0b100
                    && (insn >> 19) & 0x7 == 0b011
                    && matches!((insn >> 16) & 0x7, 0b000 | 0b001 | 0b011) =>
            {
                self.exec_sve_shift_wide_pred(insn, zd, zn, pg, esize)
            }

            // Unpredicated shift by wide elements (ASR/LSR/LSL Zd, Zn, Zm.D).
            0b000
                if top == 0b0000_0100
                    && (insn >> 21) & 1 == 1
                    && (insn >> 13) & 0x7 == 0b100
                    && matches!((insn >> 10) & 0x7, 0b000 | 0b001 | 0b011) =>
            {
                self.exec_sve_shift_wide_unpred(insn, zd, zn, zm, esize)
            }

            // Unpredicated shift by immediate (ASR/LSR/LSL Zd, Zn, #imm).
            0b000
                if top == 0b0000_0100
                    && (insn >> 21) & 1 == 1
                    && matches!((insn >> 10) & 0x3F, 0b100100 | 0b100101 | 0b100111) =>
            {
                self.exec_sve_shift_imm_unpred(insn)
            }

            // Predicated shift by immediate: bits[15:13]==100, bits[21:20]==00
            // (bits[21:19] 000 => ASR/LSR/LSL/ASRD/SQSHL/UQSHL, 001 => SRSHR/
            // URSHR/SQSHLU).
            0b000 if (insn >> 13) & 0x7 == 0b100 && (insn >> 20) & 0x3 == 0b00 => {
                self.exec_sve_shift_imm(insn)
            }

            // SVE predicated integer/FP unary (merging): 0x04, bits[15:13]==101,
            // bits[21:19] in {010,011}. opc=bits[21:16] selects SXTB/H/W, UXTB/
            // H/W, ABS, NEG, CLS, CLZ, CNT, CNOT, FABS, FNEG, NOT.
            0b000
                if (insn >> 24) & 0xFF == 0b00000100
                    && (insn >> 13) & 0x7 == 0b101
                    && matches!((insn >> 19) & 0x7, 0b010 | 0b011) =>
            {
                let opc = (insn >> 16) & 0x3F;
                let bits = (esize * 8) as u32;
                let mask = elem_mask(bits);
                let signbit = 1u64 << (bits - 1);
                let pred = self.sve_p[pg];
                let src = self.v[zn].to_le_bytes();
                let mut dst = self.v[zd].to_le_bytes();
                // Validity: the extend width must be smaller than the element.
                // Checked up front — an unallocated size is UNDEF even when
                // the governing predicate has no active elements.
                let ext_ok = |w: u32| bits > w;
                let opc_valid = match opc {
                    0b010000 | 0b010001 => ext_ok(8),
                    0b010010 | 0b010011 => ext_ok(16),
                    0b010100 | 0b010101 => ext_ok(32),
                    0b010110..=0b011011 | 0b011110 => true,
                    0b011100 | 0b011101 => esize >= 2,
                    _ => false,
                };
                if !opc_valid {
                    return Ok(CpuExit::Undefined(insn));
                }
                for e in 0..(16 / esize) {
                    let off = e * esize;
                    if (pred >> off) & 1 == 0 {
                        continue;
                    }
                    let v = read_elem(&src, off, esize);
                    let r: u64 = match opc {
                        0b010000 if ext_ok(8) => sext_elem(v, 8) as u64 & mask, // SXTB
                        0b010001 if ext_ok(8) => v & 0xFF,                      // UXTB
                        0b010010 if ext_ok(16) => sext_elem(v, 16) as u64 & mask, // SXTH
                        0b010011 if ext_ok(16) => v & 0xFFFF,                   // UXTH
                        0b010100 if ext_ok(32) => sext_elem(v, 32) as u64 & mask, // SXTW
                        0b010101 if ext_ok(32) => v & 0xFFFF_FFFF,              // UXTW
                        0b010110 => sext_elem(v, bits).unsigned_abs() as u64 & mask, // ABS
                        0b010111 => (-sext_elem(v, bits)) as u64 & mask,        // NEG
                        0b011000 => count_leading_sign(v, bits),                // CLS
                        0b011001 => count_leading_zeros_elem(v, bits),          // CLZ
                        0b011010 => (v & mask).count_ones() as u64,             // CNT
                        0b011011 => u64::from(v & mask == 0),                   // CNOT
                        0b011100 if esize >= 2 => fp_abs_bits_with_fpcr(v, bits, self.fpcr), // FABS
                        0b011101 if esize >= 2 => fp_neg_bits_with_fpcr(v, bits, self.fpcr), // FNEG
                        0b011110 => !v & mask,                                  // NOT
                        _ => return Ok(CpuExit::Undefined(insn)),
                    };
                    write_elem(&mut dst, off, esize, r);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE predicated integer multiply-add: 0x04, bit21==0, bits[15:13]
            // ==010 (MLA: d=Za+Zn*Zm), 011 (MLS: d=Za-Zn*Zm), 110 (MAD:
            // d=Za+Zdn*Zm), 111 (MSB: d=Za-Zdn*Zm). Low-half integer multiply,
            // merging. MLA/MLS keep Za in Zd; MAD/MSB keep a multiplicand in Zd.
            0b000
                if (insn >> 24) & 0xFF == 0b00000100
                    && (insn >> 21) & 1 == 0
                    && matches!((insn >> 13) & 0x7, 0b010 | 0b011 | 0b110 | 0b111) =>
            {
                let op3 = (insn >> 13) & 0x7;
                let sub = op3 & 1 == 1; // MLS/MSB
                let mad = op3 & 0x4 != 0; // MAD/MSB (Zdn is a multiplicand)
                let rm = ((insn >> 16) & 0x1F) as usize;
                let r95 = ((insn >> 5) & 0x1F) as usize;
                let (f1, f2, ar) = if mad { (zd, rm, r95) } else { (r95, rm, zd) };
                let pred = self.sve_p[pg];
                let mask = elem_mask((esize * 8) as u32);
                let fb1 = self.v[f1].to_le_bytes();
                let fb2 = self.v[f2].to_le_bytes();
                let ab = self.v[ar].to_le_bytes();
                let mut dst = self.v[zd].to_le_bytes();
                for e in 0..(16 / esize) {
                    let off = e * esize;
                    if (pred >> off) & 1 == 0 {
                        continue;
                    }
                    let prod =
                        read_elem(&fb1, off, esize).wrapping_mul(read_elem(&fb2, off, esize));
                    let a = read_elem(&ab, off, esize);
                    let r = if sub {
                        a.wrapping_sub(prod)
                    } else {
                        a.wrapping_add(prod)
                    } & mask;
                    write_elem(&mut dst, off, esize, r);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // Integer predicated binary operations
            0b000 if (op1 & 0x2) == 0 && (op2 & 0x10) == 0 => {
                self.exec_sve_int_pred(insn, zd, zn, zm, pg, esize)
            }

            // Unpredicated arithmetic
            0b000 if op1 == 0b01 => self.exec_sve_int_unpred(insn, zd, zn, zm, esize),

            // SVE2.1 PSEL (predicate select): 0x25, bit21==1, bits[15:14]==01,
            // bit9==0, bit4==0. Pd = Pn if the Pm element at (Wv+imm) mod
            // elements is active, else Pd is all-false. The element size and imm
            // are tsz-encoded in bits[23:18]; Wv = W(bits[17:16]+12).
            0b001
                if (insn >> 24) & 0xFF == 0b00100101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 14) & 0x3 == 0b01
                    && (insn >> 9) & 1 == 0
                    && (insn >> 4) & 1 == 0 =>
            {
                if !has_sve2p1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let b2322 = (insn >> 22) & 0x3;
                let b20 = (insn >> 20) & 1;
                let b19 = (insn >> 19) & 1;
                let b18 = (insn >> 18) & 1;
                let (esz, imm): (usize, u64) = if b18 == 1 {
                    (0, ((b2322 << 2) | ((insn >> 19) & 0x3)) as u64)
                } else if b19 == 1 {
                    (1, ((b2322 << 1) | b20) as u64)
                } else if b20 == 1 {
                    (2, b2322 as u64)
                } else if (insn >> 22) & 1 == 1 {
                    (3, ((insn >> 23) & 1) as u64)
                } else {
                    return Ok(CpuExit::Undefined(insn));
                };
                let esize = 1usize << esz;
                let elements = (16 / esize) as u64;
                let rv = 12 + ((insn >> 16) & 0x3) as u8;
                let wv = self.get_x(rv) & 0xFFFF_FFFF;
                let idx = (wv.wrapping_add(imm)) % elements;
                let pm = ((insn >> 5) & 0xF) as usize;
                let pn = ((insn >> 10) & 0xF) as usize;
                let pd = (insn & 0xF) as usize;
                let active = (self.sve_p[pm] >> (idx as usize * esize)) & 1 == 1;
                self.sve_p[pd] = if active { self.sve_p[pn] } else { 0 };
                Ok(CpuExit::Continue)
            }

            // SVE2.1 PEXT (extract a predicate from a predicate-as-counter):
            // 0x25, bit21==1, bits[20:16]==00000, bits[15:12]==0111, bit4==1.
            // PEXT_1 (bits[11:10]==00, imm=bits[9:8]) writes one predicate;
            // PEXT_2 (bits[11:9]==010, imm=bit8) writes the pair {Pd, Pd+1}. The
            // counter source is PN(8+bits[7:5]). Decodes the counter (qemu
            // decode_counter/CounterToPredicate) into the `part`-th VL-sized chunk.
            0b001
                if (insn >> 24) & 0xFF == 0b00100101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 16) & 0x1F == 0b00000
                    && (insn >> 12) & 0xF == 0b0111
                    && (insn >> 4) & 1 == 1 =>
            {
                if !has_sve2p1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let v_esz = ((insn >> 22) & 0x3) as usize;
                let (n, imm) = if (insn >> 10) & 0x3 == 0b00 {
                    (1usize, ((insn >> 8) & 0x3) as usize) // PEXT_1
                } else if (insn >> 9) & 0x7 == 0b010 {
                    (2usize, ((insn >> 8) & 1) as usize) // PEXT_2
                } else {
                    return Ok(CpuExit::Undefined(insn));
                };
                let png = self.sve_p[(8 + ((insn >> 5) & 0x7)) as usize] & 0xFFFF;
                let rd_base = (insn & 0xF) as usize;
                const VL_BYTES: usize = 16; // VL=128 -> 16-byte vector, 16-bit predicate
                const PRED_MASKS: [u32; 5] = [0xFFFF, 0x5555, 0x1111, 0x0101, 0x0001];
                // decode_counter (predicate-as-counter -> element count/invert/stride)
                let (count, lg2_stride, invert) = if png & 0xF != 0 {
                    let p_esz = png.trailing_zeros() as usize;
                    let mut count = (png & ((VL_BYTES as u32) * 8 - 1)) as usize; // pow2ceil(16)<<3 -1 = 127
                    count >>= p_esz + 1;
                    let invert = (png >> 15) & 1 == 1;
                    let mut stride = 0usize;
                    if p_esz != v_esz {
                        if p_esz < v_esz {
                            let shift = v_esz - p_esz;
                            let trunc = count >> shift;
                            count = trunc + (count != (trunc << shift)) as usize;
                        } else {
                            let shift = p_esz - v_esz;
                            count <<= shift;
                            stride = shift;
                        }
                    }
                    (count, stride, invert)
                } else {
                    (0, 0, false)
                };
                let esz_mask = PRED_MASKS[v_esz + lg2_stride];
                let oprbits = VL_BYTES; // 16 predicate bit-positions at VL=128
                for i in 0..n {
                    let rd = (rd_base + i) % 16;
                    let part = imm * n + i;
                    let b_count = ((count << v_esz) as i64) - (VL_BYTES * part) as i64;
                    let pd = if invert {
                        if b_count <= 0 {
                            esz_mask // whilel(all)
                        } else if (b_count as usize) < oprbits {
                            // whileg: last (oprbits - b_count) positions active
                            let inv = b_count as usize;
                            esz_mask & !((1u32 << inv) - 1) & ((1u32 << oprbits) - 1)
                        } else {
                            0
                        }
                    } else if b_count > 0 {
                        // whilel: first min(b_count, oprbits) positions active
                        let c = (b_count as usize).min(oprbits);
                        esz_mask & ((1u32 << c) - 1)
                    } else {
                        0
                    };
                    self.sve_p[rd] = pd;
                }
                Ok(CpuExit::Continue)
            }

            // Predicate operations (WHILE, PTRUE, etc.)
            0b001 => self.exec_sve_pred_ops(insn),

            // DUP/MOV/INDEX
            0b000 if op1 == 0b10 || op1 == 0b11 => self.exec_sve_permute(insn, zd, zn, zm, esize),

            // FP predicated operations
            0b011 => self.exec_sve_fp_pred(insn, zd, zn, zm, pg, esize),

            // SVE2 integer add/subtract long/wide and abs-diff long: 0x45,
            // bit21==0, bits[15:13] selects the group — 000 = add/sub LONG (both
            // operands widened from half-width), 001 = ABS-DIFF long (|a-b|,
            // S=bit12 must be 1), 010 = add/sub WIDE (Zn already full width, Zm
            // widened). T (bit10) picks odd/even half-width source elements;
            // U (bit11) unsigned widening; S (bit12) subtract. size=00 reserved.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 0
                    && matches!((insn >> 13) & 0x7, 0b000 | 0b001 | 0b010) =>
            {
                let group = (insn >> 13) & 0x7;
                let size = (insn >> 22) & 0x3;
                if size == 0 || (group == 0b001 && (insn >> 12) & 1 == 0) {
                    return Ok(CpuExit::Undefined(insn));
                }
                let d_esize = 1usize << size;
                let s_esize = d_esize / 2;
                let s_bits = (s_esize * 8) as u32;
                let sub = (insn >> 12) & 1 == 1;
                let unsigned = (insn >> 11) & 1 == 1;
                let top = (insn >> 10) & 1 == 1;
                let elements = 16 / d_esize;
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = [0u8; 16];
                let mask = elem_mask((d_esize * 8) as u32);
                let widen = |x: u64| -> i128 {
                    if unsigned {
                        uext_elem(x, s_bits) as i128
                    } else {
                        sext_elem(x, s_bits)
                    }
                };
                for d in 0..elements {
                    let s_off = (2 * d + top as usize) * s_esize;
                    let vm = widen(read_elem(&b, s_off, s_esize));
                    let r: i128 = match group {
                        0b000 => {
                            let vn = widen(read_elem(&a, s_off, s_esize));
                            if sub { vn - vm } else { vn + vm }
                        }
                        0b001 => (widen(read_elem(&a, s_off, s_esize)) - vm).abs(),
                        _ => {
                            let vn = read_elem(&a, d * d_esize, d_esize) as i128;
                            if sub { vn - vm } else { vn + vm }
                        }
                    };
                    write_elem(&mut dst, d * d_esize, d_esize, (r as u64) & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 integer add/subtract interleaved long (SADDLBT/SSUBLBT/
            // SSUBLTB): 0x45, bit21==0, bits[15:12]==1000. Signed widening where
            // the two narrow source halves come from DIFFERENT positions:
            // bits[11:10]==00 SADDLBT (Zn bottom + Zm top), 10 SSUBLBT (Zn bottom
            // - Zm top), 11 SSUBLTB (Zn top - Zm bottom); 01 unallocated. size=00
            // reserved. Mirrors qemu's saddl/ssubl helper with sel={2,2,1}.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 0
                    && (insn >> 12) & 0xF == 0b1000 =>
            {
                let size = (insn >> 22) & 0x3;
                let op = (insn >> 10) & 0x3;
                if size == 0 || op == 0b01 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let sub = op != 0b00;
                // (seln, selm): which narrow half of Zn/Zm (0=bottom, 1=top).
                let (seln, selm) = if op == 0b11 {
                    (1usize, 0usize)
                } else {
                    (0usize, 1usize)
                };
                let d_esize = 1usize << size;
                let s_esize = d_esize / 2;
                let s_bits = (s_esize * 8) as u32;
                let elements = 16 / d_esize;
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = [0u8; 16];
                let mask = elem_mask((d_esize * 8) as u32);
                for d in 0..elements {
                    let n_off = (2 * d + seln) * s_esize;
                    let m_off = (2 * d + selm) * s_esize;
                    let vn = sext_elem(read_elem(&a, n_off, s_esize), s_bits);
                    let vm = sext_elem(read_elem(&b, m_off, s_esize), s_bits);
                    let r = if sub { vn - vm } else { vn + vm };
                    write_elem(&mut dst, d * d_esize, d_esize, (r as u64) & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2.1 SCLAMP/UCLAMP (signed/unsigned clamp): 0x44, bit21==0,
            // bits[15:11]==11000. bit10=U. Zd = min(max(Zd, Zn), Zm) per element
            // (Zd is both the clamped value and the destination).
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 0
                    && (insn >> 11) & 0x1F == 0b11000 =>
            {
                if !has_sve2p1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let unsigned = (insn >> 10) & 1 == 1;
                let bits = (esize * 8) as u32;
                let mask = elem_mask(bits);
                let n = self.v[zn].to_le_bytes();
                let m = self.v[zm].to_le_bytes();
                let d = self.v[zd].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..(16 / esize) {
                    let off = e * esize;
                    let (dv, nv, mv) = (
                        read_elem(&d, off, esize),
                        read_elem(&n, off, esize),
                        read_elem(&m, off, esize),
                    );
                    let r = if unsigned {
                        uext_elem(dv, bits)
                            .max(uext_elem(nv, bits))
                            .min(uext_elem(mv, bits)) as u64
                    } else {
                        sext_elem(dv, bits)
                            .max(sext_elem(nv, bits))
                            .min(sext_elem(mv, bits)) as u64
                    };
                    write_elem(&mut dst, off, esize, r & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2.1 quadword permutes ZIPQ1/ZIPQ2/UZPQ1/UZPQ2/TBLQ: 0x44,
            // bit21==0, bits[15:13]==111. opc=bits[12:10] (000/001 ZIP, 010/011
            // UZP, 110 TBLQ). These permute within each 128-bit segment; at VL=128
            // (a single segment) they coincide with the non-quadword ZIP/UZP/TBL
            // over the whole register. (TBXQ lives in the 0x05 space, handled
            // separately.)
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 0
                    && (insn >> 13) & 0x7 == 0b111 =>
            {
                if !has_sve2p1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let opc = (insn >> 10) & 0x7;
                let n = self.v[zn].to_le_bytes();
                let m = self.v[zm].to_le_bytes();
                let nelt = 16 / esize;
                let half = nelt / 2;
                let mut dst = [0u8; 16];
                match opc {
                    0b000 | 0b001 => {
                        // ZIPQ1 (low half) / ZIPQ2 (high half): interleave Zn,Zm.
                        let base = if opc == 0b001 { half } else { 0 };
                        for i in 0..half {
                            write_elem(
                                &mut dst,
                                (2 * i) * esize,
                                esize,
                                read_elem(&n, (base + i) * esize, esize),
                            );
                            write_elem(
                                &mut dst,
                                (2 * i + 1) * esize,
                                esize,
                                read_elem(&m, (base + i) * esize, esize),
                            );
                        }
                    }
                    0b010 | 0b011 => {
                        // UZPQ1 (even) / UZPQ2 (odd): deinterleave Zn:Zm.
                        let start = (opc & 1) as usize; // 0 even, 1 odd
                        for i in 0..nelt {
                            let src_idx = 2 * i + start;
                            let v = if src_idx < nelt {
                                read_elem(&n, src_idx * esize, esize)
                            } else {
                                read_elem(&m, (src_idx - nelt) * esize, esize)
                            };
                            write_elem(&mut dst, i * esize, esize, v);
                        }
                    }
                    0b110 => {
                        // TBLQ: per-segment table lookup of Zn indexed by Zm,
                        // zero-filling out-of-range indices.
                        for i in 0..nelt {
                            let idx = read_elem(&m, i * esize, esize) as usize;
                            let v = if idx < nelt {
                                read_elem(&n, idx * esize, esize)
                            } else {
                                0
                            };
                            write_elem(&mut dst, i * esize, esize, v);
                        }
                    }
                    _ => return Ok(CpuExit::Undefined(insn)),
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 shift right and accumulate: 0x45, bit21==0, bits[15:12]==1110.
            // SSRA/USRA (R=bit11=0) and SRSRA/URSRA (R=1); U=bit10 signedness.
            // Same-size (tsz=tszh:tszl 4 bits); shift = 2*esize - tsz:imm3.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 0
                    && (insn >> 12) & 0xF == 0b1110 =>
            {
                let tsize = (((insn >> 22) & 0x3) << 2) | ((insn >> 19) & 0x3);
                if tsize == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let bits = 8 << (31 - tsize.leading_zeros());
                let esize = (bits / 8) as usize;
                let amount = 2 * bits - ((tsize << 3) | ((insn >> 16) & 0x7));
                let round = (insn >> 11) & 1 == 1;
                let unsigned = (insn >> 10) & 1 == 1;
                let mask = elem_mask(bits);
                let elements = 16 / esize;
                let acc = self.v[zd].to_le_bytes();
                let n = self.v[zn].to_le_bytes();
                let mut dst = acc;
                for e in 0..elements {
                    let off = e * esize;
                    let x = read_elem(&n, off, esize);
                    let v: i128 = if unsigned {
                        uext_elem(x, bits) as i128
                    } else {
                        sext_elem(x, bits)
                    };
                    let shifted = if round {
                        (v + (1i128 << (amount - 1))) >> amount
                    } else {
                        v >> amount
                    };
                    let cur = read_elem(&acc, off, esize) as i128;
                    write_elem(&mut dst, off, esize, (cur + shifted) as u64 & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 shift and insert: 0x45, bit21==0, bits[15:11]==11110. op=bit10
            // selects SLI (shift left, preserve low bits) vs SRI (shift right,
            // preserve high bits). Same-size tsz decode.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 0
                    && (insn >> 11) & 0x1F == 0b11110 =>
            {
                let tsize = (((insn >> 22) & 0x3) << 2) | ((insn >> 19) & 0x3);
                if tsize == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let bits = 8 << (31 - tsize.leading_zeros());
                let esize = (bits / 8) as usize;
                let tszimm = (tsize << 3) | ((insn >> 16) & 0x7);
                let sli = (insn >> 10) & 1 == 1;
                let amount = if sli {
                    tszimm - bits
                } else {
                    2 * bits - tszimm
                };
                let mask = elem_mask(bits);
                let elements = 16 / esize;
                let dn = self.v[zd].to_le_bytes();
                let n = self.v[zn].to_le_bytes();
                let mut dst = dn;
                for e in 0..elements {
                    let off = e * esize;
                    let x = read_elem(&n, off, esize);
                    let d = read_elem(&dn, off, esize);
                    let r = if sli {
                        let keep = (1u64 << amount) - 1; // low `amount` dest bits preserved
                        ((x << amount) & mask) | (d & keep)
                    } else {
                        // SRI shift is 1..=esize; a full-width shift yields 0 (a
                        // u64 `>> bits` would otherwise wrap when bits==64).
                        let shifted = if amount >= bits {
                            0
                        } else {
                            (x >> amount) & mask
                        };
                        let keep = mask & !((1u64 << (bits - amount)).wrapping_sub(1)); // high bits
                        shifted | (d & keep)
                    };
                    write_elem(&mut dst, off, esize, r & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 abs-diff accumulate long: 0x45, bit21==0, bits[15:12]==1100.
            // SABALB/T (U=0) / UABALB/T (U=1): Zda += |widen(Zn) - widen(Zm)| over
            // the half-width even (T=0) / odd (T=1) source elements.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 0
                    && (insn >> 12) & 0xF == 0b1100 =>
            {
                let size = (insn >> 22) & 0x3;
                if size == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let d_esize = 1usize << size;
                let s_esize = d_esize / 2;
                let s_bits = (s_esize * 8) as u32;
                let mask = elem_mask((d_esize * 8) as u32);
                let unsigned = (insn >> 11) & 1 == 1;
                let top = (insn >> 10) & 1 == 1;
                let elements = 16 / d_esize;
                let acc = self.v[zd].to_le_bytes();
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = acc;
                let widen = |x: u64| -> i128 {
                    if unsigned {
                        uext_elem(x, s_bits) as i128
                    } else {
                        sext_elem(x, s_bits)
                    }
                };
                for d in 0..elements {
                    let off = (2 * d + top as usize) * s_esize;
                    let diff = (widen(read_elem(&a, off, s_esize))
                        - widen(read_elem(&b, off, s_esize)))
                    .abs();
                    let cur = read_elem(&acc, d * d_esize, d_esize) as i128;
                    write_elem(&mut dst, d * d_esize, d_esize, (cur + diff) as u64 & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 SABA/UABA (abs-diff accumulate, same width): 0x45, bit21==0,
            // bits[15:11]==11111, bit10 selects UABA(1)/SABA(0). The destination
            // accumulates the per-element absolute difference at full width.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 0
                    && (insn >> 11) & 0x1F == 0b11111 =>
            {
                let bits = (esize * 8) as u32;
                let mask = elem_mask(bits);
                let unsigned = (insn >> 10) & 1 == 1;
                let elements = 16 / esize;
                let acc = self.v[zd].to_le_bytes();
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = acc;
                for e in 0..elements {
                    let off = e * esize;
                    let (av, bv) = if unsigned {
                        (
                            uext_elem(read_elem(&a, off, esize), bits) as i128,
                            uext_elem(read_elem(&b, off, esize), bits) as i128,
                        )
                    } else {
                        (
                            sext_elem(read_elem(&a, off, esize), bits),
                            sext_elem(read_elem(&b, off, esize), bits),
                        )
                    };
                    let diff = (av - bv).abs() as u64 & mask;
                    let cur = read_elem(&acc, off, esize);
                    write_elem(&mut dst, off, esize, cur.wrapping_add(diff) & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 bit permute (BEXT/BDEP/BGRP): 0x45, bit21==0, bits[15:12]==1011.
            // opc=bits[11:10]: 00=BEXT (gather Zn bits at Zm's set bits to the
            // bottom, like PEXT), 01=BDEP (scatter Zn's low bits to Zm's set bits,
            // like PDEP), 10=BGRP (Zm-selected bits to the bottom, rest on top).
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 0
                    && (insn >> 12) & 0xF == 0b1011 =>
            {
                let opc = (insn >> 10) & 0x3;
                let bits = (esize * 8) as u32;
                let mask = elem_mask(bits);
                let elements = 16 / esize;
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..elements {
                    let off = e * esize;
                    let zn_e = read_elem(&a, off, esize);
                    let zm_e = read_elem(&b, off, esize);
                    let r = match opc {
                        0b00 => {
                            let mut r = 0u64;
                            let mut k = 0;
                            for i in 0..bits {
                                if (zm_e >> i) & 1 == 1 {
                                    r |= ((zn_e >> i) & 1) << k;
                                    k += 1;
                                }
                            }
                            r
                        }
                        0b01 => {
                            let mut r = 0u64;
                            let mut k = 0;
                            for i in 0..bits {
                                if (zm_e >> i) & 1 == 1 {
                                    r |= ((zn_e >> k) & 1) << i;
                                    k += 1;
                                }
                            }
                            r
                        }
                        0b10 => {
                            let (mut low, mut lk, mut high, mut hk) = (0u64, 0u32, 0u64, 0u32);
                            for i in 0..bits {
                                let bit = (zn_e >> i) & 1;
                                if (zm_e >> i) & 1 == 1 {
                                    low |= bit << lk;
                                    lk += 1;
                                } else {
                                    high |= bit << hk;
                                    hk += 1;
                                }
                            }
                            // When every mask bit is set, `lk` reaches `bits`
                            // (up to 64) and `high` stays 0; guard the shift so
                            // an all-ones 64-bit mask element cannot overflow.
                            low | high.checked_shl(lk).unwrap_or(0)
                        }
                        _ => return Ok(CpuExit::Undefined(insn)),
                    };
                    write_elem(&mut dst, off, esize, r & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 shift left long: 010001010 tszh 0 tszl imm3 1010 U T Zn Zd.
            // Widens the half-width source elements (signed U=0 / unsigned U=1,
            // even T=0 / odd T=1) and shifts them left. src esize from highest
            // set bit of tsz, dst 2x; shift = tsz:imm3 - src_bits.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 23) & 1 == 0
                    && (insn >> 21) & 1 == 0
                    && (insn >> 12) & 0xF == 0b1010 =>
            {
                let tsize = (((insn >> 22) & 1) << 2) | ((insn >> 19) & 0x3);
                if tsize == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let src_esize = 1usize << (31 - tsize.leading_zeros());
                let dst_esize = src_esize * 2;
                let src_bits = (src_esize * 8) as u32;
                let dst_bits = (dst_esize * 8) as u32;
                let dmask = elem_mask(dst_bits);
                let amount = ((tsize << 3) | ((insn >> 16) & 0x7)) - src_bits;
                let unsigned = (insn >> 11) & 1 == 1;
                let top = (insn >> 10) & 1 == 1;
                let n_dst = 16 / dst_esize;
                let a = self.v[zn].to_le_bytes();
                let mut dst = [0u8; 16];
                for d in 0..n_dst {
                    let x = read_elem(&a, (2 * d + top as usize) * src_esize, src_esize);
                    let widened: u128 = if unsigned {
                        uext_elem(x, src_bits)
                    } else {
                        sext_elem(x, src_bits) as u128
                    };
                    write_elem(
                        &mut dst,
                        d * dst_esize,
                        dst_esize,
                        (widened << amount) as u64 & dmask,
                    );
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 integer multiply long: 0x45, bit21==0, bits[15:13]==011.
            // (op=bit12, U=bit11): (1,0)=SMULLB/T, (1,1)=UMULLB/T, (0,0)=
            // SQDMULLB/T (saturating doubling), (0,1)=PMULLB/T (polynomial).
            // Source elements are half-width; T picks odd/even. size=00 reserved;
            // PMULL is only defined for the H form (size=01) in base SVE2.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 0
                    && (insn >> 13) & 0x7 == 0b011 =>
            {
                let size = (insn >> 22) & 0x3;
                let op = (insn >> 12) & 1;
                let unsigned = (insn >> 11) & 1 == 1;
                let top = (insn >> 10) & 1 == 1;
                let is_pmull = op == 0 && unsigned;
                // SMULL/UMULL/SQDMULL need a half-width source so size==0 is
                // reserved; PMULL is valid for .q (size==0, 64->128), .h
                // (size==01, 8->16) and .d (size==11, 32->64) but not size==10.
                if (size == 0 && !is_pmull) || (is_pmull && size == 2) {
                    return Ok(CpuExit::Undefined(insn));
                }
                if is_pmull && size == 0 {
                    // PMULLB/T .q <- .d: 64x64 -> 128 carryless. T selects the
                    // odd (high) 64-bit lane of the segment, B the even (low).
                    let lane = top as usize;
                    let xn = (self.v[zn] >> (lane * 64)) as u64;
                    let xm = (self.v[zm] >> (lane * 64)) as u64;
                    self.v[zd] = poly_mul_64(xn, xm);
                    return Ok(CpuExit::Continue);
                }
                let d_esize = 1usize << size;
                let s_esize = d_esize / 2;
                let s_bits = (s_esize * 8) as u32;
                let d_bits = (d_esize * 8) as u32;
                let elements = 16 / d_esize;
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = [0u8; 16];
                let mask = elem_mask(d_bits);
                for d in 0..elements {
                    let off = (2 * d + top as usize) * s_esize;
                    let xn = read_elem(&a, off, s_esize);
                    let xm = read_elem(&b, off, s_esize);
                    let r: u64 = match (op, unsigned) {
                        (1, false) => (sext_elem(xn, s_bits) * sext_elem(xm, s_bits)) as u64 & mask,
                        (1, true) => (uext_elem(xn, s_bits) * uext_elem(xm, s_bits)) as u64 & mask,
                        (0, false) => {
                            let prod = 2i128 * sext_elem(xn, s_bits) * sext_elem(xm, s_bits);
                            let hi = (1i128 << (d_bits - 1)) - 1;
                            let lo = -(1i128 << (d_bits - 1));
                            prod.clamp(lo, hi) as u64 & mask
                        }
                        _ => poly_mul_wide(xn, xm, s_bits) & mask,
                    };
                    write_elem(&mut dst, d * d_esize, d_esize, r);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 complex integer add (CADD/SQCADD): 0x45, bits[21:17]==00000,
            // bits[15:11]==11011. Treats element pairs as (real, imag); adds Zm
            // rotated by 90 (rot=0) or 270 (rot=1) degrees into Zdn. op=bit16
            // selects the saturating form (SQCADD).
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 17) & 0x1F == 0
                    && (insn >> 11) & 0x1F == 0b11011 =>
            {
                let size = (insn >> 22) & 0x3;
                let esize = 1usize << size;
                let bits = (esize * 8) as u32;
                let mask = elem_mask(bits);
                let sat = (insn >> 16) & 1 == 1;
                let rot = (insn >> 10) & 1;
                let dn = self.v[zd].to_le_bytes(); // Zdn
                let m = self.v[zn].to_le_bytes(); // Zm (bits[9:5])
                let mut dst = dn;
                let pairs = (16 / esize) / 2;
                let hi = (1i128 << (bits - 1)) - 1;
                let lo = -(1i128 << (bits - 1));
                let clamp = |v: i128| if sat { v.clamp(lo, hi) } else { v };
                for p in 0..pairs {
                    let (re, im) = (2 * p * esize, (2 * p + 1) * esize);
                    let dn_re = sext_elem(read_elem(&dn, re, esize), bits);
                    let dn_im = sext_elem(read_elem(&dn, im, esize), bits);
                    let m_re = sext_elem(read_elem(&m, re, esize), bits);
                    let m_im = sext_elem(read_elem(&m, im, esize), bits);
                    let (r_re, r_im) = if rot == 0 {
                        (dn_re - m_im, dn_im + m_re) // rotate Zm by 90 degrees
                    } else {
                        (dn_re + m_im, dn_im - m_re) // rotate Zm by 270 degrees
                    };
                    write_elem(&mut dst, re, esize, clamp(r_re) as u64 & mask);
                    write_elem(&mut dst, im, esize, clamp(r_im) as u64 & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 shift right narrow: 010001010 tszh 1 tszl imm3 00 op U R T.
            // (op,U): (0,1)=SHRN/RSHRN, (0,0)=SQSHRUN, (1,0)=SQSHRN, (1,1)=UQSHRN
            // (R=bit11 adds rounding). dst esize from highest set bit of tsz, src
            // 2x; shift amount = 2*dst_bits - (tsz:imm3).
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 23) & 1 == 0
                    && (insn >> 21) & 1 == 1
                    && (insn >> 14) & 0x3 == 0 =>
            {
                let tsize = (((insn >> 22) & 1) << 2) | ((insn >> 19) & 0x3);
                if tsize == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let dst_esize = 1usize << (31 - tsize.leading_zeros());
                let src_esize = dst_esize * 2;
                let dst_bits = (dst_esize * 8) as u32;
                let src_bits = (src_esize * 8) as u32;
                let dmask = elem_mask(dst_bits);
                let tszimm = (tsize << 3) | ((insn >> 16) & 0x7);
                let amount = src_bits - tszimm; // 1..=dst_bits
                let op = (insn >> 13) & 1;
                let u = (insn >> 12) & 1;
                let round = (insn >> 11) & 1 == 1;
                let top = (insn >> 10) & 1 == 1;
                let n_src = 16 / src_esize;
                let a = self.v[zn].to_le_bytes();
                let mut dst = if top {
                    self.v[zd].to_le_bytes()
                } else {
                    [0u8; 16]
                };
                for d in 0..n_src {
                    let x = read_elem(&a, d * src_esize, src_esize);
                    let narrow: u64 = match (op, u) {
                        (0, 1) => {
                            let v = uext_elem(x, src_bits);
                            let r = if round {
                                (v + (1u128 << (amount - 1))) >> amount
                            } else {
                                v >> amount
                            };
                            r as u64 & dmask
                        }
                        (0, 0) => {
                            let v = sext_elem(x, src_bits);
                            let r = if round {
                                (v + (1i128 << (amount - 1))) >> amount
                            } else {
                                v >> amount
                            };
                            r.clamp(0, dmask as i128) as u64
                        }
                        (1, 0) => {
                            let v = sext_elem(x, src_bits);
                            let r = if round {
                                (v + (1i128 << (amount - 1))) >> amount
                            } else {
                                v >> amount
                            };
                            let hi = (1i128 << (dst_bits - 1)) - 1;
                            let lo = -(1i128 << (dst_bits - 1));
                            r.clamp(lo, hi) as u64 & dmask
                        }
                        _ => {
                            let v = uext_elem(x, src_bits);
                            let r = if round {
                                (v + (1u128 << (amount - 1))) >> amount
                            } else {
                                v >> amount
                            };
                            r.min(dmask as u128) as u64
                        }
                    };
                    write_elem(
                        &mut dst,
                        (2 * d + top as usize) * dst_esize,
                        dst_esize,
                        narrow,
                    );
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2.1 multi-vector saturating extract narrow SQCVTN/UQCVTN/
            // SQCVTUN (.s pair -> .h): 0x45, bits[23:22]==00, bit21==1,
            // bits[20:16]==10001, bits[15:13]==010. op=bits[12:10] (000 SQCVTN
            // signed->signed, 010 UQCVTN unsigned->unsigned, 100 SQCVTUN
            // signed->unsigned). Reads the register pair {Zn, Zn+1} and
            // interleaves: Zd.h[2i]=sat(Zn.s[i]), Zd.h[2i+1]=sat(Zn+1.s[i]).
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 22) & 0x3 == 0b00
                    && (insn >> 21) & 1 == 1
                    && (insn >> 16) & 0x1F == 0b10001
                    && (insn >> 13) & 0x7 == 0b010 =>
            {
                if !has_sve2p1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let (signed_in, signed_out) = match (insn >> 10) & 0x7 {
                    0b000 => (true, true),   // SQCVTN
                    0b010 => (false, false), // UQCVTN
                    0b100 => (true, false),  // SQCVTUN
                    _ => return Ok(CpuExit::Undefined(insn)),
                };
                let zn = ((insn >> 5) & 0x1F) as usize;
                let s0 = self.v[zn].to_le_bytes();
                let s1 = self.v[(zn + 1) % 32].to_le_bytes();
                let mut dst = [0u8; 16];
                let narrow = |bytes: &[u8; 16], i: usize| -> u64 {
                    let v = read_elem(bytes, i * 4, 4);
                    let w = if signed_in {
                        sext_elem(v, 32)
                    } else {
                        uext_elem(v, 32) as i128
                    };
                    if signed_out {
                        sat_signed(w, 16)
                    } else {
                        sat_unsigned(w, 16)
                    }
                };
                for i in 0..4 {
                    write_elem(&mut dst, (2 * i) * 2, 2, narrow(&s0, i));
                    write_elem(&mut dst, (2 * i + 1) * 2, 2, narrow(&s1, i));
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 saturating extract narrow: 010001010 tszh 1 tszl 000010 vv T.
            // (bit12,bit11): 00=SQXTN (signed->signed sat), 01=UQXTN (unsigned->
            // unsigned sat), 10=SQXTUN (signed->unsigned sat). The dest element
            // size comes from the highest set bit of tsz=tszh:tszl, source 2x.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 23) & 1 == 0
                    && (insn >> 21) & 1 == 1
                    && (insn >> 13) & 0x3F == 0b000010 =>
            {
                let tsz = (((insn >> 22) & 1) << 2) | ((insn >> 19) & 0x3);
                // Only the one-hot tsz values 0b001/0b010/0b100 are allocated;
                // 0b000 and the non-one-hot 0b011/0b101/0b110/0b111 are reserved.
                if !tsz.is_power_of_two() {
                    return Ok(CpuExit::Undefined(insn));
                }
                // Only variants 0b00 (SQXTN), 0b01 (UQXTN), 0b10 (SQXTUN) are
                // defined; 0b11 is reserved and must trap.
                let variant = (insn >> 11) & 0x3;
                if variant == 0b11 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let hsb = 31 - tsz.leading_zeros();
                let dst_esize = 1usize << hsb;
                let src_esize = dst_esize * 2;
                let dst_bits = (dst_esize * 8) as u32;
                let src_bits = (src_esize * 8) as u32;
                let dmask = elem_mask(dst_bits);
                let top = (insn >> 10) & 1 == 1;
                let n_src = 16 / src_esize;
                let a = self.v[zn].to_le_bytes();
                let mut dst = if top {
                    self.v[zd].to_le_bytes()
                } else {
                    [0u8; 16]
                };
                for d in 0..n_src {
                    let x = read_elem(&a, d * src_esize, src_esize);
                    let narrow: u64 = match variant {
                        0b00 => {
                            let v = sext_elem(x, src_bits);
                            let hi = (1i128 << (dst_bits - 1)) - 1;
                            let lo = -(1i128 << (dst_bits - 1));
                            v.clamp(lo, hi) as u64 & dmask
                        }
                        0b01 => uext_elem(x, src_bits).min(dmask as u128) as u64,
                        _ => sext_elem(x, src_bits).clamp(0, dmask as i128) as u64,
                    };
                    write_elem(
                        &mut dst,
                        (2 * d + top as usize) * dst_esize,
                        dst_esize,
                        narrow,
                    );
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 add/subtract high narrow: 0x45, bit21==1, bits[15:13]==011.
            // ADDHN/SUBHN (S=bit12) with optional rounding (R=bit11). The result
            // is the high half of the (full-width) sum/difference, written to the
            // even (T=0, bottom, other half zeroed) or odd (T=1, top, other half
            // preserved) narrow elements. size=00 reserved.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 13) & 0x7 == 0b011 =>
            {
                let size = (insn >> 22) & 0x3;
                if size == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let src_esize = 1usize << size;
                let dst_esize = src_esize / 2;
                let src_mask = elem_mask((src_esize * 8) as u32);
                let dst_bits = (dst_esize * 8) as u32;
                let dst_mask = elem_mask(dst_bits);
                let sub = (insn >> 12) & 1 == 1;
                let round = (insn >> 11) & 1 == 1;
                let top = (insn >> 10) & 1 == 1;
                let n_src = 16 / src_esize;
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = if top {
                    self.v[zd].to_le_bytes()
                } else {
                    [0u8; 16]
                };
                for d in 0..n_src {
                    let xn = read_elem(&a, d * src_esize, src_esize);
                    let xm = read_elem(&b, d * src_esize, src_esize);
                    let sum = if sub {
                        xn.wrapping_sub(xm)
                    } else {
                        xn.wrapping_add(xm)
                    };
                    let rounded = if round {
                        sum.wrapping_add(1u64 << (dst_bits - 1))
                    } else {
                        sum
                    } & src_mask;
                    let narrow = (rounded >> dst_bits) & dst_mask;
                    write_elem(
                        &mut dst,
                        (2 * d + top as usize) * dst_esize,
                        dst_esize,
                        narrow,
                    );
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 integer multiply-add long: 0x44, bit21==0, bits[15:13]==010.
            // S?MLALB/T (S=0) and S?MLSLB/T (S=1); U widening sign; T odd/even.
            // Zda (the destination, bits[4:0]) accumulates widen(Zn)*widen(Zm).
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 0
                    && (insn >> 13) & 0x7 == 0b010 =>
            {
                let size = (insn >> 22) & 0x3;
                if size == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let d_esize = 1usize << size;
                let s_esize = d_esize / 2;
                let s_bits = (s_esize * 8) as u32;
                let mask = elem_mask((d_esize * 8) as u32);
                let sub = (insn >> 12) & 1 == 1;
                let unsigned = (insn >> 11) & 1 == 1;
                let top = (insn >> 10) & 1 == 1;
                let elements = 16 / d_esize;
                let acc = self.v[zd].to_le_bytes();
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = acc;
                for d in 0..elements {
                    let off = (2 * d + top as usize) * s_esize;
                    let xn = read_elem(&a, off, s_esize);
                    let xm = read_elem(&b, off, s_esize);
                    let prod: i128 = if unsigned {
                        (uext_elem(xn, s_bits) * uext_elem(xm, s_bits)) as i128
                    } else {
                        sext_elem(xn, s_bits) * sext_elem(xm, s_bits)
                    };
                    let cur = read_elem(&acc, d * d_esize, d_esize) as i128;
                    let r = if sub { cur - prod } else { cur + prod };
                    write_elem(&mut dst, d * d_esize, d_esize, (r as u64) & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 saturating doubling multiply-add long: 0x44, bit21==0,
            // bits[15:12]==0110. SQDMLALB/T (S=0) / SQDMLSLB/T (S=1). The doubled
            // signed product is saturated, then the accumulate is saturated.
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 0
                    && (insn >> 12) & 0xF == 0b0110 =>
            {
                let size = (insn >> 22) & 0x3;
                if size == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let d_esize = 1usize << size;
                let s_esize = d_esize / 2;
                let s_bits = (s_esize * 8) as u32;
                let d_bits = (d_esize * 8) as u32;
                let mask = elem_mask(d_bits);
                let sub = (insn >> 11) & 1 == 1;
                let top = (insn >> 10) & 1 == 1;
                let elements = 16 / d_esize;
                let hi = (1i128 << (d_bits - 1)) - 1;
                let lo = -(1i128 << (d_bits - 1));
                let acc = self.v[zd].to_le_bytes();
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = acc;
                for d in 0..elements {
                    let off = (2 * d + top as usize) * s_esize;
                    let prod = 2i128
                        * sext_elem(read_elem(&a, off, s_esize), s_bits)
                        * sext_elem(read_elem(&b, off, s_esize), s_bits);
                    let sat = prod.clamp(lo, hi);
                    let cur = sext_elem(read_elem(&acc, d * d_esize, d_esize), d_bits);
                    let r = if sub { cur - sat } else { cur + sat };
                    write_elem(
                        &mut dst,
                        d * d_esize,
                        d_esize,
                        (r.clamp(lo, hi) as u64) & mask,
                    );
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 saturating doubling multiply-add long (interleaved):
            // SQDMLALBT/SQDMLSLBT. 0x44, bit21==0, bits[15:11]==00001, bit10=S
            // (0=add, 1=sub). The two narrow sources come from DIFFERENT halves
            // (Zn bottom * Zm top, sel=2), unlike the B/T forms above; the doubled
            // product saturates, then the accumulate saturates. size=00 reserved.
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 0
                    && (insn >> 11) & 0x1F == 0b00001 =>
            {
                let size = (insn >> 22) & 0x3;
                if size == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let d_esize = 1usize << size;
                let s_esize = d_esize / 2;
                let s_bits = (s_esize * 8) as u32;
                let d_bits = (d_esize * 8) as u32;
                let mask = elem_mask(d_bits);
                let sub = (insn >> 10) & 1 == 1;
                let elements = 16 / d_esize;
                let hi = (1i128 << (d_bits - 1)) - 1;
                let lo = -(1i128 << (d_bits - 1));
                let acc = self.v[zd].to_le_bytes();
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = acc;
                for d in 0..elements {
                    let n_off = (2 * d) * s_esize; // Zn bottom
                    let m_off = (2 * d + 1) * s_esize; // Zm top
                    let prod = 2i128
                        * sext_elem(read_elem(&a, n_off, s_esize), s_bits)
                        * sext_elem(read_elem(&b, m_off, s_esize), s_bits);
                    let sat = prod.clamp(lo, hi);
                    let cur = sext_elem(read_elem(&acc, d * d_esize, d_esize), d_bits);
                    let r = if sub { cur - sat } else { cur + sat };
                    write_elem(
                        &mut dst,
                        d * d_esize,
                        d_esize,
                        (r.clamp(lo, hi) as u64) & mask,
                    );
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 SADALP/UADALP (add long pairwise, accumulate): 0x44,
            // bits[21:17]==00010, bits[15:13]==101. U=bit16. Each (active)
            // destination element gains the widened sum of a pair of half-width
            // source elements; inactive lanes keep the prior accumulator.
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 17) & 0x1F == 0b00010
                    && (insn >> 13) & 0x7 == 0b101 =>
            {
                let size = (insn >> 22) & 0x3;
                if size == 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let d_esize = 1usize << size;
                let s_esize = d_esize / 2;
                let s_bits = (s_esize * 8) as u32;
                let mask = elem_mask((d_esize * 8) as u32);
                let unsigned = (insn >> 16) & 1 == 1;
                let pred = self.sve_p[pg];
                let elements = 16 / d_esize;
                let acc = self.v[zd].to_le_bytes();
                let n = self.v[zn].to_le_bytes();
                let mut dst = acc;
                let widen = |x: u64| -> i128 {
                    if unsigned {
                        uext_elem(x, s_bits) as i128
                    } else {
                        sext_elem(x, s_bits)
                    }
                };
                for d in 0..elements {
                    if (pred >> (d * d_esize)) & 1 == 0 {
                        continue;
                    }
                    let pair = widen(read_elem(&n, 2 * d * s_esize, s_esize))
                        + widen(read_elem(&n, (2 * d + 1) * s_esize, s_esize));
                    let cur = read_elem(&acc, d * d_esize, d_esize) as i128;
                    write_elem(&mut dst, d * d_esize, d_esize, (cur + pair) as u64 & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 predicated integer pairwise: 0x44, bits[21:19]==010,
            // bits[15:13]==101. opc=bits[18:17] (00=ADDP, 10=MAXP, 11=MINP),
            // U=bit16. The pairwise results of Zdn and Zm are INTERLEAVED (even
            // output = Zdn pair, odd = Zm pair); merged into Zdn under Pg.
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 19) & 0x7 == 0b010
                    && (insn >> 13) & 0x7 == 0b101 =>
            {
                let opc = (insn >> 17) & 0x3;
                let unsigned = (insn >> 16) & 1 == 1;
                // Only (opc,U) in {(00,1)=ADDP, (10,0)=SMAXP, (10,1)=UMAXP,
                // (11,0)=SMINP, (11,1)=UMINP} are allocated; (00,0), (01,0) and
                // (01,1) are reserved and must trap rather than execute.
                if !matches!((opc, unsigned), (0b00, true) | (0b10, _) | (0b11, _)) {
                    return Ok(CpuExit::Undefined(insn));
                }
                let bits = (esize * 8) as u32;
                let mask = elem_mask(bits);
                let elements = 16 / esize;
                let h = elements / 2;
                let pred = self.sve_p[pg];
                let dn = self.v[zd].to_le_bytes(); // Zdn
                let m = self.v[zn].to_le_bytes(); // Zm
                let op = |a: u64, b: u64| -> u64 {
                    match opc {
                        0b00 => a.wrapping_add(b) & mask,
                        0b10 if unsigned => a.max(b),
                        0b10 => (sext_elem(a, bits).max(sext_elem(b, bits)) as u64) & mask,
                        _ if unsigned => a.min(b),
                        _ => (sext_elem(a, bits).min(sext_elem(b, bits)) as u64) & mask,
                    }
                };
                let mut res = [0u8; 16];
                for p in 0..h {
                    let dnv = op(
                        read_elem(&dn, 2 * p * esize, esize),
                        read_elem(&dn, (2 * p + 1) * esize, esize),
                    );
                    let mv = op(
                        read_elem(&m, 2 * p * esize, esize),
                        read_elem(&m, (2 * p + 1) * esize, esize),
                    );
                    write_elem(&mut res, 2 * p * esize, esize, dnv);
                    write_elem(&mut res, (2 * p + 1) * esize, esize, mv);
                }
                let mut dst = dn;
                for e in 0..elements {
                    if (pred >> (e * esize)) & 1 == 1 {
                        write_elem(
                            &mut dst,
                            e * esize,
                            esize,
                            read_elem(&res, e * esize, esize),
                        );
                    }
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 SQRDMLAH/SQRDMLSH (saturating rounding doubling multiply-add):
            // 0x44, bit21==0, bits[15:11]==01110. S=bit10 selects subtract. The
            // rounded doubling-high is unsaturated; only the accumulate saturates.
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 0
                    && (insn >> 11) & 0x1F == 0b01110 =>
            {
                let sub = (insn >> 10) & 1 == 1;
                let bits = (esize * 8) as u32;
                let mask = elem_mask(bits);
                let elements = 16 / esize;
                let acc = self.v[zd].to_le_bytes();
                let a = self.v[zn].to_le_bytes();
                let b = self.v[zm].to_le_bytes();
                let mut dst = acc;
                let hi = (1i128 << (bits - 1)) - 1;
                let lo = -(1i128 << (bits - 1));
                for e in 0..elements {
                    let off = e * esize;
                    let prod = sext_elem(read_elem(&a, off, esize), bits)
                        * sext_elem(read_elem(&b, off, esize), bits);
                    // The Zm factor is negated BEFORE the rounding bias is added
                    // (matching qemu), so the rounding of SQRDMLSH is applied to
                    // -prod rather than negating the rounded SQRDMLAH result —
                    // the two differ at exact rounding ties.
                    let p = if sub { -prod } else { prod };
                    let sdrh = (p + (1i128 << (bits - 2))) >> (bits - 1);
                    let cur = sext_elem(read_elem(&acc, off, esize), bits);
                    write_elem(
                        &mut dst,
                        off,
                        esize,
                        (cur + sdrh).clamp(lo, hi) as u64 & mask,
                    );
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 CDOT (complex integer dot product): 0x44, bit21==0,
            // bits[15:12]==0001. rot=bits[11:10]. Each destination element
            // accumulates two complex products of half-width signed elements
            // (.s from int8, .d from int16): real += r*a, then += i*b*(+/-1).
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 0
                    && (insn >> 12) & 0xF == 0b0001 =>
            {
                let size = (insn >> 22) & 0x3;
                if size < 2 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let esize = 1usize << size; // 4 (.s) or 8 (.d)
                let nb = esize / 4; // narrow element bytes (1 or 2)
                let nbits = (nb * 8) as u32;
                let dbits = (esize * 8) as u32;
                let mask = elem_mask(dbits);
                let rot = (insn >> 10) & 0x3;
                let sel_a = (rot & 1) as usize;
                let sel_b = sel_a ^ 1;
                let sub_i: i128 = if rot == 0 || rot == 3 { -1 } else { 1 };
                let n = self.v[zn].to_le_bytes();
                let m = self.v[zm].to_le_bytes();
                let a = self.v[zd].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..(16 / esize) {
                    let mut acc = sext_elem(read_elem(&a, e * esize, esize), dbits);
                    for i in 0..2 {
                        let base = e * esize + i * 2 * nb;
                        let e1r = sext_elem(read_elem(&n, base, nb), nbits);
                        let e1i = sext_elem(read_elem(&n, base + nb, nb), nbits);
                        let e2a = sext_elem(read_elem(&m, base + nb * sel_a, nb), nbits);
                        let e2b = sext_elem(read_elem(&m, base + nb * sel_b, nb), nbits);
                        acc += e1r * e2a + e1i * e2b * sub_i;
                    }
                    write_elem(&mut dst, e * esize, esize, acc as u64 & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 CDOT by indexed element: 0x44, bit21==1, bits[15:12]==0100.
            // Like CDOT but the Zm complex element is taken from a fixed index
            // within the 128-bit segment and shared across the segment. .s:
            // index=bits[20:19], Zm=bits[18:16]; .d: index=bit20, Zm=bits[19:16].
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 1
                    && (insn >> 12) & 0xF == 0b0100 =>
            {
                let size = (insn >> 22) & 0x3;
                if size < 2 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let esize = 1usize << size;
                let nb = esize / 4;
                let nbits = (nb * 8) as u32;
                let dbits = (esize * 8) as u32;
                let mask = elem_mask(dbits);
                let rot = (insn >> 10) & 0x3;
                let sel_a = (rot & 1) as usize;
                let sel_b = sel_a ^ 1;
                let sub_i: i128 = if rot == 0 || rot == 3 { -1 } else { 1 };
                let (index, zmr) = if size == 2 {
                    (((insn >> 19) & 0x3) as usize, ((insn >> 16) & 0x7) as usize)
                } else {
                    (((insn >> 20) & 1) as usize, ((insn >> 16) & 0xF) as usize)
                };
                let n = self.v[zn].to_le_bytes();
                let m = self.v[zmr].to_le_bytes();
                let a = self.v[zd].to_le_bytes();
                let m_base = index * esize;
                let mut dst = [0u8; 16];
                for e in 0..(16 / esize) {
                    let mut acc = sext_elem(read_elem(&a, e * esize, esize), dbits);
                    let nbase = e * esize;
                    for i in 0..2 {
                        let e1r = sext_elem(read_elem(&n, nbase + i * 2 * nb, nb), nbits);
                        let e1i = sext_elem(read_elem(&n, nbase + i * 2 * nb + nb, nb), nbits);
                        let e2a =
                            sext_elem(read_elem(&m, m_base + i * 2 * nb + nb * sel_a, nb), nbits);
                        let e2b =
                            sext_elem(read_elem(&m, m_base + i * 2 * nb + nb * sel_b, nb), nbits);
                        acc += e1r * e2a + e1i * e2b * sub_i;
                    }
                    write_elem(&mut dst, e * esize, esize, acc as u64 & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE integer dot product (vector): 0x44, bit21==0, with bit23==1
            // and bits[15:11]==00000 (SDOT/UDOT, u=bit10) or bits[23:22]==10 and
            // bits[15:10]==011110 (USDOT).
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 0
                    && (((insn >> 23) & 1 == 1 && (insn >> 11) & 0x1F == 0b00000)
                        || ((insn >> 22) & 0x3 == 0b10 && (insn >> 10) & 0x3F == 0b011110)) =>
            {
                self.exec_sve_dot(insn)
            }

            // SVE integer dot product (indexed): 0x44, bit21==1,
            // bits[15:10] in {SDOT, UDOT, USDOT, SUDOT}.
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 1
                    && matches!(
                        (insn >> 10) & 0x3F,
                        0b000000 | 0b000001 | 0b000110 | 0b000111
                    ) =>
            {
                self.exec_sve_dot(insn)
            }

            // SVE2.1 two-way integer dot product SDOT/UDOT (.s <- .h): 0x44,
            // bit21==0, bits[15:11]==11001 (bit10=U). Each .s lane accumulates two
            // 16-bit products. bits[23:22]==00 is the vector form; ==10 is the
            // indexed form (index=bits[20:19], Zm=bits[18:16]) broadcasting the
            // index-th .h pair within the 128-bit segment.
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 0
                    && (insn >> 11) & 0x1F == 0b11001 =>
            {
                if !has_sve2p1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let signed = (insn >> 10) & 1 == 0;
                let indexed = (insn >> 22) & 0x3 == 0b10;
                let zd = (insn & 0x1F) as usize;
                let zn = ((insn >> 5) & 0x1F) as usize;
                let (zm, index) = if indexed {
                    (((insn >> 16) & 0x7) as usize, ((insn >> 19) & 0x3) as usize)
                } else {
                    (((insn >> 16) & 0x1F) as usize, 0)
                };
                let n = self.v[zn].to_le_bytes();
                let m = self.v[zm].to_le_bytes();
                let a = self.v[zd].to_le_bytes();
                let ext = |b: &[u8; 16], off: usize| -> i64 {
                    if signed {
                        sext_elem(read_elem(b, off, 2), 16) as i64
                    } else {
                        uext_elem(read_elem(b, off, 2), 16) as i64
                    }
                };
                let mut dst = [0u8; 16];
                for i in 0..4 {
                    let mut acc = read_elem(&a, i * 4, 4) as u32 as i64;
                    for k in 0..2 {
                        let n_off = i * 4 + k * 2;
                        let m_off = if indexed { (index * 2 + k) * 2 } else { n_off };
                        acc = acc.wrapping_add(ext(&n, n_off) * ext(&m, m_off));
                    }
                    write_elem(&mut dst, i * 4, 4, acc as u32 as u64);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 predicated integer ALU (saturating/rounding shifts, halving
            // add/sub, saturating add/sub, SQABS/SQNEG): 0x44, bit21==0,
            // bits[15:13]==100, or bits[15:13]==101 with bits[21:19]==001. The
            // pairwise group (bits[15:13]==101, bits[21:19]==010) is handled by
            // its own arm and excluded here.
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 0
                    && ((insn >> 13) & 0x7 == 0b100
                        || ((insn >> 13) & 0x7 == 0b101 && (insn >> 19) & 0x7 == 0b001)
                        || ((insn >> 13) & 0x7 == 0b101
                            && (insn >> 19) & 0x7 == 0b000
                            && matches!((insn >> 16) & 0x7, 0b000 | 0b001))) =>
            {
                self.exec_sve2_pred_alu(insn)
            }

            // SVE2 complex integer multiply-add (CMLA/SQRDCMLAH): 0x44, bit21==0,
            // bits[15:13]==001. op=bit12 picks the saturating-rounding-doubling
            // SQRDCMLAH; rot=bits[11:10] is the 0/90/180/270 rotation. Each
            // complex pair accumulates one selected-component product.
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 0
                    && (insn >> 13) & 0x7 == 0b001 =>
            {
                let size = (insn >> 22) & 0x3;
                let esize = 1usize << size;
                let bits = (esize * 8) as u32;
                let mask = elem_mask(bits);
                let sat = (insn >> 12) & 1 == 1;
                let rot = (insn >> 10) & 0x3;
                let acc = self.v[zd].to_le_bytes();
                let n = self.v[zn].to_le_bytes();
                let m = self.v[zm].to_le_bytes();
                let mut dst = acc;
                let hi = (1i128 << (bits - 1)) - 1;
                let lo = -(1i128 << (bits - 1));
                let pairs = (16 / esize) / 2;
                for p in 0..pairs {
                    let (re, im) = (2 * p * esize, (2 * p + 1) * esize);
                    let n_re = sext_elem(read_elem(&n, re, esize), bits);
                    let n_im = sext_elem(read_elem(&n, im, esize), bits);
                    let m_re = sext_elem(read_elem(&m, re, esize), bits);
                    let m_im = sext_elem(read_elem(&m, im, esize), bits);
                    let acc_re = sext_elem(read_elem(&acc, re, esize), bits);
                    let acc_im = sext_elem(read_elem(&acc, im, esize), bits);
                    let zn_sel = if rot == 0 || rot == 2 { n_re } else { n_im };
                    // The signed Zm factor for the real/imag accumulation.
                    let (mfr, mfi): (i128, i128) = match rot {
                        0 => (m_re, m_im),
                        1 => (-m_im, m_re),
                        2 => (-m_re, -m_im),
                        _ => (m_im, -m_re),
                    };
                    let (r_re, r_im) = if sat {
                        // SignedDoublingRoundingHigh: (2*prod + 2^(bits-1)) >> bits,
                        // rewritten as (prod + 2^(bits-2)) >> (bits-1) to avoid the
                        // doubled product overflowing i128 at the 64-bit size. As in
                        // NEON SQRDMLAH the rounded high part is NOT saturated; only
                        // the final accumulate is.
                        let sdrh = |prod: i128| (prod + (1i128 << (bits - 2))) >> (bits - 1);
                        (
                            (acc_re + sdrh(zn_sel * mfr)).clamp(lo, hi),
                            (acc_im + sdrh(zn_sel * mfi)).clamp(lo, hi),
                        )
                    } else {
                        (acc_re + zn_sel * mfr, acc_im + zn_sel * mfi)
                    };
                    write_elem(&mut dst, re, esize, r_re as u64 & mask);
                    write_elem(&mut dst, im, esize, r_im as u64 & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 integer multiply / multiply-add (indexed): 0x44, bit21==1.
            // The second factor is a single element Zm[index] broadcast to every
            // lane; the (index, Zm) packing depends on the element size.
            // bits[15:10] selects MUL/SQDMULH/SQRDMULH (1111xx), MLA/MLS
            // (00001x) or SQRDMLAH/SQRDMLSH (00010x). Widening and complex
            // indexed forms have their own dispatch arms below.
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 1
                    && matches!(
                        (insn >> 10) & 0x3F,
                        0b111110 | 0b111100 | 0b111101 | 0b000010 | 0b000011 | 0b000100 | 0b000101
                    ) =>
            {
                self.exec_sve2_mul_indexed(insn, zn, zd)
            }

            // SVE2 widening multiply-add long by indexed element: 0x44, bit21==1.
            // bits[15:12] selects S/U MULL/MLAL/MLSL and SQDMULL/SQDMLAL/SQDMLSL;
            // the narrow source is half the destination width and bit10 (T) picks
            // the odd/even narrow lane. Distinct op fields from the same-width
            // indexed group above (1111xx / 0000xx), so no overlap.
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 1
                    && matches!(
                        (insn >> 12) & 0xF,
                        0b1000
                            | 0b1001
                            | 0b1010
                            | 0b1011
                            | 0b1100
                            | 0b1101
                            | 0b1110
                            | 0b0010
                            | 0b0011
                    ) =>
            {
                self.exec_sve2_mull_indexed(insn, zn, zd)
            }

            // SVE2 CMLA / SQRDCMLAH by indexed element: 0x44, bit21==1,
            // bits[15:13]==011. bit12 picks SQRDCMLAH (saturating-rounding-
            // doubling) over plain CMLA. rot=bits[11:10]; the indexed Zm complex
            // pair (at 2*index) is broadcast. .h: index=bits[20:19],
            // Zm=bits[18:16]; .s: index=bit20, Zm=bits[19:16]. For SQRDCMLAH the
            // doubled-rounded high product is accumulated then saturated, exactly
            // as the non-indexed SQRDCMLAH (qemu sve2_sqrdcmlah_idx).
            0b010
                if (insn >> 24) & 0xFF == 0b01000100
                    && (insn >> 21) & 1 == 1
                    && (insn >> 13) & 0x7 == 0b011 =>
            {
                let size = (insn >> 22) & 0x3;
                if size < 2 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let sat = (insn >> 12) & 1 == 1;
                let esize = 1usize << (size - 1); // .h=2, .s=4
                let bits = (esize * 8) as u32;
                let mask = elem_mask(bits);
                let hi = (1i128 << (bits - 1)) - 1;
                let lo = -(1i128 << (bits - 1));
                let sdrh = |prod: i128| (prod + (1i128 << (bits - 2))) >> (bits - 1);
                let rot = (insn >> 10) & 0x3;
                let sel_a = (rot & 1) as usize;
                let sel_b = sel_a ^ 1;
                let sub_r: i128 = if rot == 1 || rot == 2 { -1 } else { 1 };
                let sub_i: i128 = if rot >= 2 { -1 } else { 1 };
                let (index, zm) = if size == 2 {
                    (((insn >> 19) & 0x3) as usize, ((insn >> 16) & 0x7) as usize)
                } else {
                    (((insn >> 20) & 1) as usize, ((insn >> 16) & 0xF) as usize)
                };
                let idx = index * 2;
                let n = self.v[zn].to_le_bytes();
                let m = self.v[zm].to_le_bytes();
                let a = self.v[zd].to_le_bytes();
                let e2a = sext_elem(read_elem(&m, (idx + sel_a) * esize, esize), bits);
                let e2b = sext_elem(read_elem(&m, (idx + sel_b) * esize, esize), bits);
                let mut dst = [0u8; 16];
                for p in 0..((16 / esize) / 2) {
                    let (re, im) = (2 * p, 2 * p + 1);
                    let e1 = sext_elem(read_elem(&n, (re + sel_a) * esize, esize), bits);
                    let ar = sext_elem(read_elem(&a, re * esize, esize), bits);
                    let ai = sext_elem(read_elem(&a, im * esize, esize), bits);
                    let (r_re, r_im) = if sat {
                        (
                            (ar + sdrh(e1 * e2a * sub_r)).clamp(lo, hi),
                            (ai + sdrh(e1 * e2b * sub_i)).clamp(lo, hi),
                        )
                    } else {
                        (ar + e1 * e2a * sub_r, ai + e1 * e2b * sub_i)
                    };
                    write_elem(&mut dst, re * esize, esize, r_re as u64 & mask);
                    write_elem(&mut dst, im * esize, esize, r_im as u64 & mask);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 MATCH / NMATCH (character match -> predicate): 0x45, bit21==1,
            // bits[15:13]==100. For each Pg-active Zn element the result bit is
            // set if that element value equals any Zm element in the same
            // 128-bit segment (MATCH) or none of them (NMATCH, bit4==1). The
            // result is zeroing and sets NZCV via PredTest(Pg). size b/h only.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 13) & 0x7 == 0b100 =>
            {
                let size = (insn >> 22) & 0x3;
                if size > 1 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let esize = 1usize << size;
                let elements = 16 / esize;
                let nmatch = (insn >> 4) & 1 == 1;
                let pg = ((insn >> 10) & 0x7) as usize;
                let pd = (insn & 0xF) as usize;
                let n = self.v[zn].to_le_bytes();
                let m = self.v[zm].to_le_bytes();
                let gov = self.sve_p[pg];
                let mut result = 0u32;
                for e in 0..elements {
                    let off = e * esize;
                    if (gov >> off) & 1 == 0 {
                        continue; // zeroing predication
                    }
                    let ne = read_elem(&n, off, esize);
                    let matched = (0..elements).any(|j| read_elem(&m, j * esize, esize) == ne);
                    if matched ^ nmatch {
                        result |= 1 << off;
                    }
                }
                self.sve_p[pd] = result;
                let (nf, zf, cf, vf) = pred_test(gov, result, elements, esize);
                self.set_n(nf);
                self.set_z(zf);
                self.set_c(cf);
                self.set_v(vf);
                Ok(CpuExit::Continue)
            }

            // SVE2 HISTSEG (histogram segment): 0x45, bit21==1,
            // bits[15:10]==101000, size==b. Each result byte is the number of Zm
            // bytes (in the 128-bit segment) equal to the corresponding Zn byte.
            // Unpredicated.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 10) & 0x3F == 0b101000 =>
            {
                if (insn >> 22) & 0x3 != 0 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let n = self.v[zn].to_le_bytes();
                let m = self.v[zm].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..16 {
                    dst[e] = m.iter().filter(|&&b| b == n[e]).count() as u8;
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 HISTCNT (histogram count): 0x45, bit21==1, bits[15:13]==110,
            // size s/d. For each Pg-active element i, the result is the number of
            // active elements j<=i whose Zm value equals Zn[i]; inactive lanes
            // are zeroed.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 13) & 0x7 == 0b110 =>
            {
                let size = (insn >> 22) & 0x3;
                if size < 2 {
                    return Ok(CpuExit::Undefined(insn));
                }
                let esize = 1usize << size;
                let elements = 16 / esize;
                let pg = ((insn >> 10) & 0x7) as usize;
                let gov = self.sve_p[pg];
                let n = self.v[zn].to_le_bytes();
                let m = self.v[zm].to_le_bytes();
                let mut dst = [0u8; 16];
                for i in 0..elements {
                    let off_i = i * esize;
                    if (gov >> off_i) & 1 == 0 {
                        continue; // zeroing
                    }
                    let nn = read_elem(&n, off_i, esize);
                    let mut count = 0u64;
                    for j in 0..=i {
                        let off_j = j * esize;
                        if (gov >> off_j) & 1 == 1 && read_elem(&m, off_j, esize) == nn {
                            count += 1;
                        }
                    }
                    write_elem(&mut dst, off_i, esize, count);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE2 crypto (AES / SM4 / SHA3-RAX1): 0x45, bit21==1,
            // bits[15:13]==111. At VL=128 each op acts on the single 128-bit
            // segment, identical to its NEON counterpart.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 1
                    && (insn >> 13) & 0x7 == 0b111 =>
            {
                self.exec_sve2_crypto(insn)
            }

            // SVE2 ADCLB/ADCLT/SBCLB/SBCLT (long add/subtract with carry): 0x45,
            // bit21==0, bits[15:11]==11010. The carry-in is bit `esize` of each
            // Zm element; bit23 inverts the Zn operand (SBCL = add of the one's
            // complement); bit22 selects .d (1) / .s (0); bit10 (T) the odd/even
            // Zn half. Zda holds the low half; the full sum (with carry-out) is
            // written across the doubled container.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 0
                    && (insn >> 11) & 0x1F == 0b11010 =>
            {
                let d_form = (insn >> 22) & 1 == 1;
                let top = ((insn >> 10) & 1) as u32;
                let inv = (insn >> 23) & 1 == 1;
                if d_form {
                    let e1 = self.v[zd] as u64;
                    let mut e2 = (self.v[zn] >> (top * 64)) as u64;
                    if inv {
                        e2 = !e2;
                    }
                    let c = ((self.v[zm] >> 64) & 1) as u64;
                    self.v[zd] = (e1 as u128) + (e2 as u128) + (c as u128);
                } else {
                    let mut dst = 0u128;
                    for i in 0..2 {
                        let e1 = (self.v[zd] >> (i * 64)) as u32;
                        let mut e2 = ((self.v[zn] >> (i * 64)) >> (top * 32)) as u32;
                        if inv {
                            e2 = !e2;
                        }
                        let c = ((self.v[zm] >> (i * 64 + 32)) & 1) as u32;
                        let sum = e1 as u64 + e2 as u64 + c as u64; // 33-bit, holds carry-out
                        dst |= (sum as u128) << (i * 64);
                    }
                    self.v[zd] = dst;
                }
                Ok(CpuExit::Continue)
            }

            // SVE2 EORBT/EORTB (interleaving exclusive OR): 0x45, bit21==0,
            // bits[15:11]==10010, bit10 selects EORTB(1)/EORBT(0). EORBT writes
            // the even result lanes as Zn_even ^ Zm_odd (odd lanes keep the prior
            // Zd); EORTB writes the odd lanes as Zn_odd ^ Zm_even.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 0
                    && (insn >> 11) & 0x1F == 0b10010 =>
            {
                let esize = 1usize << ((insn >> 22) & 0x3);
                let tb = (insn >> 10) & 1 == 1; // EORTB
                let (sel1, sel2) = if tb {
                    (1usize, 0usize)
                } else {
                    (0usize, 1usize)
                };
                let n = self.v[zn].to_le_bytes();
                let m = self.v[zm].to_le_bytes();
                let mut dst = self.v[zd].to_le_bytes(); // keep prior Zd in unwritten lanes
                for p in 0..((16 / esize) / 2) {
                    let base = 2 * p * esize;
                    let nn = read_elem(&n, base + sel1 * esize, esize);
                    let mm = read_elem(&m, base + sel2 * esize, esize);
                    write_elem(&mut dst, base + sel1 * esize, esize, nn ^ mm);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // SVE I8MM integer matrix multiply-accumulate (SMMLA/UMMLA/USMMLA):
            // 0x45, bit21==0, bits[15:10]==100110. bit23 = Zn unsigned, bit22 =
            // Zm unsigned (the signed-by-unsigned pair is unallocated). Computes
            // a 2x2 int32 tile: Zda += Zn(2x8 int8) * Zm(2x8 int8)^T, each entry
            // an 8-element dot product accumulated mod 2^32.
            0b010
                if (insn >> 24) & 0xFF == 0b01000101
                    && (insn >> 21) & 1 == 0
                    && (insn >> 10) & 0x3F == 0b100110 =>
            {
                let n_uns = (insn >> 23) & 1 == 1;
                let m_uns = (insn >> 22) & 1 == 1;
                if !n_uns && m_uns {
                    return Ok(CpuExit::Undefined(insn)); // signed-by-unsigned: unallocated
                }
                let n = self.v[zn].to_le_bytes();
                let m = self.v[zm].to_le_bytes();
                let acc = self.v[zd].to_le_bytes();
                let dot = |nrow: usize, mrow: usize| -> u32 {
                    let mut s = 0i64;
                    for k in 0..8 {
                        let nv = n[nrow * 8 + k];
                        let mv = m[mrow * 8 + k];
                        let np = if n_uns { nv as i64 } else { nv as i8 as i64 };
                        let mp = if m_uns { mv as i64 } else { mv as i8 as i64 };
                        s = s.wrapping_add(np * mp);
                    }
                    s as u32
                };
                let mut dst = [0u8; 16];
                for (idx, &(nr, mr)) in [(0, 0), (0, 1), (1, 0), (1, 1)].iter().enumerate() {
                    let a = u32::from_le_bytes(acc[idx * 4..idx * 4 + 4].try_into().unwrap());
                    let r = a.wrapping_add(dot(nr, mr));
                    dst[idx * 4..idx * 4 + 4].copy_from_slice(&r.to_le_bytes());
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // Load/Store
            0b100 | 0b101 | 0b110 | 0b111 => self.exec_sve_ldst(insn),

            _ => Err(ArmError::Unimplemented(format!(
                "SVE op0={:03b} op1={:02b}",
                op0, op1
            ))),
        }
    }


    /// Execute SVE integer dot product (SDOT/UDOT/USDOT/SUDOT), vector and
    /// indexed. Each destination element (S from 8-bit sources, D from 16-bit)
    /// accumulates a 4-element dot product; the indexed form broadcasts the
    /// index-th 4-element group of Zm across the segment. Sign treatment is
    /// per-operand; no saturation.
    pub(crate) fn exec_sve_dot(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let indexed = (insn >> 21) & 1 == 1;
        let d_esize = if (insn >> 22) & 1 == 0 { 4usize } else { 8 };
        let s_esize = d_esize / 4;
        let s_bits = (s_esize * 8) as u32;
        let d_bits = (d_esize * 8) as u32;
        let mask = elem_mask(d_bits);
        let (n_signed, m_signed) = if indexed {
            match (insn >> 10) & 0x3F {
                0b000000 => (true, true),   // SDOT
                0b000001 => (false, false), // UDOT
                0b000110 => (false, true),  // USDOT (Zn unsigned, Zm signed)
                0b000111 => (true, false),  // SUDOT (Zn signed, Zm unsigned)
                _ => return Ok(CpuExit::Undefined(insn)),
            }
        } else if (insn >> 10) & 0x3F == 0b011110 {
            (false, true) // USDOT vector
        } else {
            let u = (insn >> 10) & 1 == 1;
            (!u, !u) // SDOT(u=0) / UDOT(u=1)
        };
        let zd = (insn & 0x1F) as usize;
        let zn = ((insn >> 5) & 0x1F) as usize;
        let (zm, index) = if indexed {
            if d_esize == 4 {
                (((insn >> 16) & 0x7) as usize, ((insn >> 19) & 0x3) as usize)
            } else {
                (((insn >> 16) & 0xF) as usize, ((insn >> 20) & 1) as usize)
            }
        } else {
            (((insn >> 16) & 0x1F) as usize, 0)
        };
        let n = self.v[zn].to_le_bytes();
        let m = self.v[zm].to_le_bytes();
        let a = self.v[zd].to_le_bytes();
        let ext = |b: &[u8; 16], off: usize, s: bool| -> i128 {
            if s {
                sext_elem(read_elem(b, off, s_esize), s_bits)
            } else {
                uext_elem(read_elem(b, off, s_esize), s_bits) as i128
            }
        };
        let mut dst = [0u8; 16];
        for i in 0..(16 / d_esize) {
            let mut acc = sext_elem(read_elem(&a, i * d_esize, d_esize), d_bits);
            for k in 0..4 {
                let n_off = i * d_esize + k * s_esize;
                let m_off = if indexed {
                    (index * 4 + k) * s_esize
                } else {
                    n_off
                };
                acc += ext(&n, n_off, n_signed) * ext(&m, m_off, m_signed);
            }
            write_elem(&mut dst, i * d_esize, d_esize, acc as u64 & mask);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute the SVE2 predicated integer ALU group at 0x44 bits[15:14]==10:
    /// saturating/rounding shifts by vector (SRSHL/URSHL/SQSHL/UQSHL/SQRSHL/
    /// UQRSHL and their reversed forms), halving add/sub (SHADD/UHADD/SHSUB/
    /// UHSUB/SRHADD/URHADD/SHSUBR/UHSUBR), saturating add/sub (SQADD/UQADD/
    /// SQSUB/UQSUB/SUQADD/USQADD/SQSUBR/UQSUBR) at bits[15:13]==100, and the
    /// unary SQABS/SQNEG at bits[15:13]==101 bits[21:19]==001. All merge under
    /// Pg. The op is keyed on bits[21:16]; reversed forms swap the operands.
    pub(crate) fn exec_sve2_pred_alu(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let esize = 1usize << ((insn >> 22) & 0x3);
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let pg = ((insn >> 10) & 0x7) as usize;
        let rd = (insn & 0x1F) as usize;
        let rfield = ((insn >> 5) & 0x1F) as usize;
        let pred = self.sve_p[pg];
        let dst_prior = self.v[rd].to_le_bytes();
        let mut dst = dst_prior;
        let elements = 16 / esize;

        if (insn >> 13) & 0x7 == 0b101 {
            // Unary, source = rfield, dest = rd, merging. bits[21:19]==001 ->
            // SQABS/SQNEG; bits[21:19]==000 -> URECPE/URSQRTE (S-only unsigned
            // reciprocal estimates).
            let opc6 = (insn >> 16) & 0x3F;
            let src = self.v[rfield].to_le_bytes();
            let recip = opc6 >> 3 == 0b000;
            if !matches!(opc6, 0b000000 | 0b000001 | 0b001000 | 0b001001) {
                return Ok(CpuExit::Undefined(insn));
            }
            if recip && esize != 4 {
                return Ok(CpuExit::Undefined(insn));
            }
            let sel = (insn >> 16) & 1 == 1; // SQNEG / URSQRTE
            for e in 0..elements {
                let off = e * esize;
                if (pred >> off) & 1 == 0 {
                    continue;
                }
                let r = if recip {
                    let a = read_elem(&src, off, 4) as u32;
                    (if sel {
                        unsigned_rsqrt_estimate(a)
                    } else {
                        unsigned_recip_estimate(a)
                    }) as u64
                } else {
                    let n = sext_elem(read_elem(&src, off, esize), bits);
                    if sel {
                        sat_signed(-n, bits)
                    } else {
                        sat_signed(n.abs(), bits)
                    }
                };
                write_elem(&mut dst, off, esize, r);
            }
            self.v[rd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        let opc6 = (insn >> 16) & 0x3F;
        if !matches!(
            opc6,
            0b000010
                | 0b000011
                | 0b000110
                | 0b000111
                | 0b001000..=0b001111
                | 0b010000..=0b011111
        ) {
            return Ok(CpuExit::Undefined(insn));
        }
        let reversed = matches!(
            opc6,
            0b000110
                | 0b000111
                | 0b001100
                | 0b001101
                | 0b001110
                | 0b001111
                | 0b010110
                | 0b010111
                | 0b011110
                | 0b011111
        );
        let field = self.v[rfield].to_le_bytes();
        let do_shift = |val: u64, sh: i64, signed: bool, round: bool, sat: bool| -> u64 {
            if bits == 64 {
                if signed {
                    sqrshl_d(val as i64, sh, round, sat) as u64
                } else {
                    uqrshl_d(val, sh, round, sat)
                }
            } else if signed {
                sqrshl_bhs(sext_elem(val, bits) as i32, sh as i32, bits, round, sat) as u64 & mask
            } else {
                uqrshl_bhs((val & mask) as u32, sh as i32, bits, round, sat) as u64 & mask
            }
        };
        for e in 0..elements {
            let off = e * esize;
            if (pred >> off) & 1 == 0 {
                continue;
            }
            let rd_v = read_elem(&dst_prior, off, esize);
            let fv = read_elem(&field, off, esize);
            let (a, b) = if reversed { (fv, rd_v) } else { (rd_v, fv) };
            let (sa, sb) = (sext_elem(a, bits), sext_elem(b, bits));
            let (ua, ub) = (uext_elem(a, bits) as i128, uext_elem(b, bits) as i128);
            let r: u64 = match opc6 {
                0b000010 | 0b000110 => do_shift(a, sb as i64, true, true, false), // SRSHL(R)
                0b000011 | 0b000111 => do_shift(a, sb as i64, false, true, false), // URSHL(R)
                0b001000 | 0b001100 => do_shift(a, sb as i64, true, false, true), // SQSHL(R)
                0b001001 | 0b001101 => do_shift(a, sb as i64, false, false, true), // UQSHL(R)
                0b001010 | 0b001110 => do_shift(a, sb as i64, true, true, true),  // SQRSHL(R)
                0b001011 | 0b001111 => do_shift(a, sb as i64, false, true, true), // UQRSHL(R)
                0b010000 => ((sa + sb) >> 1) as u64 & mask,                       // SHADD
                0b010001 => ((ua + ub) >> 1) as u64 & mask,                       // UHADD
                0b010010 | 0b010110 => ((sa - sb) >> 1) as u64 & mask,            // SHSUB(R)
                0b010011 | 0b010111 => ((ua - ub) >> 1) as u64 & mask,            // UHSUB(R)
                0b010100 => ((sa + sb + 1) >> 1) as u64 & mask,                   // SRHADD
                0b010101 => ((ua + ub + 1) >> 1) as u64 & mask,                   // URHADD
                0b011000 => sat_signed(sa + sb, bits),                            // SQADD
                0b011001 => sat_unsigned(ua + ub, bits),                          // UQADD
                0b011010 | 0b011110 => sat_signed(sa - sb, bits),                 // SQSUB(R)
                0b011011 | 0b011111 => sat_unsigned(ua - ub, bits),               // UQSUB(R)
                0b011100 => sat_signed(sa + ub, bits),                            // SUQADD
                0b011101 => sat_unsigned(ua + sb, bits),                          // USQADD
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[rd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute SVE2 widening multiply-add long by an indexed element. The
    /// narrow source elements (half the destination width) are sign- or
    /// zero-extended; `Zm[index]` is the shared broadcast factor and bit10 (T)
    /// selects the odd/even narrow lane of Zn. SQDMULL doubles-and-saturates;
    /// the SQDMLAL/SQDMLSL accumulate saturates a second time.
    pub(crate) fn exec_sve2_mull_indexed(
        &mut self,
        insn: u32,
        zn: usize,
        zd: usize,
    ) -> Result<CpuExit, ArmError> {
        let size = (insn >> 22) & 0x3; // 2=.s (h src), 3=.d (s src)
        if size < 2 {
            return Ok(CpuExit::Undefined(insn));
        }
        let d_esize = 1usize << size;
        let s_esize = d_esize / 2;
        let s_bits = (s_esize * 8) as u32;
        let d_bits = (d_esize * 8) as u32;
        let top = (insn >> 10) & 1 == 1;
        let op = (insn >> 12) & 0xF;
        // Index and Zm packing differ per size: .s uses a 3-bit index
        // (bit20:bit19:bit11) with Zm in z0-z7; .d a 2-bit index (bit20:bit11)
        // with Zm in z0-z15.
        let (index, zm) = if size == 2 {
            let idx = (((insn >> 20) & 1) << 2) | (((insn >> 19) & 1) << 1) | ((insn >> 11) & 1);
            (idx as usize, ((insn >> 16) & 0x7) as usize)
        } else {
            let idx = (((insn >> 20) & 1) << 1) | ((insn >> 11) & 1);
            (idx as usize, ((insn >> 16) & 0xF) as usize)
        };
        let n = self.v[zn].to_le_bytes();
        let m = self.v[zm].to_le_bytes();
        let acc = self.v[zd].to_le_bytes();
        let mut dst = [0u8; 16];
        let mask = elem_mask(d_bits);
        let hi = (1i128 << (d_bits - 1)) - 1;
        let lo = -(1i128 << (d_bits - 1));
        let m_raw = read_elem(&m, index * s_esize, s_esize);
        let elements = 16 / d_esize;
        for e in 0..elements {
            let n_raw = read_elem(&n, (2 * e + top as usize) * s_esize, s_esize);
            let aa = read_elem(&acc, e * d_esize, d_esize);
            let aa_s = sext_elem(aa, d_bits);
            let nm_s = sext_elem(n_raw, s_bits) * sext_elem(m_raw, s_bits);
            let nm_u = (uext_elem(n_raw, s_bits) * uext_elem(m_raw, s_bits)) as i128;
            let sqdmull = (2 * nm_s).clamp(lo, hi); // saturating doubling product
            let r: u64 = match op {
                0b1100 => nm_s as u64 & mask,                           // SMULLB/T
                0b1101 => nm_u as u64 & mask,                           // UMULLB/T
                0b1000 => (aa_s + nm_s) as u64 & mask,                  // SMLALB/T
                0b1001 => (aa_s + nm_u) as u64 & mask,                  // UMLALB/T
                0b1010 => (aa_s - nm_s) as u64 & mask,                  // SMLSLB/T
                0b1011 => (aa_s - nm_u) as u64 & mask,                  // UMLSLB/T
                0b1110 => sqdmull as u64 & mask,                        // SQDMULLB/T
                0b0010 => (aa_s + sqdmull).clamp(lo, hi) as u64 & mask, // SQDMLALB/T
                0b0011 => (aa_s - sqdmull).clamp(lo, hi) as u64 & mask, // SQDMLSLB/T
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, e * d_esize, d_esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute the SVE2 crypto group (AES round/mix, SM4, SHA3 RAX1). At
    /// VL=128 every operation works on the single 128-bit segment, so it reuses
    /// the NEON primitives directly. AES family: bits[15:11]==11100, sub-decoded
    /// by bits[23:16] and bit10; SM4EKEY/RAX1: bits[15:11]==11110, bit10.
    pub(crate) fn exec_sve2_crypto(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let zd = (insn & 0x1F) as usize;
        let n = ((insn >> 5) & 0x1F) as usize; // bits[9:5] (Zm for AES/SM4E, Zn for *KEY/RAX1)
        let m = ((insn >> 16) & 0x1F) as usize; // bits[20:16] (Zm for *KEY/RAX1)
        let inv = (insn >> 10) & 1 == 1;
        match (insn >> 11) & 0x1F {
            0b11100 => {
                match (insn >> 16) & 0xFF {
                    // AESMC / AESIMC: Zd = (Inv)MixColumns(Zd). bits[9:5] must be 0.
                    0x20 if n == 0 => self.v[zd] = aes_mix_columns(self.v[zd], inv),
                    // AESE / AESD: Zd = (Inv)SubBytes((Inv)ShiftRows(Zd ^ Zm)).
                    0x22 => {
                        let st = self.v[zd] ^ self.v[n];
                        self.v[zd] = aes_sub_bytes(aes_shift_rows(st, inv), inv);
                    }
                    // SM4E: Zd = SM4E(Zd, Zm).
                    0x23 if !inv => self.v[zd] = sm4_rounds(self.v[zd], self.v[n], true),
                    _ => return Ok(CpuExit::Undefined(insn)),
                }
                Ok(CpuExit::Continue)
            }
            0b11110 => {
                if (insn >> 21) & 0x7 != 0b001 {
                    return Ok(CpuExit::Undefined(insn));
                }
                if !inv {
                    // SM4EKEY: Zd = SM4EKEY(Zn, Zm).
                    self.v[zd] = sm4_rounds(self.v[n], self.v[m], false);
                } else {
                    // RAX1: per 64-bit element, Zd = Zn ^ ROL(Zm, 1).
                    let (zn, zm) = (self.v[n], self.v[m]);
                    let lo = (zn as u64) ^ (zm as u64).rotate_left(1);
                    let hi = ((zn >> 64) as u64) ^ ((zm >> 64) as u64).rotate_left(1);
                    self.v[zd] = (lo as u128) | ((hi as u128) << 64);
                }
                Ok(CpuExit::Continue)
            }
            _ => Ok(CpuExit::Undefined(insn)),
        }
    }


    /// Execute SVE2 integer multiply / multiply-add by an indexed element.
    /// `Zm[index]` (selected within the single 128-bit segment for VL=128) is
    /// the shared second factor for every destination lane. MUL/MLA/MLS take
    /// the truncated low half of the product; SQDMULH/SQRDMULH take the
    /// saturating (optionally rounded) doubled high half.
    pub(crate) fn exec_sve2_mul_indexed(
        &mut self,
        insn: u32,
        zn: usize,
        zd: usize,
    ) -> Result<CpuExit, ArmError> {
        // Element size and (index, Zm) packing differ per size: H uses a 3-bit
        // index (bit22:bit20:bit19) with Zm in z0-z7; S a 2-bit index
        // (bit20:bit19) with Zm in z0-z7; D a 1-bit index (bit20), Zm in z0-z15.
        let (esize, index, zm): (usize, usize, usize) = if (insn >> 23) & 1 == 0 {
            let idx = (((insn >> 22) & 1) << 2) | (((insn >> 20) & 1) << 1) | ((insn >> 19) & 1);
            (2, idx as usize, ((insn >> 16) & 0x7) as usize)
        } else if (insn >> 22) & 1 == 0 {
            let idx = (((insn >> 20) & 1) << 1) | ((insn >> 19) & 1);
            (4, idx as usize, ((insn >> 16) & 0x7) as usize)
        } else {
            (
                8,
                ((insn >> 20) & 1) as usize,
                ((insn >> 16) & 0xF) as usize,
            )
        };
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let op = (insn >> 10) & 0x3F;
        let n = self.v[zn].to_le_bytes();
        let m = self.v[zm].to_le_bytes();
        let acc = self.v[zd].to_le_bytes();
        let mut dst = acc;
        let m_val = read_elem(&m, index * esize, esize);
        let m_s = sext_elem(m_val, bits);
        let lo = -(1i128 << (bits - 1));
        let hi = (1i128 << (bits - 1)) - 1;
        let elements = 16 / esize;
        for e in 0..elements {
            let off = e * esize;
            let a = read_elem(&n, off, esize);
            let res: u64 = match op {
                0b111110 => a.wrapping_mul(m_val) & mask, // MUL (low half)
                0b000010 => read_elem(&acc, off, esize).wrapping_add(a.wrapping_mul(m_val)) & mask, // MLA
                0b000011 => read_elem(&acc, off, esize).wrapping_sub(a.wrapping_mul(m_val)) & mask, // MLS
                0b111100 => {
                    // SQDMULH: sat((2*a*b) >> bits) == sat((a*b) >> (bits-1)).
                    let prod = sext_elem(a, bits) * m_s;
                    (prod >> (bits - 1)).clamp(lo, hi) as u64 & mask
                }
                0b111101 => {
                    // SQRDMULH: sat((2*a*b + 2^(bits-1)) >> bits), rewritten as
                    // sat((a*b + 2^(bits-2)) >> (bits-1)) to avoid i128 overflow.
                    let prod = sext_elem(a, bits) * m_s;
                    ((prod + (1i128 << (bits - 2))) >> (bits - 1)).clamp(lo, hi) as u64 & mask
                }
                0b000100 | 0b000101 => {
                    // SQRDMLAH (000100) / SQRDMLSH (000101): Zda + rounded
                    // doubling-high of (+/-)a*Zm[idx]; the accumulate saturates.
                    // The product is negated before the rounding bias, matching
                    // qemu (differs from negating the rounded result at ties).
                    let prod = sext_elem(a, bits) * m_s;
                    let p = if op == 0b000101 { -prod } else { prod };
                    let sdrh = (p + (1i128 << (bits - 2))) >> (bits - 1);
                    let cur = sext_elem(read_elem(&acc, off, esize), bits);
                    (cur + sdrh).clamp(lo, hi) as u64 & mask
                }
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, res);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
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


    /// Execute SVE predicated shift by immediate (destructive, merging). The
    /// element size AND shift amount are jointly encoded in tsz:imm: esize is
    /// the lowest set bit of tsize=tszh:tszl; for ASR/LSR amount = 2*esize -
    /// tszimm, for LSL amount = tszimm - esize.
    pub(crate) fn exec_sve_shift_imm(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let tszh = (insn >> 22) & 0x3;
        let tszl = (insn >> 8) & 0x3;
        let imm3 = (insn >> 5) & 0x7;
        let tsize = (tszh << 2) | tszl;
        if tsize == 0 {
            return Ok(CpuExit::Undefined(insn));
        }
        // esize from the highest set bit of tsize (0001->8, 001x->16, 01xx->32,
        // 1xxx->64).
        let bits: u32 = if tsize & 0b1000 != 0 {
            64
        } else if tsize & 0b0100 != 0 {
            32
        } else if tsize & 0b0010 != 0 {
            16
        } else {
            8
        };
        let esize = (bits / 8) as usize;
        let tszimm = (tsize << 3) | imm3;
        // The operation is the full bits[21:16]; the low three bits alone do not
        // distinguish e.g. ASRD (000_100) from SRSHR (001_100).
        let op6 = (insn >> 16) & 0x3F;
        if !matches!(
            op6,
            0b000_000
                | 0b000_001
                | 0b000_011
                | 0b000_100
                | 0b000_110
                | 0b000_111
                | 0b001_100
                | 0b001_101
                | 0b001_111
        ) {
            return Ok(CpuExit::Undefined(insn));
        }
        // Shift-left ops take amount = tszimm - bits; shift-right ops take
        // amount = 2*bits - tszimm.
        let is_shl = matches!(op6, 0b000_011 | 0b000_110 | 0b000_111 | 0b001_111);
        let amount = if is_shl {
            tszimm - bits
        } else {
            2 * bits - tszimm
        };
        let pg = ((insn >> 10) & 0x7) as usize;
        let zd = (insn & 0x1F) as usize;
        let pred = self.sve_p[pg];
        let elements = 16 / esize;
        let mask = elem_mask(bits);
        let a_reg = self.v[zd].to_le_bytes();
        let mut dst = a_reg;
        for e in 0..elements {
            if (pred >> (e * esize)) & 1 == 0 {
                continue;
            }
            let off = e * esize;
            let v = read_elem(&a_reg, off, esize);
            let r = match op6 {
                0b000_000 => (sext_elem(v, bits) >> amount) as u64 & mask, // ASR
                0b000_001 => (uext_elem(v, bits) >> amount) as u64 & mask, // LSR
                0b000_011 => (uext_elem(v, bits) << amount) as u64 & mask, // LSL
                0b000_100 => {
                    // ASRD: signed shift-right rounding toward zero (divide).
                    let n = sext_elem(v, bits);
                    let bias = if n < 0 { (1i128 << amount) - 1 } else { 0 };
                    ((n + bias) >> amount) as u64 & mask
                }
                0b000_110 => {
                    // SQSHL: signed saturating shift left.
                    if bits == 64 {
                        sqrshl_d(v as i64, amount as i64, false, true) as u64 & mask
                    } else {
                        sqrshl_bhs(sext_elem(v, bits) as i32, amount as i32, bits, false, true)
                            as u64
                            & mask
                    }
                }
                0b000_111 => {
                    // UQSHL: unsigned saturating shift left.
                    if bits == 64 {
                        uqrshl_d(uext_elem(v, bits) as u64, amount as i64, false, true) & mask
                    } else {
                        uqrshl_bhs(uext_elem(v, bits) as u32, amount as i32, bits, false, true)
                            as u64
                            & mask
                    }
                }
                0b001_100 => sve_srshr(sext_elem(v, bits) as i64, amount) as u64 & mask, // SRSHR
                0b001_101 => sve_urshr(uext_elem(v, bits) as u64, amount) & mask,        // URSHR
                0b001_111 => {
                    // SQSHLU: signed shift left, saturating into the unsigned range.
                    let src = sext_elem(v, bits);
                    if src < 0 {
                        0
                    } else if bits == 64 {
                        uqrshl_d(src as u64, amount as i64, false, true) & mask
                    } else {
                        uqrshl_bhs(src as u32, amount as i32, bits, false, true) as u64 & mask
                    }
                }
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute SVE unpredicated shift by immediate. The element size and shift
    /// amount are encoded in tszh:tszl:imm3.
    pub(crate) fn exec_sve_shift_imm_unpred(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let tsize = (((insn >> 22) & 0x3) << 2) | ((insn >> 19) & 0x3);
        if tsize == 0 {
            return Ok(CpuExit::Undefined(insn));
        }
        let bits: u32 = if tsize & 0b1000 != 0 {
            64
        } else if tsize & 0b0100 != 0 {
            32
        } else if tsize & 0b0010 != 0 {
            16
        } else {
            8
        };
        let imm3 = (insn >> 16) & 0x7;
        let tszimm = (tsize << 3) | imm3;
        let op = (insn >> 10) & 0x3F;
        let is_lsl = op == 0b100111;
        let amount = if is_lsl {
            tszimm - bits
        } else {
            2 * bits - tszimm
        };
        let esize = (bits / 8) as usize;
        let elements = 16 / esize;
        let mask = elem_mask(bits);
        let zn = ((insn >> 5) & 0x1F) as usize;
        let zd = (insn & 0x1F) as usize;
        let src = self.v[zn].to_le_bytes();
        let mut dst = [0u8; 16];
        for e in 0..elements {
            let off = e * esize;
            let v = read_elem(&src, off, esize);
            let r = match op {
                0b100100 => (sext_elem(v, bits) >> amount) as u64 & mask,
                0b100101 => {
                    if amount >= bits {
                        0
                    } else {
                        (v >> amount) & mask
                    }
                }
                0b100111 => (v << amount) & mask,
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute SVE predicated shift by vector (destructive): Zdn = shift(Zdn,
    /// Zm) per active element. opc=bits[18:16]: 000=ASR, 001=LSR, 011=LSL. The
    /// shift amount is the (unsigned) Zm element; out-of-range gives 0 (LSR/LSL)
    /// or a full arithmetic shift (ASR). Pg is byte-granular.
    pub(crate) fn exec_sve_shift_pred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        pg: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        let opc = (insn >> 16) & 0x7;
        let pred = self.sve_p[pg];
        let elements = 16 / esize;
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let a_reg = self.v[zd].to_le_bytes(); // Zdn
        let b_reg = self.v[zn].to_le_bytes(); // Zm-field
        // bit18 selects the reversed form (ASRR/LSRR/LSLR), which swaps the
        // value and shift-amount operands; base op is bits[17:16].
        let reversed = opc & 0b100 != 0;
        let base_op = opc & 0b011;
        if base_op == 0b010 {
            return Ok(CpuExit::Undefined(insn));
        }
        let mut dst = a_reg;
        for e in 0..elements {
            if (pred >> (e * esize)) & 1 == 0 {
                continue;
            }
            let off = e * esize;
            let za = read_elem(&a_reg, off, esize);
            let zb = read_elem(&b_reg, off, esize);
            let (a, sh) = if reversed { (zb, za) } else { (za, zb) };
            let r = match base_op {
                0b000 => {
                    let s = sh.min((bits - 1) as u64);
                    (sext_elem(a, bits) >> s) as u64 & mask
                }
                0b001 => {
                    if sh >= bits as u64 {
                        0
                    } else {
                        (a >> sh) & mask
                    }
                }
                0b011 => {
                    if sh >= bits as u64 {
                        0
                    } else {
                        (a << sh) & mask
                    }
                }
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute SVE predicated shift by wide elements. Element sizes B/H/S use
    /// the 64-bit Zm lane that covers the destination element as the shift
    /// amount; D elements are unallocated.
    pub(crate) fn exec_sve_shift_wide_pred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        pg: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        if esize == 8 {
            return Ok(CpuExit::Undefined(insn));
        }

        let opc = (insn >> 16) & 0x7;
        let pred = self.sve_p[pg];
        let elements = 16 / esize;
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let a_reg = self.v[zd].to_le_bytes(); // Zdn
        let b_reg = self.v[zn].to_le_bytes(); // Zm.D shift amounts
        let mut dst = a_reg;
        for e in 0..elements {
            let off = e * esize;
            if (pred >> off) & 1 == 0 {
                continue;
            }
            let a = read_elem(&a_reg, off, esize);
            let sh = read_elem(&b_reg, (off / 8) * 8, 8);
            let r = match opc {
                0b000 => {
                    let s = sh.min((bits - 1) as u64);
                    (sext_elem(a, bits) >> s) as u64 & mask
                }
                0b001 => {
                    if sh >= bits as u64 {
                        0
                    } else {
                        (a >> sh) & mask
                    }
                }
                0b011 => {
                    if sh >= bits as u64 {
                        0
                    } else {
                        (a << sh) & mask
                    }
                }
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute SVE unpredicated shift by wide elements. Element sizes B/H/S use
    /// the 64-bit Zm lane that covers the source element as the shift amount; D
    /// elements are unallocated.
    pub(crate) fn exec_sve_shift_wide_unpred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        zm: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        if esize == 8 {
            return Ok(CpuExit::Undefined(insn));
        }

        let opc = (insn >> 10) & 0x7;
        let elements = 16 / esize;
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let a_reg = self.v[zn].to_le_bytes();
        let b_reg = self.v[zm].to_le_bytes(); // Zm.D shift amounts
        let mut dst = [0u8; 16];
        for e in 0..elements {
            let off = e * esize;
            let a = read_elem(&a_reg, off, esize);
            let sh = read_elem(&b_reg, (off / 8) * 8, 8);
            let r = match opc {
                0b000 => {
                    let s = sh.min((bits - 1) as u64);
                    (sext_elem(a, bits) >> s) as u64 & mask
                }
                0b001 => {
                    if sh >= bits as u64 {
                        0
                    } else {
                        (a >> sh) & mask
                    }
                }
                0b011 => {
                    if sh >= bits as u64 {
                        0
                    } else {
                        (a << sh) & mask
                    }
                }
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


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


    /// Execute SVE integer reduction (predicated) to a scalar in Vd. opc6 =
    /// bits[21:16]: SADDV(000000)/UADDV(000001) give a 64-bit sum; SMAXV/UMAXV/
    /// SMINV/UMINV (0010xx) and ANDV/ORV/EORV (0110xx) give an esize result.
    /// Inactive elements use the operation identity. Pg is byte-granular.
    pub(crate) fn exec_sve_int_reduce(&mut self, insn: u32, esize: usize) -> Result<CpuExit, ArmError> {
        let opc6 = (insn >> 16) & 0x3F;
        // SADDV has no 64-bit form (use UADDV.D for that).
        if opc6 == 0b000000 && esize == 8 {
            return Ok(CpuExit::Undefined(insn));
        }
        let pg = ((insn >> 10) & 0x7) as usize;
        let zn = ((insn >> 5) & 0x1F) as usize;
        let vd = (insn & 0x1F) as usize;
        let pred = self.sve_p[pg];
        let elements = 16 / esize;
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let src = self.v[zn].to_le_bytes();
        let mut act: Vec<u64> = Vec::with_capacity(elements);
        for e in 0..elements {
            if (pred >> (e * esize)) & 1 == 1 {
                act.push(read_elem(&src, e * esize, esize));
            }
        }
        let result: u128 = match opc6 {
            0b000000 => (act.iter().map(|&x| sext_elem(x, bits)).sum::<i128>() as u64) as u128,
            0b000001 => (act.iter().map(|&x| uext_elem(x, bits)).sum::<u128>() as u64) as u128,
            0b001000 => {
                (act.iter()
                    .map(|&x| sext_elem(x, bits))
                    .max()
                    .unwrap_or(-(1i128 << (bits - 1))) as u64
                    & mask) as u128
            }
            0b001001 => {
                (act.iter().map(|&x| uext_elem(x, bits)).max().unwrap_or(0) as u64 & mask) as u128
            }
            0b001010 => {
                (act.iter()
                    .map(|&x| sext_elem(x, bits))
                    .min()
                    .unwrap_or((1i128 << (bits - 1)) - 1) as u64
                    & mask) as u128
            }
            0b001011 => {
                (act.iter()
                    .map(|&x| uext_elem(x, bits))
                    .min()
                    .unwrap_or(mask as u128) as u64
                    & mask) as u128
            }
            0b011000 => (act.iter().fold(0u64, |a, &x| a | x) & mask) as u128, // ORV
            0b011001 => (act.iter().fold(0u64, |a, &x| a ^ x) & mask) as u128, // EORV
            0b011010 => (act.iter().fold(mask, |a, &x| a & x) & mask) as u128, // ANDV
            _ => return Ok(CpuExit::Undefined(insn)),
        };
        self.v[vd] = result;
        Ok(CpuExit::Continue)
    }


    /// Execute an SVE2.1 integer quadword reduction (ADDQV/SMAXQV/UMAXQV/SMINQV/
    /// UMINQV/ANDQV/ORQV/EORQV) to Vd. opc6=bits[21:16]. Each element position is
    /// reduced across the 128-bit segments of Zn (seeded with the op identity);
    /// at VL=128 (one segment) an active lane keeps Zn's value while an inactive
    /// lane takes the identity. Pg is byte-granular. Mirrors qemu DO_VPQ /
    /// DO_LOGIC_QV (the identity is the reduction of the empty active set).
    pub(crate) fn exec_sve_qv_reduce_int(&mut self, insn: u32, esize: usize) -> Result<CpuExit, ArmError> {
        let opc6 = (insn >> 16) & 0x3F;
        let pg = ((insn >> 10) & 0x7) as usize;
        let zn = ((insn >> 5) & 0x1F) as usize;
        let vd = (insn & 0x1F) as usize;
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let ident: u64 = match opc6 {
            0b000101 => 0,                                    // ADDQV
            0b001100 => 1u64 << (bits - 1),                   // SMAXQV  -> INT_MIN
            0b001101 => 0,                                    // UMAXQV
            0b001110 => (1u64 << (bits - 1)).wrapping_sub(1), // SMINQV -> INT_MAX
            0b001111 => mask,                                 // UMINQV  -> UINT_MAX
            0b011100 => 0,                                    // ORQV
            0b011101 => 0,                                    // EORQV
            0b011110 => mask,                                 // ANDQV   -> all-ones
            _ => return Ok(CpuExit::Undefined(insn)),
        };
        let pred = self.sve_p[pg];
        let src = self.v[zn].to_le_bytes();
        let mut dst = [0u8; 16];
        for e in 0..(16 / esize) {
            let off = e * esize;
            let v = if (pred >> off) & 1 == 1 {
                read_elem(&src, off, esize)
            } else {
                ident
            };
            write_elem(&mut dst, off, esize, v & mask);
        }
        self.v[vd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute SVE integer unpredicated operations.
    pub(crate) fn exec_sve_int_unpred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        zm: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        // bits[12:10]: 000=ADD 001=SUB 100=SQADD 101=UQADD 110=SQSUB 111=UQSUB.
        // Map each to the verified NEON three-same integer core (u, opcode).
        let opc = (insn >> 10) & 0x7;
        let (u, neon_op) = match opc {
            0b000 => (0, 0b10000), // ADD
            0b001 => (1, 0b10000), // SUB
            0b100 => (0, 0b00001), // SQADD
            0b101 => (1, 0b00001), // UQADD
            0b110 => (0, 0b00101), // SQSUB
            0b111 => (1, 0b00101), // UQSUB
            _ => return Ok(CpuExit::Undefined(insn)),
        };
        let bits = (esize * 8) as u32;
        let elements = 16 / esize;
        let src = self.v[zn].to_le_bytes();
        let src2 = self.v[zm].to_le_bytes();
        let mut dst = [0u8; 16];
        for e in 0..elements {
            let off = e * esize;
            let a = read_elem(&src, off, esize);
            let b = read_elem(&src2, off, esize);
            let r = adv_simd_three_same_int(u, neon_op, bits, a, b, 0).0;
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute SVE unpredicated bitwise logical (AND/ORR/EOR/BIC), selected by
    /// bits[23:22], over the whole vector (element size is irrelevant).
    pub(crate) fn exec_sve_logical_unpred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        zm: usize,
    ) -> Result<CpuExit, ArmError> {
        let a = self.v[zn];
        let b = self.v[zm];
        self.v[zd] = match (insn >> 22) & 0x3 {
            0b00 => a & b, // AND
            0b01 => a | b, // ORR
            0b10 => a ^ b, // EOR
            _ => a & !b,   // BIC
        };
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


    /// Execute SVE FP predicated operations.
    /// FEAT_SVE_B16B16 bf16 data-processing. Returns `Some(result)` if `insn` is
    /// a recognised bf16 op, else `None` (so the caller continues its normal
    /// f16/f32/f64 dispatch). All forms are 8-bit-exponent bf16 with FPCR-default
    /// (round-to-nearest, no flush, propagate-NaN) handling via `bf16_binop` /
    /// `bf16_fma`. The bf16 ops occupy the size==00 encoding slots (and, for the
    /// indexed forms, use bit22 as the high index bit).
    pub(crate) fn try_exec_sve_bf16(&mut self, insn: u32) -> Option<Result<CpuExit, ArmError>> {
        let top = (insn >> 24) & 0xFF;
        let zd = (insn & 0x1F) as usize;
        let has_sve_b16b16 = self.config.features.contains(ArmFeatures::SVE_B16B16);
        // ---- 0x65: unpredicated 3-same, predicated binary, predicated FMA ----
        if top == 0b01100101 && (insn >> 22) & 0x3 == 0b00 {
            let bit21 = (insn >> 21) & 1;
            // Unpredicated BFADD/BFSUB/BFMUL: bit21==0, bits[15:12]==0000,
            // opc=bits[11:10]. Zm=bits[20:16], Zn=bits[9:5].
            if bit21 == 0 && (insn >> 12) & 0xF == 0b0000 {
                if !has_sve_b16b16 {
                    return Some(Ok(CpuExit::Undefined(insn)));
                }
                let kind = match (insn >> 10) & 0x3 {
                    0b00 => FpKind::Add,
                    0b01 => FpKind::Sub,
                    0b10 => FpKind::Mul,
                    _ => return Some(Ok(CpuExit::Undefined(insn))),
                };
                let n = self.v[((insn >> 5) & 0x1F) as usize].to_le_bytes();
                let m = self.v[((insn >> 16) & 0x1F) as usize].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..8 {
                    let r = bf16_binop(
                        kind,
                        read_elem(&n, e * 2, 2) as u16,
                        read_elem(&m, e * 2, 2) as u16,
                    );
                    write_elem(&mut dst, e * 2, 2, r as u64);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                return Some(Ok(CpuExit::Continue));
            }
            // Predicated BFADD/.../BFMIN (merging): bit21==0, bits[15:13]==100,
            // opc=bits[20:16]. Zdn=Zd, Zm=bits[9:5], Pg=bits[12:10].
            if bit21 == 0 && (insn >> 13) & 0x7 == 0b100 {
                if !has_sve_b16b16 {
                    return Some(Ok(CpuExit::Undefined(insn)));
                }
                let kind = match (insn >> 16) & 0x1F {
                    0b00000 => FpKind::Add,
                    0b00001 => FpKind::Sub,
                    0b00010 => FpKind::Mul,
                    0b00100 => FpKind::MaxNm,
                    0b00101 => FpKind::MinNm,
                    0b00110 => FpKind::Max,
                    0b00111 => FpKind::Min,
                    _ => return Some(Ok(CpuExit::Undefined(insn))),
                };
                let pred = self.sve_p[((insn >> 10) & 0x7) as usize];
                let dn = self.v[zd].to_le_bytes();
                let m = self.v[((insn >> 5) & 0x1F) as usize].to_le_bytes();
                let mut dst = dn;
                for e in 0..8 {
                    if (pred >> (e * 2)) & 1 == 0 {
                        continue;
                    }
                    let r = bf16_binop(
                        kind,
                        read_elem(&dn, e * 2, 2) as u16,
                        read_elem(&m, e * 2, 2) as u16,
                    );
                    write_elem(&mut dst, e * 2, 2, r as u64);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                return Some(Ok(CpuExit::Continue));
            }
            // Predicated BFMLA (bit13==0) / BFMLS (bit13==1) (merging): bit21==1,
            // bits[15:14]==00. Zda=Zd, Zn=bits[9:5], Zm=bits[20:16], Pg=bits[12:10].
            if bit21 == 1 && (insn >> 14) & 0x3 == 0b00 {
                if !has_sve_b16b16 {
                    return Some(Ok(CpuExit::Undefined(insn)));
                }
                let sub = (insn >> 13) & 1 == 1;
                let pred = self.sve_p[((insn >> 10) & 0x7) as usize];
                let a = self.v[zd].to_le_bytes();
                let n = self.v[((insn >> 5) & 0x1F) as usize].to_le_bytes();
                let m = self.v[((insn >> 16) & 0x1F) as usize].to_le_bytes();
                let mut dst = a;
                for e in 0..8 {
                    if (pred >> (e * 2)) & 1 == 0 {
                        continue;
                    }
                    let r = bf16_fma(
                        read_elem(&a, e * 2, 2) as u16,
                        read_elem(&n, e * 2, 2) as u16,
                        read_elem(&m, e * 2, 2) as u16,
                        sub,
                    );
                    write_elem(&mut dst, e * 2, 2, r as u64);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                return Some(Ok(CpuExit::Continue));
            }
            return None;
        }
        // ---- 0x64: BFCLAMP and indexed BFMUL/BFMLA/BFMLS ----
        if top == 0b01100100 && (insn >> 23) & 1 == 0 && (insn >> 21) & 1 == 1 {
            let op6 = (insn >> 10) & 0x3F;
            // BFCLAMP (size==00): Zd = bf16 minnum(maxnum(Zn, Zd), Zm).
            if op6 == 0b001001 && (insn >> 22) & 0x3 == 0b00 {
                if !has_sve_b16b16 {
                    return Some(Ok(CpuExit::Undefined(insn)));
                }
                let n = self.v[((insn >> 5) & 0x1F) as usize].to_le_bytes();
                let m = self.v[((insn >> 16) & 0x1F) as usize].to_le_bytes();
                let d = self.v[zd].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..8 {
                    let lo = bf16_binop(
                        FpKind::MaxNm,
                        read_elem(&n, e * 2, 2) as u16,
                        read_elem(&d, e * 2, 2) as u16,
                    );
                    let r = bf16_binop(FpKind::MinNm, lo, read_elem(&m, e * 2, 2) as u16);
                    write_elem(&mut dst, e * 2, 2, r as u64);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                return Some(Ok(CpuExit::Continue));
            }
            // Indexed BFMUL/BFMLA/BFMLS (.h): index=(bit22<<2)|bits[20:19],
            // Zm=bits[18:16], Zn=bits[9:5]. The index-th bf16 of each 128-bit
            // segment is the broadcast second factor (Zm.h[index] at VL=128).
            let (mul, sub) = match op6 {
                0b001010 => (true, false),  // BFMUL
                0b000010 => (false, false), // BFMLA
                0b000011 => (false, true),  // BFMLS
                _ => return None,
            };
            if !has_sve_b16b16 {
                return Some(Ok(CpuExit::Undefined(insn)));
            }
            let index = ((((insn >> 22) & 1) << 2) | ((insn >> 19) & 0x3)) as usize;
            let n = self.v[((insn >> 5) & 0x1F) as usize].to_le_bytes();
            let m = self.v[((insn >> 16) & 0x7) as usize].to_le_bytes();
            let a = self.v[zd].to_le_bytes();
            let mb = read_elem(&m, index * 2, 2) as u16; // Zm.h[index]
            let mut dst = [0u8; 16];
            for e in 0..8 {
                let ne = read_elem(&n, e * 2, 2) as u16;
                let r = if mul {
                    bf16_binop(FpKind::Mul, ne, mb)
                } else {
                    bf16_fma(read_elem(&a, e * 2, 2) as u16, ne, mb, sub)
                };
                write_elem(&mut dst, e * 2, 2, r as u64);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Some(Ok(CpuExit::Continue));
        }
        None
    }


    pub(crate) fn exec_sve_fp_pred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        zm: usize,
        pg: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        // FEAT_SVE_B16B16 bf16 arithmetic (BFADD/BFSUB/BFMUL/BFMAX/BFMIN/
        // BFMAXNM/BFMINNM, BFMLA/BFMLS, indexed BFMUL/BFMLA/BFMLS, BFCLAMP).
        // These use the size==00 encoding slots, distinct from the f16/f32/f64
        // ops handled below, so intercept them first.
        if let Some(r) = self.try_exec_sve_bf16(insn) {
            return r;
        }

        // SVE unpredicated FADD/FSUB/FMUL (vectors): 0x65, bit21==0,
        // bits[15:12]==0000, opc=bits[11:10]. Size 00 is BF16 and is handled
        // above by the optional FEAT_SVE_B16B16 path.
        if (insn >> 24) & 0xFF == 0b01100101
            && (insn >> 21) & 1 == 0
            && (insn >> 12) & 0xF == 0
            && (insn >> 10) & 0x3 != 0b11
        {
            let kind = match (insn >> 10) & 0x3 {
                0b00 => FpKind::Add,
                0b01 => FpKind::Sub,
                0b10 => FpKind::Mul,
                _ => unreachable!(),
            };
            if esize < 2 {
                return Ok(CpuExit::Undefined(insn));
            }
            let n = self.v[zn].to_le_bytes();
            let m = self.v[zm].to_le_bytes();
            let mut dst = [0u8; 16];
            for e in 0..(16 / esize) {
                let off = e * esize;
                let a = read_elem(&n, off, esize);
                let b = read_elem(&m, off, esize);
                let r = match esize {
                    2 => sve_fp16_binop_with_fpcr(kind, a as u16, b as u16, self.fpcr) as u64,
                    4 => fp_three_same_f32_with_fpcr(kind, a as u32, b as u32, 0, self.fpcr) as u64,
                    8 => fp_three_same_f64_with_fpcr(kind, a, b, 0, self.fpcr),
                    _ => return Ok(CpuExit::Undefined(insn)),
                };
                self.fpsr |= fp_three_same_status_with_fpcr(esize, kind, a, b, 0, r, self.fpcr);
                write_elem(&mut dst, off, esize, r);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // SVE BFDOT (bf16 dot product, round-to-odd): 0x64, bits[23:22]==01,
        // bit21==1, bits[15:10]==100000 (zzzz) or 010000 (zzxw indexed). Each
        // f32 lane sums two bf16 products; the indexed form broadcasts Zm's
        // 32-bit group at `index` (bits[20:19], Zm in bits[18:16]).
        if (insn >> 24) & 0xFF == 0b01100100
            && (insn >> 22) & 0x3 == 0b01
            && (insn >> 21) & 1 == 1
            && matches!((insn >> 10) & 0x3F, 0b100000 | 0b010000)
        {
            let indexed = (insn >> 10) & 0x3F == 0b010000;
            let (m, a) = (
                if indexed {
                    self.v[((insn >> 16) & 0x7) as usize]
                } else {
                    self.v[zm]
                },
                self.v[zd],
            );
            let n = self.v[zn];
            let m_idx = if indexed {
                (m >> (((insn >> 19) & 0x3) * 32)) as u32
            } else {
                0
            };
            let mut r = 0u128;
            for e in 0..4 {
                let m_pair = if indexed {
                    m_idx
                } else {
                    (m >> (e * 32)) as u32
                };
                let res = bf16_dot_result_with_fpcr(
                    sve_bfdot_lane((a >> (e * 32)) as u32, (n >> (e * 32)) as u32, m_pair),
                    self.fpcr,
                );
                r |= (res as u128) << (e * 32);
            }
            self.v[zd] = r;
            return Ok(CpuExit::Continue);
        }

        // SVE2.1 FDOT (FP 2-way dot product, f16 -> f32): 0x64, bits[23:22]==00,
        // bit21==1, bits[15:10]==100000 (zzzz) or 010000 (zzxz indexed). Each f32
        // lane sums two f16 products (single-rounded) into the accumulator; the
        // indexed form broadcasts Zm's 32-bit group at index=bits[20:19]
        // (Zm=bits[18:16]).
        if (insn >> 24) & 0xFF == 0b01100100
            && (insn >> 22) & 0x3 == 0b00
            && (insn >> 21) & 1 == 1
            && matches!((insn >> 10) & 0x3F, 0b100000 | 0b010000)
        {
            if !self.config.features.contains(ArmFeatures::SVE2P1) {
                return Ok(CpuExit::Undefined(insn));
            }
            let indexed = (insn >> 10) & 0x3F == 0b010000;
            let n = self.v[zn];
            let (m, a) = if indexed {
                (self.v[((insn >> 16) & 0x7) as usize], self.v[zd])
            } else {
                (self.v[zm], self.v[zd])
            };
            let m_idx = if indexed {
                (m >> (((insn >> 19) & 0x3) * 32)) as u32
            } else {
                0
            };
            let mut r = 0u128;
            for e in 0..4 {
                let m_pair = if indexed {
                    m_idx
                } else {
                    (m >> (e * 32)) as u32
                };
                let res = f16_dotadd((a >> (e * 32)) as u32, (n >> (e * 32)) as u32, m_pair);
                r |= (res as u128) << (e * 32);
            }
            self.v[zd] = r;
            return Ok(CpuExit::Continue);
        }

        // SVE2 FMLAL/FMLSL (f16) and BFMLALB/T, BFMLSLB/T (bf16) widening fused
        // multiply-add into f32: 0x64, bit21==1, bits[15:11]==10000 (add) or
        // 10100 (sub); bit10 picks the odd(T)/even(B) lane. bits[23:22] selects
        // the source format: 10=f16, 11=bf16 (the bf16 subtract form BFMLSL is
        // FEAT_SVE2p1). The narrow inputs are widened to f32 (bf16 by a 16-bit
        // left shift, f16 by the standard widening) and accumulated with a
        // single ARM-correct fused multiply-add (processing the addend NaN
        // first); FMLSL/BFMLSL negate the Zn input (the FPCR.AH=0 form,
        // matching the oracle).
        if (insn >> 24) & 0xFF == 0b01100100
            && (insn >> 21) & 1 == 1
            && matches!((insn >> 22) & 0x3, 0b10 | 0b11)
            && matches!((insn >> 11) & 0x1F, 0b10000 | 0b10100)
        {
            let bf = (insn >> 22) & 0x3 == 0b11;
            let sub = (insn >> 13) & 1 == 1;
            if bf && sub && !self.config.features.contains(ArmFeatures::SVE2P1) {
                return Ok(CpuExit::Undefined(insn));
            }
            let top = (insn >> 10) & 1 == 1;
            let n = self.v[zn].to_le_bytes();
            let m = self.v[zm].to_le_bytes();
            let acc = self.v[zd].to_le_bytes();
            let fpcr = self.fpcr;
            let widen = |b: u16| -> u32 {
                if bf {
                    (b as u32) << 16
                } else {
                    let b = fp16_flush_input_with_fpcr(b, fpcr);
                    Self::fp16_to_f32(b).to_bits()
                }
            };
            let mut dst = acc;
            for j in 0..4 {
                let h_off = (2 * j + top as usize) * 2;
                let nbits = read_elem(&n, h_off, 2) as u16;
                let nbits = if sub {
                    fp_neg_bits_with_fpcr(nbits as u64, 16, self.fpcr) as u16
                } else {
                    nbits
                };
                let nn_raw = widen(nbits) as u64;
                let mm_raw = widen(read_elem(&m, h_off, 2) as u16) as u64;
                let aa_raw = read_elem(&acc, j * 4, 4);
                let nn = if bf {
                    bfmlal_f32_input_with_fpcr(nn_raw as u32, self.fpcr)
                } else {
                    fp_flush_input_bits_with_fpcr(nn_raw, 32, self.fpcr) as u32
                };
                let mm = if bf {
                    bfmlal_f32_input_with_fpcr(mm_raw as u32, self.fpcr)
                } else {
                    fp_flush_input_bits_with_fpcr(mm_raw, 32, self.fpcr) as u32
                };
                let aa = if bf {
                    bfmlal_f32_input_with_fpcr(aa_raw as u32, self.fpcr)
                } else {
                    fp_flush_input_bits_with_fpcr(aa_raw, 32, self.fpcr) as u32
                };
                let ah_nan_result = if !bf && self.fpcr & FPCR_AH != 0 {
                    fmlal_ah_result(aa_raw as u32, nn_raw as u32, mm_raw as u32, self.fpcr)
                } else {
                    None
                };
                let r = if bf {
                    bfmlal_ah_result(aa_raw as u32, nn_raw as u32, mm_raw as u32, self.fpcr)
                } else {
                    ah_nan_result
                }
                .or_else(|| {
                    fmlal_default_invalid_result(aa as u32, nn as u32, mm as u32, self.fpcr)
                })
                .unwrap_or_else(|| {
                    fp_muladd_bits_with_fpcr(aa as u64, nn as u64, mm as u64, 32, self.fpcr) as u32
                });
                let mut status = if bf && self.fpcr & FPCR_AH != 0 {
                    0
                } else {
                    fp_status_fma(4, aa as u64, nn as u64, mm as u64, r as u64)
                };
                if !(bf && self.fpcr & FPCR_AH != 0)
                    && bf
                    && fp_fz_fma_output(4, aa as u64, nn as u64, mm as u64, r as u64, self.fpcr)
                        .is_some()
                {
                    status &= !FPSR_IXC;
                }
                let input_status = if bf {
                    bfmlal_f32_input_status(aa_raw, self.fpcr)
                        | bfmlal_f32_input_status(nn_raw, self.fpcr)
                        | bfmlal_f32_input_status(mm_raw, self.fpcr)
                } else if ah_nan_result.is_some() {
                    0
                } else {
                    fp_fz_input_status(4, aa_raw, self.fpcr)
                        | fp_fz_input_status(4, nn_raw, self.fpcr)
                        | fp_fz_input_status(4, mm_raw, self.fpcr)
                };
                self.fpsr |= status | input_status;
                write_elem(&mut dst, j * 4, 4, r as u64);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // SVE2 FMLAL/FMLSL (f16) and BFMLALB/T, BFMLSLB/T (bf16) by indexed
        // element: 0x64, bit21==1, bits[23:22]==10(f16)/11(bf16),
        // bits[15:14]==01, bit12==0. sub=bit13, T=bit10, Zm=bits[18:16],
        // index=(bits[20:19]<<1)|bit11. Like the non-indexed form but Zm.h[index]
        // is the broadcast second factor; the FMA uses the ARM-correct
        // float32_muladd (addend NaN processed first).
        if (insn >> 24) & 0xFF == 0b01100100
            && (insn >> 21) & 1 == 1
            && matches!((insn >> 22) & 0x3, 0b10 | 0b11)
            && (insn >> 14) & 0x3 == 0b01
            && (insn >> 12) & 1 == 0
        {
            let bf = (insn >> 22) & 0x3 == 0b11;
            let sub = (insn >> 13) & 1 == 1; // FMLSL / BFMLSL
            if bf && sub && !self.config.features.contains(ArmFeatures::SVE2P1) {
                return Ok(CpuExit::Undefined(insn));
            }
            let top = (insn >> 10) & 1 == 1; // odd half of Zn
            let index = ((((insn >> 19) & 0x3) << 1) | ((insn >> 11) & 1)) as usize;
            let zmr = ((insn >> 16) & 0x7) as usize;
            let n = self.v[zn].to_le_bytes();
            let m = self.v[zmr].to_le_bytes();
            let acc = self.v[zd].to_le_bytes();
            let fpcr = self.fpcr;
            let widen = |b: u16| -> u32 {
                if bf {
                    (b as u32) << 16
                } else {
                    let b = fp16_flush_input_with_fpcr(b, fpcr);
                    Self::fp16_to_f32(b).to_bits()
                }
            };
            let mm_raw = widen(read_elem(&m, index * 2, 2) as u16) as u64;
            let mm = if bf {
                bfmlal_f32_input_with_fpcr(mm_raw as u32, self.fpcr)
            } else {
                fp_flush_input_bits_with_fpcr(mm_raw, 32, self.fpcr) as u32
            }; // Zm.h[index]
            let mut dst = acc;
            for j in 0..4 {
                let h_off = (2 * j + top as usize) * 2;
                let nbits = read_elem(&n, h_off, 2) as u16;
                let nbits = if sub {
                    fp_neg_bits_with_fpcr(nbits as u64, 16, self.fpcr) as u16
                } else {
                    nbits
                };
                let nn_raw = widen(nbits) as u64;
                let aa_raw = read_elem(&acc, j * 4, 4);
                let nn = if bf {
                    bfmlal_f32_input_with_fpcr(nn_raw as u32, self.fpcr)
                } else {
                    fp_flush_input_bits_with_fpcr(nn_raw, 32, self.fpcr) as u32
                };
                let aa = if bf {
                    bfmlal_f32_input_with_fpcr(aa_raw as u32, self.fpcr)
                } else {
                    fp_flush_input_bits_with_fpcr(aa_raw, 32, self.fpcr) as u32
                };
                let ah_nan_result = if !bf && self.fpcr & FPCR_AH != 0 {
                    fmlal_ah_result(aa_raw as u32, nn_raw as u32, mm_raw as u32, self.fpcr)
                } else {
                    None
                };
                let r = if bf {
                    bfmlal_ah_result(aa_raw as u32, nn_raw as u32, mm_raw as u32, self.fpcr)
                } else {
                    ah_nan_result
                }
                .or_else(|| {
                    fmlal_default_invalid_result(aa as u32, nn as u32, mm as u32, self.fpcr)
                })
                .unwrap_or_else(|| {
                    fp_muladd_bits_with_fpcr(aa as u64, nn as u64, mm as u64, 32, self.fpcr) as u32
                });
                let mut status = if bf && self.fpcr & FPCR_AH != 0 {
                    0
                } else {
                    fp_status_fma(4, aa as u64, nn as u64, mm as u64, r as u64)
                };
                if !(bf && self.fpcr & FPCR_AH != 0)
                    && bf
                    && fp_fz_fma_output(4, aa as u64, nn as u64, mm as u64, r as u64, self.fpcr)
                        .is_some()
                {
                    status &= !FPSR_IXC;
                }
                let input_status = if bf {
                    bfmlal_f32_input_status(aa_raw, self.fpcr)
                        | bfmlal_f32_input_status(nn_raw, self.fpcr)
                        | bfmlal_f32_input_status(mm_raw, self.fpcr)
                } else if ah_nan_result.is_some() {
                    0
                } else {
                    fp_fz_input_status(4, aa_raw, self.fpcr)
                        | fp_fz_input_status(4, nn_raw, self.fpcr)
                        | fp_fz_input_status(4, mm_raw, self.fpcr)
                };
                self.fpsr |= status | input_status;
                write_elem(&mut dst, j * 4, 4, r as u64);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // SVE FMMLA / BFMMLA (FP matrix multiply-accumulate): 0x64, bit21==1,
        // bits[15:10]==111001. bits[23:22]: 01=BFMMLA, 10=FMMLA.s, 11=FMMLA.d.
        // The 2x2 f32 tile is N(row i) . M(row j) with plain (non-fused) mul/add.
        // FMMLA.d needs a 256-bit segment (VL >= 4*8 bytes), so at VL=128 it is
        // an unallocated encoding. BFMMLA reuses the NEON path (same semantics).
        if (insn >> 24) & 0xFF == 0b01100100
            && (insn >> 21) & 1 == 1
            && (insn >> 10) & 0x3F == 0b111001
        {
            match (insn >> 22) & 0x3 {
                0b01 => return self.exec_simd_bfmmla(insn),
                0b10 => {
                    if !self.config.features.contains(ArmFeatures::SVE_F32MM) {
                        return Ok(CpuExit::Undefined(insn));
                    }
                    let (n, m, a) = (self.v[zn], self.v[zm], self.v[zd]);
                    let f = |v: u128, i: u32| f32::from_bits((v >> (i * 32)) as u32);
                    let (n00, n01, n10, n11) = (f(n, 0), f(n, 1), f(n, 2), f(n, 3));
                    let (m00, m01, m10, m11) = (f(m, 0), f(m, 1), f(m, 2), f(m, 3));
                    let d = [
                        f(a, 0) + (n00 * m00 + n01 * m01),
                        f(a, 1) + (n00 * m10 + n01 * m11),
                        f(a, 2) + (n10 * m00 + n11 * m01),
                        f(a, 3) + (n10 * m10 + n11 * m11),
                    ];
                    let mut r = 0u128;
                    for (i, v) in d.iter().enumerate() {
                        r |= (v.to_bits() as u128) << (i * 32);
                    }
                    self.v[zd] = r;
                    return Ok(CpuExit::Continue);
                }
                // FMMLA.d (esz=3): VL=128 < 4*8 bytes, so it is unallocated.
                _ => return Ok(CpuExit::Undefined(insn)),
            }
        }

        // SVE FCADD (FP complex add, predicated): 0x64, bits[21:17]==00000,
        // bits[15:13]==100. rot=bit16 (0=90,1=270). Per complex pair, merging:
        // re = Zdn_re + (rot? Zm_im : -Zm_im); im = Zdn_im + (rot? -Zm_re : Zm_re).
        if (insn >> 24) & 0xFF == 0b01100100
            && (insn >> 17) & 0x1F == 0
            && (insn >> 13) & 0x7 == 0b100
        {
            let size = (insn >> 22) & 0x3;
            if size == 0 {
                return Ok(CpuExit::Undefined(insn));
            }
            let bits = 8u32 << size;
            let esz = (bits / 8) as usize;
            let rot = (insn >> 16) & 1;
            let (dn, zmv) = (self.v[zd], self.v[zn]); // Zdn, Zm
            let pred = self.sve_p[pg];
            let mask = elem_mask(bits) as u128;
            let elem = |v: u128, idx: usize| ((v >> (idx * bits as usize)) & mask) as u64;
            let mut result = dn;
            for e in 0..(16 / (2 * esz)) {
                let (re, im) = (2 * e, 2 * e + 1);
                let (add_re, add_im) = if rot == 0 {
                    (
                        fp_neg_bits_with_fpcr(elem(zmv, im), bits, self.fpcr),
                        elem(zmv, re),
                    )
                } else {
                    (
                        elem(zmv, im),
                        fp_neg_bits_with_fpcr(elem(zmv, re), bits, self.fpcr),
                    )
                };
                if (pred >> (re * esz)) & 1 == 1 {
                    let lhs = elem(dn, re);
                    let r = fp_add_bits_with_fpcr(lhs, add_re, bits, self.fpcr);
                    self.fpsr |=
                        fp_status_binop_with_fpcr(esz, FpKind::Add, lhs, add_re, r, self.fpcr);
                    result = (result & !(mask << (re * bits as usize)))
                        | ((r as u128 & mask) << (re * bits as usize));
                }
                if (pred >> (im * esz)) & 1 == 1 {
                    let lhs = elem(dn, im);
                    let r = fp_add_bits_with_fpcr(lhs, add_im, bits, self.fpcr);
                    self.fpsr |=
                        fp_status_binop_with_fpcr(esz, FpKind::Add, lhs, add_im, r, self.fpcr);
                    result = (result & !(mask << (im * bits as usize)))
                        | ((r as u128 & mask) << (im * bits as usize));
                }
            }
            self.v[zd] = result;
            return Ok(CpuExit::Continue);
        }

        // SVE FCMLA (FP complex multiply-add, predicated): 0x64, bit21==0,
        // bit15==0. rot=bits[14:13]; Zn=bits[9:5], Zm=bits[20:16], Zda=Zd. Per
        // complex pair, merging, same operand selection as NEON FCMLA.
        if (insn >> 24) & 0xFF == 0b01100100 && (insn >> 21) & 1 == 0 && (insn >> 15) & 1 == 0 {
            let size = (insn >> 22) & 0x3;
            if size == 0 {
                return Ok(CpuExit::Undefined(insn));
            }
            let bits = 8u32 << size;
            let esz = (bits / 8) as usize;
            let rot = (insn >> 13) & 0x3;
            let (n, mv, acc) = (self.v[zn], self.v[zm], self.v[zd]);
            let pred = self.sve_p[pg];
            let mask = elem_mask(bits) as u128;
            let elem = |v: u128, idx: usize| ((v >> (idx * bits as usize)) & mask) as u64;
            let mut result = acc;
            for e in 0..(16 / (2 * esz)) {
                let (re, im) = (2 * e, 2 * e + 1);
                let (a_re_raw, a_im_raw) = (elem(n, re), elem(n, im));
                let (b_re_raw, b_im_raw) = (elem(mv, re), elem(mv, im));
                let (a_re, a_im) = (
                    fp_flush_input_bits_with_fpcr(a_re_raw, bits, self.fpcr),
                    fp_flush_input_bits_with_fpcr(a_im_raw, bits, self.fpcr),
                );
                let (b_re, b_im) = (
                    fp_flush_input_bits_with_fpcr(b_re_raw, bits, self.fpcr),
                    fp_flush_input_bits_with_fpcr(b_im_raw, bits, self.fpcr),
                );
                let (xr, yr, xi, yi) = match rot {
                    0b00 => (a_re, b_re, a_re, b_im),
                    0b01 => (
                        a_im,
                        fp_neg_bits_with_fpcr(b_im, bits, self.fpcr),
                        a_im,
                        b_re,
                    ),
                    0b10 => (
                        a_re,
                        fp_neg_bits_with_fpcr(b_re, bits, self.fpcr),
                        a_re,
                        fp_neg_bits_with_fpcr(b_im, bits, self.fpcr),
                    ),
                    _ => (
                        a_im,
                        b_im,
                        a_im,
                        fp_neg_bits_with_fpcr(b_re, bits, self.fpcr),
                    ),
                };
                let (xr_raw, yr_raw, xi_raw, yi_raw) = match rot {
                    0b00 => (a_re_raw, b_re_raw, a_re_raw, b_im_raw),
                    0b01 => (
                        a_im_raw,
                        fp_neg_bits_with_fpcr(b_im_raw, bits, self.fpcr),
                        a_im_raw,
                        b_re_raw,
                    ),
                    0b10 => (
                        a_re_raw,
                        fp_neg_bits_with_fpcr(b_re_raw, bits, self.fpcr),
                        a_re_raw,
                        fp_neg_bits_with_fpcr(b_im_raw, bits, self.fpcr),
                    ),
                    _ => (
                        a_im_raw,
                        b_im_raw,
                        a_im_raw,
                        fp_neg_bits_with_fpcr(b_re_raw, bits, self.fpcr),
                    ),
                };
                if (pred >> (re * esz)) & 1 == 1 {
                    let aa_raw = elem(acc, re);
                    let aa = fp_flush_input_bits_with_fpcr(aa_raw, bits, self.fpcr);
                    let r = fp_fcmla_muladd_bits_with_fpcr(aa, xr, yr, bits, self.fpcr);
                    self.fpsr |= fp_status_fma_with_fpcr(esz, aa_raw, xr_raw, yr_raw, r, self.fpcr);
                    result = (result & !(mask << (re * bits as usize)))
                        | ((r as u128 & mask) << (re * bits as usize));
                }
                if (pred >> (im * esz)) & 1 == 1 {
                    let aa_raw = elem(acc, im);
                    let aa = fp_flush_input_bits_with_fpcr(aa_raw, bits, self.fpcr);
                    let r = fp_fcmla_muladd_bits_with_fpcr(aa, xi, yi, bits, self.fpcr);
                    self.fpsr |= fp_status_fma_with_fpcr(esz, aa_raw, xi_raw, yi_raw, r, self.fpcr);
                    result = (result & !(mask << (im * bits as usize)))
                        | ((r as u128 & mask) << (im * bits as usize));
                }
            }
            self.v[zd] = result;
            return Ok(CpuExit::Continue);
        }

        // SVE FP multiply / multiply-add by indexed element: 0x64, bit21==1,
        // bits[15:11]==00000 (FMLA=000000 / FMLS=000001) or bits[15:10]==001000
        // (FMUL). The indexed Zm element is broadcast. Size: bit23==0 -> .h
        // (fp16, bit22 is the index MSB), bits[23:22]==10 -> .s, ==11 -> .d.
        // FMLA/FMLS are fused; FMUL is a plain multiply (unpredicated).
        if (insn >> 24) & 0xFF == 0b01100100
            && (insn >> 21) & 1 == 1
            && ((insn >> 11) & 0x1F == 0b00000 || (insn >> 10) & 0x3F == 0b001000)
        {
            let (esz, index, zmr): (usize, usize, usize) = if (insn >> 23) & 1 == 0 {
                // .h: index = bit22:bits[20:19], Zm = bits[18:16].
                let idx = (((insn >> 22) & 1) << 2) | ((insn >> 19) & 0x3);
                (2, idx as usize, ((insn >> 16) & 0x7) as usize)
            } else if (insn >> 22) & 1 == 0 {
                (
                    4,
                    ((insn >> 19) & 0x3) as usize,
                    ((insn >> 16) & 0x7) as usize,
                )
            } else {
                (
                    8,
                    ((insn >> 20) & 1) as usize,
                    ((insn >> 16) & 0xF) as usize,
                )
            };
            let ebits = (esz * 8) as u32;
            let is_fmul = (insn >> 10) & 0x3F == 0b001000;
            let is_fmls = !is_fmul && (insn >> 10) & 1 == 1;
            let n = self.v[zn].to_le_bytes();
            let m = self.v[zmr].to_le_bytes();
            let acc = self.v[zd].to_le_bytes();
            let mm = read_elem(&m, index * esz, esz); // Zm[index]
            let mut dst = acc;
            for e in 0..(16 / esz) {
                let off = e * esz;
                let ne = read_elem(&n, off, esz);
                let (r, status) = if is_fmul {
                    let r = match esz {
                        2 => sve_fp16_binop_with_fpcr(FpKind::Mul, ne as u16, mm as u16, self.fpcr)
                            as u64,
                        4 => fp_three_same_f32_with_fpcr(
                            FpKind::Mul,
                            ne as u32,
                            mm as u32,
                            0,
                            self.fpcr,
                        ) as u64,
                        _ => fp_three_same_f64_with_fpcr(FpKind::Mul, ne, mm, 0, self.fpcr),
                    };
                    (
                        r,
                        fp_status_binop_with_fpcr(esz, FpKind::Mul, ne, mm, r, self.fpcr),
                    )
                } else {
                    let aa_raw = read_elem(&acc, off, esz);
                    let ne_raw = ne;
                    let mm_raw = mm;
                    let (aa, ne, mm) = (
                        fp_flush_input_bits_with_fpcr(aa_raw, ebits, self.fpcr),
                        fp_flush_input_bits_with_fpcr(ne, ebits, self.fpcr),
                        fp_flush_input_bits_with_fpcr(mm, ebits, self.fpcr),
                    );
                    let nn = if is_fmls {
                        fp_neg_bits_with_fpcr(ne, ebits, self.fpcr)
                    } else {
                        ne
                    };
                    let nn_raw = if is_fmls {
                        fp_neg_bits_with_fpcr(ne_raw, ebits, self.fpcr)
                    } else {
                        ne_raw
                    };
                    let r = fp_muladd_bits_with_fpcr(aa, nn, mm, ebits, self.fpcr);
                    let status = fp_status_fma_with_fpcr(esz, aa_raw, nn_raw, mm_raw, r, self.fpcr);
                    (r, fp_status_sve_underflow(esz, r, status))
                };
                self.fpsr |= status;
                write_elem(&mut dst, off, esz, r);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // SVE FCMLA by indexed element: 0x64, bit21==1, bits[15:12]==0001.
        // rot=bits[11:10]; the indexed Zm complex pair (2*index) is broadcast.
        // Unpredicated, fused. .h: index=bits[20:19], Zm=bits[18:16]; .s:
        // index=bit20, Zm=bits[19:16]. Same flip/negate math as FCMLA.
        if (insn >> 24) & 0xFF == 0b01100100
            && (insn >> 21) & 1 == 1
            && (insn >> 12) & 0xF == 0b0001
        {
            let size = (insn >> 22) & 0x3;
            if size < 2 {
                return Ok(CpuExit::Undefined(insn));
            }
            let esize = 1usize << (size - 1); // .h=2, .s=4
            let bits = (esize * 8) as u32;
            let mask = elem_mask(bits) as u128;
            let rot = (insn >> 10) & 0x3;
            let flip = rot & 1;
            let negf_imag = (rot >> 1) & 1;
            let negf_real = flip ^ negf_imag;
            let (index, zmr) = if size == 2 {
                (((insn >> 19) & 0x3) as usize, ((insn >> 16) & 0x7) as usize)
            } else {
                (((insn >> 20) & 1) as usize, ((insn >> 16) & 0xF) as usize)
            };
            let (n, mv, acc) = (self.v[zn], self.v[zmr], self.v[zd]);
            let elem = |v: u128, e: usize| ((v >> (e * bits as usize)) & mask) as u64;
            let (mr_raw, mi_raw) = (elem(mv, 2 * index), elem(mv, 2 * index + 1));
            let (mr, mi) = (
                fp_flush_input_bits_with_fpcr(mr_raw, bits, self.fpcr),
                fp_flush_input_bits_with_fpcr(mi_raw, bits, self.fpcr),
            );
            let e1b = if flip == 1 { mi } else { mr };
            let e3b = if flip == 1 { mr } else { mi };
            let e1b_raw = if flip == 1 { mi_raw } else { mr_raw };
            let e3b_raw = if flip == 1 { mr_raw } else { mi_raw };
            let e1 = if negf_real == 1 {
                fp_neg_bits_with_fpcr(e1b, bits, self.fpcr)
            } else {
                e1b
            };
            let e3 = if negf_imag == 1 {
                fp_neg_bits_with_fpcr(e3b, bits, self.fpcr)
            } else {
                e3b
            };
            let e1_raw = if negf_real == 1 {
                fp_neg_bits_with_fpcr(e1b_raw, bits, self.fpcr)
            } else {
                e1b_raw
            };
            let e3_raw = if negf_imag == 1 {
                fp_neg_bits_with_fpcr(e3b_raw, bits, self.fpcr)
            } else {
                e3b_raw
            };
            let mut result = acc;
            for p in 0..((16 / esize) / 2) {
                let (re, im) = (2 * p, 2 * p + 1);
                let e2_raw = if flip == 1 { elem(n, im) } else { elem(n, re) };
                let e2 = fp_flush_input_bits_with_fpcr(e2_raw, bits, self.fpcr);
                let ar_raw = elem(acc, re);
                let ai_raw = elem(acc, im);
                let ar = fp_flush_input_bits_with_fpcr(ar_raw, bits, self.fpcr);
                let ai = fp_flush_input_bits_with_fpcr(ai_raw, bits, self.fpcr);
                let dr = fp_fcmla_muladd_bits_with_fpcr(ar, e2, e1, bits, self.fpcr);
                let di = fp_fcmla_muladd_bits_with_fpcr(ai, e2, e3, bits, self.fpcr);
                self.fpsr |= fp_status_fma_with_fpcr(esize, ar_raw, e2_raw, e1_raw, dr, self.fpcr);
                self.fpsr |= fp_status_fma_with_fpcr(esize, ai_raw, e2_raw, e3_raw, di, self.fpcr);
                result = (result & !(mask << (re * bits as usize)))
                    | ((dr as u128 & mask) << (re * bits as usize));
                result = (result & !(mask << (im * bits as usize)))
                    | ((di as u128 & mask) << (im * bits as usize));
            }
            self.v[zd] = result;
            return Ok(CpuExit::Continue);
        }

        // SVE predicated FP fused multiply-add: 0x65, bit21==1. bits[14:13]
        // select FMLA(00)/FMLS(01)/FNMLA(10)/FNMLS(11); bit15 picks the form
        // (0: Zd is the addend Za, multiplicands Zn/Zm; 1: Zd is a multiplicand
        // Zdn with addend Za). neg_prod=bit13^bit14, neg_addend=bit14 (FPCR.AH=0
        // negates via the sign bit). Single fused multiply-add; merging.
        if (insn >> 24) & 0xFF == 0b01100101 && (insn >> 21) & 1 == 1 {
            let size = (insn >> 22) & 0x3;
            if size == 0 {
                return Ok(CpuExit::Undefined(insn));
            }
            let esz = 1usize << size;
            let ebits = (esz * 8) as u32;
            let op3 = (insn >> 13) & 0x7;
            let neg_prod = ((insn >> 13) & 1) ^ ((insn >> 14) & 1) == 1;
            let neg_add = (insn >> 14) & 1 == 1;
            let rm = ((insn >> 16) & 0x1F) as usize;
            let r95 = ((insn >> 5) & 0x1F) as usize;
            // mad form (bit15==1): Zdn=zd is a multiplicand, Zm=bits[9:5],
            // addend=bits[20:16]. else: Zn=bits[9:5], Zm=bits[20:16], addend=Za=zd.
            let (n_reg, m_reg, a_reg) = if (insn >> 15) & 1 == 1 {
                (zd, r95, rm)
            } else {
                (r95, rm, zd)
            };
            let pred = self.sve_p[pg];
            let nb = self.v[n_reg].to_le_bytes();
            let mb = self.v[m_reg].to_le_bytes();
            let ab = self.v[a_reg].to_le_bytes();
            let mut dst = self.v[zd].to_le_bytes();
            for e in 0..(16 / esz) {
                let off = e * esz;
                if (pred >> off) & 1 == 0 {
                    continue;
                }
                let a_raw = read_elem(&ab, off, esz);
                let n_raw = read_elem(&nb, off, esz);
                let m_raw = read_elem(&mb, off, esz);
                let mut a = a_raw;
                let mut n = n_raw;
                let mut m = m_raw;
                if matches!(op3, 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7) {
                    n = fp_flush_input_bits_with_fpcr(n, ebits, self.fpcr);
                    m = fp_flush_input_bits_with_fpcr(m, ebits, self.fpcr);
                    a = fp_flush_input_bits_with_fpcr(a, ebits, self.fpcr);
                }
                let mut a_status = a_raw;
                let mut n_status = n_raw;
                if neg_prod {
                    n = fp_neg_bits_with_fpcr(n, ebits, self.fpcr);
                    n_status = fp_neg_bits_with_fpcr(n_status, ebits, self.fpcr);
                }
                if neg_add {
                    a = fp_neg_bits_with_fpcr(a, ebits, self.fpcr);
                    a_status = fp_neg_bits_with_fpcr(a_status, ebits, self.fpcr);
                }
                let r = fp_muladd_bits_with_fpcr(a, n, m, ebits, self.fpcr);
                let status = fp_status_fma_with_fpcr(esz, a_status, n_status, m_raw, r, self.fpcr);
                self.fpsr |= fp_status_sve_underflow(esz, r, status);
                write_elem(&mut dst, off, esz, r);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // SVE FRECPE / FRSQRTE (reciprocal / reciprocal-sqrt estimate,
        // unpredicated): 0x65, bits[21:16]==001110 (FRECPE) / 001111 (FRSQRTE),
        // bits[15:10]==001100. Reuses the FP estimate helpers.
        if (insn >> 24) & 0xFF == 0b01100101
            && matches!((insn >> 16) & 0x3F, 0b001110 | 0b001111)
            && (insn >> 10) & 0x3F == 0b001100
        {
            let size = (insn >> 22) & 0x3;
            if size == 0 {
                return Ok(CpuExit::Undefined(insn));
            }
            let esz = 1usize << size;
            let rsqrt = (insn >> 16) & 1 == 1;
            let n = self.v[zn].to_le_bytes();
            let mut dst = [0u8; 16];
            for e in 0..(16 / esz) {
                let off = e * esz;
                let raw = read_elem(&n, off, esz);
                let x = fp_estimate_input_with_fpcr(raw, (esz * 8) as u32, self.fpcr);
                let r = match esz {
                    2 => {
                        (if rsqrt {
                            fp16_rsqrte_with_fpcr(x as u16, self.fpcr)
                        } else {
                            fp16_recpe(x as u16)
                        }) as u64
                    }
                    4 => {
                        (if rsqrt {
                            fp_rsqrt_estimate_f32_with_fpcr(x as u32, self.fpcr)
                        } else {
                            fp_recip_estimate_f32(x as u32)
                        }) as u64
                    }
                    _ => {
                        if rsqrt {
                            fp_rsqrt_estimate_f64_with_fpcr(x, self.fpcr)
                        } else {
                            fp_recip_estimate_f64(x)
                        }
                    }
                };
                self.fpsr |= fp_status_estimate_with_fpcr(esz, rsqrt, raw, r, self.fpcr);
                write_elem(&mut dst, off, esz, r);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // SVE FP compare (register) -> predicate: 0x65, bit21==0, the condition
        // is (bits[15:13], bit4): FCMGE/FCMGT/FCMEQ/FCMNE/FCMUO/FACGE/FACGT.
        // Zeroing under Pg; sets NZCV via PredTest(Pg).
        if (insn >> 24) & 0xFF == 0b01100101
            && (insn >> 21) & 1 == 0
            && matches!(
                ((insn >> 13) & 0x7, (insn >> 4) & 1),
                (0b010, 0)
                    | (0b010, 1)
                    | (0b011, 0)
                    | (0b011, 1)
                    | (0b110, 0)
                    | (0b110, 1)
                    | (0b111, 1)
            )
        {
            let size = (insn >> 22) & 0x3;
            if size == 0 {
                return Ok(CpuExit::Undefined(insn));
            }
            let esz = 1usize << size;
            let cc = ((insn >> 13) & 0x7, (insn >> 4) & 1);
            let pred = self.sve_p[pg];
            let n = self.v[zn].to_le_bytes();
            let m = self.v[zm].to_le_bytes();
            let mut pd = 0u32;
            for e in 0..(16 / esz) {
                let off = e * esz;
                if (pred >> off) & 1 == 1 {
                    let a = read_elem(&n, off, esz);
                    let b = read_elem(&m, off, esz);
                    if fp_is_snan_bits(esz, a)
                        || fp_is_snan_bits(esz, b)
                        || ((cc.0 == 0b010 || (cc.1 == 1 && matches!(cc.0, 0b110 | 0b111)))
                            && (fp_is_nan_bits(esz, a) || fp_is_nan_bits(esz, b)))
                    {
                        self.fpsr |= FPSR_IOC;
                    }
                    self.fpsr |= fp_fz_input_status(esz, a, self.fpcr)
                        | fp_fz_input_status(esz, b, self.fpcr);
                    let a = fp_flush_input_bits_with_fpcr(a, (esz * 8) as u32, self.fpcr);
                    let b = fp_flush_input_bits_with_fpcr(b, (esz * 8) as u32, self.fpcr);
                    if sve_fp_compare(esz, cc, a, b) {
                        pd |= 1 << off;
                    }
                }
            }
            // FP compares write only the predicate; they do not set NZCV.
            self.sve_p[(insn & 0xF) as usize] = pd;
            return Ok(CpuExit::Continue);
        }

        // SVE FP compare with zero -> predicate: 0x65, bits[21:18]==0100,
        // bits[15:13]==001. bits[17:16] select GE/GT (00), LT/LE (01), EQ/NE
        // (10); bit4 picks within. Zeroing; sets NZCV via PredTest(Pg).
        if (insn >> 24) & 0xFF == 0b01100101
            && (insn >> 18) & 0xF == 0b0100
            && (insn >> 13) & 0x7 == 0b001
        {
            let size = (insn >> 22) & 0x3;
            if size == 0 {
                return Ok(CpuExit::Undefined(insn));
            }
            let esz = 1usize << size;
            let sub = (insn >> 16) & 0x3;
            let bit4 = (insn >> 4) & 1;
            // Valid: GE/GT (00,0/1), LT/LE (01,0/1), EQ (10,0), NE (11,0).
            // (10,1) and (11,1) are unallocated and fault on hardware.
            if (sub == 0b10 || sub == 0b11) && bit4 == 1 {
                return Ok(CpuExit::Undefined(insn));
            }
            let pred = self.sve_p[pg];
            let n = self.v[zn].to_le_bytes();
            let mut pd = 0u32;
            for e in 0..(16 / esz) {
                let off = e * esz;
                if (pred >> off) & 1 == 1 {
                    let a = read_elem(&n, off, esz);
                    if fp_is_snan_bits(esz, a) || (sub <= 0b01 && fp_is_nan_bits(esz, a)) {
                        self.fpsr |= FPSR_IOC;
                    }
                    self.fpsr |= fp_fz_input_status(esz, a, self.fpcr);
                    let a = fp_flush_input_bits_with_fpcr(a, (esz * 8) as u32, self.fpcr);
                    if sve_fp_compare_zero(esz, sub, bit4, a) {
                        pd |= 1 << off;
                    }
                }
            }
            // FP compares write only the predicate; they do not set NZCV.
            self.sve_p[(insn & 0xF) as usize] = pd;
            return Ok(CpuExit::Continue);
        }

        // SVE FRECPS / FRSQRTS (reciprocal / reciprocal-sqrt step, unpredicated):
        // 0x65, bit21==0, bits[15:10]==000110 (FRECPS) / 000111 (FRSQRTS).
        // Fused step with the inf*0 special (2.0 / 1.5).
        if (insn >> 24) & 0xFF == 0b01100101
            && (insn >> 21) & 1 == 0
            && matches!((insn >> 10) & 0x3F, 0b000110 | 0b000111)
        {
            let size = (insn >> 22) & 0x3;
            if size == 0 {
                return Ok(CpuExit::Undefined(insn));
            }
            let esz = 1usize << size;
            let rsqrt = (insn >> 10) & 1 == 1;
            let n = self.v[zn].to_le_bytes();
            let m = self.v[zm].to_le_bytes();
            let mut dst = [0u8; 16];
            for e in 0..(16 / esz) {
                let off = e * esz;
                let (x, y) = (read_elem(&n, off, esz), read_elem(&m, off, esz));
                let r = match (rsqrt, esz) {
                    (true, 2) => fp16_rsqrts_with_fpcr(x as u16, y as u16, self.fpcr) as u64,
                    (false, 2) => fp16_recps_with_fpcr(x as u16, y as u16, self.fpcr) as u64,
                    (true, 4) => fp_three_same_f32_with_fpcr(
                        FpKind::Rsqrts,
                        x as u32,
                        y as u32,
                        0,
                        self.fpcr,
                    ) as u64,
                    (false, 4) => {
                        fp_three_same_f32_with_fpcr(FpKind::Recps, x as u32, y as u32, 0, self.fpcr)
                            as u64
                    }
                    (true, _) => fp_three_same_f64_with_fpcr(FpKind::Rsqrts, x, y, 0, self.fpcr),
                    (false, _) => fp_three_same_f64_with_fpcr(FpKind::Recps, x, y, 0, self.fpcr),
                };
                self.fpsr |= fp_status_recps_rsqrts_with_fpcr(esz, rsqrt, x, y, r, self.fpcr);
                write_elem(&mut dst, off, esz, r);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // SVE FTSMUL (trigonometric starting value): 0x65, bit21==0,
        // bits[15:10]==000011. result = Zn[e]^2 with sign from Zm[e] bit0.
        if (insn >> 24) & 0xFF == 0b01100101
            && (insn >> 21) & 1 == 0
            && (insn >> 10) & 0x3F == 0b000011
        {
            let size = (insn >> 22) & 0x3;
            if size == 0 {
                return Ok(CpuExit::Undefined(insn));
            }
            let esz = 1usize << size;
            let n = self.v[zn].to_le_bytes();
            let m = self.v[zm].to_le_bytes();
            let mut dst = [0u8; 16];
            for e in 0..(16 / esz) {
                let off = e * esz;
                let raw_x = read_elem(&n, off, esz);
                let x = fp_flush_input_bits_with_fpcr(raw_x, (esz * 8) as u32, self.fpcr);
                let sgn = read_elem(&m, off, esz) & 1;
                let r = sve_ftsmul(esz, x, sgn, self.fpcr);
                let status = if fp_is_snan_bits(esz, x) {
                    FPSR_IOC
                } else if esz == 8 {
                    if fp_is_nan_bits(esz, x) || fp_is_inf_bits(esz, x) || fp_is_zero_bits(esz, x) {
                        0
                    } else if fp64_is_inf(r) {
                        FPSR_OFC | FPSR_IXC
                    } else if fp64_is_tiny(r) || fp64_is_zero(r) {
                        FPSR_UFC | FPSR_IXC
                    } else if fp64_mul_exact(x, x) {
                        0
                    } else {
                        FPSR_IXC
                    }
                } else {
                    let exact = sve_fp_to_f64(esz, x) * sve_fp_to_f64(esz, x);
                    let signed_exact = if sgn != 0 { -exact } else { exact };
                    fp_status_from_exact_f64(esz, signed_exact, r)
                };
                self.fpsr |= status | fp_fz_input_status(esz, raw_x, self.fpcr);
                write_elem(&mut dst, off, esz, r);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // SVE FTMAD (trigonometric multiply-add coefficient): 0x65,
        // bits[21:19]==010, bits[15:10]==100000. Destructive: Zdn = fused(Zdn,
        // |Zm|, coeff[imm + 8*(Zm<0)]); Zm is at bits[9:5], imm at bits[18:16].
        if (insn >> 24) & 0xFF == 0b01100101
            && (insn >> 19) & 0x7 == 0b010
            && (insn >> 10) & 0x3F == 0b100000
        {
            let size = (insn >> 22) & 0x3;
            if size == 0 {
                return Ok(CpuExit::Undefined(insn));
            }
            let esz = 1usize << size;
            let imm = ((insn >> 16) & 0x7) as usize;
            let dn = self.v[zd].to_le_bytes(); // Zdn
            let m = self.v[zn].to_le_bytes(); // Zm at bits[9:5]
            let mut dst = [0u8; 16];
            for e in 0..(16 / esz) {
                let off = e * esz;
                let nn = read_elem(&dn, off, esz);
                let mm = read_elem(&m, off, esz);
                let r = sve_ftmad(esz, nn, mm, imm, self.fpcr);
                let neg = match esz {
                    2 => mm & 0x8000 != 0,
                    4 => mm & 0x8000_0000 != 0,
                    _ => mm & 0x8000_0000_0000_0000 != 0,
                };
                let m_abs = mm & elem_mask((esz * 8 - 1) as u32);
                let coeff = match esz {
                    2 => FTMAD_COEFF_H[imm + if neg { 8 } else { 0 }] as u64,
                    4 => FTMAD_COEFF_S[imm + if neg { 8 } else { 0 }] as u64,
                    _ => FTMAD_COEFF_D[imm + if neg { 8 } else { 0 }],
                };
                self.fpsr |= fp_status_fma_with_fpcr(esz, coeff, nn, m_abs, r, self.fpcr);
                write_elem(&mut dst, off, esz, r);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // SVE2.1 FCLAMP (FP clamp): 0x64, bit21==1, bits[15:10]==001001.
        // Zd = fminnum(fmaxnum(Zn, Zd), Zm) per element (Zd is the clamped
        // value). Must precede the reduce/unary dispatch (which keys on
        // bits[15:13], that FCLAMP's 001001 would otherwise hit as 001).
        if (insn >> 24) & 0xFF == 0b01100100
            && (insn >> 21) & 1 == 1
            && (insn >> 10) & 0x3F == 0b001001
        {
            if !self.config.features.contains(ArmFeatures::SVE2P1) {
                return Ok(CpuExit::Undefined(insn));
            }
            let n = self.v[zn].to_le_bytes();
            let m = self.v[zm].to_le_bytes();
            let d = self.v[zd].to_le_bytes();
            let mut dst = [0u8; 16];
            for e in 0..(16 / esize) {
                let off = e * esize;
                let lo = sve_fp_combine(
                    FpKind::MaxNm,
                    esize,
                    read_elem(&n, off, esize),
                    read_elem(&d, off, esize),
                );
                let r = sve_fp_combine(FpKind::MinNm, esize, lo, read_elem(&m, off, esize));
                write_elem(&mut dst, off, esize, r);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // SVE2.1 FP quadword reductions (FADDQV/FMAXNMQV/FMINNMQV/FMAXQV/
        // FMINQV): 0x64, bits[21:19]==010, bits[15:13]==101. opc=bits[18:16].
        // Each element position is reduced across the 128-bit segments seeded
        // with the op identity; at VL=128 an active lane is combined with the
        // identity (so FADD normalises -0.0/quiets NaN) and an inactive lane is
        // the raw identity. Must precede the unary dispatch (bits[15:13]==101).
        if (insn >> 24) & 0xFF == 0b01100100
            && (insn >> 19) & 0x7 == 0b010
            && (insn >> 13) & 0x7 == 0b101
        {
            if !self.config.features.contains(ArmFeatures::SVE2P1) {
                return Ok(CpuExit::Undefined(insn));
            }
            let kind = match (insn >> 16) & 0x7 {
                0b000 => FpKind::Add,
                0b100 => FpKind::MaxNm,
                0b101 => FpKind::MinNm,
                0b110 => FpKind::Max,
                0b111 => FpKind::Min,
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            // At VL=128 each element position reduces a single-element column,
            // so an active lane is the raw Zn element (no combine with the
            // identity -> -0.0 and NaN payloads are preserved, matching hw); an
            // inactive lane is the op identity (the empty reduction).
            let ident = sve_fp_identity(kind, esize);
            let pred = self.sve_p[pg];
            let src = self.v[zn].to_le_bytes();
            let mut dst = [0u8; 16];
            for e in 0..(16 / esize) {
                let off = e * esize;
                let v = if (pred >> off) & 1 == 1 {
                    read_elem(&src, off, esize)
                } else {
                    ident
                };
                write_elem(&mut dst, off, esize, v);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // FP fast reductions / FADDA live at bits[15:13]==001; FP unary at
        // bits[15:13]==101; predicated binary arith at bits[15:13]==100.
        match (insn >> 13) & 0x7 {
            0b001 => return self.exec_sve_fp_reduce(insn, esize),
            0b101 => return self.exec_sve_fp_unary(insn, zd, zn, pg, esize),
            0b100 => {}
            _ => return Ok(CpuExit::Undefined(insn)),
        }

        if (insn >> 24) & 0xFF == 0b01100100 && (insn >> 19) & 0x7 != 0b010 {
            return Ok(CpuExit::Undefined(insn));
        }

        // FP pairwise (FADDP/FMAXNMP/FMINNMP/FMAXP/FMINP): 0x64, bits[21:19]==010.
        // Interleaves the pairwise results of Zdn and Zm (even = Zdn pair, odd =
        // Zm pair), merged into Zdn under Pg. opc=bits[18:16].
        if (insn >> 24) & 0xFF == 0b01100100 && (insn >> 19) & 0x7 == 0b010 {
            // FP pairwise (FADDP/FMAXNMP/FMINNMP/FMAXP/FMINP) is only defined for
            // H/S/D elements, so size==00 is reserved, and only opc values 000,
            // 100, 101, 110, 111 are allocated. Reject the reserved size and the
            // reserved opc values (001/010/011) instead of executing them.
            if (insn >> 22) & 0x3 == 0b00 {
                return Ok(CpuExit::Undefined(insn));
            }
            let kind = match (insn >> 16) & 0x7 {
                0b000 => FpKind::Add,
                0b100 => FpKind::MaxNm,
                0b101 => FpKind::MinNm,
                0b110 => FpKind::Max,
                0b111 => FpKind::Min,
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            let pred = self.sve_p[pg];
            let elements = 16 / esize;
            let h = elements / 2;
            let dn = self.v[zd].to_le_bytes(); // Zdn
            let m = self.v[zn].to_le_bytes(); // Zm
            let mut res = [0u8; 16];
            for p in 0..h {
                let even_off = 2 * p * esize;
                let odd_off = (2 * p + 1) * esize;
                let dn0 = read_elem(&dn, even_off, esize);
                let dn1 = read_elem(&dn, odd_off, esize);
                let m0 = read_elem(&m, even_off, esize);
                let m1 = read_elem(&m, odd_off, esize);
                let dnv =
                    sve_fp_pairwise_reduce_combine_with_fpcr(kind, esize, dn0, dn1, self.fpcr);
                let mv = sve_fp_pairwise_reduce_combine_with_fpcr(kind, esize, m0, m1, self.fpcr);
                if (pred >> even_off) & 1 == 1 {
                    self.fpsr |=
                        fp_pairwise_reduce_status_with_fpcr(esize, kind, dn0, dn1, dnv, self.fpcr);
                }
                if (pred >> odd_off) & 1 == 1 {
                    self.fpsr |=
                        fp_pairwise_reduce_status_with_fpcr(esize, kind, m0, m1, mv, self.fpcr);
                }
                write_elem(&mut res, even_off, esize, dnv);
                write_elem(&mut res, odd_off, esize, mv);
            }
            let mut dst = dn;
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 1 {
                    write_elem(
                        &mut dst,
                        e * esize,
                        esize,
                        read_elem(&res, e * esize, esize),
                    );
                }
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }
        let opc5 = (insn >> 16) & 0x1F;
        // FSCALE (opc5==01001): Zdn = Zdn * 2^(signed Zm element), merging. The
        // Zm element is a signed integer exponent, not a float.
        if opc5 == 0b01001 {
            if (insn >> 22) & 0x3 == 0 {
                return Ok(CpuExit::Undefined(insn));
            }
            let pred = self.sve_p[pg];
            let a = self.v[zd].to_le_bytes(); // Zdn
            let b = self.v[zn].to_le_bytes(); // Zm
            let mut dst = a;
            let ibits = (esize * 8) as u32;
            for e in 0..(16 / esize) {
                let off = e * esize;
                if (pred >> off) & 1 == 0 {
                    continue;
                }
                let raw_x = read_elem(&a, off, esize);
                let x = fp_flush_input_bits_with_fpcr(raw_x, ibits, self.fpcr);
                let n = sext_elem(read_elem(&b, off, esize), ibits) as i64;
                let r = sve_fscale(esize, x, n, self.fpcr);
                let status = fp_status_fscale(esize, x, n, r);
                let input_status = fp_fz_input_status(esize, raw_x, self.fpcr);
                self.fpsr |= if self.fpcr & FPCR_AH != 0
                    && input_status != 0
                    && status == (FPSR_UFC | FPSR_IXC)
                {
                    input_status
                } else {
                    status | input_status
                };
                write_elem(&mut dst, off, esize, r);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }
        let (kind, swap) = match opc5 {
            0b00000 => (FpKind::Add, false),
            0b00001 => (FpKind::Sub, false),
            0b00010 => (FpKind::Mul, false),
            0b00011 => (FpKind::Sub, true), // FSUBR
            0b00100 => (FpKind::MaxNm, false),
            0b00101 => (FpKind::MinNm, false),
            0b00110 => (FpKind::Max, false),
            0b00111 => (FpKind::Min, false),
            0b01000 => (FpKind::Abd, false),
            0b01010 => (FpKind::Mulx, false), // FMULX
            0b01100 => (FpKind::Div, true),   // FDIVR
            0b01101 => (FpKind::Div, false),
            0b11000 => (FpKind::Add, false),   // FADD #0.5/#1.0
            0b11001 => (FpKind::Sub, false),   // FSUB #0.5/#1.0
            0b11010 => (FpKind::Mul, false),   // FMUL #0.5/#1.0
            0b11011 => (FpKind::Sub, true),    // FSUBR #0.5/#1.0
            0b11100 => (FpKind::MaxNm, false), // FMAXNM #0.0/#1.0
            0b11101 => (FpKind::MinNm, false), // FMINNM #0.0/#1.0
            0b11110 => (FpKind::Max, false),   // FMAX #0.0/#1.0
            0b11111 => (FpKind::Min, false),   // FMIN #0.0/#1.0
            _ => return Ok(CpuExit::Undefined(insn)),
        };
        let immediate_scalar = opc5 >= 0b11000;
        let scalar = if immediate_scalar {
            let one = (insn >> 5) & 1 == 1;
            let zero = opc5 >= 0b11100 && !one;
            Some(match (esize, zero, one) {
                (2, true, _) => 0,
                (2, false, false) => 0x3800,
                (2, false, true) => 0x3c00,
                (4, true, _) => 0,
                (4, false, false) => 0x3f00_0000,
                (4, false, true) => 0x3f80_0000,
                (8, true, _) => 0,
                (8, false, false) => 0x3fe0_0000_0000_0000,
                (8, false, true) => 0x3ff0_0000_0000_0000,
                _ => return Ok(CpuExit::Undefined(insn)),
            })
        } else {
            None
        };
        let pred = self.sve_p[pg];
        let elements = 16 / esize;
        let a_reg = self.v[zd].to_le_bytes(); // Zdn (first source, dest)
        let b_reg = self.v[zn].to_le_bytes(); // Zm (second source)
        let mut dst = a_reg;
        for e in 0..elements {
            if (pred >> (e * esize)) & 1 == 0 {
                continue;
            }
            let off = e * esize;
            let a = read_elem(&a_reg, off, esize);
            let b = scalar.unwrap_or_else(|| read_elem(&b_reg, off, esize));
            let (x, y) = if swap { (b, a) } else { (a, b) };
            let mut r = match esize {
                2 => sve_fp16_binop_with_fpcr(kind, x as u16, y as u16, self.fpcr) as u64,
                4 => fp_three_same_f32_with_fpcr(kind, x as u32, y as u32, 0, self.fpcr) as u64,
                8 => fp_three_same_f64_with_fpcr(kind, x, y, 0, self.fpcr),
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            if self.fpcr & FPCR_AH != 0
                && immediate_scalar
                && kind == FpKind::Min
                && scalar == Some(0)
                && fp_is_zero_bits(esize, x)
            {
                r = 0;
            }
            let status = fp_three_same_status_with_fpcr(esize, kind, x, y, 0, r, self.fpcr);
            self.fpsr |= if kind == FpKind::Mulx {
                fp_status_sve_underflow(esize, r, status)
            } else {
                status
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute SVE FP reduction to a scalar in Vd: the "fast" tree reductions
    /// FADDV/FMAXNMV/FMINNMV/FMAXV/FMINV (opc=bits[18:16]) and the strictly
    /// ordered FADDA (bits[20:16]==11000). Pg is byte-granular.
    pub(crate) fn exec_sve_fp_reduce(&mut self, insn: u32, esize: usize) -> Result<CpuExit, ArmError> {
        if esize < 2 {
            return Ok(CpuExit::Undefined(insn));
        }
        let pg = ((insn >> 10) & 0x7) as usize;
        let zn = ((insn >> 5) & 0x1F) as usize;
        let vd = (insn & 0x1F) as usize;
        let pred = self.sve_p[pg];
        let elements = 16 / esize;
        let mask = elem_mask((esize * 8) as u32) as u128;
        // FADDA: strict left-to-right accumulate seeded by Vdn[0]; skip inactive.
        if (insn >> 16) & 0x1F == 0b11000 {
            let m_reg = self.v[zn].to_le_bytes(); // Zm
            let vd_bytes = self.v[vd].to_le_bytes();
            let mut acc = read_elem(&vd_bytes, 0, esize);
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 1 {
                    let x = read_elem(&m_reg, e * esize, esize);
                    let r = sve_fp_combine_with_fpcr(FpKind::Add, esize, acc, x, self.fpcr);
                    self.fpsr |=
                        fp_status_binop_with_fpcr(esize, FpKind::Add, acc, x, r, self.fpcr);
                    acc = r;
                }
            }
            self.v[vd] = (acc as u128) & mask;
            return Ok(CpuExit::Continue);
        }
        let kind = match (insn >> 16) & 0x7 {
            0b000 => FpKind::Add,   // FADDV
            0b100 => FpKind::MaxNm, // FMAXNMV
            0b101 => FpKind::MinNm, // FMINNMV
            0b110 => FpKind::Max,   // FMAXV
            0b111 => FpKind::Min,   // FMINV
            _ => return Ok(CpuExit::Undefined(insn)),
        };
        let ident = sve_fp_identity(kind, esize);
        let src = self.v[zn].to_le_bytes();
        let buf: Vec<u64> = (0..elements)
            .map(|e| {
                if (pred >> (e * esize)) & 1 == 1 {
                    read_elem(&src, e * esize, esize)
                } else {
                    ident
                }
            })
            .collect();
        let (r, status) = sve_fp_tree_reduce_status(&buf, kind, esize, self.fpcr);
        self.fpsr |= status;
        self.v[vd] = (r as u128) & mask;
        Ok(CpuExit::Continue)
    }


    /// Execute SVE predicated FP unary (merging): FSQRT, FRECPX and FRINT*
    /// (bits[20:16] selects the op). Inactive lanes keep their prior Zd value.
    pub(crate) fn exec_sve_fp_unary(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        pg: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        // FCVT (FP precision conversion): 0x65, bits[21:18]==0010. bits[23:22]
        // (opc) and bits[17:16] (opc2) select the src/dst float widths, NOT the
        // element size, so it bypasses the size-derived esize path entirely. The
        // 0x65 gate excludes the 0x64 FCVTNT/FCVTLT/FCVTXNT top/bottom variants.
        if (insn >> 24) & 0xFF == 0b01100101 && (insn >> 18) & 0xF == 0b0010 {
            return self.exec_sve_fcvt(insn, zd, zn, pg);
        }
        // FLOGB (find exponent): 0x65, bits[23:19]==0b00011, size in bits[18:17].
        // The element size is not bits[23:22] (those are 0), so it is computed
        // locally. Result is floor(log2|x|) as a signed integer, merging.
        if (insn >> 24) & 0xFF == 0b01100101 && (insn >> 19) & 0x1F == 0b00011 {
            let size = (insn >> 17) & 0x3;
            if size == 0 {
                return Ok(CpuExit::Undefined(insn));
            }
            let esz = 1usize << size;
            let pred = self.sve_p[pg];
            let src = self.v[zn].to_le_bytes();
            let mut dst = self.v[zd].to_le_bytes();
            let elements = 16 / esz;
            for e in 0..elements {
                let off = e * esz;
                if (pred >> off) & 1 == 0 {
                    continue;
                }
                let raw = read_elem(&src, off, esz);
                let x = fp_flush_input_bits_with_fpcr(raw, (esz * 8) as u32, self.fpcr);
                if fp_is_zero_bits(esz, x) || fp_is_nan_bits(esz, x) {
                    self.fpsr |= FPSR_IOC;
                }
                self.fpsr |= fp_fz_input_status(esz, raw, self.fpcr);
                let r = sve_flogb(esz, x);
                write_elem(&mut dst, off, esz, r as u64);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }
        // FP<->int conversions: 0x65, bits[21:19]==011 (FCVTZS/FCVTZU, FP->int)
        // or ==010 (SCVTF/UCVTF, int->FP). bits[23:22]/bits[18:17] pick the
        // widths and bit16 the signedness, so this also bypasses the esize path.
        // FLOGB (bits[23:22]==00) is intercepted above before reaching here.
        if (insn >> 24) & 0xFF == 0b01100101
            && ((insn >> 19) & 0x7 == 0b011 || (insn >> 19) & 0x7 == 0b010)
        {
            return self.exec_sve_fp_int_cvt(insn, zd, zn, pg);
        }
        // FCVTNT/FCVTXNT (narrow, top) and FCVTLT (long, top): 0x64,
        // bits[21:18]==0010. The wider element is the container; for narrowing
        // the converted result goes to the top (odd) half (bottom preserved),
        // for widening the source is read from the top (odd) half. FCVTXNT uses
        // round-to-odd. Predication is at the container (wider) granularity.
        if (insn >> 24) & 0xFF == 0b01100100 && (insn >> 18) & 0xF == 0b0010 {
            let opc = (insn >> 22) & 0x3;
            let opc2 = (insn >> 16) & 0x3;
            // (src,dst,round_odd,narrow,bf): bf marks the bf16 destination
            // (BFCVTNT), distinguished from FCVTNT s->h only by opc2 (10 vs 00).
            let (src_sz, dst_sz, round_odd, narrow, bf): (usize, usize, bool, bool, bool) =
                match (opc, opc2) {
                    (0b00, 0b10) => (8, 4, true, true, false),  // FCVTXNT d->s
                    (0b10, 0b00) => (4, 2, false, true, false), // FCVTNT  s->h
                    (0b10, 0b10) => (4, 2, false, true, true),  // BFCVTNT s->bf16
                    (0b11, 0b10) => (8, 4, false, true, false), // FCVTNT  d->s
                    (0b10, 0b01) => (2, 4, false, false, false), // FCVTLT h->s
                    (0b11, 0b11) => (4, 8, false, false, false), // FCVTLT s->d
                    _ => return Ok(CpuExit::Undefined(insn)),
                };
            let cont = src_sz.max(dst_sz);
            let elements = 16 / cont;
            let pred = self.sve_p[pg];
            let operand = self.v[zn].to_le_bytes();
            let mut dst = self.v[zd].to_le_bytes();
            for c in 0..elements {
                let coff = c * cont;
                if (pred >> coff) & 1 == 0 {
                    continue;
                }
                let convert = |x: u64| -> u64 {
                    if bf {
                        f32_to_bf16_with_fpcr(x as u32, self.fpcr) as u64
                    } else {
                        fp_cvt_elem(x, src_sz, dst_sz, round_odd, self.fpcr)
                    }
                };
                if narrow {
                    let x = read_elem(&operand, coff, src_sz);
                    let res = convert(x);
                    self.fpsr |= if bf {
                        fp_status_bfcvt_with_fpcr(x as u32, res as u16, self.fpcr)
                    } else {
                        fp_status_cvt_precision_with_fpcr_rounding(
                            x, src_sz, dst_sz, res, round_odd, self.fpcr,
                        )
                    };
                    write_elem(&mut dst, coff + dst_sz, dst_sz, res); // top half
                } else {
                    let x = read_elem(&operand, coff + src_sz, src_sz); // top half
                    let res = convert(x);
                    self.fpsr |=
                        fp_status_cvt_precision_with_fpcr(x, src_sz, dst_sz, res, self.fpcr);
                    write_elem(&mut dst, coff, dst_sz, res);
                }
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }
        if esize < 2 {
            return Ok(CpuExit::Undefined(insn));
        }
        let b20_16 = (insn >> 16) & 0x1F;
        if !matches!(
            b20_16,
            0b00000..=0b00100 | 0b00110 | 0b00111 | 0b01100 | 0b01101
        ) {
            return Ok(CpuExit::Undefined(insn));
        }
        // FRINT* rounding -> (TwoRegFp variant, fp16 mode).
        let rint = |m: u32| -> Option<(TwoRegFp, u8)> {
            Some(match m {
                0b000 => (TwoRegFp::RintN, 0),
                0b001 => (TwoRegFp::RintP, 2),
                0b010 => (TwoRegFp::RintM, 1),
                0b011 => (TwoRegFp::RintZ, 3),
                0b100 => (TwoRegFp::RintA, 4),
                0b110 => (TwoRegFp::RintX, 0),
                0b111 => (TwoRegFp::RintI, 0),
                _ => return None,
            })
        };
        let pred = self.sve_p[pg];
        let elements = 16 / esize;
        let src = self.v[zn].to_le_bytes();
        let mut dst = self.v[zd].to_le_bytes(); // merging: start from Zd
        for e in 0..elements {
            if (pred >> (e * esize)) & 1 == 0 {
                continue;
            }
            let off = e * esize;
            let lane = read_elem(&src, off, esize);
            let (r, status) = match b20_16 {
                0b01101 => {
                    let kind = TwoRegFp::Fsqrt;
                    let r = match esize {
                        2 => fp16_sqrt_with_fpcr(lane as u16, self.fpcr) as u64,
                        4 => fp_two_reg_f32_with_fpcr(kind, lane as u32, self.fpcr) as u64,
                        _ => fp_two_reg_f64_with_fpcr(kind, lane, self.fpcr),
                    };
                    (
                        r,
                        fp_status_unop_with_fpcr(esize, Some(kind), lane, r, self.fpcr),
                    )
                }
                0b01100 => {
                    let a = fp_flush_input_bits_with_fpcr(lane, (esize * 8) as u32, self.fpcr);
                    let mut status = if fp_is_snan_bits(esize, a) {
                        FPSR_IOC
                    } else {
                        0
                    };
                    if self.fpcr & FPCR_AH == 0 {
                        status |= fp_fz_input_status(esize, lane, self.fpcr);
                    }
                    (sve_fp_recpx(esize, a), status)
                }
                m if m < 0b01000 => {
                    let Some((trk, fp16m)) = rint(m) else {
                        return Ok(CpuExit::Undefined(insn));
                    };
                    let r = match esize {
                        2 if matches!(trk, TwoRegFp::RintX | TwoRegFp::RintI) => {
                            fp16_frint_with_fpcr(lane as u16, self.fpcr) as u64
                        }
                        2 => fp16_frint_fixed_with_fpcr(lane as u16, fp16m, self.fpcr) as u64,
                        4 => fp_two_reg_f32_with_fpcr(trk, lane as u32, self.fpcr) as u64,
                        _ => fp_two_reg_f64_with_fpcr(trk, lane, self.fpcr),
                    };
                    (
                        r,
                        fp_status_unop_with_fpcr(esize, Some(trk), lane, r, self.fpcr),
                    )
                }
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            self.fpsr |= status;
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute SVE FCVT (predicated FP precision conversion between fp16/fp32/
    /// fp64). The per-element container size is the larger of the source and
    /// destination widths; the source value occupies the low bits of its
    /// container and the (zero-extended) result is written back. Predication is
    /// byte-granular at the container size and merges (inactive lanes keep Zd).
    pub(crate) fn exec_sve_fcvt(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        pg: usize,
    ) -> Result<CpuExit, ArmError> {
        let opc = (insn >> 22) & 0x3;
        let opc2 = (insn >> 16) & 0x3;
        // round_odd marks FCVTX (double->single, round-to-odd) which shares the
        // (8,4) widths with regular FCVT double->single but uses RO rounding.
        let (src_sz, dst_sz, round_odd, bf): (usize, usize, bool, bool) = match (opc, opc2) {
            (0b10, 0b01) => (2, 4, false, false), // half   -> single
            (0b11, 0b01) => (2, 8, false, false), // half   -> double
            (0b10, 0b00) => (4, 2, false, false), // single -> half
            (0b11, 0b11) => (4, 8, false, false), // single -> double
            (0b11, 0b00) => (8, 2, false, false), // double -> half
            (0b11, 0b10) => (8, 4, false, false), // double -> single
            (0b00, 0b10) => (8, 4, true, false),  // FCVTX  double -> single (round-to-odd)
            (0b10, 0b10) => (4, 2, false, true),  // BFCVT  single -> bf16
            _ => return Ok(CpuExit::Undefined(insn)),
        };
        let cont = src_sz.max(dst_sz);
        let elements = 16 / cont;
        let pred = self.sve_p[pg];
        let operand = self.v[zn].to_le_bytes();
        let mut dst = self.v[zd].to_le_bytes(); // merging: start from Zd
        for e in 0..elements {
            let off = e * cont;
            if (pred >> off) & 1 == 0 {
                continue;
            }
            let x = read_elem(&operand, off, src_sz);
            let res = if !bf && fp_is_nan_bits(src_sz, x) {
                fp_convert_nan(x, src_sz, dst_sz)
            } else {
                if bf {
                    f32_to_bf16_with_fpcr(x as u32, self.fpcr) as u64
                } else {
                    fp_cvt_elem(x, src_sz, dst_sz, round_odd, self.fpcr)
                }
            };
            self.fpsr |= if bf {
                fp_status_bfcvt_with_fpcr(x as u32, res as u16, self.fpcr)
            } else {
                fp_status_cvt_precision_with_fpcr_rounding(
                    x, src_sz, dst_sz, res, round_odd, self.fpcr,
                )
            };
            write_elem(&mut dst, off, cont, res);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute SVE FCVTZS/FCVTZU (FP -> integer, round toward zero, saturating)
    /// and SCVTF/UCVTF (integer -> FP, round to nearest even). The per-element
    /// container is the larger of the FP and integer widths; the source occupies
    /// the low bits of its container and the result is zero-extended back.
    /// Predication is byte-granular at the container size and merges.
    pub(crate) fn exec_sve_fp_int_cvt(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        pg: usize,
    ) -> Result<CpuExit, ArmError> {
        let opc = (insn >> 22) & 0x3;
        let opc2 = (insn >> 17) & 0x3;
        let signed = (insn >> 16) & 1 == 0; // int_U: 0=signed, 1=unsigned
        let to_int = (insn >> 19) & 0x7 == 0b011; // FCVTZ; else SCVTF/UCVTF
        let (fp_sz, int_sz): (usize, usize) = match (opc, opc2) {
            (0b01, 0b01) => (2, 2), // fp16 <-> int16
            (0b01, 0b10) => (2, 4), // fp16 <-> int32
            (0b01, 0b11) => (2, 8), // fp16 <-> int64
            (0b10, 0b10) => (4, 4), // f32  <-> int32
            (0b11, 0b00) => (8, 4), // f64  <-> int32
            (0b11, 0b10) => (4, 8), // f32  <-> int64
            (0b11, 0b11) => (8, 8), // f64  <-> int64
            _ => return Ok(CpuExit::Undefined(insn)),
        };
        let cont = fp_sz.max(int_sz);
        let elements = 16 / cont;
        let pred = self.sve_p[pg];
        let operand = self.v[zn].to_le_bytes();
        let mut dst = self.v[zd].to_le_bytes(); // merging: start from Zd
        for e in 0..elements {
            let off = e * cont;
            if (pred >> off) & 1 == 0 {
                continue;
            }
            let res = if to_int {
                let raw = read_elem(&operand, off, fp_sz);
                let x = fp_flush_input_bits_with_fpcr(raw, (fp_sz * 8) as u32, self.fpcr);
                let res = sve_fcvtz(fp_sz, int_sz, signed, x);
                let status = fp_to_int_status(sve_fp_to_f64(fp_sz, x), signed, (int_sz * 8) as u32);
                let input_status = fp_fz_input_status(fp_sz, raw, self.fpcr);
                self.fpsr |= fp_status_merge_input_status(status, input_status, self.fpcr);
                res
            } else {
                let x = read_elem(&operand, off, int_sz);
                let res = sve_cvtf(int_sz, fp_sz, signed, x, self.fpcr);
                let raw_int = if signed {
                    match int_sz {
                        2 => (x as u16 as i16 as i128).unsigned_abs(),
                        4 => (x as u32 as i32 as i128).unsigned_abs(),
                        _ => (x as i64 as i128).unsigned_abs(),
                    }
                } else {
                    match int_sz {
                        2 => (x as u16) as u128,
                        4 => (x as u32) as u128,
                        _ => x as u128,
                    }
                };
                self.fpsr |= fp_status_int_to_fp_scaled(raw_int, fp_sz, res);
                res
            };
            write_elem(&mut dst, off, cont, res);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute SVE load/store instructions. Currently models the contiguous
    /// LD1{B,H,W,D}/LD1S{B,H,W} and ST1{B,H,W,D} forms with a scalar base plus a
    /// VL-scaled immediate (the `_Z.P.BI_` encodings). Predication is
    /// byte-granular; loads zero inactive elements, stores skip them.
    pub(crate) fn exec_sve_ldst(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        // SVE prefetch (PRF*) instructions are architectural hints with no
        // register or memory effect -> no-op. They share the load/store space;
        // detect the PRF encodings (all have bit4==0) per the ARM decode.
        if (insn >> 4) & 1 == 0 {
            let b3125 = (insn >> 25) & 0x7F;
            let (b2423, b2221, b1513) =
                ((insn >> 23) & 0x3, (insn >> 21) & 0x3, (insn >> 13) & 0x7);
            let is_prf = if b3125 == 0b1000010 {
                (b2423 == 0 && (insn >> 21) & 1 == 1 && (insn >> 15) & 1 == 0)
                    || (b2221 == 0 && b1513 == 0b111)
                    || (b2423 == 0b11 && (insn >> 22) & 1 == 1 && (insn >> 15) & 1 == 0)
                    || (b2221 == 0 && b1513 == 0b110)
            } else if b3125 == 0b1100010 {
                (b2423 == 0 && b2221 == 0b11 && (insn >> 15) & 1 == 1)
                    || (b2423 == 0 && (insn >> 21) & 1 == 1 && (insn >> 15) & 1 == 0)
                    || (b2221 == 0 && b1513 == 0b111)
            } else {
                false
            };
            if is_prf {
                // Register-offset PRF[BHWD]_I.P.BR_S reserves Rm==31.
                if b3125 == 0b1000010 && b2221 == 0 && b1513 == 0b110 && (insn >> 16) & 0x1F == 31 {
                    return Ok(CpuExit::Undefined(insn));
                }
                return Ok(CpuExit::Continue);
            }
        }
        let pg = ((insn >> 10) & 0x7) as usize;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let zt = (insn & 0x1F) as usize;
        let imm4 = ((((insn >> 16) & 0xF) as i32) << 28 >> 28) as i64; // signed 4-bit
        let pred = self.sve_p[pg];
        let is_store = (insn >> 30) & 1 == 1;
        let base = if rn == 31 {
            let sp = self.current_sp();
            if sp & 0xF != 0 {
                return Err(ArmError::MemoryError(MemoryFaultInfo {
                    address: sp,
                    access: if is_store {
                        crate::isa::arm::common::cpu::AccessType::Write
                    } else {
                        crate::isa::arm::common::cpu::AccessType::Read
                    },
                    fault_type: MemoryFaultType::Alignment,
                    stage2: false,
                }));
            }
            sp
        } else {
            self.get_x(rn)
        };
        let b15_13 = (insn >> 13) & 0x7;
        // imm9 = SInt(imm9h:imm9l) for the whole-register LDR/STR forms.
        let imm9 = (((((insn >> 16) & 0x3F) << 3) | ((insn >> 10) & 0x7)) as i32) << 23 >> 23;
        let imm9 = imm9 as i64;

        // LDR/STR whole-register fill/spill (unpredicated). Zt loads/stores the
        // full VL/8 (=16) bytes; Pt loads/stores PL/8 (=2) bytes. bits[15:13]:
        // 010 = vector register, 000 = predicate register. The immediate is
        // scaled by the register's byte size.
        if insn >> 22 == 0b1000010110 && b15_13 == 0b010 {
            let addr = base.wrapping_add((imm9 * 16) as u64);
            let mut bytes = [0u8; 16];
            for (i, b) in bytes.iter_mut().enumerate() {
                *b = self.memory.read_u8(self.translate_address(
                    addr.wrapping_add(i as u64),
                    false,
                    false,
                )?)?;
            }
            self.v[zt] = u128::from_le_bytes(bytes);
            return Ok(CpuExit::Continue);
        }
        if insn >> 22 == 0b1110010110 && b15_13 == 0b010 {
            let addr = base.wrapping_add((imm9 * 16) as u64);
            let bytes = self.v[zt].to_le_bytes();
            for (i, b) in bytes.iter().enumerate() {
                self.memory.write_u8(
                    self.translate_address(addr.wrapping_add(i as u64), true, false)?,
                    *b,
                )?;
            }
            return Ok(CpuExit::Continue);
        }
        if insn >> 22 == 0b1000010110 && b15_13 == 0b000 {
            let pt = (insn & 0xF) as usize;
            let addr = base.wrapping_add((imm9 * 2) as u64);
            let b0 = self
                .memory
                .read_u8(self.translate_address(addr, false, false)?)? as u32;
            let b1 = self.memory.read_u8(self.translate_address(
                addr.wrapping_add(1),
                false,
                false,
            )?)? as u32;
            self.sve_p[pt] = b0 | (b1 << 8);
            return Ok(CpuExit::Continue);
        }
        if insn >> 22 == 0b1110010110 && b15_13 == 0b000 {
            let pt = (insn & 0xF) as usize;
            let addr = base.wrapping_add((imm9 * 2) as u64);
            let p = self.sve_p[pt];
            self.memory
                .write_u8(self.translate_address(addr, true, false)?, p as u8)?;
            self.memory.write_u8(
                self.translate_address(addr.wrapping_add(1), true, false)?,
                (p >> 8) as u8,
            )?;
            return Ok(CpuExit::Continue);
        }

        // LD1R (load and replicate): 1000010 dtypeh 1 imm6 1 dtypel Pg Rn Zt.
        // Reads one element at base + imm6*mbytes, extends it to the element
        // width and broadcasts it to every active lane (zeroing the inactive).
        if insn >> 25 == 0b1000010 && (insn >> 22) & 1 == 1 && (insn >> 15) & 1 == 1 {
            let dtype = (((insn >> 23) & 0x3) << 2) | ((insn >> 13) & 0x3);
            let (esize, mbytes, signed) = sve_ld1_dtype(dtype);
            let imm6 = (insn >> 16) & 0x3F; // unsigned
            let elements = 16 / esize;
            let addr = base.wrapping_add((imm6 as u64).wrapping_mul(mbytes as u64));
            let any_active = (0..elements).any(|e| (pred >> (e * esize)) & 1 == 1);
            let val = if any_active {
                let pa = self.translate_address(addr, false, false)?;
                let raw: u64 = match mbytes {
                    1 => self.memory.read_u8(pa)? as u64,
                    2 => self.memory.read_u16(pa)? as u64,
                    4 => self.memory.read_u32(pa)? as u64,
                    _ => self.memory.read_u64(pa)?,
                };
                if signed {
                    (sext_elem(raw, (mbytes * 8) as u32) as u64) & elem_mask((esize * 8) as u32)
                } else {
                    raw
                }
            } else {
                0
            };
            let mut dst = [0u8; 16];
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 1 {
                    write_elem(&mut dst, e * esize, esize, val);
                }
            }
            self.v[zt] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // Contiguous LD1 (scalar + immediate): 1010010 dtype 0 imm4 101 Pg Rn Zt.
        if !is_store && insn >> 25 == 0b1010010 && b15_13 == 0b101 && (insn >> 20) & 1 == 0 {
            let dtype = (insn >> 21) & 0xF;
            let (esize, mbytes, signed) = sve_ld1_dtype(dtype);
            let elements = 16 / esize;
            // Scalar+immediate offset scales by the contiguous memory footprint,
            // not the architectural Z-register byte width. For example, LD1B
            // Zt.H has 8 halfword lanes but reads 8 bytes, so #1 addresses
            // base+8 at VL=128.
            let addr0 = base.wrapping_add((imm4 * (elements * mbytes) as i64) as u64);
            let mut dst = [0u8; 16];
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue; // inactive -> zero (LD1 is zeroing)
                }
                let ea = addr0.wrapping_add((e * mbytes) as u64);
                let pa = self.translate_address(ea, false, false)?;
                let raw: u64 = match mbytes {
                    1 => self.memory.read_u8(pa)? as u64,
                    2 => self.memory.read_u16(pa)? as u64,
                    4 => self.memory.read_u32(pa)? as u64,
                    _ => self.memory.read_u64(pa)?,
                };
                let val = if signed {
                    (sext_elem(raw, (mbytes * 8) as u32) as u64) & elem_mask((esize * 8) as u32)
                } else {
                    raw
                };
                write_elem(&mut dst, e * esize, esize, val);
            }
            self.v[zt] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // Contiguous ST1 (scalar + immediate): 1110010 msz size 0 imm4 111 Pg Rn Zt.
        // msz=bits[24:23] memory width, size=bits[22:21] element width (>= msz).
        if is_store && insn >> 25 == 0b1110010 && b15_13 == 0b111 && (insn >> 20) & 1 == 0 {
            let msz = (insn >> 23) & 0x3;
            let size = (insn >> 21) & 0x3;
            if size < msz {
                return Ok(CpuExit::Undefined(insn)); // element must be >= memory size
            }
            let esize = 1usize << size;
            let mbytes = 1usize << msz;
            let elements = 16 / esize;
            // Scalar+immediate offset scales by the contiguous memory footprint,
            // not the architectural Z-register byte width.
            let addr0 = base.wrapping_add((imm4 * (elements * mbytes) as i64) as u64);
            let src = self.v[zt].to_le_bytes();
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue; // inactive -> leave memory unchanged
                }
                let ea = addr0.wrapping_add((e * mbytes) as u64);
                let pa = self.translate_address(ea, true, false)?;
                let val = read_elem(&src, e * esize, esize); // low msize bytes stored
                match mbytes {
                    1 => self.memory.write_u8(pa, val as u8)?,
                    2 => self.memory.write_u16(pa, val as u16)?,
                    4 => self.memory.write_u32(pa, val as u32)?,
                    _ => self.memory.write_u64(pa, val)?,
                }
            }
            return Ok(CpuExit::Continue);
        }

        // LD1 (scalar + scalar register offset): 1010010 dtype Rm 010 Pg Rn Zt.
        // addr = base + (Xm + e) * mbytes. Rm==31 is UNDEFINED.
        if !is_store && insn >> 25 == 0b1010010 && b15_13 == 0b010 {
            let rm = ((insn >> 16) & 0x1F) as u8;
            if rm == 31 {
                return Ok(CpuExit::Undefined(insn));
            }
            let dtype = (insn >> 21) & 0xF;
            let (esize, mbytes, signed) = sve_ld1_dtype(dtype);
            let elements = 16 / esize;
            let addr0 = base.wrapping_add(self.get_x(rm).wrapping_mul(mbytes as u64));
            let mut dst = [0u8; 16];
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue;
                }
                let pa =
                    self.translate_address(addr0.wrapping_add((e * mbytes) as u64), false, false)?;
                let raw: u64 = match mbytes {
                    1 => self.memory.read_u8(pa)? as u64,
                    2 => self.memory.read_u16(pa)? as u64,
                    4 => self.memory.read_u32(pa)? as u64,
                    _ => self.memory.read_u64(pa)?,
                };
                let val = if signed {
                    (sext_elem(raw, (mbytes * 8) as u32) as u64) & elem_mask((esize * 8) as u32)
                } else {
                    raw
                };
                write_elem(&mut dst, e * esize, esize, val);
            }
            self.v[zt] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // LDFF1 (first-fault contiguous, scalar+scalar): 1010010 dtype Rm 011 Pg
        // Rn Zt. Like LD1 (addr=base+(Xm+e)*mbytes) but the first active element
        // faults normally while later elements are suppressed (FFR cleared).
        if !is_store && insn >> 25 == 0b1010010 && b15_13 == 0b011 {
            let dtype = (insn >> 21) & 0xF;
            let (esize, mbytes, signed) = sve_ld1_dtype(dtype);
            let elements = 16 / esize;
            let rm = ((insn >> 16) & 0x1F) as u8;
            let addr0 = base.wrapping_add(self.get_x(rm).wrapping_mul(mbytes as u64));
            return self.exec_sve_ff_load(addr0, mbytes, esize, signed, elements, pred, zt, false);
        }

        // LDNF1 (non-fault contiguous, scalar+imm): 1010010 dtype 1 imm4 101 Pg
        // Rn Zt (bit20==1 separates it from LD1's bit20==0). No access faults;
        // any element that would fault is suppressed (FFR cleared).
        if !is_store && insn >> 25 == 0b1010010 && b15_13 == 0b101 && (insn >> 20) & 1 == 1 {
            let dtype = (insn >> 21) & 0xF;
            let (esize, mbytes, signed) = sve_ld1_dtype(dtype);
            let elements = 16 / esize;
            // Scalar+immediate offset scales by the contiguous memory footprint,
            // not the architectural Z-register byte width.
            let addr0 = base.wrapping_add((imm4 * (elements * mbytes) as i64) as u64);
            return self.exec_sve_ff_load(addr0, mbytes, esize, signed, elements, pred, zt, true);
        }

        // ST1 (scalar + scalar register offset): 1110010 msz size Rm 010 Pg Rn Zt.
        if is_store && insn >> 25 == 0b1110010 && b15_13 == 0b010 {
            let rm = ((insn >> 16) & 0x1F) as u8;
            if rm == 31 {
                return Ok(CpuExit::Undefined(insn));
            }
            let msz = (insn >> 23) & 0x3;
            let size = (insn >> 21) & 0x3;
            if size < msz {
                return Ok(CpuExit::Undefined(insn));
            }
            let esize = 1usize << size;
            let mbytes = 1usize << msz;
            let elements = 16 / esize;
            let addr0 = base.wrapping_add(self.get_x(rm).wrapping_mul(mbytes as u64));
            let src = self.v[zt].to_le_bytes();
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue;
                }
                let pa =
                    self.translate_address(addr0.wrapping_add((e * mbytes) as u64), true, false)?;
                let val = read_elem(&src, e * esize, esize);
                match mbytes {
                    1 => self.memory.write_u8(pa, val as u8)?,
                    2 => self.memory.write_u16(pa, val as u16)?,
                    4 => self.memory.write_u32(pa, val as u32)?,
                    _ => self.memory.write_u64(pa, val)?,
                }
            }
            return Ok(CpuExit::Continue);
        }

        // LD1 gather (64-bit scalar base + vector offset, D elements):
        // 1100010 msz ig1 Zm 1 U ff Pg Rn Zt. esize=64; addr[e] = Xn +
        // (Zm[e] << scale); scale = msz when scaled (bits[22:21]==11) else 0;
        // load msize bytes and sign(U=0)/zero(U=1)-extend; inactive lanes zero.
        // bit22==1 (ig1 high) separates it from the vector-base form (ig1==01).
        // ff=bit13 selects the first-fault LDFF1 gather (faults on non-first active
        // lanes are suppressed and reflected in FFR; see exec_sve_gather_load).
        if insn >> 25 == 0b1100010 && (insn >> 22) & 1 == 1 && (insn >> 15) & 1 == 1 {
            let msz = (insn >> 23) & 0x3;
            let scaled = (insn >> 21) & 0x3 == 0b11;
            let unsigned = (insn >> 14) & 1 == 1;
            // No scaled-byte gather exists, and there is no signed 64-bit load
            // (LD1SD); both are unallocated and must trap, not read memory.
            if (scaled && msz == 0) || (!unsigned && msz == 3) {
                return Ok(CpuExit::Undefined(insn));
            }
            let zm = ((insn >> 16) & 0x1F) as usize;
            let mbytes = 1usize << msz;
            let scale = if scaled { msz } else { 0 };
            let esize = 8usize; // D
            let elements = 16 / esize;
            let offs = self.v[zm].to_le_bytes();
            let mut addrs = [0u64; 4];
            for (e, slot) in addrs.iter_mut().enumerate().take(elements) {
                let off = read_elem(&offs, e * esize, esize); // 64-bit unsigned offset
                *slot = base.wrapping_add(off << scale);
            }
            let first_fault = (insn >> 13) & 1 == 1;
            return self.exec_sve_gather_load(
                &addrs[..elements],
                mbytes,
                esize,
                !unsigned,
                pred,
                zt,
                first_fault,
            );
        }

        // LD1 gather (unpacked: D elements, 32-bit vector offset): 1100010 msz
        // xs scaled Zm 0 U ff Pg Rn Zt (bit15==0 vs the D.64 form's bit15==1).
        // esize=64; offset[e] = extend(Zm[e]<31:0>, xs) << scale. ff=bit13 selects the first-fault (LDFF1) variant; see exec_sve_gather_load.
        if insn >> 25 == 0b1100010 && (insn >> 15) & 1 == 0 {
            let msz = (insn >> 23) & 0x3;
            let xs_signed = (insn >> 22) & 1 == 1;
            let scaled = (insn >> 21) & 1 == 1;
            let unsigned = (insn >> 14) & 1 == 1;
            // No scaled-byte gather, and no signed 64-bit load (LD1SD); reject.
            if (scaled && msz == 0) || (!unsigned && msz == 3) {
                return Ok(CpuExit::Undefined(insn));
            }
            let zm = ((insn >> 16) & 0x1F) as usize;
            let mbytes = 1usize << msz;
            let scale = if scaled { msz } else { 0 };
            let esize = 8usize; // D
            let elements = 16 / esize;
            let offs = self.v[zm].to_le_bytes();
            let mut addrs = [0u64; 4];
            for (e, slot) in addrs.iter_mut().enumerate().take(elements) {
                let off32 = read_elem(&offs, e * esize, 4) as u32; // low 32 bits
                let off = if xs_signed {
                    off32 as i32 as i64 as u64
                } else {
                    off32 as u64
                };
                *slot = base.wrapping_add(off << scale);
            }
            let first_fault = (insn >> 13) & 1 == 1;
            return self.exec_sve_gather_load(
                &addrs[..elements],
                mbytes,
                esize,
                !unsigned,
                pred,
                zt,
                first_fault,
            );
        }

        // LD1 gather (32-bit scalar base + vector offset, S elements): 1000010
        // msz xs scaled Zm 0 U ff Pg Rn Zt. esize=32; offset[e] = extend(Zm[e]
        // <31:0>, xs) << scale (xs=1 SXTW signed, 0 UXTW unsigned). Checked after
        // LDR/STR/LD1R (which share the 1000010 prefix but have bits[24:23]==11
        // or bit15==1), so those win first for their encodings.
        if insn >> 25 == 0b1000010 && (insn >> 15) & 1 == 0 {
            let msz = (insn >> 23) & 0x3;
            if msz == 3 {
                return Ok(CpuExit::Undefined(insn)); // no doubleword in S-form
            }
            let xs_signed = (insn >> 22) & 1 == 1;
            let scaled = (insn >> 21) & 1 == 1;
            let unsigned = (insn >> 14) & 1 == 1;
            // No scaled-byte gather, and no signed word->S load (msz==2, U==0).
            if (scaled && msz == 0) || (!unsigned && msz == 2) {
                return Ok(CpuExit::Undefined(insn));
            }
            let zm = ((insn >> 16) & 0x1F) as usize;
            let mbytes = 1usize << msz;
            let scale = if scaled { msz } else { 0 };
            let esize = 4usize; // S
            let elements = 16 / esize;
            let offs = self.v[zm].to_le_bytes();
            let mut addrs = [0u64; 4];
            for (e, slot) in addrs.iter_mut().enumerate().take(elements) {
                let off32 = read_elem(&offs, e * esize, esize) as u32;
                let off = if xs_signed {
                    off32 as i32 as i64 as u64
                } else {
                    off32 as u64
                };
                *slot = base.wrapping_add(off << scale);
            }
            let first_fault = (insn >> 13) & 1 == 1;
            return self.exec_sve_gather_load(
                &addrs[..elements],
                mbytes,
                esize,
                !unsigned,
                pred,
                zt,
                first_fault,
            );
        }

        // ST1 scatter (64-bit scalar base + vector offset, D elements):
        // 1110010 msz ig1 Zm 101 Pg Rn Zt. addr[e] = Xn + (Zm[e] << scale);
        // scale = msz when scaled (bits[22:21]==01) else 0; store the low msize
        // bytes of each active D element (inactive lanes leave memory unchanged).
        // bit22==0 separates it from the vector-base scatter (ig1==10).
        if insn >> 25 == 0b1110010 && (insn >> 22) & 1 == 0 && (insn >> 13) & 0x7 == 0b101 {
            let msz = (insn >> 23) & 0x3;
            let scaled = (insn >> 21) & 0x3 == 0b01;
            if scaled && msz == 0 {
                return Ok(CpuExit::Undefined(insn)); // no scaled-byte scatter
            }
            let zm = ((insn >> 16) & 0x1F) as usize;
            let mbytes = 1usize << msz;
            let scale = if scaled { msz } else { 0 };
            let esize = 8usize; // D
            let elements = 16 / esize;
            let offs = self.v[zm].to_le_bytes();
            let src = self.v[zt].to_le_bytes();
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue;
                }
                let off = read_elem(&offs, e * esize, esize);
                let pa = self.translate_address(base.wrapping_add(off << scale), true, false)?;
                let val = read_elem(&src, e * esize, esize);
                match mbytes {
                    1 => self.memory.write_u8(pa, val as u8)?,
                    2 => self.memory.write_u16(pa, val as u16)?,
                    4 => self.memory.write_u32(pa, val as u32)?,
                    _ => self.memory.write_u64(pa, val)?,
                }
            }
            return Ok(CpuExit::Continue);
        }

        // ST1 scatter (32-bit scalar base + vector offset, S elements): 1110010
        // msz ig1 Zm 1 xs 0 Pg Rn Zt. esize=32; offset[e] = extend(Zm[e]<31:0>,
        // xs) << scale; scale = msz when scaled (bits[22:21]==11) else 0. bit13==0
        // separates this from the D-form scatter (bits[15:13]==101); bit22==1
        // (ig1 high) separates it from the unpacked x32 D-form scatter below.
        if insn >> 25 == 0b1110010
            && (insn >> 22) & 1 == 1
            && (insn >> 15) & 1 == 1
            && (insn >> 13) & 1 == 0
        {
            let msz = (insn >> 23) & 0x3;
            if msz == 3 {
                return Ok(CpuExit::Undefined(insn));
            }
            let scaled = (insn >> 21) & 0x3 == 0b11;
            if scaled && msz == 0 {
                return Ok(CpuExit::Undefined(insn)); // no scaled-byte scatter
            }
            let xs_signed = (insn >> 14) & 1 == 1;
            let zm = ((insn >> 16) & 0x1F) as usize;
            let mbytes = 1usize << msz;
            let scale = if scaled { msz } else { 0 };
            let esize = 4usize; // S
            let elements = 16 / esize;
            let offs = self.v[zm].to_le_bytes();
            let src = self.v[zt].to_le_bytes();
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue;
                }
                let off32 = read_elem(&offs, e * esize, esize) as u32;
                let off = if xs_signed {
                    off32 as i32 as i64 as u64
                } else {
                    off32 as u64
                };
                let pa = self.translate_address(base.wrapping_add(off << scale), true, false)?;
                let val = read_elem(&src, e * esize, esize);
                match mbytes {
                    1 => self.memory.write_u8(pa, val as u8)?,
                    2 => self.memory.write_u16(pa, val as u16)?,
                    _ => self.memory.write_u32(pa, val as u32)?,
                }
            }
            return Ok(CpuExit::Continue);
        }

        // ST1 scatter (unpacked: D elements, 32-bit vector offset): 1110010 msz
        // ig1 Zm 1 xs 0 Pg Rn Zt with bit22==0 (ig1 high clear). esize=64;
        // offset[e] = extend(Zm[e]<31:0>, xs) << scale.
        if insn >> 25 == 0b1110010
            && (insn >> 22) & 1 == 0
            && (insn >> 15) & 1 == 1
            && (insn >> 13) & 1 == 0
        {
            let msz = (insn >> 23) & 0x3;
            let scaled = (insn >> 21) & 1 == 1;
            if scaled && msz == 0 {
                return Ok(CpuExit::Undefined(insn)); // no scaled-byte scatter
            }
            let xs_signed = (insn >> 14) & 1 == 1;
            let zm = ((insn >> 16) & 0x1F) as usize;
            let mbytes = 1usize << msz;
            let scale = if scaled { msz } else { 0 };
            let esize = 8usize; // D
            let elements = 16 / esize;
            let offs = self.v[zm].to_le_bytes();
            let src = self.v[zt].to_le_bytes();
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue;
                }
                let off32 = read_elem(&offs, e * esize, 4) as u32;
                let off = if xs_signed {
                    off32 as i32 as i64 as u64
                } else {
                    off32 as u64
                };
                let pa = self.translate_address(base.wrapping_add(off << scale), true, false)?;
                let val = read_elem(&src, e * esize, esize);
                match mbytes {
                    1 => self.memory.write_u8(pa, val as u8)?,
                    2 => self.memory.write_u16(pa, val as u16)?,
                    4 => self.memory.write_u32(pa, val as u32)?,
                    _ => self.memory.write_u64(pa, val)?,
                }
            }
            return Ok(CpuExit::Continue);
        }

        // LDNT1 gather (vector base + scalar offset, D elements): 1100010 msz
        // 00 Rm 1 U 0 Pg Zn Zt. Each element's base is Zn.d[e], offset by Xm
        // (or zero for Rm==31). The non-temporal hint has no architectural
        // effect. Signed byte/half/word variants sign-extend into 64-bit lanes;
        // there is no signed doubleword form.
        if insn >> 25 == 0b1100010
            && (insn >> 21) & 0x3 == 0b00
            && (insn >> 15) & 1 == 1
            && (insn >> 13) & 1 == 0
        {
            let msz = (insn >> 23) & 0x3;
            let unsigned = (insn >> 14) & 1 == 1;
            if !unsigned && msz == 3 {
                return Ok(CpuExit::Undefined(insn));
            }
            let rm = ((insn >> 16) & 0x1F) as u8;
            let offset = if rm == 31 { 0 } else { self.get_x(rm) };
            let zn_base = ((insn >> 5) & 0x1F) as usize;
            let mbytes = 1usize << msz;
            let esize = 8usize; // D
            let elements = 16 / esize;
            let bases = self.v[zn_base].to_le_bytes();
            let mut addrs = [0u64; 4];
            for (e, slot) in addrs.iter_mut().enumerate().take(elements) {
                *slot = read_elem(&bases, e * esize, esize).wrapping_add(offset);
            }
            return self.exec_sve_gather_load(
                &addrs[..elements],
                mbytes,
                esize,
                !unsigned,
                pred,
                zt,
                false,
            );
        }

        // LD1 gather (vector base + immediate, D elements): 1100010 msz 01 imm5
        // 1 U ff Pg Zn Zt. Each element's base IS Zn[e]; addr[e] = Zn[e] +
        // imm5 * mbytes. esize=64; load msize bytes, sign/zero-extend; zeroing.
        // ff=bit13 selects the first-fault (LDFF1) variant.
        if insn >> 25 == 0b1100010 && (insn >> 21) & 0x3 == 0b01 && (insn >> 15) & 1 == 1 {
            let msz = (insn >> 23) & 0x3;
            let unsigned = (insn >> 14) & 1 == 1;
            // No signed 64-bit load (LD1SD): msz==3 with U==0 is unallocated.
            if !unsigned && msz == 3 {
                return Ok(CpuExit::Undefined(insn));
            }
            let imm5 = (insn >> 16) & 0x1F;
            let zn_base = ((insn >> 5) & 0x1F) as usize;
            let mbytes = 1usize << msz;
            let esize = 8usize; // D
            let elements = 16 / esize;
            let bases = self.v[zn_base].to_le_bytes();
            let mut addrs = [0u64; 4];
            for (e, slot) in addrs.iter_mut().enumerate().take(elements) {
                let elem_base = read_elem(&bases, e * esize, esize);
                *slot = elem_base.wrapping_add((imm5 as u64) * (mbytes as u64));
            }
            let first_fault = (insn >> 13) & 1 == 1;
            return self.exec_sve_gather_load(
                &addrs[..elements],
                mbytes,
                esize,
                !unsigned,
                pred,
                zt,
                first_fault,
            );
        }

        // ST1 scatter (vector base + immediate, D elements): 1110010 msz 10 imm5
        // 101 Pg Zn Zt. addr[e] = Zn[e] + imm5 * mbytes; store low msize bytes.
        if insn >> 25 == 0b1110010 && (insn >> 21) & 0x3 == 0b10 && (insn >> 13) & 0x7 == 0b101 {
            let msz = (insn >> 23) & 0x3;
            let imm5 = (insn >> 16) & 0x1F;
            let zn_base = ((insn >> 5) & 0x1F) as usize;
            let mbytes = 1usize << msz;
            let esize = 8usize; // D
            let elements = 16 / esize;
            let bases = self.v[zn_base].to_le_bytes();
            let src = self.v[zt].to_le_bytes();
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue;
                }
                let elem_base = read_elem(&bases, e * esize, esize);
                let ea = elem_base.wrapping_add((imm5 as u64) * (mbytes as u64));
                let pa = self.translate_address(ea, true, false)?;
                let val = read_elem(&src, e * esize, esize);
                match mbytes {
                    1 => self.memory.write_u8(pa, val as u8)?,
                    2 => self.memory.write_u16(pa, val as u16)?,
                    4 => self.memory.write_u32(pa, val as u32)?,
                    _ => self.memory.write_u64(pa, val)?,
                }
            }
            return Ok(CpuExit::Continue);
        }

        // LD1RQ (load and replicate quadword): 1010010 msz 00 ... with
        // bits[15:13]==001 (scalar+imm: addr=base+imm4*16) or ==000 (scalar+Xm:
        // addr=base+(Xm+e)*mbytes, Rm==31 UNDEFINED). At VL=128 the quadword is
        // the whole register, so this is a packed contiguous load (zeroing).
        if !is_store
            && insn >> 25 == 0b1010010
            && (insn >> 21) & 0x3 == 0b00
            && ((b15_13 == 0b001 && (insn >> 20) & 1 == 0) || b15_13 == 0b000)
        {
            let esize = 1usize << ((insn >> 23) & 0x3);
            let elements = 16 / esize;
            let addr0 = if b15_13 == 0b001 {
                base.wrapping_add((imm4 * 16) as u64)
            } else {
                let rm = ((insn >> 16) & 0x1F) as u8;
                if rm == 31 {
                    return Ok(CpuExit::Undefined(insn));
                }
                base.wrapping_add(self.get_x(rm).wrapping_mul(esize as u64))
            };
            let mut dst = [0u8; 16];
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue;
                }
                let pa =
                    self.translate_address(addr0.wrapping_add((e * esize) as u64), false, false)?;
                let val: u64 = match esize {
                    1 => self.memory.read_u8(pa)? as u64,
                    2 => self.memory.read_u16(pa)? as u64,
                    4 => self.memory.read_u32(pa)? as u64,
                    _ => self.memory.read_u64(pa)?,
                };
                write_elem(&mut dst, e * esize, esize, val);
            }
            self.v[zt] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // LDNT1 (non-temporal contiguous load): 1010010 msz 000 imm4 111 Pg Rn
        // Zt. The non-temporal hint has no architectural effect, so this is a
        // packed LD1 (esize=msize, no extension, zeroing inactive).
        if !is_store && insn >> 25 == 0b1010010 && b15_13 == 0b111 && (insn >> 20) & 0x7 == 0b000 {
            let esize = 1usize << ((insn >> 23) & 0x3);
            let elements = 16 / esize;
            let addr0 = base.wrapping_add((imm4 * (elements * esize) as i64) as u64);
            let mut dst = [0u8; 16];
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue;
                }
                let pa =
                    self.translate_address(addr0.wrapping_add((e * esize) as u64), false, false)?;
                let val: u64 = match esize {
                    1 => self.memory.read_u8(pa)? as u64,
                    2 => self.memory.read_u16(pa)? as u64,
                    4 => self.memory.read_u32(pa)? as u64,
                    _ => self.memory.read_u64(pa)?,
                };
                write_elem(&mut dst, e * esize, esize, val);
            }
            self.v[zt] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // STNT1 (non-temporal contiguous store): 1110010 msz 001 imm4 111 Pg Rn
        // Zt (bits[22:20]==001). A packed ST1.
        if is_store && insn >> 25 == 0b1110010 && b15_13 == 0b111 && (insn >> 20) & 0x7 == 0b001 {
            let esize = 1usize << ((insn >> 23) & 0x3);
            let elements = 16 / esize;
            let addr0 = base.wrapping_add((imm4 * (elements * esize) as i64) as u64);
            let src = self.v[zt].to_le_bytes();
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue;
                }
                let pa =
                    self.translate_address(addr0.wrapping_add((e * esize) as u64), true, false)?;
                let val = read_elem(&src, e * esize, esize);
                match esize {
                    1 => self.memory.write_u8(pa, val as u8)?,
                    2 => self.memory.write_u16(pa, val as u16)?,
                    4 => self.memory.write_u32(pa, val as u32)?,
                    _ => self.memory.write_u64(pa, val)?,
                }
            }
            return Ok(CpuExit::Continue);
        }

        // LDNT1 (non-temporal contiguous load, scalar+scalar register):
        // 1010010 msz 00 Rm 110 Pg Rn Zt. The non-temporal hint has no
        // architectural state effect.
        if !is_store && insn >> 25 == 0b1010010 && b15_13 == 0b110 && (insn >> 21) & 0x3 == 0 {
            let rm = ((insn >> 16) & 0x1F) as u8;
            if rm == 31 {
                return Ok(CpuExit::Undefined(insn));
            }
            let esize = 1usize << ((insn >> 23) & 0x3);
            let elements = 16 / esize;
            let addr0 = base.wrapping_add(self.get_x(rm).wrapping_mul(esize as u64));
            let mut dst = [0u8; 16];
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue;
                }
                let pa =
                    self.translate_address(addr0.wrapping_add((e * esize) as u64), false, false)?;
                let val: u64 = match esize {
                    1 => self.memory.read_u8(pa)? as u64,
                    2 => self.memory.read_u16(pa)? as u64,
                    4 => self.memory.read_u32(pa)? as u64,
                    _ => self.memory.read_u64(pa)?,
                };
                write_elem(&mut dst, e * esize, esize, val);
            }
            self.v[zt] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // STNT1 (non-temporal contiguous store, scalar+scalar register):
        // 1110010 msz 00 Rm 011 Pg Rn Zt. A packed ST1 with a hint.
        if is_store && insn >> 25 == 0b1110010 && b15_13 == 0b011 && (insn >> 21) & 0x3 == 0 {
            let rm = ((insn >> 16) & 0x1F) as u8;
            if rm == 31 {
                return Ok(CpuExit::Undefined(insn));
            }
            let esize = 1usize << ((insn >> 23) & 0x3);
            let elements = 16 / esize;
            let addr0 = base.wrapping_add(self.get_x(rm).wrapping_mul(esize as u64));
            let src = self.v[zt].to_le_bytes();
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue;
                }
                let pa =
                    self.translate_address(addr0.wrapping_add((e * esize) as u64), true, false)?;
                let val = read_elem(&src, e * esize, esize);
                match esize {
                    1 => self.memory.write_u8(pa, val as u8)?,
                    2 => self.memory.write_u16(pa, val as u16)?,
                    4 => self.memory.write_u32(pa, val as u32)?,
                    _ => self.memory.write_u64(pa, val)?,
                }
            }
            return Ok(CpuExit::Continue);
        }

        // LD2/LD3/LD4 (contiguous, scalar+scalar register):
        // 1010010 msz opc Rm 110 Pg Rn Zt, opc in {01,10,11}.
        if !is_store && insn >> 25 == 0b1010010 && b15_13 == 0b110 && (insn >> 21) & 0x3 != 0 {
            let rm = ((insn >> 16) & 0x1F) as u8;
            if rm == 31 {
                return Ok(CpuExit::Undefined(insn));
            }
            let nreg = (((insn >> 21) & 0x3) + 1) as usize;
            let msz = (insn >> 23) & 0x3;
            let esize = 1usize << msz;
            let mbytes = esize;
            let elements = 16 / esize;
            let addr0 = base.wrapping_add(self.get_x(rm).wrapping_mul(esize as u64));
            let mut regs = [[0u8; 16]; 4];
            let mut a = addr0;
            for e in 0..elements {
                let active = (pred >> (e * esize)) & 1 == 1;
                for reg in regs.iter_mut().take(nreg) {
                    if active {
                        let pa = self.translate_address(a, false, false)?;
                        let val: u64 = match mbytes {
                            1 => self.memory.read_u8(pa)? as u64,
                            2 => self.memory.read_u16(pa)? as u64,
                            4 => self.memory.read_u32(pa)? as u64,
                            _ => self.memory.read_u64(pa)?,
                        };
                        write_elem(reg, e * esize, esize, val);
                    }
                    a = a.wrapping_add(mbytes as u64);
                }
            }
            for (r, reg) in regs.iter().enumerate().take(nreg) {
                self.v[(zt + r) % 32] = u128::from_le_bytes(*reg);
            }
            return Ok(CpuExit::Continue);
        }

        // ST2/ST3/ST4 (contiguous, scalar+scalar register):
        // 1110010 msz opc Rm 011 Pg Rn Zt, opc in {01,10,11}.
        if is_store && insn >> 25 == 0b1110010 && b15_13 == 0b011 && (insn >> 21) & 0x3 != 0 {
            let rm = ((insn >> 16) & 0x1F) as u8;
            if rm == 31 {
                return Ok(CpuExit::Undefined(insn));
            }
            let nreg = (((insn >> 21) & 0x3) + 1) as usize;
            let msz = (insn >> 23) & 0x3;
            let esize = 1usize << msz;
            let mbytes = esize;
            let elements = 16 / esize;
            let addr0 = base.wrapping_add(self.get_x(rm).wrapping_mul(esize as u64));
            let mut srcs = [[0u8; 16]; 4];
            for (r, src) in srcs.iter_mut().enumerate().take(nreg) {
                *src = self.v[(zt + r) % 32].to_le_bytes();
            }
            let mut a = addr0;
            for e in 0..elements {
                let active = (pred >> (e * esize)) & 1 == 1;
                for src in srcs.iter().take(nreg) {
                    if active {
                        let pa = self.translate_address(a, true, false)?;
                        let val = read_elem(src, e * esize, esize);
                        match mbytes {
                            1 => self.memory.write_u8(pa, val as u8)?,
                            2 => self.memory.write_u16(pa, val as u16)?,
                            4 => self.memory.write_u32(pa, val as u32)?,
                            _ => self.memory.write_u64(pa, val)?,
                        }
                    }
                    a = a.wrapping_add(mbytes as u64);
                }
            }
            return Ok(CpuExit::Continue);
        }

        // LD2/LD3/LD4 (contiguous, de-interleaving): 1010010 msz opc 0 imm4 111
        // Pg Rn Zt. opc=bits[22:21] in {01,10,11} -> nreg in {2,3,4}. Reads
        // nreg*elements consecutive structures and de-interleaves them so that
        // Z[(t+r)%32][e] = Mem[base + (e*nreg + r)*mbytes]; zeroes inactive lanes.
        if !is_store
            && insn >> 25 == 0b1010010
            && b15_13 == 0b111
            && (insn >> 20) & 1 == 0 // fixed 0; bit20==1 is unallocated
            && (insn >> 21) & 0x3 != 0b00
        {
            let nreg = (((insn >> 21) & 0x3) + 1) as usize;
            let msz = (insn >> 23) & 0x3;
            let esize = 1usize << msz;
            let elements = 16 / esize;
            let mbytes = esize;
            let addr0 = base.wrapping_add((imm4 * (elements * nreg * mbytes) as i64) as u64);
            let mut regs = [[0u8; 16]; 4];
            let mut a = addr0;
            for e in 0..elements {
                let active = (pred >> (e * esize)) & 1 == 1;
                for reg in regs.iter_mut().take(nreg) {
                    if active {
                        let pa = self.translate_address(a, false, false)?;
                        let val: u64 = match mbytes {
                            1 => self.memory.read_u8(pa)? as u64,
                            2 => self.memory.read_u16(pa)? as u64,
                            4 => self.memory.read_u32(pa)? as u64,
                            _ => self.memory.read_u64(pa)?,
                        };
                        write_elem(reg, e * esize, esize, val);
                    }
                    a = a.wrapping_add(mbytes as u64);
                }
            }
            for r in 0..nreg {
                self.v[(zt + r) % 32] = u128::from_le_bytes(regs[r]);
            }
            return Ok(CpuExit::Continue);
        }

        // ST2/ST3/ST4 (contiguous, interleaving): 1110010 msz opc 1 imm4 111 Pg
        // Rn Zt. bit20==1 separates it from ST1 (bit20==0). Interleaves the nreg
        // source registers: Mem[base + (e*nreg + r)*mbytes] = Z[(t+r)%32][e].
        if is_store
            && insn >> 25 == 0b1110010
            && b15_13 == 0b111
            && (insn >> 20) & 1 == 1
            && (insn >> 21) & 0x3 != 0b00
        {
            let nreg = (((insn >> 21) & 0x3) + 1) as usize;
            let msz = (insn >> 23) & 0x3;
            let esize = 1usize << msz;
            let elements = 16 / esize;
            let mbytes = esize;
            let addr0 = base.wrapping_add((imm4 * (elements * nreg * mbytes) as i64) as u64);
            let mut srcs = [[0u8; 16]; 4];
            for r in 0..nreg {
                srcs[r] = self.v[(zt + r) % 32].to_le_bytes();
            }
            let mut a = addr0;
            for e in 0..elements {
                let active = (pred >> (e * esize)) & 1 == 1;
                for src in srcs.iter().take(nreg) {
                    if active {
                        let pa = self.translate_address(a, true, false)?;
                        let val = read_elem(src, e * esize, esize);
                        match mbytes {
                            1 => self.memory.write_u8(pa, val as u8)?,
                            2 => self.memory.write_u16(pa, val as u16)?,
                            4 => self.memory.write_u32(pa, val as u32)?,
                            _ => self.memory.write_u64(pa, val)?,
                        }
                    }
                    a = a.wrapping_add(mbytes as u64);
                }
            }
            return Ok(CpuExit::Continue);
        }

        // LD1 gather (S-form vector base + immediate): 1000010 msz 01 imm5 1 U
        // ff Pg Zn Zt. esize=32; the per-element base is the 32-bit Zn[e]
        // (zero-extended); addr[e] = Zn[e] + imm5*mbytes. bit22==0 (bits[22:21]
        // ==01) separates it from LD1R (bit22==1). ff=bit13 selects the first-fault (LDFF1) variant.
        if insn >> 25 == 0b1000010 && (insn >> 21) & 0x3 == 0b01 && (insn >> 15) & 1 == 1 {
            let msz = (insn >> 23) & 0x3;
            if msz == 3 {
                return Ok(CpuExit::Undefined(insn)); // no doubleword in S-form
            }
            let unsigned = (insn >> 14) & 1 == 1;
            // No signed word->S load: msz==2 with U==0 is unallocated.
            if !unsigned && msz == 2 {
                return Ok(CpuExit::Undefined(insn));
            }
            let imm5 = (insn >> 16) & 0x1F;
            let zn_base = ((insn >> 5) & 0x1F) as usize;
            let mbytes = 1usize << msz;
            let esize = 4usize; // S
            let elements = 16 / esize;
            let bases = self.v[zn_base].to_le_bytes();
            let mut addrs = [0u64; 4];
            for (e, slot) in addrs.iter_mut().enumerate().take(elements) {
                let elem_base = read_elem(&bases, e * esize, esize); // 32-bit base
                *slot = elem_base.wrapping_add((imm5 as u64) * (mbytes as u64));
            }
            let first_fault = (insn >> 13) & 1 == 1;
            return self.exec_sve_gather_load(
                &addrs[..elements],
                mbytes,
                esize,
                !unsigned,
                pred,
                zt,
                first_fault,
            );
        }

        // ST1 scatter (S-form vector base + immediate): 1110010 msz 11 imm5 101
        // Pg Zn Zt. esize=32; addr[e] = Zn[e]<31:0> + imm5*mbytes. bits[22:21]
        // ==11 separates it from the D.64 (00/01) and D vector-base (10) forms.
        if insn >> 25 == 0b1110010 && (insn >> 21) & 0x3 == 0b11 && (insn >> 13) & 0x7 == 0b101 {
            let msz = (insn >> 23) & 0x3;
            if msz == 3 {
                return Ok(CpuExit::Undefined(insn));
            }
            let imm5 = (insn >> 16) & 0x1F;
            let zn_base = ((insn >> 5) & 0x1F) as usize;
            let mbytes = 1usize << msz;
            let esize = 4usize; // S
            let elements = 16 / esize;
            let bases = self.v[zn_base].to_le_bytes();
            let src = self.v[zt].to_le_bytes();
            for e in 0..elements {
                if (pred >> (e * esize)) & 1 == 0 {
                    continue;
                }
                let elem_base = read_elem(&bases, e * esize, esize);
                let ea = elem_base.wrapping_add((imm5 as u64) * (mbytes as u64));
                let pa = self.translate_address(ea, true, false)?;
                let val = read_elem(&src, e * esize, esize);
                match mbytes {
                    1 => self.memory.write_u8(pa, val as u8)?,
                    2 => self.memory.write_u16(pa, val as u16)?,
                    _ => self.memory.write_u32(pa, val as u32)?,
                }
            }
            return Ok(CpuExit::Continue);
        }

        // Other SVE memory forms (gather/vector first-fault) are not yet modelled.
        Ok(CpuExit::Undefined(insn))
    }


    pub(crate) fn read_sve_mem_element(&self, ea: u64, mbytes: usize) -> Result<u64, ArmError> {
        match self.translate_address(ea, false, false) {
            Ok(pa) => match mbytes {
                1 => self
                    .memory
                    .read_u8(pa)
                    .map(|v| v as u64)
                    .map_err(Into::into),
                2 => self
                    .memory
                    .read_u16(pa)
                    .map(|v| v as u64)
                    .map_err(Into::into),
                4 => self
                    .memory
                    .read_u32(pa)
                    .map(|v| v as u64)
                    .map_err(Into::into),
                _ => self.memory.read_u64(pa).map_err(Into::into),
            },
            Err(err) => Err(err),
        }
    }


    pub(crate) fn clear_sve_ffr_from_element(&mut self, e: usize, esize: usize) {
        let bit = e * esize;
        let keep = if bit == 0 { 0 } else { (1u32 << bit) - 1 };
        self.sve_ffr &= keep;
    }


    /// Shared body for the contiguous first-fault (LDFF1) and non-fault (LDNF1)
    /// loads. Loads each active element; on an access that cannot be performed
    /// the access is suppressed: for LDFF1 the very first active element still
    /// faults normally, but any later element (and every element for LDNF1) is
    /// suppressed, the FFR is cleared from that element onward, and the
    /// suppressed/inactive lanes are zeroed. With no fault this is exactly LD1.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn exec_sve_ff_load(
        &mut self,
        addr0: u64,
        mbytes: usize,
        esize: usize,
        signed: bool,
        elements: usize,
        pred: u32,
        zt: usize,
        nonfault: bool,
    ) -> Result<CpuExit, ArmError> {
        let mut dst = [0u8; 16];
        let mut first = true;
        let mut faulted = false;
        for e in 0..elements {
            if (pred >> (e * esize)) & 1 != 1 {
                continue; // inactive -> zero
            }
            if faulted {
                continue;
            }
            let ea = addr0.wrapping_add((e * mbytes) as u64);
            let read = self.read_sve_mem_element(ea, mbytes);
            match read {
                Ok(raw) => {
                    let val = if signed {
                        (sext_elem(raw, (mbytes * 8) as u32) as u64) & elem_mask((esize * 8) as u32)
                    } else {
                        raw
                    };
                    write_elem(&mut dst, e * esize, esize, val);
                    first = false;
                }
                Err(err) => {
                    if first && !nonfault {
                        return Err(err); // LDFF1's first active element faults normally
                    }
                    self.clear_sve_ffr_from_element(e, esize);
                    faulted = true;
                }
            }
        }
        self.v[zt] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Shared gather-load body with first-fault (LDFF1) modelling. `addrs[e]` is
    /// the precomputed effective address for lane `e`. For a plain LD1 gather
    /// (`first_fault == false`) every active lane faults normally; for an LDFF1
    /// gather (`first_fault == true`) the first active lane faults normally while
    /// any later faulting lane is suppressed (its result left zero), FFR is
    /// cleared from that element onward, and later lanes are still attempted.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn exec_sve_gather_load(
        &mut self,
        addrs: &[u64],
        mbytes: usize,
        esize: usize,
        signed: bool,
        pred: u32,
        zt: usize,
        first_fault: bool,
    ) -> Result<CpuExit, ArmError> {
        let mut dst = [0u8; 16];
        let mut first = true;
        for (e, &ea) in addrs.iter().enumerate() {
            if (pred >> (e * esize)) & 1 != 1 {
                continue; // inactive -> zero
            }
            let read = self.read_sve_mem_element(ea, mbytes);
            match read {
                Ok(raw) => {
                    let val = if signed {
                        (sext_elem(raw, (mbytes * 8) as u32) as u64) & elem_mask((esize * 8) as u32)
                    } else {
                        raw
                    };
                    write_elem(&mut dst, e * esize, esize, val);
                    first = false;
                }
                Err(err) => {
                    // Plain LD1 gather: any active-lane fault propagates. LDFF1
                    // gather: the first active lane faults normally; later faults
                    // are suppressed and reflected in FFR.
                    if !first_fault || first {
                        return Err(err);
                    }
                    self.clear_sve_ffr_from_element(e, esize);
                }
            }
        }
        self.v[zt] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }
}
