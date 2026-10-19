//! The direct CPU's `step` is: charge the meter, fetch, decode, execute.
//!
//! The per-opcode proofs cover `decode_insn` and `execute_decoded` on the
//! instruction bytes. This proof covers the rest of `step` for every program
//! counter, memory content, text range, meter state, and byte order: the
//! meter traps exactly when exhausted and otherwise charges one unit before
//! the fetch, and the decoder receives exactly the 8 (or, after an opcode
//! 0x18, 16) bytes at the PC, or the step traps with the precise fault.

use rax::isa::ebpf::{
    EbpfConfig, EbpfCpu, EbpfDecodeError, EbpfDialect, EbpfInsn, EbpfMemFault, EbpfTrap,
};
use rax::smir::ir::types::Endian;

use super::{MEMORY, flat_memory};

/// The bytes and length of the last decode request (proofs are single-threaded).
static mut SEEN: ([u8; 16], usize) = ([0; 16], 0);

/// Records the bytes handed to the decoder and rejects them.
fn decode_spy(
    bytes: &[u8],
    _dialect: EbpfDialect,
    _endian: Endian,
) -> Result<EbpfInsn, EbpfDecodeError> {
    let mut seen = [0u8; 16];
    seen[..bytes.len()].copy_from_slice(bytes);
    // SAFETY: Kani executes the harness on one thread; no reference escapes.
    unsafe { SEEN = (seen, bytes.len()) };
    Err(EbpfDecodeError::InvalidOpcode { opc: 0xff })
}

#[kani::proof]
#[kani::stub(std::hash::RandomState::new, super::fixed_random_state)]
#[kani::stub(rax::isa::ebpf::decode::decode_insn, decode_spy)]
#[kani::unwind(17)]
fn step_meters_then_fetches_the_instruction_at_pc() {
    let bytes: [u8; MEMORY] = kani::any();
    let pc: u64 = kani::any();
    let text_range: Option<(u64, u64)> = kani::any();
    let max_instructions: Option<u64> = kani::any();
    let insn_count: u64 = kani::any();
    let endian = if kani::any() {
        Endian::Little
    } else {
        Endian::Big
    };
    let mut memory = flat_memory(&bytes);
    let mut cpu = EbpfCpu::new(EbpfConfig {
        endian,
        max_instructions,
        text_range,
        ..EbpfConfig::default()
    });
    cpu.regs.pc = pc;
    cpu.insn_count = insn_count;
    let result = cpu.step(&mut memory);

    if max_instructions.is_some_and(|max| insn_count >= max) {
        assert!(result == Err(EbpfTrap::ExceededMaxInstructions { pc }));
        assert!(cpu.insn_count == insn_count);
        return;
    }
    assert!(cpu.insn_count == insn_count.wrapping_add(1));

    // validate_target_pc: aligned, and inside the text when one is set.
    let in_text = |at: u64| {
        text_range.is_none_or(|(start, end)| {
            at >= start && at.checked_add(8).is_some_and(|next| next <= end)
        })
    };
    if pc % 8 != 0 || !in_text(pc) {
        assert!(result == Err(EbpfTrap::CallOutsideTextSegment { pc, target_pc: pc }));
        return;
    }
    let readable = |at: u64| at.checked_add(8).is_some_and(|end| end <= MEMORY as u64);
    if !readable(pc) {
        let fault = EbpfMemFault {
            addr: pc,
            size: 8,
            write: false,
        };
        assert!(result == Err(EbpfTrap::MemoryFault { pc, fault }));
        return;
    }
    let first = pc as usize;
    let wide = bytes[first] == 0x18;
    if wide {
        let ends_in_text =
            text_range.is_none_or(|(_, end)| pc.checked_add(16).is_some_and(|next| next <= end));
        if !ends_in_text {
            let (_, end) = text_range.unwrap();
            let error = EbpfDecodeError::UnexpectedEof {
                needed: 16,
                available: end.saturating_sub(pc) as usize,
            };
            assert!(result == Err(EbpfTrap::DecodeError { pc, error }));
            return;
        }
        if !readable(pc + 8) {
            let fault = EbpfMemFault {
                addr: pc + 8,
                size: 8,
                write: false,
            };
            assert!(result == Err(EbpfTrap::MemoryFault { pc, fault }));
            return;
        }
    }
    let length = if wide { 16 } else { 8 };
    let error = EbpfDecodeError::InvalidOpcode { opc: 0xff };
    assert!(result == Err(EbpfTrap::DecodeError { pc, error }));
    // SAFETY: single-threaded harness; the value is copied out.
    let (seen, seen_length) = unsafe { SEEN };
    assert!(seen_length == length);
    assert!(seen[..length] == bytes[first..first + length]);
}
