//! memory.rs

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
    /// Write the SVE first-fault register. Exposed for the differential harness.
    pub fn set_sve_ffr(&mut self, v: u32) {
        self.sve_ffr = v & 0xFFFF;
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
