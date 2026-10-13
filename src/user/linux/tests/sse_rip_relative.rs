//! A guest that loads a code pointer through a legacy SSE instruction whose
//! RIP-relative operand is followed by an imm8, then jumps through it.
//!
//! Intel Pin's launcher fills its pseudo-system-call table this way
//! (`pinsrq $1, slot(%rip), %xmm0`). RIP-relative addressing is relative to
//! the end of the whole instruction, imm8 included (Intel SDM Vol. 2A section
//! 2.2.1.6); resolving it one byte low loads the pointer shifted left by 8
//! bits, and the jump faults.
use super::harness::{CODE, DATA, Harness};
use crate::user::linux::ExitStatus;
use crate::user::linux::abi::LinuxAbi;

const ENTRY: u64 = CODE + 0x1000;
/// `exit(37)`.
const STUB: u64 = ENTRY + 0x100;
/// The pointer slot, in the writable data segment like a GOT entry.
const SLOT: u64 = DATA + 0x40;

#[test]
fn pinsrq_rip_relative_code_pointer_reaches_its_target() {
    let mut h = Harness::embedded(LinuxAbi::X86_64);

    let mut code = vec![0x66, 0x0F, 0xEF, 0xC0]; // pxor xmm0, xmm0
    // pinsrq xmm0, [rip+disp32], 1 - 66 REX.W 0F 3A 22 /r ib; the next
    // instruction starts after the imm8.
    code.extend_from_slice(&[0x66, 0x48, 0x0F, 0x3A, 0x22, 0x05]);
    let next = ENTRY + code.len() as u64 + 4 + 1;
    code.extend_from_slice(&(SLOT.wrapping_sub(next) as u32).to_le_bytes());
    code.push(0x01);
    code.extend_from_slice(&[0x66, 0x48, 0x0F, 0x3A, 0x16, 0xC0, 0x01]); // pextrq rax, xmm0, 1
    code.extend_from_slice(&[0xFF, 0xE0]); // jmp rax
    let exit = [0xB8, 60, 0, 0, 0, 0xBF, 37, 0, 0, 0, 0x0F, 0x05]; // exit(37)

    let space = &h.proc.state.space;
    space.write_raw(ENTRY, &code).unwrap();
    space.write_raw(STUB, &exit).unwrap();
    space.write_raw(SLOT, &STUB.to_le_bytes()).unwrap();

    assert_eq!(h.proc.run(), ExitStatus::Exited(37));
}
