//! RIP-relative memory operands of legacy-encoded SSE and MMX instructions
//! that end in an imm8.
//!
//! In 64-bit mode ModR/M `mod=00 rm=101` addresses `RIP + disp32`, where RIP
//! is the address of the *next* instruction (Intel SDM Vol. 2A section
//! 2.2.1.6), so the imm8 after the displacement is part of the distance. In
//! the legacy maps every 0F3A opcode and 0F 70, C2, C4, and C6 have such an
//! imm8. Each case places its operand at an address computed from the
//! complete encoded length, surrounded by distinct bytes, so an operand read
//! or written one byte early or late produces a different value. Encodings
//! were produced by `llvm-mc -triple=x86_64 -show-encoding`.

use crate::common::*;
use rax::vm::vcpu::Registers;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

/// 64-byte-aligned operand address.
const TARGET: u64 = 0x3000;

/// Bytes 0xA0..=0xAF as XMM qwords, an initial value distinct from the ramp.
const BYTES_A0: [u64; 2] = [0xA7A6_A5A4_A3A2_A1A0, 0xAFAE_ADAC_ABAA_A9A8];

/// `head` runs through the ModR/M byte (`mod=00 rm=101`); `imm` is the
/// trailing immediate. The disp32 is chosen so the operand is `TARGET`.
fn rip_code(head: &[u8], imm: u8) -> Vec<u8> {
    let len = (head.len() + 4 + 1) as u64;
    let disp = TARGET.wrapping_sub(CODE_ADDR + len) as u32;
    let mut code = head.to_vec();
    code.extend_from_slice(&disp.to_le_bytes());
    code.push(imm);
    code.push(0xF4); // HLT
    code
}

/// Runs `code` with the 0x40.. byte ramp around `TARGET` (bytes 0x3F and
/// 0x80 flank it, so a one-byte shift in either direction is visible).
fn run_with_memory(code: &[u8], initial: Registers) -> (Registers, Arc<GuestMemoryMmap>) {
    let (mut vcpu, mem) = setup_vm(code, Some(initial));
    let ramp: Vec<u8> = (0x3F..=0x80).collect();
    mem.write_slice(&ramp, GuestAddress(TARGET - 1)).unwrap();
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(
        regs.rip,
        CODE_ADDR + code.len() as u64,
        "{code:02X?}: stopped at the HLT"
    );
    (regs, mem)
}

fn run(code: &[u8], initial: Registers) -> Registers {
    run_with_memory(code, initial).0
}

/// The ramp qword starting `offset` bytes past `TARGET`.
fn ramp64(offset: u8) -> u64 {
    (0..8).fold(0, |acc, i| acc | (u64::from(0x40 + offset + i) << (8 * i)))
}

/// The 16-byte ramp chunk starting `offset` bytes past `TARGET`, as XMM qwords.
fn ramp128(offset: u8) -> [u64; 2] {
    [ramp64(offset), ramp64(offset + 8)]
}

fn with_xmm(index: usize, value: [u64; 2]) -> Registers {
    let mut regs = Registers::default();
    regs.xmm[index] = value;
    regs
}

/// Asserts that `TARGET - 1 .. TARGET + stored.len() + 1` holds 0x3F, the
/// stored bytes, then the untouched ramp byte that followed them.
fn assert_stored(mem: &GuestMemoryMmap, stored: &[u8], code: &[u8]) {
    let mut window = vec![0; stored.len() + 2];
    mem.read_slice(&mut window, GuestAddress(TARGET - 1))
        .unwrap();
    let mut want = vec![0x3F];
    want.extend_from_slice(stored);
    want.push(0x40 + stored.len() as u8);
    assert_eq!(window, want, "{code:02X?}");
}

#[test]
fn pinsr_loads_from_rip_relative_operand() {
    // PINSRB xmm1, [rip+disp32], 15 - 66 0F 3A 20 /r ib
    let code = rip_code(&[0x66, 0x0F, 0x3A, 0x20, 0x0D], 15);
    let regs = run(&code, with_xmm(1, BYTES_A0));
    assert_eq!(regs.xmm[1], [BYTES_A0[0], 0x40AE_ADAC_ABAA_A9A8]);

    // PINSRW xmm2, [rip+disp32], 7 - 66 0F C4 /r ib
    let code = rip_code(&[0x66, 0x0F, 0xC4, 0x15], 7);
    let regs = run(&code, with_xmm(2, BYTES_A0));
    assert_eq!(regs.xmm[2], [BYTES_A0[0], 0x4140_ADAC_ABAA_A9A8]);

    // PINSRD xmm3, [rip+disp32], 3 - 66 0F 3A 22 /r ib
    let code = rip_code(&[0x66, 0x0F, 0x3A, 0x22, 0x1D], 3);
    let regs = run(&code, with_xmm(3, BYTES_A0));
    assert_eq!(regs.xmm[3], [BYTES_A0[0], 0x4342_4140_ABAA_A9A8]);

    // PINSRQ xmm0, [rip+disp32], 1 - 66 REX.W 0F 3A 22 /r ib: the
    // instruction that loads Pin's syscall-table pointers.
    let code = rip_code(&[0x66, 0x48, 0x0F, 0x3A, 0x22, 0x05], 1);
    let regs = run(&code, with_xmm(0, BYTES_A0));
    assert_eq!(regs.xmm[0], [BYTES_A0[0], ramp64(0)]);

    // PINSRQ xmm12, [rip+disp32], 0 - 66 REX.WR 0F 3A 22 /r ib
    let code = rip_code(&[0x66, 0x4C, 0x0F, 0x3A, 0x22, 0x25], 0);
    let regs = run(&code, with_xmm(12, BYTES_A0));
    assert_eq!(regs.xmm[12], [ramp64(0), BYTES_A0[1]]);
}

#[test]
fn mmx_pinsrw_loads_from_rip_relative_operand() {
    // PINSRW mm2, [rip+disp32], 3 - NP 0F C4 /r ib
    let code = rip_code(&[0x0F, 0xC4, 0x15], 3);
    let mut initial = Registers::default();
    initial.mm[2] = BYTES_A0[0];
    let regs = run(&code, initial);
    assert_eq!(regs.mm[2], 0x4140_A5A4_A3A2_A1A0);
}

#[test]
fn pextr_and_extractps_store_to_rip_relative_operand() {
    let source = with_xmm(1, BYTES_A0);

    // PEXTRB [rip+disp32], xmm1, 5 - 66 0F 3A 14 /r ib
    let code = rip_code(&[0x66, 0x0F, 0x3A, 0x14, 0x0D], 5);
    let (_, mem) = run_with_memory(&code, source.clone());
    assert_stored(&mem, &[0xA5], &code);

    // PEXTRW [rip+disp32], xmm1, 6 - 66 0F 3A 15 /r ib
    let code = rip_code(&[0x66, 0x0F, 0x3A, 0x15, 0x0D], 6);
    let (_, mem) = run_with_memory(&code, source.clone());
    assert_stored(&mem, &[0xAC, 0xAD], &code);

    // PEXTRD [rip+disp32], xmm1, 2 - 66 0F 3A 16 /r ib
    let code = rip_code(&[0x66, 0x0F, 0x3A, 0x16, 0x0D], 2);
    let (_, mem) = run_with_memory(&code, source.clone());
    assert_stored(&mem, &[0xA8, 0xA9, 0xAA, 0xAB], &code);

    // EXTRACTPS [rip+disp32], xmm1, 3 - 66 0F 3A 17 /r ib
    let code = rip_code(&[0x66, 0x0F, 0x3A, 0x17, 0x0D], 3);
    let (_, mem) = run_with_memory(&code, source);
    assert_stored(&mem, &[0xAC, 0xAD, 0xAE, 0xAF], &code);

    // PEXTRQ [rip+disp32], xmm9, 1 - 66 REX.WR 0F 3A 16 /r ib
    let code = rip_code(&[0x66, 0x4C, 0x0F, 0x3A, 0x16, 0x0D], 1);
    let (_, mem) = run_with_memory(&code, with_xmm(9, BYTES_A0));
    assert_stored(
        &mem,
        &[0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xAF],
        &code,
    );
}

#[test]
fn pshuf_sources_from_rip_relative_operand() {
    // PSHUFD xmm3, [rip+disp32], 0xE4 (identity) - 66 0F 70 /r ib, and the
    // REX.R form reaching xmm11.
    for (head, dst) in [
        (&[0x66, 0x0F, 0x70, 0x1D][..], 3),
        (&[0x66, 0x44, 0x0F, 0x70, 0x1D][..], 11),
    ] {
        let code = rip_code(head, 0xE4);
        let regs = run(&code, Registers::default());
        assert_eq!(regs.xmm[dst], ramp128(0), "{code:02X?}");
    }

    // PSHUFHW xmm4, [rip+disp32], 0x1B - F3 0F 70 /r ib: reverses the high
    // words, copies the low qword.
    let code = rip_code(&[0xF3, 0x0F, 0x70, 0x25], 0x1B);
    let regs = run(&code, Registers::default());
    assert_eq!(regs.xmm[4], [ramp64(0), 0x4948_4B4A_4D4C_4F4E]);

    // PSHUFLW xmm5, [rip+disp32], 0x1B - F2 0F 70 /r ib: reverses the low
    // words, copies the high qword.
    let code = rip_code(&[0xF2, 0x0F, 0x70, 0x2D], 0x1B);
    let regs = run(&code, Registers::default());
    assert_eq!(regs.xmm[5], [0x4140_4342_4544_4746, ramp64(8)]);

    // PSHUFW mm3, [rip+disp32], 0x1B - NP 0F 70 /r ib
    let code = rip_code(&[0x0F, 0x70, 0x1D], 0x1B);
    let regs = run(&code, Registers::default());
    assert_eq!(regs.mm[3], 0x4140_4342_4544_4746);
}

#[test]
fn shufp_sources_from_rip_relative_operand() {
    // SHUFPS xmm5, [rip+disp32], 0x44 - NP 0F C6 /r ib:
    // dwords {xmm5[0], xmm5[1], m[0], m[1]}.
    let code = rip_code(&[0x0F, 0xC6, 0x2D], 0x44);
    let regs = run(&code, with_xmm(5, BYTES_A0));
    assert_eq!(regs.xmm[5], [BYTES_A0[0], ramp64(0)]);

    // SHUFPD xmm6, [rip+disp32], 2 - 66 0F C6 /r ib: {xmm6[0], m[1]}.
    let code = rip_code(&[0x66, 0x0F, 0xC6, 0x35], 2);
    let regs = run(&code, with_xmm(6, BYTES_A0));
    assert_eq!(regs.xmm[6], [BYTES_A0[0], ramp64(8)]);
}

#[test]
fn cmp_sources_from_rip_relative_operand() {
    // The ramp's dwords and qwords are normal floating-point values, so an
    // equal compare is a bitwise compare here.
    let [lo, hi] = ramp128(0);

    // CMPEQPS xmm7, [rip+disp32] - NP 0F C2 /r ib (imm 0): dword 2 differs.
    let code = rip_code(&[0x0F, 0xC2, 0x3D], 0);
    let regs = run(&code, with_xmm(7, [lo, hi ^ 1]));
    assert_eq!(regs.xmm[7], [u64::MAX, 0xFFFF_FFFF_0000_0000]);

    // CMPNEQPD xmm7, [rip+disp32] - 66 0F C2 /r ib (imm 4): qword 1 differs.
    let code = rip_code(&[0x66, 0x0F, 0xC2, 0x3D], 4);
    let regs = run(&code, with_xmm(7, [lo, hi ^ 1]));
    assert_eq!(regs.xmm[7], [0, u64::MAX]);

    // CMPEQSS xmm7, [rip+disp32] - F3 0F C2 /r ib (imm 0): bits 127:32 kept.
    let code = rip_code(&[0xF3, 0x0F, 0xC2, 0x3D], 0);
    let initial = [0xDEAD_BEEF_0000_0000 | (lo & 0xFFFF_FFFF), BYTES_A0[1]];
    let regs = run(&code, with_xmm(7, initial));
    assert_eq!(regs.xmm[7], [0xDEAD_BEEF_FFFF_FFFF, BYTES_A0[1]]);

    // CMPNEQSD xmm7, [rip+disp32] - F2 0F C2 /r ib (imm 4): bits 127:64 kept.
    let code = rip_code(&[0xF2, 0x0F, 0xC2, 0x3D], 4);
    let regs = run(&code, with_xmm(7, [lo, BYTES_A0[1]]));
    assert_eq!(regs.xmm[7], [0, BYTES_A0[1]]);
}

#[test]
fn palignr_sources_from_rip_relative_operand() {
    // PALIGNR xmm2, [rip+disp32], 0 - 66 0F 3A 0F /r ib: the memory operand.
    let code = rip_code(&[0x66, 0x0F, 0x3A, 0x0F, 0x15], 0);
    let regs = run(&code, with_xmm(2, BYTES_A0));
    assert_eq!(regs.xmm[2], ramp128(0));

    // PALIGNR xmm2, [rip+disp32], 15: xmm2:m128 shifted right 15 bytes is
    // m[15] followed by xmm2 bytes 0..=14.
    let code = rip_code(&[0x66, 0x0F, 0x3A, 0x0F, 0x15], 15);
    let regs = run(&code, with_xmm(2, BYTES_A0));
    assert_eq!(regs.xmm[2], [0xA6A5_A4A3_A2A1_A04F, 0xAEAD_ACAB_AAA9_A8A7]);

    // PALIGNR mm4, [rip+disp32], 3 - NP 0F 3A 0F /r ib: m64 bytes 3..=7
    // followed by mm4 bytes 0..=2.
    let code = rip_code(&[0x0F, 0x3A, 0x0F, 0x25], 3);
    let mut initial = Registers::default();
    initial.mm[4] = 0xB7B6_B5B4_B3B2_B1B0;
    let regs = run(&code, initial);
    assert_eq!(regs.mm[4], 0xB2B1_B047_4645_4443);
}

#[test]
fn insert_blend_round_source_from_rip_relative_operand() {
    // INSERTPS xmm1, [rip+disp32], 0x30 - 66 0F 3A 21 /r ib: m32 into dword 3.
    let code = rip_code(&[0x66, 0x0F, 0x3A, 0x21, 0x0D], 0x30);
    let regs = run(&code, with_xmm(1, BYTES_A0));
    assert_eq!(regs.xmm[1], [BYTES_A0[0], 0x4342_4140_ABAA_A9A8]);

    // BLENDPS xmm1, [rip+disp32], 5 - 66 0F 3A 0C /r ib: dwords 0 and 2 from m.
    let code = rip_code(&[0x66, 0x0F, 0x3A, 0x0C, 0x0D], 5);
    let regs = run(&code, with_xmm(1, BYTES_A0));
    assert_eq!(regs.xmm[1], [0xA7A6_A5A4_4342_4140, 0xAFAE_ADAC_4B4A_4948]);

    // PBLENDW xmm1, [rip+disp32], 0xA5 - 66 0F 3A 0E /r ib: words 0, 2, 5,
    // and 7 from m.
    let code = rip_code(&[0x66, 0x0F, 0x3A, 0x0E, 0x0D], 0xA5);
    let regs = run(&code, with_xmm(1, BYTES_A0));
    assert_eq!(regs.xmm[1], [0xA7A6_4544_A3A2_4140, 0x4F4E_ADAC_4B4A_A9A8]);

    // ROUNDSS xmm1, [rip+disp32], 0 - 66 0F 3A 0A /r ib: round to nearest.
    // m32 = 0x43424140 = 194.2548828125, so the result is 194.0 = 0x43420000;
    // bits 127:32 are kept.
    let code = rip_code(&[0x66, 0x0F, 0x3A, 0x0A, 0x0D], 0);
    let regs = run(&code, with_xmm(1, BYTES_A0));
    assert_eq!(regs.xmm[1], [0xA7A6_A5A4_4342_0000, BYTES_A0[1]]);
}
