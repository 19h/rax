//! RIP-relative memory operands of APX map-4 instructions that end in an
//! immediate.
//!
//! RIP-relative addressing is relative to the end of the instruction (Intel
//! SDM Vol. 2A section 2.2.1.6), and map 4 sizes its immediates per opcode:
//! Ib for 6B/80/83/C0/C1/F6, Iz (two bytes with a 16-bit operand size, else
//! four) for 69/81/F7. Each case places its operand at an address computed
//! from the complete encoded length, surrounded by distinct bytes, so an
//! operand one or more bytes early or late produces a different result.
//! Encodings were produced by `llvm-mc -triple=x86_64
//! -mattr=+egpr,+ndd,+nf,+ccmp -show-encoding`.

use crate::common::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const TARGET: u64 = 0x3000;

/// `head` runs through the ModR/M byte (`mod=00 rm=101`); `imm` is the
/// trailing immediate. The disp32 is chosen so the operand is `TARGET`.
fn rip_code(head: &[u8], imm: &[u8]) -> Vec<u8> {
    let len = (head.len() + 4 + imm.len()) as u64;
    let disp = TARGET.wrapping_sub(CODE_ADDR + len) as u32;
    let mut code = head.to_vec();
    code.extend_from_slice(&disp.to_le_bytes());
    code.extend_from_slice(imm);
    code.push(0xF4); // HLT
    code
}

/// Runs `code` with the 0x40.. byte ramp around `TARGET` (0x3F before it).
fn run(code: &[u8], initial: Registers) -> (Registers, Arc<GuestMemoryMmap>) {
    let (mut vcpu, mem) = setup_apx_vm(code, Some(initial));
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

/// The bytes `TARGET - 1 .. TARGET + n + 1`.
fn window(mem: &GuestMemoryMmap, n: usize) -> Vec<u8> {
    let mut bytes = vec![0; n + 2];
    mem.read_slice(&mut bytes, GuestAddress(TARGET - 1))
        .unwrap();
    bytes
}

#[test]
fn map4_group1_immediates_follow_the_operand() {
    // {evex} ADD byte [rip], 0x11 - Ib.
    let code = rip_code(&[0x62, 0xF4, 0x7C, 0x08, 0x80, 0x05], &[0x11]);
    let (_, mem) = run(&code, Registers::default());
    assert_eq!(window(&mem, 1), [0x3F, 0x51, 0x41]);

    // {evex} ADD word [rip], 0x1234 - Iz with EVEX.pp=01: an imm16.
    let code = rip_code(&[0x62, 0xF4, 0x7D, 0x08, 0x81, 0x05], &[0x34, 0x12]);
    let (_, mem) = run(&code, Registers::default());
    assert_eq!(window(&mem, 2), [0x3F, 0x74, 0x53, 0x42]);

    // {nf} ADD word [rip], 0x1234.
    let code = rip_code(&[0x62, 0xF4, 0x7D, 0x0C, 0x81, 0x05], &[0x34, 0x12]);
    let (_, mem) = run(&code, Registers::default());
    assert_eq!(window(&mem, 2), [0x3F, 0x74, 0x53, 0x42]);

    // {evex} ADD dword [rip], 0x01020304 - Iz: an imm32.
    let code = rip_code(
        &[0x62, 0xF4, 0x7C, 0x08, 0x81, 0x05],
        &[0x04, 0x03, 0x02, 0x01],
    );
    let (_, mem) = run(&code, Registers::default());
    assert_eq!(window(&mem, 4), [0x3F, 0x44, 0x44, 0x44, 0x44, 0x44]);

    // ADD ecx, dword [rip], 0x01020304 - the NDD form leaves memory alone.
    let code = rip_code(
        &[0x62, 0xF4, 0x74, 0x18, 0x81, 0x05],
        &[0x04, 0x03, 0x02, 0x01],
    );
    let (regs, mem) = run(&code, Registers::default());
    assert_eq!(regs.rcx, 0x4444_4444);
    assert_eq!(window(&mem, 4), [0x3F, 0x40, 0x41, 0x42, 0x43, 0x44]);
}

#[test]
fn map4_imul_and_shift_immediates_follow_the_operand() {
    // {evex} IMUL ax, word [rip], 0x0102 - 69 with an imm16.
    let code = rip_code(&[0x62, 0xF4, 0x7D, 0x08, 0x69, 0x05], &[0x02, 0x01]);
    let (regs, _) = run(&code, Registers::default());
    assert_eq!(regs.rax, 0xC280); // 0x4140 * 0x0102 mod 2^16

    // {evex} IMUL eax, dword [rip], 0x01020304 - 69 with an imm32.
    let code = rip_code(
        &[0x62, 0xF4, 0x7C, 0x08, 0x69, 0x05],
        &[0x04, 0x03, 0x02, 0x01],
    );
    let (regs, _) = run(&code, Registers::default());
    assert_eq!(regs.rax, 0x964C_C500); // 0x43424140 * 0x01020304 mod 2^32

    // {evex} IMUL eax, dword [rip], 3 - 6B with an imm8.
    let code = rip_code(&[0x62, 0xF4, 0x7C, 0x08, 0x6B, 0x05], &[0x03]);
    let (regs, _) = run(&code, Registers::default());
    assert_eq!(regs.rax, 0xC9C6_C3C0);

    // {evex} SHL dword [rip], 4 - C1 /4 ib.
    let code = rip_code(&[0x62, 0xF4, 0x7C, 0x08, 0xC1, 0x25], &[0x04]);
    let (_, mem) = run(&code, Registers::default());
    assert_eq!(window(&mem, 4), [0x3F, 0x00, 0x14, 0x24, 0x34, 0x44]);
}

#[test]
fn map4_conditional_immediates_follow_the_operand() {
    // CCMPE {dfv=} dword [rip], 0x43424140 - 81 /7 id. With ZF=1 the
    // condition holds and the compare of equal values sets ZF; any other
    // operand clears it.
    let mut initial = Registers::default();
    initial.rflags = 0x2 | 0x40;
    let code = rip_code(
        &[0x62, 0xF4, 0x04, 0x04, 0x81, 0x3D],
        &[0x40, 0x41, 0x42, 0x43],
    );
    let (regs, _) = run(&code, initial.clone());
    assert_ne!(regs.rflags & 0x40, 0, "ZF: compared the operand at TARGET");

    // CTESTE {dfv=} dword [rip], 0x01020304 - F7 /0 id: the AND is
    // 0x01020100, non-zero, so ZF clears.
    let code = rip_code(
        &[0x62, 0xF4, 0x04, 0x04, 0xF7, 0x05],
        &[0x04, 0x03, 0x02, 0x01],
    );
    let (regs, _) = run(&code, initial);
    assert_eq!(regs.rflags & 0x40, 0, "ZF: tested the operand at TARGET");
}
