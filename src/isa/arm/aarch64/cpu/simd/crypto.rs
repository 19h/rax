//! crypto.rs

use crate::isa::arm::aarch64::cpu::simd::*;
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
    /// Execute cryptographic operations (AES, SHA, SM3, SM4).
    /// For now, this is a stub that allows the instruction to execute.
    pub(crate) fn exec_crypto(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        // SHA512 (FEAT_SHA512) and SHA3 (FEAT_SHA3) 64-bit-lane crypto: 0xCE.
        if (insn >> 24) & 0xFF == 0xCE {
            let rm = ((insn >> 16) & 0x1F) as usize;
            let ra = ((insn >> 10) & 0x1F) as usize;
            let grp = (insn >> 21) & 0x7; // bits[23:21]
            let o = (insn >> 10) & 0x3F; // bits[15:10]
            let lanes = |v: u128| (v as u64, (v >> 64) as u64);
            let pack = |a: u64, b: u64| (a as u128) | ((b as u128) << 64);
            if grp == 0b011 && o == 0b100000 {
                // SHA512H
                let (d0, d1) = lanes(self.v[rd]);
                let (m0, m1) = lanes(self.v[rm]);
                let (n0, n1) = lanes(self.v[rn]);
                let s1 = |x: u64| x.rotate_right(14) ^ x.rotate_right(18) ^ x.rotate_right(41);
                let cho = |x: u64, y: u64, z: u64| (x & (y ^ z)) ^ z;
                let nd1 = d1.wrapping_add(s1(m1)).wrapping_add(cho(m1, n0, n1));
                let t = nd1.wrapping_add(m0);
                let nd0 = d0.wrapping_add(s1(t)).wrapping_add(cho(t, m1, n0));
                self.v[rd] = pack(nd0, nd1);
                return Ok(CpuExit::Continue);
            }
            if grp == 0b011 && o == 0b100001 {
                // SHA512H2
                let (d0, d1) = lanes(self.v[rd]);
                let (m0, m1) = lanes(self.v[rm]);
                let (n0, _n1) = lanes(self.v[rn]);
                let s0 = |x: u64| x.rotate_right(28) ^ x.rotate_right(34) ^ x.rotate_right(39);
                let maj = |x: u64, y: u64, z: u64| (x & y) | ((x | y) & z);
                let nd1 = d1.wrapping_add(s0(m0)).wrapping_add(maj(n0, m1, m0));
                let nd0 = d0.wrapping_add(s0(nd1)).wrapping_add(maj(nd1, m0, m1));
                self.v[rd] = pack(nd0, nd1);
                return Ok(CpuExit::Continue);
            }
            if grp == 0b110 && o == 0b100000 {
                // SHA512SU0
                let (d0, d1) = lanes(self.v[rd]);
                let (n0, _n1) = lanes(self.v[rn]);
                let sig0 = |x: u64| x.rotate_right(1) ^ x.rotate_right(8) ^ (x >> 7);
                self.v[rd] = pack(d0.wrapping_add(sig0(d1)), d1.wrapping_add(sig0(n0)));
                return Ok(CpuExit::Continue);
            }
            if grp == 0b011 && o == 0b100010 {
                // SHA512SU1
                let (d0, d1) = lanes(self.v[rd]);
                let (m0, m1) = lanes(self.v[rm]);
                let (n0, n1) = lanes(self.v[rn]);
                let sig1 = |x: u64| x.rotate_right(19) ^ x.rotate_right(61) ^ (x >> 6);
                self.v[rd] = pack(
                    d0.wrapping_add(sig1(n0)).wrapping_add(m0),
                    d1.wrapping_add(sig1(n1)).wrapping_add(m1),
                );
                return Ok(CpuExit::Continue);
            }
            if grp == 0b011 && o == 0b100011 {
                // RAX1: Vd[i] = Vn[i] ^ ROL64(Vm[i], 1)
                let (n0, n1) = lanes(self.v[rn]);
                let (m0, m1) = lanes(self.v[rm]);
                self.v[rd] = pack(n0 ^ m0.rotate_left(1), n1 ^ m1.rotate_left(1));
                return Ok(CpuExit::Continue);
            }
            if grp == 0b100 {
                // XAR: Vd[i] = ROR64(Vn[i] ^ Vm[i], imm6)
                let imm = o; // bits[15:10]
                let (n0, n1) = lanes(self.v[rn]);
                let (m0, m1) = lanes(self.v[rm]);
                self.v[rd] = pack((n0 ^ m0).rotate_right(imm), (n1 ^ m1).rotate_right(imm));
                return Ok(CpuExit::Continue);
            }
            if (grp == 0b000 || grp == 0b001) && (insn >> 15) & 1 == 0 {
                // EOR3 (grp 000): Vd = Vn ^ Vm ^ Va.  BCAX (grp 001): Vn ^ (Vm & ~Va).
                let (n, m, a) = (self.v[rn], self.v[rm], self.v[ra]);
                self.v[rd] = if grp == 0b000 {
                    n ^ m ^ a
                } else {
                    n ^ (m & !a)
                };
                return Ok(CpuExit::Continue);
            }
        }

        // AES single-block operations: bits[31:24]=0x4E, opcode bits[16:12].
        if (insn >> 24) & 0xFF == 0x4E {
            let opcode = (insn >> 12) & 0x1F;
            match opcode {
                0b00100 => {
                    // AESE: ShiftRows(SubBytes(Vd EOR Vn))
                    let st = self.v[rd] ^ self.v[rn];
                    self.v[rd] = aes_sub_bytes(aes_shift_rows(st, false), false);
                    return Ok(CpuExit::Continue);
                }
                0b00101 => {
                    // AESD: InvShiftRows then InvSubBytes of (Vd EOR Vn)
                    let st = self.v[rd] ^ self.v[rn];
                    self.v[rd] = aes_sub_bytes(aes_shift_rows(st, true), true);
                    return Ok(CpuExit::Continue);
                }
                0b00110 => {
                    // AESMC
                    self.v[rd] = aes_mix_columns(self.v[rn], false);
                    return Ok(CpuExit::Continue);
                }
                0b00111 => {
                    // AESIMC
                    self.v[rd] = aes_mix_columns(self.v[rn], true);
                    return Ok(CpuExit::Continue);
                }
                _ => {}
            }
        }

        let rm = ((insn >> 16) & 0x1F) as usize;

        // SHA-1 / SHA-256 (bits[31:24]=0x5E).
        if (insn >> 24) & 0xFF == 0x5E {
            // Two-register SHA: bits[21:17]==10100, opcode at bits[16:12].
            if (insn >> 17) & 0x1F == 0b10100 {
                let opcode = (insn >> 12) & 0x1F;
                match opcode {
                    0b00000 => {
                        // SHA1H Sd, Sn: ROL(Sn, 30) on the low 32 bits.
                        self.v[rd] = (self.v[rn] as u32).rotate_left(30) as u128;
                        return Ok(CpuExit::Continue);
                    }
                    0b00001 => {
                        // SHA1SU1 Vd.4S, Vn.4S
                        let op1 = self.v[rd];
                        let op2 = self.v[rn];
                        let t = op1 ^ (op2 >> 32);
                        let t0 = sha_elem(t, 0).rotate_left(1);
                        let t1 = sha_elem(t, 1).rotate_left(1);
                        let t2 = sha_elem(t, 2).rotate_left(1);
                        let t3 = sha_elem(t, 3).rotate_left(1) ^ sha_elem(t, 0).rotate_left(2);
                        let mut r = 0u128;
                        sha_set_elem(&mut r, 0, t0);
                        sha_set_elem(&mut r, 1, t1);
                        sha_set_elem(&mut r, 2, t2);
                        sha_set_elem(&mut r, 3, t3);
                        self.v[rd] = r;
                        return Ok(CpuExit::Continue);
                    }
                    0b00010 => {
                        // SHA256SU0 Vd.4S, Vn.4S
                        let x = self.v[rd];
                        let y = self.v[rn];
                        let t = (y << 96) | (x >> 32); // Y<31:0> : X<127:32>
                        let mut r = 0u128;
                        for e in 0..4 {
                            let elt = sha_elem(t, e);
                            let s = elt.rotate_right(7) ^ elt.rotate_right(18) ^ (elt >> 3);
                            sha_set_elem(&mut r, e, s.wrapping_add(sha_elem(x, e)));
                        }
                        self.v[rd] = r;
                        return Ok(CpuExit::Continue);
                    }
                    _ => {}
                }
            } else if (insn >> 21) & 7 == 0 && (insn >> 10) & 3 == 0 {
                // Three-register SHA: opcode at bits[14:12].
                let opcode = (insn >> 12) & 0x7;
                match opcode {
                    0b000 => {
                        // SHA1C
                        self.v[rd] =
                            sha1_hash(self.v[rd], self.v[rn] as u32, self.v[rm], sha_choose);
                        return Ok(CpuExit::Continue);
                    }
                    0b001 => {
                        // SHA1P
                        self.v[rd] =
                            sha1_hash(self.v[rd], self.v[rn] as u32, self.v[rm], sha_parity);
                        return Ok(CpuExit::Continue);
                    }
                    0b010 => {
                        // SHA1M
                        self.v[rd] =
                            sha1_hash(self.v[rd], self.v[rn] as u32, self.v[rm], sha_majority);
                        return Ok(CpuExit::Continue);
                    }
                    0b011 => {
                        // SHA1SU0 Vd.4S, Vn.4S, Vm.4S
                        let op1 = self.v[rd];
                        let op2 = self.v[rn];
                        let op3 = self.v[rm];
                        // result = (Vn<63:0> : Vd<127:64>) EOR Vd EOR Vm
                        let r = ((op2 << 64) | (op1 >> 64)) ^ op1 ^ op3;
                        self.v[rd] = r;
                        return Ok(CpuExit::Continue);
                    }
                    0b100 => {
                        // SHA256H Qd, Qn, Vm: SHA256hash(Vd, Vn, Vm, part1=true)
                        self.v[rd] = sha256_hash(self.v[rd], self.v[rn], self.v[rm], true);
                        return Ok(CpuExit::Continue);
                    }
                    0b101 => {
                        // SHA256H2 Qd, Qn, Vm: SHA256hash(Vn, Vd, Vm, part1=false)
                        self.v[rd] = sha256_hash(self.v[rn], self.v[rd], self.v[rm], false);
                        return Ok(CpuExit::Continue);
                    }
                    0b110 => {
                        // SHA256SU1 Vd.4S, Vn.4S, Vm.4S
                        let x = self.v[rd];
                        let y = self.v[rn];
                        let z = self.v[rm];
                        let t0 = (z << 96) | (y >> 32); // Z<31:0> : Y<127:32>
                        let mut r = 0u128;
                        // e = 0,1 use T1 = Z<127:64>
                        for e in 0..2 {
                            let elt = sha_elem(z >> 64, e); // Z<127:64> element e
                            let s = elt.rotate_right(17) ^ elt.rotate_right(19) ^ (elt >> 10);
                            let v = s.wrapping_add(sha_elem(x, e)).wrapping_add(sha_elem(t0, e));
                            sha_set_elem(&mut r, e, v);
                        }
                        // e = 2,3 use T1 = result<63:0>
                        for e in 2..4 {
                            let elt = sha_elem(r, e - 2); // result<63:0> element (e-2)
                            let s = elt.rotate_right(17) ^ elt.rotate_right(19) ^ (elt >> 10);
                            let v = s.wrapping_add(sha_elem(x, e)).wrapping_add(sha_elem(t0, e));
                            sha_set_elem(&mut r, e, v);
                        }
                        self.v[rd] = r;
                        return Ok(CpuExit::Continue);
                    }
                    _ => {}
                }
            }
        }

        // SM4 (bits[31:24]==0xCE).
        if (insn >> 24) & 0xFF == 0xCE {
            // SM4E Vd.4S, Vn.4S: 11001110 11000000 100001 Rn Rd.
            if (insn >> 16) & 0xFF == 0xC0 && (insn >> 10) & 0x3F == 0b100001 {
                self.v[rd] = sm4_rounds(self.v[rd], self.v[rn], true);
                return Ok(CpuExit::Continue);
            }
            // SM4EKEY Vd.4S, Vn.4S, Vm.4S: 11001110 011 Rm 110010 Rn Rd.
            if (insn >> 21) & 0x7 == 0b011 && (insn >> 10) & 0x3F == 0b110010 {
                self.v[rd] = sm4_rounds(self.v[rn], self.v[rm], false);
                return Ok(CpuExit::Continue);
            }

            // SM3 group.
            let grp = (insn >> 21) & 0x7;
            if grp == 0b010 {
                if (insn >> 15) & 1 == 0 {
                    // SM3SS1 Vd.4S, Vn.4S, Vm.4S, Va.4S (Va = Ra at bits[14:10]).
                    let ra = ((insn >> 10) & 0x1F) as usize;
                    let t = (self.v[rn] >> 96) as u32;
                    let val = t
                        .rotate_left(12)
                        .wrapping_add((self.v[rm] >> 96) as u32)
                        .wrapping_add((self.v[ra] >> 96) as u32)
                        .rotate_left(7);
                    self.v[rd] = (val as u128) << 96;
                    return Ok(CpuExit::Continue);
                } else if (insn >> 14) & 0x3 == 0b10 {
                    // SM3TT1A/SM3TT1B/SM3TT2A/SM3TT2B (sel = bits[11:10], i = imm2).
                    let i = (insn >> 12) & 0x3;
                    let sel = (insn >> 10) & 0x3;
                    self.v[rd] = sm3_tt(self.v[rd], self.v[rn], self.v[rm], i, sel);
                    return Ok(CpuExit::Continue);
                }
            } else if grp == 0b011 {
                if (insn >> 10) & 0x3F == 0b110000 {
                    self.v[rd] = sm3_partw1(self.v[rd], self.v[rn], self.v[rm]);
                    return Ok(CpuExit::Continue);
                }
                if (insn >> 10) & 0x3F == 0b110001 {
                    self.v[rd] = sm3_partw2(self.v[rd], self.v[rn], self.v[rm]);
                    return Ok(CpuExit::Continue);
                }
            }
        }

        // Any remaining crypto encoding is unallocated.
        Ok(CpuExit::Undefined(insn))
    }
}
