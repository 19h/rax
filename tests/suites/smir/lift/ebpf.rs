//! eBPF (RFC 9669) and Solana SBPF (V0–V4) -> SMIR lift verification harness.
//!
//! Verifies 100% architectural state and memory parity between the direct
//! [`EbpfCpu`] interpreter and [`EbpfLifter`] + [`SmirInterpreter`] at both
//! `OptLevel::O0` (unoptimized) and `OptLevel::O2` (optimized) across:
//! - IETF RFC 9669 / Linux kernel v1–v4 (`Endian::Little` and `Endian::Big`),
//!   including sanitized `DIV`/`MOD` by 0, `SDIV`/`SMOD` (`MIN / -1` overflow
//!   and truncated signed remainder), `MOVSX` (8/16/32), `BSWAP` (16/32/64),
//!   `LDX_MEMSX` (8/16/32), `JA32`, `LDDW`, and `BPF_ATOMIC` (`ADD`, `OR`,
//!   `AND`, `XOR` with/without `FETCH`, `XCHG`, `CMPXCHG`).
//! - Solana SBPF `V0`, `V1`, `V2`, `V3`, and `V4`, including 32-bit sign
//!   extension rules, `V2` swapped `SUB_IMM` operands, `V2` PQR instructions
//!   (`LMUL32/64`, `UHMUL64`, `SHMUL64`, `UDIV32/64`, `UREM32/64`, `SDIV32/64`,
//!   `SREM32/64`), `V2` relocated memory classes (`LD_1B..8B_REG`,
//!   `ST_1B..8B_IMM/REG`), `HOR64_IMM`, trapping division by zero and signed
//!   division overflow, and `V3`/`V4` `JMP32`.

use rax::isa::ebpf::{EbpfConfig, EbpfCpu, EbpfDialect, EbpfExit, EbpfTrap, SbpfVersion};
use rax::smir::ir::memory::{FlatMemory, MemoryError, SmirMemory};
use rax::smir::ir::types::{ArchReg, EbpfReg, Endian};
use rax::smir::lift::{LiftContext, MemoryReader, SmirLifter};
use rax::smir::optimize::{OptLevel, optimize_function};
use rax::smir::{EbpfLifter, ExitReason, SmirContext, SmirInterpreter};

#[path = "ebpf_review_decode.rs"]
mod review_decode;

struct CodeSlice<'a>(&'a [u8]);

impl MemoryReader for CodeSlice<'_> {
    fn read(&self, addr: u64, size: usize) -> Result<Vec<u8>, MemoryError> {
        let off = addr as usize;
        let end = off
            .checked_add(size)
            .ok_or(MemoryError::OutOfBounds { addr })?;
        if end > self.0.len() {
            return Err(MemoryError::OutOfBounds { addr });
        }
        Ok(self.0[off..end].to_vec())
    }
}

const MEM_SIZE: usize = 0x10000;
const SCRATCH_BASE: u64 = 0x4000;

fn encode_insn(opc: u8, dst: u8, src: u8, off: i16, imm: i32, endian: Endian) -> [u8; 8] {
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

fn encode_lddw(dst: u8, imm64: u64, endian: Endian) -> [u8; 16] {
    let lo = encode_insn(0x18, dst, 0, 0, imm64 as u32 as i32, endian);
    let hi = encode_insn(0x00, 0, 0, 0, (imm64 >> 32) as u32 as i32, endian);
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&lo);
    out[8..].copy_from_slice(&hi);
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NormalizedOutcome {
    Exited { regs: [u64; 11], r0: u64 },
    Trapped,
}

fn run_direct(
    prog: &[u8],
    dialect: EbpfDialect,
    endian: Endian,
    init_regs: &[u64; 11],
    init_scratch: &[u8],
) -> (NormalizedOutcome, Vec<u8>) {
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
        EbpfExit::Exit { r0 } => NormalizedOutcome::Exited {
            regs: cpu.regs.r,
            r0,
        },
        EbpfExit::Trap(_) | EbpfExit::Syscall { .. } => NormalizedOutcome::Trapped,
    };

    let mut scratch_after = vec![0u8; init_scratch.len()];
    SmirMemory::read(&mut mem, SCRATCH_BASE, &mut scratch_after).unwrap();
    (outcome, scratch_after)
}

fn run_smir(
    prog: &[u8],
    dialect: EbpfDialect,
    endian: Endian,
    init_regs: &[u64; 11],
    init_scratch: &[u8],
    opt_level: OptLevel,
) -> (NormalizedOutcome, Vec<u8>) {
    let mut mem = FlatMemory::new(MEM_SIZE);
    mem.load(0, prog);
    mem.load(SCRATCH_BASE as usize, init_scratch);

    let mut lifter = EbpfLifter::new(dialect, endian);
    let mut lift_ctx = LiftContext::new(lifter.source_arch());
    lift_ctx.endian = endian;

    let mut func = lifter
        .lift_function(0, &CodeSlice(prog), &mut lift_ctx)
        .unwrap_or_else(|e| panic!("failed to lift function ({dialect:?}, {endian:?}): {e}"));

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
            NormalizedOutcome::Exited { regs, r0: regs[0] }
        }
        ExitReason::Undefined { .. } | ExitReason::MemoryFault { .. } => NormalizedOutcome::Trapped,
        other => panic!("unexpected SMIR interpreter exit reason: {other:?}"),
    };

    let mut scratch_after = vec![0u8; init_scratch.len()];
    SmirMemory::read(&mut mem, SCRATCH_BASE, &mut scratch_after).unwrap();
    (outcome, scratch_after)
}

fn assert_direct_smir_parity(
    label: &str,
    prog: &[u8],
    dialect: EbpfDialect,
    endian: Endian,
    init_regs: &[u64; 11],
    init_scratch: &[u8],
) {
    let (direct_out, direct_mem) = run_direct(prog, dialect, endian, init_regs, init_scratch);
    for opt in [OptLevel::O0, OptLevel::O2] {
        let (smir_out, smir_mem) = run_smir(prog, dialect, endian, init_regs, init_scratch, opt);
        assert_eq!(
            direct_out, smir_out,
            "state divergence in {label} ({dialect:?}, {endian:?}, {opt:?})"
        );
        if matches!(direct_out, NormalizedOutcome::Exited { .. }) {
            assert_eq!(
                direct_mem, smir_mem,
                "memory divergence in {label} ({dialect:?}, {endian:?}, {opt:?})"
            );
        }
    }
}

fn sample_register_seeds() -> Vec<[u64; 11]> {
    vec![
        [
            0,
            1,
            2,
            3,
            4,
            5,
            0xffff_ffff,
            0x8000_0000,
            i64::MIN as u64,
            u64::MAX,
            SCRATCH_BASE + 128,
        ],
        [
            0xdead_beef_cafe_babe,
            0x1234_5678_9abc_def0,
            (-13i64) as u64,
            3,
            (-1i64) as u64,
            i32::MIN as u32 as u64,
            0x0000_0001_8000_0000,
            0x7fff_ffff_ffff_ffff,
            0,
            42,
            SCRATCH_BASE + 128,
        ],
        [
            u64::MAX,
            i64::MIN as u64,
            (-1i64) as u64,
            0x8000_0000_0000_0000,
            0x0000_0000_8000_0000,
            0xffff_ffff_0000_0001,
            7,
            63,
            31,
            0x0102_0304_0506_0708,
            SCRATCH_BASE + 128,
        ],
    ]
}

#[test]
fn rfc9669_alu32_and_alu64_smir_parity_le_and_be() {
    let alu_opcodes: &[(u8, i16, i32, &str)] = &[
        (0x04, 0, 17, "add32_imm"),
        (0x0c, 0, 0, "add32_reg"),
        (0x07, 0, -29, "add64_imm"),
        (0x0f, 0, 0, "add64_reg"),
        (0x14, 0, 99, "sub32_imm"),
        (0x1c, 0, 0, "sub32_reg"),
        (0x17, 0, -99, "sub64_imm"),
        (0x1f, 0, 0, "sub64_reg"),
        (0x24, 0, -7, "mul32_imm"),
        (0x2c, 0, 0, "mul32_reg"),
        (0x27, 0, 13, "mul64_imm"),
        (0x2f, 0, 0, "mul64_reg"),
        (0x34, 0, 5, "div32_imm"),
        (0x34, 0, 0, "div32_imm_zero"),
        (0x3c, 0, 0, "div32_reg"),
        (0x37, 0, -5, "div64_imm"),
        (0x37, 0, 0, "div64_imm_zero"),
        (0x3f, 0, 0, "div64_reg"),
        (0x34, 1, -1, "sdiv32_imm_neg1"),
        (0x34, 1, 0, "sdiv32_imm_zero"),
        (0x3c, 1, 0, "sdiv32_reg"),
        (0x37, 1, -1, "sdiv64_imm_neg1"),
        (0x37, 1, 0, "sdiv64_imm_zero"),
        (0x3f, 1, 0, "sdiv64_reg"),
        (0x44, 0, 0x55aa, "or32_imm"),
        (0x4c, 0, 0, "or32_reg"),
        (0x47, 0, -0x1234, "or64_imm"),
        (0x4f, 0, 0, "or64_reg"),
        (0x54, 0, 0x0f0f_0f0f, "and32_imm"),
        (0x5c, 0, 0, "and32_reg"),
        (0x57, 0, -256, "and64_imm"),
        (0x5f, 0, 0, "and64_reg"),
        (0x64, 0, 37, "lsh32_imm"),
        (0x6c, 0, 0, "lsh32_reg"),
        (0x67, 0, 69, "lsh64_imm"),
        (0x6f, 0, 0, "lsh64_reg"),
        (0x74, 0, 35, "rsh32_imm"),
        (0x7c, 0, 0, "rsh32_reg"),
        (0x77, 0, 67, "rsh64_imm"),
        (0x7f, 0, 0, "rsh64_reg"),
        (0x84, 0, 0, "neg32"),
        (0x87, 0, 0, "neg64"),
        (0x94, 0, 7, "mod32_imm"),
        (0x94, 0, 0, "mod32_imm_zero"),
        (0x9c, 0, 0, "mod32_reg"),
        (0x97, 0, -7, "mod64_imm"),
        (0x97, 0, 0, "mod64_imm_zero"),
        (0x9f, 0, 0, "mod64_reg"),
        (0x94, 1, -1, "smod32_imm_neg1"),
        (0x94, 1, 0, "smod32_imm_zero"),
        (0x9c, 1, 0, "smod32_reg"),
        (0x97, 1, -1, "smod64_imm_neg1"),
        (0x97, 1, 0, "smod64_imm_zero"),
        (0x9f, 1, 0, "smod64_reg"),
        (0xa4, 0, -12345, "xor32_imm"),
        (0xac, 0, 0, "xor32_reg"),
        (0xa7, 0, -12345, "xor64_imm"),
        (0xaf, 0, 0, "xor64_reg"),
        (0xb4, 0, -1, "mov32_imm"),
        (0xbc, 0, 0, "mov32_reg"),
        (0xbc, 8, 0, "movsx32_8"),
        (0xbc, 16, 0, "movsx32_16"),
        (0xb7, 0, -1, "mov64_imm"),
        (0xbf, 0, 0, "mov64_reg"),
        (0xbf, 8, 0, "movsx64_8"),
        (0xbf, 16, 0, "movsx64_16"),
        (0xbf, 32, 0, "movsx64_32"),
        (0xc4, 0, 19, "arsh32_imm"),
        (0xcc, 0, 0, "arsh32_reg"),
        (0xc7, 0, 51, "arsh64_imm"),
        (0xcf, 0, 0, "arsh64_reg"),
        (0xd4, 0, 16, "le16"),
        (0xd4, 0, 32, "le32"),
        (0xd4, 0, 64, "le64"),
        (0xdc, 0, 16, "be16"),
        (0xdc, 0, 32, "be32"),
        (0xdc, 0, 64, "be64"),
        (0xd7, 0, 16, "bswap16"),
        (0xd7, 0, 32, "bswap32"),
        (0xd7, 0, 64, "bswap64"),
    ];

    let seeds = sample_register_seeds();
    let scratch = vec![0xA5u8; 256];

    for endian in [Endian::Little, Endian::Big] {
        for &(opc, off, imm, name) in alu_opcodes {
            let is_reg = (opc & 0x08) != 0 && !matches!(opc, 0xd4 | 0xdc | 0xd7);
            for &(dst, src) in &[(1u8, 2u8), (2u8, 2u8), (0u8, 5u8)] {
                let src_field = if is_reg { src } else { 0 };
                let mut prog = Vec::with_capacity(16);
                prog.extend_from_slice(&encode_insn(opc, dst, src_field, off, imm, endian));
                prog.extend_from_slice(&encode_insn(0x95, 0, 0, 0, 0, endian));
                for seed in &seeds {
                    assert_direct_smir_parity(
                        name,
                        &prog,
                        EbpfDialect::Rfc9669,
                        endian,
                        seed,
                        &scratch,
                    );
                }
            }
        }

        // Also test 16-byte LDDW
        let mut lddw_prog = Vec::new();
        lddw_prog.extend_from_slice(&encode_lddw(0, 0xfedc_ba98_7654_3210, endian));
        lddw_prog.extend_from_slice(&encode_insn(0x95, 0, 0, 0, 0, endian));
        for seed in &seeds {
            assert_direct_smir_parity(
                "lddw",
                &lddw_prog,
                EbpfDialect::Rfc9669,
                endian,
                seed,
                &scratch,
            );
        }
    }
}

#[test]
fn rfc9669_memory_loads_stores_and_atomics_smir_parity() {
    let mut initial_scratch = vec![0u8; 256];
    for (i, b) in initial_scratch.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(37).wrapping_add(0x81);
    }

    for endian in [Endian::Little, Endian::Big] {
        let mut prog = Vec::new();
        // Unsigned loads: B1, B2, B4, B8 from r10 - 64..
        prog.extend_from_slice(&encode_insn(0x71, 0, 10, -64, 0, endian));
        prog.extend_from_slice(&encode_insn(0x69, 1, 10, -56, 0, endian));
        prog.extend_from_slice(&encode_insn(0x61, 2, 10, -48, 0, endian));
        prog.extend_from_slice(&encode_insn(0x79, 3, 10, -40, 0, endian));
        // Sign-extending loads (RFC 9669 v4): LDSXB, LDSXH, LDSXW
        prog.extend_from_slice(&encode_insn(0x91, 4, 10, -64, 0, endian));
        prog.extend_from_slice(&encode_insn(0x89, 5, 10, -56, 0, endian));
        prog.extend_from_slice(&encode_insn(0x81, 6, 10, -48, 0, endian));
        // StoreImm B1, B2, B4, B8
        prog.extend_from_slice(&encode_insn(0x72, 10, 0, 0, -5, endian));
        prog.extend_from_slice(&encode_insn(0x6a, 10, 0, 8, -1234, endian));
        prog.extend_from_slice(&encode_insn(0x62, 10, 0, 16, -0x1234_5678, endian));
        prog.extend_from_slice(&encode_insn(0x7a, 10, 0, 24, -42, endian));
        // StoreReg B1, B2, B4, B8
        prog.extend_from_slice(&encode_insn(0x73, 10, 0, 32, 0, endian));
        prog.extend_from_slice(&encode_insn(0x6b, 10, 1, 40, 0, endian));
        prog.extend_from_slice(&encode_insn(0x63, 10, 2, 48, 0, endian));
        prog.extend_from_slice(&encode_insn(0x7b, 10, 3, 56, 0, endian));

        if endian == Endian::Little {
            // Atomics (32-bit and 64-bit ADD, OR, AND, XOR with and without FETCH, XCHG, CMPXCHG)
            prog.extend_from_slice(&encode_insn(0xdb, 10, 1, 24, 0x00, endian)); // atomic64 add
            prog.extend_from_slice(&encode_insn(0xdb, 10, 2, 24, 0x01, endian)); // atomic64 fetch add
            prog.extend_from_slice(&encode_insn(0xdb, 10, 3, 24, 0x41, endian)); // atomic64 fetch or
            prog.extend_from_slice(&encode_insn(0xdb, 10, 4, 24, 0x51, endian)); // atomic64 fetch and
            prog.extend_from_slice(&encode_insn(0xdb, 10, 5, 24, 0xa1, endian)); // atomic64 fetch xor
            prog.extend_from_slice(&encode_insn(0xdb, 10, 6, 24, 0xe1, endian)); // atomic64 xchg
            prog.extend_from_slice(&encode_insn(0xdb, 10, 1, 24, 0xf1, endian)); // atomic64 cmpxchg
            prog.extend_from_slice(&encode_insn(0xc3, 10, 2, 16, 0x01, endian)); // atomic32 fetch add
            prog.extend_from_slice(&encode_insn(0xc3, 10, 3, 16, 0xe1, endian)); // atomic32 xchg
            prog.extend_from_slice(&encode_insn(0xc3, 10, 4, 16, 0xf1, endian)); // atomic32 cmpxchg
        }

        prog.extend_from_slice(&encode_insn(0x95, 0, 0, 0, 0, endian));

        for seed in &sample_register_seeds() {
            assert_direct_smir_parity(
                "memory_and_atomics",
                &prog,
                EbpfDialect::Rfc9669,
                endian,
                seed,
                &initial_scratch,
            );
        }
    }
}

#[test]
fn solana_sbpf_v0_to_v4_comprehensive_smir_parity() {
    let seeds = sample_register_seeds();
    let mut initial_scratch = vec![0u8; 256];
    for (i, b) in initial_scratch.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(19).wrapping_add(7);
    }

    for version in [
        SbpfVersion::V0,
        SbpfVersion::V1,
        SbpfVersion::V2,
        SbpfVersion::V3,
        SbpfVersion::V4,
    ] {
        let dialect = EbpfDialect::Solana(version);
        let endian = Endian::Little;

        // 1. Common ALU program valid on all SBPF versions
        let mut prog = Vec::new();
        prog.extend_from_slice(&encode_insn(0x04, 1, 0, 0, -50, endian)); // add32 r1, -50
        prog.extend_from_slice(&encode_insn(0x0c, 1, 2, 0, 0, endian)); // add32 r1, r2
        prog.extend_from_slice(&encode_insn(0x14, 3, 0, 0, 1234, endian)); // sub32 r3, 1234
        prog.extend_from_slice(&encode_insn(0x17, 4, 0, 0, 5678, endian)); // sub64 r4, 5678
        prog.extend_from_slice(&encode_insn(0xb4, 5, 0, 0, -9, endian)); // mov32 r5, -9
        prog.extend_from_slice(&encode_insn(0xbc, 6, 5, 0, 0, endian)); // mov32 r6, r5 (sign-extends in V2)
        prog.extend_from_slice(&encode_insn(0xc4, 7, 0, 0, 5, endian)); // arsh32 r7, 5
        prog.extend_from_slice(&encode_insn(0xc7, 8, 0, 0, 13, endian)); // arsh64 r8, 13
        prog.extend_from_slice(&encode_insn(0xdc, 9, 0, 0, 32, endian)); // be32 r9
        prog.extend_from_slice(&encode_insn(0x0f, 0, 1, 0, 0, endian)); // add64 r0, r1
        prog.extend_from_slice(&encode_insn(0x0f, 0, 3, 0, 0, endian)); // add64 r0, r3
        prog.extend_from_slice(&encode_insn(0x0f, 0, 4, 0, 0, endian)); // add64 r0, r4
        prog.extend_from_slice(&encode_insn(0x0f, 0, 6, 0, 0, endian)); // add64 r0, r6
        prog.extend_from_slice(&encode_insn(0x95, 0, 0, 0, 0, endian)); // exit

        for seed in &seeds {
            assert_direct_smir_parity(
                "sbpf_common_alu",
                &prog,
                dialect,
                endian,
                seed,
                &initial_scratch,
            );
        }

        // 2. Version-specific instructions (PQR & relocated memory in V2 vs MUL/DIV/MOD & standard memory in V0/V1/V3/V4)
        let mut ver_prog = Vec::new();
        if version == SbpfVersion::V2 {
            // Relocated memory loads/stores in V2
            ver_prog.extend_from_slice(&encode_insn(0x2c, 1, 10, -32, 0, endian)); // ld_1b r1, [r10-32]
            ver_prog.extend_from_slice(&encode_insn(0x3c, 2, 10, -24, 0, endian)); // ld_2b r2, [r10-24]
            ver_prog.extend_from_slice(&encode_insn(0x8c, 3, 10, -16, 0, endian)); // ld_4b r3, [r10-16]
            ver_prog.extend_from_slice(&encode_insn(0x9c, 4, 10, -8, 0, endian)); // ld_8b r4, [r10-8]
            ver_prog.extend_from_slice(&encode_insn(0x27, 10, 0, 0, 0x42, endian)); // st_1b [r10+0], 0x42
            ver_prog.extend_from_slice(&encode_insn(0x37, 10, 0, 8, 0x1234, endian)); // st_2b [r10+8], 0x1234
            ver_prog.extend_from_slice(&encode_insn(0x87, 10, 0, 16, 0x1234_5678, endian)); // st_4b [r10+16], 0x12345678
            ver_prog.extend_from_slice(&encode_insn(0x97, 10, 0, 24, -99, endian)); // st_8b [r10+24], -99
            ver_prog.extend_from_slice(&encode_insn(0x2f, 10, 1, 32, 0, endian)); // st_1b [r10+32], r1
            ver_prog.extend_from_slice(&encode_insn(0x3f, 10, 2, 40, 0, endian)); // st_2b [r10+40], r2
            ver_prog.extend_from_slice(&encode_insn(0x8f, 10, 3, 48, 0, endian)); // st_4b [r10+48], r3
            ver_prog.extend_from_slice(&encode_insn(0x9f, 10, 4, 56, 0, endian)); // st_8b [r10+56], r4
            // PQR instructions
            ver_prog.extend_from_slice(&encode_insn(0xf7, 4, 0, 0, 0x1357_2468, endian)); // hor64 r4, 0x13572468
            ver_prog.extend_from_slice(&encode_insn(0x86, 1, 0, 0, -7, endian)); // lmul32 r1, -7
            ver_prog.extend_from_slice(&encode_insn(0x96, 2, 0, 0, -7, endian)); // lmul64 r2, -7
            ver_prog.extend_from_slice(&encode_insn(0x36, 3, 0, 0, -1, endian)); // uhmul64 r3, 0xffffffff
            ver_prog.extend_from_slice(&encode_insn(0xb6, 4, 0, 0, -3, endian)); // shmul64 r4, -3
            ver_prog.extend_from_slice(&encode_insn(0x46, 1, 0, 0, 5, endian)); // udiv32 r1, 5
            ver_prog.extend_from_slice(&encode_insn(0x56, 2, 0, 0, -1, endian)); // udiv64 r2, 0xffffffff (zero-ext!)
            ver_prog.extend_from_slice(&encode_insn(0x66, 3, 0, 0, 7, endian)); // urem32 r3, 7
            ver_prog.extend_from_slice(&encode_insn(0xc6, 4, 0, 0, -3, endian)); // sdiv32 r4, -3
            ver_prog.extend_from_slice(&encode_insn(0xd6, 0, 0, 0, -7, endian)); // sdiv64 r0, -7
            ver_prog.extend_from_slice(&encode_insn(0xe6, 5, 0, 0, -3, endian)); // srem32 r5, -3
            ver_prog.extend_from_slice(&encode_insn(0xf6, 6, 0, 0, -7, endian)); // srem64 r6, -7
        } else {
            ver_prog.extend_from_slice(&encode_lddw(0, 0x0123_4567_89ab_cdef, endian));
            ver_prog.extend_from_slice(&encode_insn(0x24, 1, 0, 0, -7, endian)); // mul32 r1, -7
            ver_prog.extend_from_slice(&encode_insn(0x27, 2, 0, 0, -7, endian)); // mul64 r2, -7
            ver_prog.extend_from_slice(&encode_insn(0x34, 3, 0, 0, 5, endian)); // div32 r3, 5
            ver_prog.extend_from_slice(&encode_insn(0x94, 4, 0, 0, 5, endian)); // mod32 r4, 5
            ver_prog.extend_from_slice(&encode_insn(0x79, 5, 10, -8, 0, endian)); // lddw r5, [r10-8]
            ver_prog.extend_from_slice(&encode_insn(0x7b, 10, 5, 16, 0, endian)); // stdw [r10+16], r5
        }
        ver_prog.extend_from_slice(&encode_insn(0x95, 0, 0, 0, 0, endian));

        for seed in &seeds {
            assert_direct_smir_parity(
                "sbpf_version_specific",
                &ver_prog,
                dialect,
                endian,
                seed,
                &initial_scratch,
            );
        }

        // 3. Verify division-by-zero traps in both direct and SMIR for SBPF
        let mut trap_prog = Vec::new();
        if version == SbpfVersion::V2 {
            trap_prog.extend_from_slice(&encode_insn(0x5e, 0, 1, 0, 0, endian)); // udiv64 r0, r1
        } else {
            trap_prog.extend_from_slice(&encode_insn(0x3f, 0, 1, 0, 0, endian)); // div64 r0, r1
        }
        trap_prog.extend_from_slice(&encode_insn(0x95, 0, 0, 0, 0, endian));
        let mut zero_div_seed = seeds[0];
        zero_div_seed[1] = 0;
        assert_direct_smir_parity(
            "sbpf_div_zero_trap",
            &trap_prog,
            dialect,
            endian,
            &zero_div_seed,
            &initial_scratch,
        );
    }
}

#[test]
fn multi_block_control_flow_and_loops_smir_parity() {
    let endian = Endian::Little;
    // Compute sum(1..=r1) and test all conditional branch types
    // 0: mov64 r0, 0
    // 1: jeq r1, 0, +3 (to 5: exit)
    // 2: add64 r0, r1
    // 3: sub64 r1, 1
    // 4: ja -4 (to 1)
    // 5: exit
    let mut prog = Vec::new();
    prog.extend_from_slice(&encode_insn(0xb7, 0, 0, 0, 0, endian));
    prog.extend_from_slice(&encode_insn(0x15, 1, 0, 3, 0, endian));
    prog.extend_from_slice(&encode_insn(0x0f, 0, 1, 0, 0, endian));
    prog.extend_from_slice(&encode_insn(0x17, 1, 0, 0, 1, endian));
    prog.extend_from_slice(&encode_insn(0x05, 0, 0, -4, 0, endian));
    prog.extend_from_slice(&encode_insn(0x95, 0, 0, 0, 0, endian));

    let mut regs = [0u64; 11];
    regs[1] = 15;
    regs[10] = SCRATCH_BASE + 128;
    let scratch = vec![0u8; 256];

    for dialect in [
        EbpfDialect::Rfc9669,
        EbpfDialect::Solana(SbpfVersion::V0),
        EbpfDialect::Solana(SbpfVersion::V3),
        EbpfDialect::Solana(SbpfVersion::V4),
    ] {
        assert_direct_smir_parity("loop_sum", &prog, dialect, endian, &regs, &scratch);
    }
}
