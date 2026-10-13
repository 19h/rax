//! The 66h prefix on near branches in 64-bit mode.
//!
//! "In 64-bit mode, the operand size for all near branches (CALL, RET, JCC,
//! JCXZ, JMP, and LOOP) is forced to 64 bits": instruction-pointer
//! truncation, the return-address push and pop, the stack-pointer adjustment,
//! and the indirect-branch operand size are 64 bits "regardless of operand
//! size prefixes (operand size prefixes are silently ignored). However, the
//! displacement field for relative branches is still limited to 32 bits"
//! (Intel SDM Vol. 1 section 6.4.7; the rel16 forms are "N.S." in 64-bit
//! mode). Every target here lies above 0xFFFF, so a 16-bit operand size
//! would truncate it, and every relative branch uses a rel32 a 16-bit form
//! would misread.

use crate::common::*;
use vm_memory::{Bytes, GuestAddress};

/// A branch target above the 16-bit range; holds a HLT.
const TARGET: u64 = 0x1_2345;

fn rel32(insn_len: u64) -> [u8; 4] {
    (TARGET.wrapping_sub(CODE_ADDR + insn_len) as u32).to_le_bytes()
}

/// Runs `code` with a HLT at `TARGET` and `qword` stored at `STACK_ADDR`.
fn run(code: &[u8], initial: Registers, qword: u64) -> (Registers, u64) {
    let (mut vcpu, mem) = setup_vm(code, Some(initial));
    mem.write_slice(&[0xF4], GuestAddress(TARGET)).unwrap();
    mem.write_slice(&qword.to_le_bytes(), GuestAddress(STACK_ADDR))
        .unwrap();
    let regs = run_until_hlt(&mut vcpu).unwrap();
    let mut top = [0u8; 8];
    mem.read_slice(&mut top, GuestAddress(regs.rsp)).unwrap();
    (regs, u64::from_le_bytes(top))
}

#[test]
fn o16_relative_jmp_and_jcc_use_rel32_and_a_64_bit_rip() {
    // 66 E9 rel32
    let mut code = vec![0x66, 0xE9];
    code.extend_from_slice(&rel32(6));
    let (regs, _) = run(&code, Registers::default(), 0);
    assert_eq!(regs.rip, TARGET + 1);

    // 66 0F 84 rel32 (JZ), taken with ZF=1 and not taken with ZF=0.
    let mut code = vec![0x66, 0x0F, 0x84];
    code.extend_from_slice(&rel32(7));
    code.push(0xF4);
    let mut initial = Registers::default();
    initial.rflags = 0x2 | 0x40;
    let (regs, _) = run(&code, initial.clone(), 0);
    assert_eq!(regs.rip, TARGET + 1);
    initial.rflags = 0x2;
    let (regs, _) = run(&code, initial, 0);
    assert_eq!(regs.rip, CODE_ADDR + 8);
}

#[test]
fn o16_relative_and_indirect_call_push_an_8_byte_return_address() {
    // 66 E8 rel32
    let mut code = vec![0x66, 0xE8];
    code.extend_from_slice(&rel32(6));
    let (regs, top) = run(&code, Registers::default(), 0);
    assert_eq!(
        (regs.rip, regs.rsp, top),
        (TARGET + 1, STACK_ADDR - 8, CODE_ADDR + 6)
    );

    // 66 FF D0 (CALL rax): the whole of RAX is the target.
    let mut initial = Registers::default();
    initial.rax = TARGET;
    let (regs, top) = run(&[0x66, 0xFF, 0xD0], initial, 0);
    assert_eq!(
        (regs.rip, regs.rsp, top),
        (TARGET + 1, STACK_ADDR - 8, CODE_ADDR + 3)
    );
}

#[test]
fn o16_indirect_jmp_reads_a_64_bit_target() {
    // 66 FF E0 (JMP rax)
    let mut initial = Registers::default();
    initial.rax = TARGET;
    let (regs, _) = run(&[0x66, 0xFF, 0xE0], initial, 0);
    assert_eq!(regs.rip, TARGET + 1);

    // 66 FF 24 24 (JMP qword [rsp]): eight bytes are read.
    let (regs, _) = run(&[0x66, 0xFF, 0x24, 0x24], Registers::default(), TARGET);
    assert_eq!(regs.rip, TARGET + 1);
}

#[test]
fn o16_ret_pops_an_8_byte_return_address() {
    // 66 C3
    let (regs, _) = run(&[0x66, 0xC3], Registers::default(), TARGET);
    assert_eq!((regs.rip, regs.rsp), (TARGET + 1, STACK_ADDR + 8));

    // 66 C2 10 00 (RET 16): the 8-byte pop, then the immediate.
    let (regs, _) = run(&[0x66, 0xC2, 0x10, 0x00], Registers::default(), TARGET);
    assert_eq!((regs.rip, regs.rsp), (TARGET + 1, STACK_ADDR + 8 + 16));
}
