//! reduce.rs

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
    /// Execute SVE integer reduction (predicated) to a scalar in Vd. opc6 =
    /// bits[21:16]: SADDV(000000)/UADDV(000001) give a 64-bit sum; SMAXV/UMAXV/
    /// SMINV/UMINV (0010xx) and ANDV/ORV/EORV (0110xx) give an esize result.
    /// Inactive elements use the operation identity. Pg is byte-granular.
    pub(crate) fn exec_sve_int_reduce(
        &mut self,
        insn: u32,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
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
    pub(crate) fn exec_sve_qv_reduce_int(
        &mut self,
        insn: u32,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
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
}
