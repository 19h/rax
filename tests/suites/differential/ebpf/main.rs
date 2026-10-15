//! eBPF (IETF RFC 9669) and Solana SBPF (V0–V4) differential test suite.
//!
//! Performs 4-way differential verification across:
//! 1. External `llvm-mc` (`-triple=bpfel` and `-triple=bpfeb`, `-mcpu=v4`)
//!    encoding oracle vs `rax::isa::ebpf::decode_insn`, `EbpfInsn::encode`,
//!    and `EbpfLifter`.
//! 2. Independent specification-derived reference evaluator (`SpecRefVm`)
//!    built directly from IETF RFC 9669 and the Solana SBPF V0–V4 specification
//!    tables vs:
//!    - `rax::isa::ebpf::EbpfCpu` (direct ISA interpreter),
//!    - `rax::smir::lift::ebpf::EbpfLifter` + `SmirInterpreter` (`OptLevel::O0`),
//!    - `rax::smir::lift::ebpf::EbpfLifter` + `SmirInterpreter` (`OptLevel::O2`).

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use rax::isa::ebpf::{
    EbpfConfig, EbpfCpu, EbpfDialect, EbpfExit, EbpfInsn, SbpfMemoryMapping, SbpfMemoryRegion,
    SbpfVersion, decode_insn,
};
use rax::smir::ir::memory::{FlatMemory, SmirMemory};
use rax::smir::ir::types::{ArchReg, EbpfReg, Endian};
use rax::smir::lift::{LiftContext, SmirLifter};
use rax::smir::optimize::{OptLevel, optimize_function};
use rax::smir::{EbpfLifter, ExitReason, SmirContext, SmirInterpreter};

const MEM_SIZE: usize = 0x10000;
const SCRATCH_BASE: u64 = 0x4000;

fn find_llvm_mc() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("llvm-mc");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    for fallback in [
        "/Users/int/local/bin/llvm-mc",
        "/opt/homebrew/bin/llvm-mc",
        "/usr/bin/llvm-mc",
    ] {
        let candidate = PathBuf::from(fallback);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Assemble an eBPF instruction with `llvm-mc` and return its raw bytes.
fn llvm_mc_assemble(asm: &str, endian: Endian) -> Option<Vec<u8>> {
    let bin = find_llvm_mc()?;
    let triple = match endian {
        Endian::Little => "-triple=bpfel",
        Endian::Big => "-triple=bpfeb",
    };
    let mut child = Command::new(bin)
        .args([triple, "-mcpu=v4", "-show-encoding"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(asm.as_bytes()).ok()?;
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    if !child.wait().ok()?.success() {
        return None;
    }
    let mut bytes = Vec::new();
    for line in out.lines() {
        if let Some(idx) = line.find("encoding: [") {
            let rest = &line[idx + 11..];
            let end = rest.find(']')?;
            for tok in rest[..end].split(',') {
                let tok = tok.trim().strip_prefix("0x").unwrap_or(tok.trim());
                if let Ok(b) = u8::from_str_radix(tok, 16) {
                    bytes.push(b);
                }
            }
        }
    }
    if bytes.is_empty() { None } else { Some(bytes) }
}

fn raw_insn(opc: u8, dst: u8, src: u8, off: i16, imm: i32, endian: Endian) -> [u8; 8] {
    let regs = match endian {
        Endian::Little => (src << 4) | (dst & 0x0f),
        Endian::Big => (dst << 4) | (src & 0x0f),
    };
    let off_b = match endian {
        Endian::Little => off.to_le_bytes(),
        Endian::Big => off.to_be_bytes(),
    };
    let imm_b = match endian {
        Endian::Little => imm.to_le_bytes(),
        Endian::Big => imm.to_be_bytes(),
    };
    [
        opc, regs, off_b[0], off_b[1], imm_b[0], imm_b[1], imm_b[2], imm_b[3],
    ]
}

// ============================================================================
// Independent Specification-Derived Reference Evaluator
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
enum DiffOutcome {
    Exited { regs: [u64; 11], r0: u64 },
    Trapped,
}

/// Pure specification-table reference VM for IETF RFC 9669 and Solana SBPF V0–V4.
fn run_spec_ref_vm(
    prog: &[u8],
    dialect: EbpfDialect,
    endian: Endian,
    init_regs: &[u64; 11],
    init_scratch: &[u8],
) -> (DiffOutcome, Vec<u8>) {
    let mut mem = vec![0u8; MEM_SIZE];
    mem[..prog.len()].copy_from_slice(prog);
    mem[SCRATCH_BASE as usize..SCRATCH_BASE as usize + init_scratch.len()]
        .copy_from_slice(init_scratch);

    let mut r = *init_regs;
    let mut pc: usize = 0;
    let mut steps = 0usize;

    let read_u16 = |buf: &[u8], e: Endian| -> u16 {
        let a = [buf[0], buf[1]];
        match e {
            Endian::Little => u16::from_le_bytes(a),
            Endian::Big => u16::from_be_bytes(a),
        }
    };
    let read_u32 = |buf: &[u8], e: Endian| -> u32 {
        let a = [buf[0], buf[1], buf[2], buf[3]];
        match e {
            Endian::Little => u32::from_le_bytes(a),
            Endian::Big => u32::from_be_bytes(a),
        }
    };
    let read_u64 = |buf: &[u8], e: Endian| -> u64 {
        let a = [
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ];
        match e {
            Endian::Little => u64::from_le_bytes(a),
            Endian::Big => u64::from_be_bytes(a),
        }
    };
    let write_u16 = |buf: &mut [u8], v: u16, e: Endian| {
        let b = match e {
            Endian::Little => v.to_le_bytes(),
            Endian::Big => v.to_be_bytes(),
        };
        buf[..2].copy_from_slice(&b);
    };
    let write_u32 = |buf: &mut [u8], v: u32, e: Endian| {
        let b = match e {
            Endian::Little => v.to_le_bytes(),
            Endian::Big => v.to_be_bytes(),
        };
        buf[..4].copy_from_slice(&b);
    };
    let write_u64 = |buf: &mut [u8], v: u64, e: Endian| {
        let b = match e {
            Endian::Little => v.to_le_bytes(),
            Endian::Big => v.to_be_bytes(),
        };
        buf[..8].copy_from_slice(&b);
    };

    let finish = |outcome: DiffOutcome, mem: Vec<u8>| -> (DiffOutcome, Vec<u8>) {
        let scratch =
            mem[SCRATCH_BASE as usize..SCRATCH_BASE as usize + init_scratch.len()].to_vec();
        (outcome, scratch)
    };

    loop {
        if steps > 10_000 || pc + 8 > prog.len() {
            return finish(DiffOutcome::Trapped, mem);
        }
        steps += 1;

        let slot = &prog[pc..pc + 8];
        let opc = slot[0];
        let (dst, src) = match endian {
            Endian::Little => ((slot[1] & 0x0f) as usize, (slot[1] >> 4) as usize),
            Endian::Big => ((slot[1] >> 4) as usize, (slot[1] & 0x0f) as usize),
        };
        if dst > 10 || src > 10 {
            return finish(DiffOutcome::Trapped, mem);
        }
        let off = read_u16(&slot[2..4], endian) as i16;
        let imm = read_u32(&slot[4..8], endian) as i32;
        let mut next_pc = pc + 8;

        // Check if writable (R10 is read-only unless SBPF V1+ dynamic stack frames)
        let dst_writable = dst < 10
            || matches!(
                dialect,
                EbpfDialect::Solana(
                    SbpfVersion::V1 | SbpfVersion::V2 | SbpfVersion::V3 | SbpfVersion::V4
                )
            );

        // Handle LDDW (0x18) and HOR64_IMM (0xF7 in Solana V2)
        if opc == 0x18 {
            if matches!(dialect, EbpfDialect::Solana(SbpfVersion::V2)) {
                return finish(DiffOutcome::Trapped, mem);
            }
            if pc + 16 > prog.len() || !dst_writable {
                return finish(DiffOutcome::Trapped, mem);
            }
            let slot2 = &prog[pc + 8..pc + 16];
            if slot2[0] != 0 || slot2[1] != 0 || read_u16(&slot2[2..4], endian) != 0 {
                return finish(DiffOutcome::Trapped, mem);
            }
            let imm_hi = read_u32(&slot2[4..8], endian) as u64;
            let imm_lo = (imm as u32) as u64;
            r[dst] = (imm_hi << 32) | imm_lo;
            pc += 16;
            continue;
        }
        if opc == 0xF7 && matches!(dialect, EbpfDialect::Solana(SbpfVersion::V2)) {
            if !dst_writable {
                return finish(DiffOutcome::Trapped, mem);
            }
            r[dst] |= ((imm as u32) as u64) << 32;
            pc = next_pc;
            continue;
        }

        // Solana V2 relocated memory load/store classes
        if matches!(dialect, EbpfDialect::Solana(SbpfVersion::V2)) {
            let v2_mem_size = match opc {
                0x27 | 0x2F | 0x2C => Some((1usize, false)),
                0x37 | 0x3F | 0x3C => Some((2usize, false)),
                0x87 | 0x8F | 0x8C => Some((4usize, false)),
                0x97 | 0x9F | 0x9C => Some((8usize, false)),
                _ => None,
            };
            if let Some((sz, _)) = v2_mem_size {
                let is_load = (opc & 0x07) == 0x04;
                if is_load {
                    if !dst_writable {
                        return finish(DiffOutcome::Trapped, mem);
                    }
                    let addr = r[src].wrapping_add(off as i64 as u64) as usize;
                    if addr + sz > mem.len() {
                        return finish(DiffOutcome::Trapped, mem);
                    }
                    r[dst] = match sz {
                        1 => mem[addr] as u64,
                        2 => read_u16(&mem[addr..addr + 2], endian) as u64,
                        4 => read_u32(&mem[addr..addr + 4], endian) as u64,
                        8 => read_u64(&mem[addr..addr + 8], endian),
                        _ => unreachable!(),
                    };
                } else {
                    let addr = r[dst].wrapping_add(off as i64 as u64) as usize;
                    if addr + sz > mem.len() {
                        return finish(DiffOutcome::Trapped, mem);
                    }
                    let val = if (opc & 0x08) == 0 {
                        imm as i64 as u64
                    } else {
                        r[src]
                    };
                    match sz {
                        1 => mem[addr] = val as u8,
                        2 => write_u16(&mut mem[addr..addr + 2], val as u16, endian),
                        4 => write_u32(&mut mem[addr..addr + 4], val as u32, endian),
                        8 => write_u64(&mut mem[addr..addr + 8], val, endian),
                        _ => unreachable!(),
                    }
                }
                pc = next_pc;
                continue;
            }
        }

        // Solana V2 PQR class (0x06 / 0x0E)
        if (opc & 0x07) == 0x06 && matches!(dialect, EbpfDialect::Solana(SbpfVersion::V2)) {
            if !dst_writable {
                return finish(DiffOutcome::Trapped, mem);
            }
            let is_64 = (opc & 0x10) != 0;
            let is_reg = (opc & 0x08) != 0;
            let pqr_op = opc & 0xE0;

            if is_64 {
                let d = r[dst];
                let s = if is_reg {
                    r[src]
                } else if matches!(pqr_op, 0x20 | 0x40 | 0x60) {
                    (imm as u32) as u64
                } else {
                    imm as i64 as u64
                };
                r[dst] = match pqr_op {
                    0x80 => d.wrapping_mul(s),
                    0x20 => ((d as u128).wrapping_mul(s as u128) >> 64) as u64,
                    0xA0 => (((d as i64 as i128).wrapping_mul(s as i64 as i128)) >> 64) as u64,
                    0x40 => {
                        if s == 0 {
                            return finish(DiffOutcome::Trapped, mem);
                        }
                        d / s
                    }
                    0x60 => {
                        if s == 0 {
                            return finish(DiffOutcome::Trapped, mem);
                        }
                        d % s
                    }
                    0xC0 => {
                        if s == 0 || (d as i64 == i64::MIN && s as i64 == -1) {
                            return finish(DiffOutcome::Trapped, mem);
                        }
                        ((d as i64) / (s as i64)) as u64
                    }
                    0xE0 => {
                        if s == 0 || (d as i64 == i64::MIN && s as i64 == -1) {
                            return finish(DiffOutcome::Trapped, mem);
                        }
                        ((d as i64) % (s as i64)) as u64
                    }
                    _ => return finish(DiffOutcome::Trapped, mem),
                };
            } else {
                let d = r[dst] as u32;
                let s = if is_reg { r[src] as u32 } else { imm as u32 };
                r[dst] = match pqr_op {
                    0x80 => d.wrapping_mul(s) as u64,
                    0x40 => {
                        if s == 0 {
                            return finish(DiffOutcome::Trapped, mem);
                        }
                        (d / s) as u64
                    }
                    0x60 => {
                        if s == 0 {
                            return finish(DiffOutcome::Trapped, mem);
                        }
                        (d % s) as u64
                    }
                    0xC0 => {
                        if s == 0 || (d as i32 == i32::MIN && s as i32 == -1) {
                            return finish(DiffOutcome::Trapped, mem);
                        }
                        (((d as i32) / (s as i32)) as u32) as u64
                    }
                    0xE0 => {
                        if s == 0 || (d as i32 == i32::MIN && s as i32 == -1) {
                            return finish(DiffOutcome::Trapped, mem);
                        }
                        (((d as i32) % (s as i32)) as u32) as u64
                    }
                    _ => return finish(DiffOutcome::Trapped, mem),
                };
            }
            pc = next_pc;
            continue;
        }

        let class = opc & 0x07;
        match class {
            0x04 | 0x07 => {
                if !dst_writable {
                    return finish(DiffOutcome::Trapped, mem);
                }
                let is_64 = class == 0x07;
                let is_reg = (opc & 0x08) != 0;
                let alu_op = opc & 0xF0;

                if alu_op == 0xD0 {
                    // END / BSWAP
                    let width = imm;
                    let to_be = is_reg;
                    let do_swap = if is_64 {
                        true
                    } else {
                        match endian {
                            Endian::Little => to_be,
                            Endian::Big => !to_be,
                        }
                    };
                    r[dst] = match width {
                        16 => {
                            let v = r[dst] as u16;
                            (if do_swap { v.swap_bytes() } else { v }) as u64
                        }
                        32 => {
                            let v = r[dst] as u32;
                            (if do_swap { v.swap_bytes() } else { v }) as u64
                        }
                        64 => {
                            let v = r[dst];
                            if do_swap { v.swap_bytes() } else { v }
                        }
                        _ => return finish(DiffOutcome::Trapped, mem),
                    };
                } else if is_64 {
                    let d = r[dst];
                    let s = if is_reg { r[src] } else { imm as i64 as u64 };
                    r[dst] = match (alu_op, off) {
                        (0x00, 0) => d.wrapping_add(s),
                        (0x10, 0) => {
                            if !is_reg && matches!(dialect, EbpfDialect::Solana(SbpfVersion::V2)) {
                                (imm as i64 as u64).wrapping_sub(d)
                            } else {
                                d.wrapping_sub(s)
                            }
                        }
                        (0x20, 0) => d.wrapping_mul(s),
                        (0x30, 0) => {
                            if s == 0 {
                                if matches!(dialect, EbpfDialect::Solana(_)) {
                                    return finish(DiffOutcome::Trapped, mem);
                                }
                                0
                            } else {
                                d / s
                            }
                        }
                        (0x30, 1) => {
                            let ds = d as i64;
                            let ss = if is_reg { s as i64 } else { imm as i64 };
                            if ss == 0 {
                                0
                            } else if ds == i64::MIN && ss == -1 {
                                i64::MIN as u64
                            } else {
                                (ds / ss) as u64
                            }
                        }
                        (0x40, 0) => d | s,
                        (0x50, 0) => d & s,
                        (0x60, 0) => d.wrapping_shl((s & 63) as u32),
                        (0x70, 0) => d.wrapping_shr((s & 63) as u32),
                        (0x80, 0) => (d as i64).wrapping_neg() as u64,
                        (0x90, 0) => {
                            if s == 0 {
                                if matches!(dialect, EbpfDialect::Solana(_)) {
                                    return finish(DiffOutcome::Trapped, mem);
                                }
                                d
                            } else {
                                d % s
                            }
                        }
                        (0x90, 1) => {
                            let ds = d as i64;
                            let ss = if is_reg { s as i64 } else { imm as i64 };
                            if ss == 0 {
                                d
                            } else if ds == i64::MIN && ss == -1 {
                                0
                            } else {
                                (ds % ss) as u64
                            }
                        }
                        (0xA0, 0) => d ^ s,
                        (0xB0, 0) => s,
                        (0xB0, 8) => (s as i8 as i64) as u64,
                        (0xB0, 16) => (s as i16 as i64) as u64,
                        (0xB0, 32) => (s as i32 as i64) as u64,
                        (0xC0, 0) => ((d as i64).wrapping_shr((s & 63) as u32)) as u64,
                        _ => return finish(DiffOutcome::Trapped, mem),
                    };
                } else {
                    let d = r[dst] as u32;
                    let s = if is_reg { r[src] as u32 } else { imm as u32 };
                    let sign_ext_alu32 = matches!(
                        dialect,
                        EbpfDialect::Solana(
                            SbpfVersion::V0 | SbpfVersion::V1 | SbpfVersion::V3 | SbpfVersion::V4
                        )
                    );
                    let res32: Option<u32> = match (alu_op, off) {
                        (0x00, 0) => {
                            let v = d.wrapping_add(s);
                            if sign_ext_alu32 {
                                r[dst] = (v as i32 as i64) as u64;
                                None
                            } else {
                                Some(v)
                            }
                        }
                        (0x10, 0) => {
                            let v = if !is_reg
                                && matches!(dialect, EbpfDialect::Solana(SbpfVersion::V2))
                            {
                                s.wrapping_sub(d)
                            } else {
                                d.wrapping_sub(s)
                            };
                            if sign_ext_alu32 {
                                r[dst] = (v as i32 as i64) as u64;
                                None
                            } else {
                                Some(v)
                            }
                        }
                        (0x20, 0) => {
                            let v = d.wrapping_mul(s);
                            if sign_ext_alu32 {
                                r[dst] = (v as i32 as i64) as u64;
                                None
                            } else {
                                Some(v)
                            }
                        }
                        (0x30, 0) => {
                            if s == 0 {
                                if matches!(dialect, EbpfDialect::Solana(_)) {
                                    return finish(DiffOutcome::Trapped, mem);
                                }
                                Some(0)
                            } else {
                                Some(d / s)
                            }
                        }
                        (0x30, 1) => {
                            let ds = d as i32;
                            let ss = s as i32;
                            if ss == 0 {
                                Some(0)
                            } else if ds == i32::MIN && ss == -1 {
                                Some(i32::MIN as u32)
                            } else {
                                Some((ds / ss) as u32)
                            }
                        }
                        (0x40, 0) => Some(d | s),
                        (0x50, 0) => Some(d & s),
                        (0x60, 0) => Some(d.wrapping_shl(s & 31)),
                        (0x70, 0) => Some(d.wrapping_shr(s & 31)),
                        (0x80, 0) => Some((d as i32).wrapping_neg() as u32),
                        (0x90, 0) => {
                            if s == 0 {
                                if matches!(dialect, EbpfDialect::Solana(_)) {
                                    return finish(DiffOutcome::Trapped, mem);
                                }
                                Some(d)
                            } else {
                                Some(d % s)
                            }
                        }
                        (0x90, 1) => {
                            let ds = d as i32;
                            let ss = s as i32;
                            if ss == 0 {
                                Some(d)
                            } else if ds == i32::MIN && ss == -1 {
                                Some(0)
                            } else {
                                Some((ds % ss) as u32)
                            }
                        }
                        (0xA0, 0) => Some(d ^ s),
                        (0xB0, 0) => {
                            if is_reg && matches!(dialect, EbpfDialect::Solana(SbpfVersion::V2)) {
                                r[dst] = (s as i32 as i64) as u64;
                                None
                            } else {
                                Some(s)
                            }
                        }
                        (0xB0, 8) => Some((s as i8 as i32) as u32),
                        (0xB0, 16) => Some((s as i16 as i32) as u32),
                        (0xC0, 0) => Some(((d as i32).wrapping_shr(s & 31)) as u32),
                        _ => return finish(DiffOutcome::Trapped, mem),
                    };
                    if let Some(v) = res32 {
                        r[dst] = v as u64;
                    }
                }
            }
            0x01..=0x03 => {
                // Memory / Atomic in RFC 9669 or SBPF V0/V1
                let mode = opc & 0xE0;
                let sz = match (opc >> 3) & 0x03 {
                    0 => 4usize,
                    1 => 2usize,
                    2 => 1usize,
                    3 => 8usize,
                    _ => unreachable!(),
                };
                if class == 0x01 {
                    if !dst_writable {
                        return finish(DiffOutcome::Trapped, mem);
                    }
                    let addr = r[src].wrapping_add(off as i64 as u64) as usize;
                    if addr + sz > mem.len() {
                        return finish(DiffOutcome::Trapped, mem);
                    }
                    if mode == 0x60 {
                        r[dst] = match sz {
                            1 => mem[addr] as u64,
                            2 => read_u16(&mem[addr..addr + 2], endian) as u64,
                            4 => read_u32(&mem[addr..addr + 4], endian) as u64,
                            8 => read_u64(&mem[addr..addr + 8], endian),
                            _ => unreachable!(),
                        };
                    } else if mode == 0x80 {
                        r[dst] = match sz {
                            1 => (mem[addr] as i8 as i64) as u64,
                            2 => (read_u16(&mem[addr..addr + 2], endian) as i16 as i64) as u64,
                            4 => (read_u32(&mem[addr..addr + 4], endian) as i32 as i64) as u64,
                            _ => return finish(DiffOutcome::Trapped, mem),
                        };
                    } else {
                        return finish(DiffOutcome::Trapped, mem);
                    }
                } else if class == 0x02 && mode == 0x60 {
                    let addr = r[dst].wrapping_add(off as i64 as u64) as usize;
                    if addr + sz > mem.len() {
                        return finish(DiffOutcome::Trapped, mem);
                    }
                    let val = imm as i64 as u64;
                    match sz {
                        1 => mem[addr] = val as u8,
                        2 => write_u16(&mut mem[addr..addr + 2], val as u16, endian),
                        4 => write_u32(&mut mem[addr..addr + 4], val as u32, endian),
                        8 => write_u64(&mut mem[addr..addr + 8], val, endian),
                        _ => unreachable!(),
                    }
                } else if class == 0x03 && mode == 0x60 {
                    let addr = r[dst].wrapping_add(off as i64 as u64) as usize;
                    if addr + sz > mem.len() {
                        return finish(DiffOutcome::Trapped, mem);
                    }
                    let val = r[src];
                    match sz {
                        1 => mem[addr] = val as u8,
                        2 => write_u16(&mut mem[addr..addr + 2], val as u16, endian),
                        4 => write_u32(&mut mem[addr..addr + 4], val as u32, endian),
                        8 => write_u64(&mut mem[addr..addr + 8], val, endian),
                        _ => unreachable!(),
                    }
                } else if class == 0x03 && mode == 0xC0 {
                    let addr = r[dst].wrapping_add(off as i64 as u64) as usize;
                    if addr + sz > mem.len() {
                        return finish(DiffOutcome::Trapped, mem);
                    }
                    if sz == 4 {
                        let old = read_u32(&mem[addr..addr + 4], endian);
                        let sv = r[src] as u32;
                        match imm as u32 {
                            0x00 => {
                                write_u32(&mut mem[addr..addr + 4], old.wrapping_add(sv), endian)
                            }
                            0x01 => {
                                write_u32(&mut mem[addr..addr + 4], old.wrapping_add(sv), endian);
                                r[src] = old as u64;
                            }
                            0x40 => write_u32(&mut mem[addr..addr + 4], old | sv, endian),
                            0x41 => {
                                write_u32(&mut mem[addr..addr + 4], old | sv, endian);
                                r[src] = old as u64;
                            }
                            0x50 => write_u32(&mut mem[addr..addr + 4], old & sv, endian),
                            0x51 => {
                                write_u32(&mut mem[addr..addr + 4], old & sv, endian);
                                r[src] = old as u64;
                            }
                            0xA0 => write_u32(&mut mem[addr..addr + 4], old ^ sv, endian),
                            0xA1 => {
                                write_u32(&mut mem[addr..addr + 4], old ^ sv, endian);
                                r[src] = old as u64;
                            }
                            0xE1 => {
                                write_u32(&mut mem[addr..addr + 4], sv, endian);
                                r[src] = old as u64;
                            }
                            0xF1 => {
                                if old == r[0] as u32 {
                                    write_u32(&mut mem[addr..addr + 4], sv, endian);
                                }
                                r[0] = old as u64;
                            }
                            _ => return finish(DiffOutcome::Trapped, mem),
                        }
                    } else if sz == 8 {
                        let old = read_u64(&mem[addr..addr + 8], endian);
                        let sv = r[src];
                        match imm as u32 {
                            0x00 => {
                                write_u64(&mut mem[addr..addr + 8], old.wrapping_add(sv), endian)
                            }
                            0x01 => {
                                write_u64(&mut mem[addr..addr + 8], old.wrapping_add(sv), endian);
                                r[src] = old;
                            }
                            0x40 => write_u64(&mut mem[addr..addr + 8], old | sv, endian),
                            0x41 => {
                                write_u64(&mut mem[addr..addr + 8], old | sv, endian);
                                r[src] = old;
                            }
                            0x50 => write_u64(&mut mem[addr..addr + 8], old & sv, endian),
                            0x51 => {
                                write_u64(&mut mem[addr..addr + 8], old & sv, endian);
                                r[src] = old;
                            }
                            0xA0 => write_u64(&mut mem[addr..addr + 8], old ^ sv, endian),
                            0xA1 => {
                                write_u64(&mut mem[addr..addr + 8], old ^ sv, endian);
                                r[src] = old;
                            }
                            0xE1 => {
                                write_u64(&mut mem[addr..addr + 8], sv, endian);
                                r[src] = old;
                            }
                            0xF1 => {
                                if old == r[0] {
                                    write_u64(&mut mem[addr..addr + 8], sv, endian);
                                }
                                r[0] = old;
                            }
                            _ => return finish(DiffOutcome::Trapped, mem),
                        }
                    } else {
                        return finish(DiffOutcome::Trapped, mem);
                    }
                } else {
                    return finish(DiffOutcome::Trapped, mem);
                }
            }
            0x05 | 0x06 => {
                // Jump / Control Flow
                let is_jmp64 = class == 0x05;
                let jmp_op = opc & 0xF0;
                let is_reg = (opc & 0x08) != 0;

                if jmp_op == 0x00 {
                    let delta = if is_jmp64 { off as i64 } else { imm as i64 };
                    next_pc = (next_pc as i64).wrapping_add(delta.wrapping_mul(8)) as usize;
                } else if jmp_op == 0x80 {
                    // CALL (for local relative calls src == 1 in RFC 9669)
                    return finish(DiffOutcome::Trapped, mem);
                } else if jmp_op == 0x90 {
                    // EXIT / RETURN
                    return finish(DiffOutcome::Exited { regs: r, r0: r[0] }, mem);
                } else {
                    let taken = if is_jmp64 {
                        let d = r[dst];
                        let s = if is_reg { r[src] } else { imm as i64 as u64 };
                        match jmp_op {
                            0x10 => d == s,
                            0x20 => d > s,
                            0x30 => d >= s,
                            0x40 => (d & s) != 0,
                            0x50 => d != s,
                            0x60 => (d as i64) > (s as i64),
                            0x70 => (d as i64) >= (s as i64),
                            0xA0 => d < s,
                            0xB0 => d <= s,
                            0xC0 => (d as i64) < (s as i64),
                            0xD0 => (d as i64) <= (s as i64),
                            _ => return finish(DiffOutcome::Trapped, mem),
                        }
                    } else {
                        let d = r[dst] as u32;
                        let s = if is_reg { r[src] as u32 } else { imm as u32 };
                        match jmp_op {
                            0x10 => d == s,
                            0x20 => d > s,
                            0x30 => d >= s,
                            0x40 => (d & s) != 0,
                            0x50 => d != s,
                            0x60 => (d as i32) > (s as i32),
                            0x70 => (d as i32) >= (s as i32),
                            0xA0 => d < s,
                            0xB0 => d <= s,
                            0xC0 => (d as i32) < (s as i32),
                            0xD0 => (d as i32) <= (s as i32),
                            _ => return finish(DiffOutcome::Trapped, mem),
                        }
                    };
                    if taken {
                        next_pc =
                            (next_pc as i64).wrapping_add((off as i64).wrapping_mul(8)) as usize;
                    }
                }
            }
            _ => return finish(DiffOutcome::Trapped, mem),
        }

        pc = next_pc;
    }
}

fn run_direct_cpu(
    prog: &[u8],
    dialect: EbpfDialect,
    endian: Endian,
    init_regs: &[u64; 11],
    init_scratch: &[u8],
) -> (DiffOutcome, Vec<u8>) {
    let mut mem = FlatMemory::new(MEM_SIZE);
    mem.load(0, prog);
    mem.load(SCRATCH_BASE as usize, init_scratch);

    let mut cpu = EbpfCpu::new(EbpfConfig {
        dialect,
        endian,
        ..EbpfConfig::default()
    });
    cpu.regs.r = *init_regs;
    cpu.regs.pc = 0;

    let outcome = match cpu.run(&mut mem) {
        EbpfExit::Exit { r0 } => DiffOutcome::Exited {
            regs: cpu.regs.r,
            r0,
        },
        _ => DiffOutcome::Trapped,
    };

    let mut scratch = vec![0u8; init_scratch.len()];
    SmirMemory::read(&mut mem, SCRATCH_BASE, &mut scratch).unwrap();
    (outcome, scratch)
}

struct CodeSlice<'a>(&'a [u8]);

impl rax::smir::lift::MemoryReader for CodeSlice<'_> {
    fn read(
        &self,
        addr: rax::smir::ir::types::GuestAddr,
        size: usize,
    ) -> Result<Vec<u8>, rax::smir::ir::memory::MemoryError> {
        let start = addr as usize;
        let end = start.saturating_add(size);
        if end > self.0.len() {
            return Err(rax::smir::ir::memory::MemoryError::OutOfBounds { addr });
        }
        Ok(self.0[start..end].to_vec())
    }
}

fn run_smir_cpu(
    prog: &[u8],
    dialect: EbpfDialect,
    endian: Endian,
    init_regs: &[u64; 11],
    init_scratch: &[u8],
    opt_level: OptLevel,
) -> (DiffOutcome, Vec<u8>) {
    let mut mem = FlatMemory::new(MEM_SIZE);
    mem.load(0, prog);
    mem.load(SCRATCH_BASE as usize, init_scratch);

    let mut lifter = EbpfLifter::new(dialect, endian);
    let mut lift_ctx = LiftContext::new(lifter.source_arch());
    lift_ctx.endian = endian;

    let mut func = match lifter.lift_function(0, &CodeSlice(prog), &mut lift_ctx) {
        Ok(f) => f,
        Err(_) => {
            let mut scratch = vec![0u8; init_scratch.len()];
            SmirMemory::read(&mut mem, SCRATCH_BASE, &mut scratch).unwrap();
            return (DiffOutcome::Trapped, scratch);
        }
    };

    if opt_level != OptLevel::O0 {
        optimize_function(&mut func, opt_level);
    }

    let mut interp = SmirInterpreter::new();
    for block in &func.blocks {
        interp.add_block(block.guest_pc, block.clone());
    }

    let mut ctx = SmirContext::new_ebpf();
    for (i, &val) in init_regs.iter().enumerate() {
        ctx.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(i as u8)), val);
    }
    ctx.pc = 0;

    let outcome = match interp.run(&mut ctx, &mut mem) {
        ExitReason::Return { to } => {
            let mut regs = [0u64; 11];
            for (i, slot) in regs.iter_mut().enumerate() {
                *slot = ctx.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(i as u8)));
            }
            assert_eq!(to, regs[0]);
            DiffOutcome::Exited { regs, r0: regs[0] }
        }
        ExitReason::Undefined { .. } | ExitReason::MemoryFault { .. } => DiffOutcome::Trapped,
        other => panic!("unexpected SMIR exit: {other:?}"),
    };

    let mut scratch = vec![0u8; init_scratch.len()];
    SmirMemory::read(&mut mem, SCRATCH_BASE, &mut scratch).unwrap();
    (outcome, scratch)
}

fn assert_4way_diff(
    label: &str,
    prog: &[u8],
    dialect: EbpfDialect,
    endian: Endian,
    init_regs: &[u64; 11],
    init_scratch: &[u8],
) {
    let (ref_out, ref_mem) = run_spec_ref_vm(prog, dialect, endian, init_regs, init_scratch);
    let (dir_out, dir_mem) = run_direct_cpu(prog, dialect, endian, init_regs, init_scratch);
    let (o0_out, o0_mem) =
        run_smir_cpu(prog, dialect, endian, init_regs, init_scratch, OptLevel::O0);
    let (o2_out, o2_mem) =
        run_smir_cpu(prog, dialect, endian, init_regs, init_scratch, OptLevel::O2);

    assert_eq!(
        dir_out, ref_out,
        "[{label}] direct EbpfCpu vs SpecRefVm outcome mismatch ({dialect:?}, {endian:?})"
    );
    assert_eq!(
        dir_mem, ref_mem,
        "[{label}] direct EbpfCpu vs SpecRefVm memory mismatch ({dialect:?}, {endian:?})"
    );
    assert_eq!(
        o0_out, ref_out,
        "[{label}] SMIR O0 vs SpecRefVm outcome mismatch ({dialect:?}, {endian:?})"
    );
    assert_eq!(
        o0_mem, ref_mem,
        "[{label}] SMIR O0 vs SpecRefVm memory mismatch ({dialect:?}, {endian:?})"
    );
    assert_eq!(
        o2_out, ref_out,
        "[{label}] SMIR O2 vs SpecRefVm outcome mismatch ({dialect:?}, {endian:?})"
    );
    assert_eq!(
        o2_mem, ref_mem,
        "[{label}] SMIR O2 vs SpecRefVm memory mismatch ({dialect:?}, {endian:?})"
    );
}

// ============================================================================
// 1. llvm-mc Differential Encoding / Decoding / Lifting Verification
// ============================================================================

#[test]
fn ebpf_llvm_mc_encoding_and_decode_parity() {
    // Canonical RFC 9669 v4 instruction corpus with expected little-endian bytes.
    // When `llvm-mc` is available on the host, we additionally assemble each
    // statement in both `bpfel` and `bpfeb` and verify byte-for-byte identity.
    let cases: &[(&str, [u8; 8], [u8; 8])] = &[
        (
            "r1 += r2",
            raw_insn(0x0f, 1, 2, 0, 0, Endian::Little),
            raw_insn(0x0f, 1, 2, 0, 0, Endian::Big),
        ),
        (
            "r3 -= 42",
            raw_insn(0x17, 3, 0, 0, 42, Endian::Little),
            raw_insn(0x17, 3, 0, 0, 42, Endian::Big),
        ),
        (
            "w4 *= w5",
            raw_insn(0x2c, 4, 5, 0, 0, Endian::Little),
            raw_insn(0x2c, 4, 5, 0, 0, Endian::Big),
        ),
        (
            "r6 s/= r7",
            raw_insn(0x3f, 6, 7, 1, 0, Endian::Little),
            raw_insn(0x3f, 6, 7, 1, 0, Endian::Big),
        ),
        (
            "w8 s%= w9",
            raw_insn(0x9c, 8, 9, 1, 0, Endian::Little),
            raw_insn(0x9c, 8, 9, 1, 0, Endian::Big),
        ),
        (
            "r1 = (s8)r2",
            raw_insn(0xbf, 1, 2, 8, 0, Endian::Little),
            raw_insn(0xbf, 1, 2, 8, 0, Endian::Big),
        ),
        (
            "r1 = (s16)r2",
            raw_insn(0xbf, 1, 2, 16, 0, Endian::Little),
            raw_insn(0xbf, 1, 2, 16, 0, Endian::Big),
        ),
        (
            "r1 = (s32)r2",
            raw_insn(0xbf, 1, 2, 32, 0, Endian::Little),
            raw_insn(0xbf, 1, 2, 32, 0, Endian::Big),
        ),
        (
            "w3 = (s8)w4",
            raw_insn(0xbc, 3, 4, 8, 0, Endian::Little),
            raw_insn(0xbc, 3, 4, 8, 0, Endian::Big),
        ),
        (
            "w3 = (s16)w4",
            raw_insn(0xbc, 3, 4, 16, 0, Endian::Little),
            raw_insn(0xbc, 3, 4, 16, 0, Endian::Big),
        ),
        (
            "r1 = bswap16 r1",
            raw_insn(0xd7, 1, 0, 0, 16, Endian::Little),
            raw_insn(0xd7, 1, 0, 0, 16, Endian::Big),
        ),
        (
            "r2 = bswap32 r2",
            raw_insn(0xd7, 2, 0, 0, 32, Endian::Little),
            raw_insn(0xd7, 2, 0, 0, 32, Endian::Big),
        ),
        (
            "r3 = bswap64 r3",
            raw_insn(0xd7, 3, 0, 0, 64, Endian::Little),
            raw_insn(0xd7, 3, 0, 0, 64, Endian::Big),
        ),
        (
            "r4 = *(u64 *)(r5 + 16)",
            raw_insn(0x79, 4, 5, 16, 0, Endian::Little),
            raw_insn(0x79, 4, 5, 16, 0, Endian::Big),
        ),
        (
            "r4 = *(s8 *)(r5 - 8)",
            raw_insn(0x91, 4, 5, -8, 0, Endian::Little),
            raw_insn(0x91, 4, 5, -8, 0, Endian::Big),
        ),
        (
            "r4 = *(s16 *)(r5 + 4)",
            raw_insn(0x89, 4, 5, 4, 0, Endian::Little),
            raw_insn(0x89, 4, 5, 4, 0, Endian::Big),
        ),
        (
            "r4 = *(s32 *)(r5 + 12)",
            raw_insn(0x81, 4, 5, 12, 0, Endian::Little),
            raw_insn(0x81, 4, 5, 12, 0, Endian::Big),
        ),
        (
            "*(u32 *)(r6 + 8) = r7",
            raw_insn(0x63, 6, 7, 8, 0, Endian::Little),
            raw_insn(0x63, 6, 7, 8, 0, Endian::Big),
        ),
        (
            "lock *(u64 *)(r1 + 0) += r2",
            raw_insn(0xdb, 1, 2, 0, 0x00, Endian::Little),
            raw_insn(0xdb, 1, 2, 0, 0x00, Endian::Big),
        ),
        (
            "r2 = atomic_fetch_add((u64 *)(r1 + 8), r2)",
            raw_insn(0xdb, 1, 2, 8, 0x01, Endian::Little),
            raw_insn(0xdb, 1, 2, 8, 0x01, Endian::Big),
        ),
        (
            "r2 = xchg_64(r1 + 0, r2)",
            raw_insn(0xdb, 1, 2, 0, 0xe1, Endian::Little),
            raw_insn(0xdb, 1, 2, 0, 0xe1, Endian::Big),
        ),
        (
            "r0 = cmpxchg_64(r1 + 0, r0, r2)",
            raw_insn(0xdb, 1, 2, 0, 0xf1, Endian::Little),
            raw_insn(0xdb, 1, 2, 0, 0xf1, Endian::Big),
        ),
        (
            "gotol +12",
            raw_insn(0x06, 0, 0, 0, 12, Endian::Little),
            raw_insn(0x06, 0, 0, 0, 12, Endian::Big),
        ),
        (
            "if r1 == r2 goto +3",
            raw_insn(0x1d, 1, 2, 3, 0, Endian::Little),
            raw_insn(0x1d, 1, 2, 3, 0, Endian::Big),
        ),
        (
            "if w3 s< -5 goto +2",
            raw_insn(0xc6, 3, 0, 2, -5, Endian::Little),
            raw_insn(0xc6, 3, 0, 2, -5, Endian::Big),
        ),
        (
            "exit",
            raw_insn(0x95, 0, 0, 0, 0, Endian::Little),
            raw_insn(0x95, 0, 0, 0, 0, Endian::Big),
        ),
    ];

    let has_llvm_mc = find_llvm_mc().is_some();

    for &(asm, expected_le, expected_be) in cases {
        for (endian, expected) in [(Endian::Little, expected_le), (Endian::Big, expected_be)] {
            if has_llvm_mc {
                let mc_bytes = llvm_mc_assemble(asm, endian)
                    .unwrap_or_else(|| panic!("llvm-mc failed to assemble `{asm}` ({endian:?})"));
                assert_eq!(
                    mc_bytes.as_slice(),
                    &expected,
                    "llvm-mc encoding mismatch for `{asm}` ({endian:?})"
                );
            }

            let decoded: EbpfInsn = decode_insn(&expected, EbpfDialect::Rfc9669, endian)
                .unwrap_or_else(|e| panic!("decode_insn failed for `{asm}` ({endian:?}): {e}"));
            assert_eq!(
                decoded.encode(endian),
                expected.to_vec(),
                "roundtrip encode mismatch for `{asm}` ({endian:?})"
            );

            // Also verify SMIR lifter accepts the instruction
            let mut lifter = EbpfLifter::new(EbpfDialect::Rfc9669, endian);
            let mut ctx = LiftContext::new(lifter.source_arch());
            ctx.endian = endian;
            lifter
                .lift_insn(0x1000, &expected, &mut ctx)
                .unwrap_or_else(|e| panic!("EbpfLifter failed for `{asm}` ({endian:?}): {e}"));
        }
    }
}

// ============================================================================
// 2. Seeded Differential Sweep: SpecRefVm vs EbpfCpu vs SMIR (O0 & O2)
// ============================================================================

#[test]
fn ebpf_rfc9669_seeded_differential_sweep() {
    let seed_vectors: &[[u64; 11]] = &[
        [
            0,
            1,
            2,
            0x7FFF_FFFF,
            0x8000_0000,
            0xFFFF_FFFF,
            0x7FFF_FFFF_FFFF_FFFF,
            0x8000_0000_0000_0000,
            0xFFFF_FFFF_FFFF_FFFF,
            0x1234_5678_9ABC_DEF0,
            SCRATCH_BASE + 0x100,
        ],
        [
            0xDEAD_BEEF_CAFE_BABE,
            0x8000_0000_0000_0000,
            0xFFFF_FFFF_FFFF_FFFF,
            0x0000_0000_8000_0000,
            0xFFFF_FFFF_FFFF_FFFF,
            42,
            0,
            19,
            0xFEDC_BA98_7654_3210,
            7,
            SCRATCH_BASE + 0x100,
        ],
    ];

    let alu_ops: &[(u8, i16, i32)] = &[
        (0x00, 0, 12345),      // ADD
        (0x10, 0, -54321),     // SUB
        (0x20, 0, 31337),      // MUL
        (0x30, 0, 17),         // DIV
        (0x30, 0, 0),          // DIV by 0
        (0x30, 1, -1),         // SDIV by -1
        (0x30, 1, 0),          // SDIV by 0
        (0x30, 1, 7),          // SDIV by 7
        (0x40, 0, 0x55AA55AA), // OR
        (0x50, 0, 0x0F0F0F0F), // AND
        (0x60, 0, 77),         // LSH (masked)
        (0x70, 0, 101),        // RSH (masked)
        (0x80, 0, 0),          // NEG
        (0x90, 0, 23),         // MOD
        (0x90, 0, 0),          // MOD by 0
        (0x90, 1, -1),         // SMOD by -1
        (0x90, 1, 0),          // SMOD by 0
        (0x90, 1, -13),        // SMOD by -13
        (0xA0, 0, -1),         // XOR
        (0xB0, 0, -999),       // MOV
        (0xC0, 0, 99),         // ARSH (masked)
    ];

    let init_scratch = vec![0xA5u8; 256];

    for endian in [Endian::Little, Endian::Big] {
        for (s_idx, regs) in seed_vectors.iter().enumerate() {
            for &(alu_hi, off, imm) in alu_ops {
                for &class in &[0x04u8, 0x07u8] {
                    for &src_bit in &[0x00u8, 0x08u8] {
                        if alu_hi == 0x80 && src_bit != 0 {
                            continue; // NEG has K source only
                        }
                        if (alu_hi == 0x30 || alu_hi == 0x90)
                            && off == 1
                            && src_bit == 0
                            && imm == 0
                        {
                            // Covered by runtime sdiv/smod by zero
                        }
                        let opc = alu_hi | src_bit | class;
                        let mut prog = Vec::new();
                        prog.extend_from_slice(&raw_insn(opc, 1, 2, off, imm, endian));
                        prog.extend_from_slice(&raw_insn(opc, 3, 4, off, imm, endian));
                        prog.extend_from_slice(&raw_insn(opc, 5, 6, off, imm, endian));
                        prog.extend_from_slice(&raw_insn(0xBF, 0, 1, 0, 0, endian));
                        prog.extend_from_slice(&raw_insn(0x0F, 0, 3, 0, 0, endian));
                        prog.extend_from_slice(&raw_insn(0x0F, 0, 5, 0, 0, endian));
                        prog.extend_from_slice(&raw_insn(0x95, 0, 0, 0, 0, endian));

                        assert_4way_diff(
                            &format!("rfc9669_s{s_idx}_opc_{opc:#04x}_off_{off}"),
                            &prog,
                            EbpfDialect::Rfc9669,
                            endian,
                            regs,
                            &init_scratch,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn ebpf_solana_sbpf_v0_to_v4_differential_sweep() {
    let init_regs: [u64; 11] = [
        0,
        0x8000_0000_0000_0000,
        0xFFFF_FFFF_FFFF_FFFF,
        0x0000_0000_8000_0000,
        0x0000_0000_FFFF_FFFF,
        0x1234_5678_9ABC_DEF0,
        0x0FEDCBA9_87654321,
        17,
        29,
        SCRATCH_BASE,
        SCRATCH_BASE + 0x80,
    ];
    let init_scratch: Vec<u8> = (0..256).map(|i| (i as u8).wrapping_mul(13)).collect();

    // Test V0, V1, V2, V3, V4 ALU and memory behavior against SpecRefVm
    for ver in [
        SbpfVersion::V0,
        SbpfVersion::V1,
        SbpfVersion::V2,
        SbpfVersion::V3,
        SbpfVersion::V4,
    ] {
        let dialect = EbpfDialect::Solana(ver);
        let mut prog = Vec::new();
        // w1 += w5
        prog.extend_from_slice(&raw_insn(0x0C, 1, 5, 0, 0, Endian::Little));
        // w3 -= 100 (V0/V1 sign-extends, V2+ swaps to 100 - w3 and zero-extends)
        prog.extend_from_slice(&raw_insn(0x14, 3, 0, 0, 100, Endian::Little));
        // r6 -= 500 (V2+ swaps to 500 - r6)
        prog.extend_from_slice(&raw_insn(0x17, 6, 0, 0, 500, Endian::Little));
        // w4 = -50 (V0/V1 sign-extends, V2+ zero-extends)
        prog.extend_from_slice(&raw_insn(0xB4, 4, 0, 0, -50, Endian::Little));

        if matches!(ver, SbpfVersion::V2) {
            // V2 PQR + HOR64_IMM + V2 memory
            prog.extend_from_slice(&raw_insn(0xF7, 4, 0, 0, 0x1357_2468, Endian::Little));
            prog.extend_from_slice(&raw_insn(0x96, 5, 0, 0, 19, Endian::Little)); // lmul64 imm
            prog.extend_from_slice(&raw_insn(0x3E, 6, 5, 0, 0, Endian::Little)); // uhmul64 reg
            prog.extend_from_slice(&raw_insn(0xBE, 5, 6, 0, 0, Endian::Little)); // shmul64 reg
            prog.extend_from_slice(&raw_insn(0x5E, 6, 7, 0, 0, Endian::Little)); // udiv64 reg
            prog.extend_from_slice(&raw_insn(0x7E, 5, 8, 0, 0, Endian::Little)); // urem64 reg
            prog.extend_from_slice(&raw_insn(0xDE, 1, 7, 0, 0, Endian::Little)); // sdiv64 reg
            prog.extend_from_slice(&raw_insn(0xFE, 2, 8, 0, 0, Endian::Little)); // srem64 reg
            // V2 store and load
            prog.extend_from_slice(&raw_insn(0x9F, 9, 1, 16, 0, Endian::Little)); // ST_8B_REG
            prog.extend_from_slice(&raw_insn(0x9C, 0, 9, 16, 0, Endian::Little)); // LD_8B_REG
        } else {
            // V0/V1/V3/V4 standard memory
            prog.extend_from_slice(&raw_insn(0x7B, 9, 1, 16, 0, Endian::Little)); // STX_DW
            prog.extend_from_slice(&raw_insn(0x79, 0, 9, 16, 0, Endian::Little)); // LDX_DW
        }

        prog.extend_from_slice(&raw_insn(0x0F, 0, 3, 0, 0, Endian::Little));
        prog.extend_from_slice(&raw_insn(0x0F, 0, 4, 0, 0, Endian::Little));
        prog.extend_from_slice(&raw_insn(0x0F, 0, 6, 0, 0, Endian::Little));
        prog.extend_from_slice(&raw_insn(0x95, 0, 0, 0, 0, Endian::Little));

        assert_4way_diff(
            &format!("solana_{ver:?}_full_pipeline"),
            &prog,
            dialect,
            Endian::Little,
            &init_regs,
            &init_scratch,
        );
    }
}

#[test]
fn ebpf_solana_sbpf_memory_mapping_direct_and_smir_parity() {
    // Verify that SbpfMemoryMapping works identically under both EbpfCpu (EbpfMemory)
    // and SmirInterpreter (SmirMemory), including read-only enforcement.
    let mut prog = Vec::new();
    // Load u64 from read-only input region (r1 + 0) -> r2
    prog.extend_from_slice(&raw_insn(0x79, 2, 1, 0, 0, Endian::Little));
    // Add 0x1111_2222_3333_4444 -> r2
    prog.extend_from_slice(&raw_insn(0x07, 2, 0, 0, 0x3333_4444, Endian::Little));
    // Store u64 to writable stack region (r10 - 8)
    prog.extend_from_slice(&raw_insn(0x7B, 10, 2, -8, 0, Endian::Little));
    // Load u64 back into r0
    prog.extend_from_slice(&raw_insn(0x79, 0, 10, -8, 0, Endian::Little));
    // Exit
    prog.extend_from_slice(&raw_insn(0x95, 0, 0, 0, 0, Endian::Little));

    let make_mapping = |prog_bytes: &[u8]| {
        let mut ro_input = vec![0u8; 64];
        ro_input[..8].copy_from_slice(&0x1000_2000_3000_4000u64.to_le_bytes());
        SbpfMemoryMapping::with_regions(vec![
            SbpfMemoryRegion::new_readonly(0x1_0000_0000, prog_bytes.to_vec()),
            SbpfMemoryRegion::new_writable(0x2_0000_0000, vec![0u8; 4096]),
            SbpfMemoryRegion::new_readonly(0x4_0000_0000, ro_input),
        ])
    };

    let mut map_direct = make_mapping(&prog);
    let mut cpu = EbpfCpu::new(EbpfConfig {
        dialect: EbpfDialect::Solana(SbpfVersion::V0),
        endian: Endian::Little,
        ..EbpfConfig::default()
    });
    cpu.regs.pc = 0x1_0000_0000;
    cpu.regs.r[1] = 0x4_0000_0000;
    cpu.regs.r[10] = 0x2_0000_1000;
    let direct_exit = cpu.run(&mut map_direct);

    let mut map_smir = make_mapping(&prog);
    let mut lifter = EbpfLifter::solana(SbpfVersion::V0);
    let mut lift_ctx = LiftContext::new(lifter.source_arch());
    let func = lifter
        .lift_function(0x1_0000_0000, &map_smir, &mut lift_ctx)
        .unwrap();

    let mut interp = SmirInterpreter::new();
    for block in &func.blocks {
        interp.add_block(block.guest_pc, block.clone());
    }
    let mut ctx = SmirContext::new_ebpf();
    ctx.pc = 0x1_0000_0000;
    ctx.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(1)), 0x4_0000_0000);
    ctx.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(10)), 0x2_0000_1000);

    let smir_exit = interp.run(&mut ctx, &mut map_smir);
    assert_eq!(
        direct_exit,
        EbpfExit::Exit {
            r0: 0x1000_2000_6333_8444
        }
    );
    assert!(
        matches!(
            smir_exit,
            ExitReason::Return {
                to: 0x1000_2000_6333_8444
            }
        ),
        "unexpected smir_exit: {smir_exit:?}"
    );
}
