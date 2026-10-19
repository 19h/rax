//! Solana SBPF program admission, mirroring the reference loader verifier.
//!
//! A Solana runtime executes only programs accepted by Anza's
//! `RequisiteVerifier` (`docs/specifications/ebpf/sbpf/verifier.rs`, pinned
//! commit `3e1510b1`). [`verify_sbpf`] applies the same per-version opcode
//! table, immediate checks, LDDW pairing, jump-target rules, CALLX register
//! rule, and register policy, in the reference order, and reports the same
//! first failing instruction. Execution engines do not call it: decoding
//! accepts a superset (for example `dst = r10` on a conditional jump) whose
//! behavior on admitted programs is identical.

use std::fmt;

use super::{FRAME_PTR_REG, INSN_SIZE, SbpfVersion};

/// Reason a program is not admitted. Variants and payloads correspond to
/// the reference `VerifierError` (instruction indices count 8-byte slots).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SbpfVerifierError {
    /// The length is not a multiple of 8 bytes (`ProgramLengthNotMultiple`).
    ProgramLengthNotMultiple,
    /// The program is empty (`NoProgram`).
    NoProgram,
    /// Division or remainder by an immediate zero (`DivisionByZero`).
    DivisionByZero(usize),
    /// `LE`/`BE` width other than 16, 32, or 64 (`UnsupportedLEBEArgument`).
    UnsupportedLeBeArgument(usize),
    /// `LDDW` occupies the final slot (`LDDWCannotBeLast`).
    LddwCannotBeLast,
    /// The slot after `LDDW` has a non-zero opcode (`IncompleteLDDW`).
    IncompleteLddw(usize),
    /// A jump target lies outside the program: `(target, insn)`
    /// (`JumpOutOfCode`). A negative target is reported as its two's
    /// complement slot index, as upstream does.
    JumpOutOfCode(usize, usize),
    /// A jump targets a slot with opcode zero: `(target, insn)`
    /// (`JumpToMiddleOfLDDW`).
    JumpToMiddleOfLddw(usize, usize),
    /// Source register above `r10` (`InvalidSourceRegister`).
    InvalidSourceRegister(usize),
    /// A non-store instruction names `r10` as its destination
    /// (`CannotWriteR10`).
    CannotWriteR10(usize),
    /// Destination register above `r10` (`InvalidDestinationRegister`).
    InvalidDestinationRegister(usize),
    /// Opcode not defined for the version: `(opcode, insn)`
    /// (`UnknownOpCode`).
    UnknownOpCode(u8, usize),
    /// Immediate shift amount negative or not below the operand width:
    /// `(amount, width_bits, insn)` (`ShiftWithOverflow`).
    ShiftWithOverflow(u64, u64, usize),
    /// `CALLX` register outside `r0..=r9` (`InvalidRegister`).
    InvalidRegister(usize),
    /// Manual frame bump not a multiple of 64 bytes (`UnalignedImmediate`).
    UnalignedImmediate(usize),
}

impl fmt::Display for SbpfVerifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SBPF verifier rejected the program: {self:?}")
    }
}

impl std::error::Error for SbpfVerifierError {}

/// One 8-byte slot with the reference field extraction (little-endian;
/// `dst` in the low nibble, the immediate sign-extended to 64 bits).
#[derive(Clone, Copy)]
struct Slot {
    opc: u8,
    dst: u8,
    src: u8,
    off: i16,
    imm: i64,
}

fn slot(program: &[u8], index: usize) -> Slot {
    let bytes = &program[index * INSN_SIZE..(index + 1) * INSN_SIZE];
    Slot {
        opc: bytes[0],
        dst: bytes[1] & 0x0f,
        src: bytes[1] >> 4,
        off: i16::from_le_bytes([bytes[2], bytes[3]]),
        imm: i64::from(i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])),
    }
}

/// Per-instruction admission rule for one opcode in one version.
enum Rule {
    /// No operand checks beyond the register policy.
    Plain,
    /// A store: `dst` names the base address and may be `r10`.
    Store,
    /// Immediate divisor must be non-zero.
    NonZeroImm,
    /// Immediate shift amount must be in `0..bits`.
    Shift(u64),
    /// Byte-order width immediate.
    Endian,
    /// `LDDW`: a following slot with opcode zero.
    Lddw,
    /// PC-relative jump target.
    Jump,
    /// `CALLX` register operand.
    CallReg,
    /// `ADD64_IMM`: the manual frame bump of `r10` must be 64-byte aligned.
    Add64Imm,
}

/// The reference opcode table (`RequisiteVerifier::verify`), or `None` for
/// an opcode the version does not define.
fn rule(version: SbpfVersion, opc: u8) -> Option<Rule> {
    let classic_memory = !version.move_memory_instruction_classes();
    let moved_memory = version.move_memory_instruction_classes();
    let pqr = version.enable_pqr();
    let rule = match opc {
        0x18 if !version.disable_lddw() => Rule::Lddw,

        // BPF_LDX, BPF_ST, and BPF_STX classes.
        0x71 | 0x69 | 0x61 | 0x79 if classic_memory => Rule::Plain,
        0x72 | 0x6a | 0x62 | 0x7a | 0x73 | 0x6b | 0x63 | 0x7b if classic_memory => Rule::Store,

        // BPF_ALU32_LOAD class.
        0x04 | 0x0c | 0x14 | 0x1c => Rule::Plain,
        0x24 | 0x2c if !pqr => Rule::Plain,
        0x2c if moved_memory => Rule::Plain,
        0x34 if !pqr => Rule::NonZeroImm,
        0x3c if !pqr => Rule::Plain,
        0x3c if moved_memory => Rule::Plain,
        0x44 | 0x4c | 0x54 | 0x5c => Rule::Plain,
        0x64 | 0x74 | 0xc4 => Rule::Shift(32),
        0x6c | 0x7c | 0xcc => Rule::Plain,
        0x84 if !version.disable_neg() => Rule::Plain,
        0x8c if moved_memory => Rule::Plain,
        0x94 if !pqr => Rule::NonZeroImm,
        0x9c if !pqr => Rule::Plain,
        0x9c if moved_memory => Rule::Plain,
        0xa4 | 0xac | 0xb4 | 0xbc => Rule::Plain,
        0xd4 if !version.disable_le() => Rule::Endian,
        0xdc => Rule::Endian,

        // BPF_ALU64_STORE class.
        0x07 => Rule::Add64Imm,
        0x0f | 0x17 | 0x1f => Rule::Plain,
        0x27 if !pqr => Rule::Plain,
        0x27 if moved_memory => Rule::Store,
        0x2f if !pqr => Rule::Plain,
        0x2f if moved_memory => Rule::Store,
        0x37 if !pqr => Rule::NonZeroImm,
        0x37 if moved_memory => Rule::Store,
        0x3f if !pqr => Rule::Plain,
        0x3f if moved_memory => Rule::Store,
        0x47 | 0x4f | 0x57 | 0x5f => Rule::Plain,
        0x67 | 0x77 | 0xc7 => Rule::Shift(64),
        0x6f | 0x7f | 0xcf => Rule::Plain,
        0x87 if moved_memory => Rule::Store,
        0x87 if !version.disable_neg() => Rule::Plain,
        0x8f if moved_memory => Rule::Store,
        0x97 if !pqr => Rule::NonZeroImm,
        0x97 if moved_memory => Rule::Store,
        0x9f if !pqr => Rule::Plain,
        0x9f if moved_memory => Rule::Store,
        0xa7 | 0xaf | 0xb7 | 0xbf => Rule::Plain,
        0xf7 if version.disable_lddw() => Rule::Plain,

        // BPF_PQR class (V2).
        0x86 | 0x8e | 0x96 | 0x9e | 0x36 | 0x3e | 0xb6 | 0xbe if pqr => Rule::Plain,
        0x4e | 0x5e | 0x6e | 0x7e | 0xce | 0xde | 0xee | 0xfe if pqr => Rule::Plain,
        0x46 | 0x56 | 0x66 | 0x76 | 0xc6 | 0xd6 | 0xe6 | 0xf6 if pqr => Rule::NonZeroImm,

        // BPF_JMP32 class (V3/V4).
        0x16 | 0x1e | 0x26 | 0x2e | 0x36 | 0x3e | 0xa6 | 0xae | 0xb6 | 0xbe | 0x46 | 0x4e
        | 0x56 | 0x5e | 0x66 | 0x6e | 0x76 | 0x7e | 0xc6 | 0xce | 0xd6 | 0xde
            if version.enable_jmp32() =>
        {
            Rule::Jump
        }

        // BPF_JMP64 class.
        0x05 | 0x15 | 0x1d | 0x25 | 0x2d | 0x35 | 0x3d | 0xa5 | 0xad | 0xb5 | 0xbd | 0x45
        | 0x4d | 0x55 | 0x5d | 0x65 | 0x6d | 0x75 | 0x7d | 0xc5 | 0xcd | 0xd5 | 0xdd => Rule::Jump,
        0x85 | 0x95 => Rule::Plain,
        0x8d => Rule::CallReg,

        _ => return None,
    };
    Some(rule)
}

/// Admit `program` for `version` exactly as the reference
/// `RequisiteVerifier` does, reporting its first error.
///
/// Time is O(n) and space O(1) in the number of 8-byte slots `n`.
pub fn verify_sbpf(program: &[u8], version: SbpfVersion) -> Result<(), SbpfVerifierError> {
    if program.len() % INSN_SIZE != 0 {
        return Err(SbpfVerifierError::ProgramLengthNotMultiple);
    }
    if program.is_empty() {
        return Err(SbpfVerifierError::NoProgram);
    }
    let slots = program.len() / INSN_SIZE;
    let mut index = 0;
    while index < slots {
        let insn = slot(program, index);
        let Some(rule) = rule(version, insn.opc) else {
            return Err(SbpfVerifierError::UnknownOpCode(insn.opc, index));
        };
        let mut store = false;
        match rule {
            Rule::Plain => {}
            Rule::Store => store = true,
            Rule::NonZeroImm => {
                if insn.imm == 0 {
                    return Err(SbpfVerifierError::DivisionByZero(index));
                }
            }
            Rule::Shift(bits) => {
                if insn.imm < 0 || insn.imm as u64 >= bits {
                    return Err(SbpfVerifierError::ShiftWithOverflow(
                        insn.imm as u64,
                        bits,
                        index,
                    ));
                }
            }
            Rule::Endian => {
                if !matches!(insn.imm, 16 | 32 | 64) {
                    return Err(SbpfVerifierError::UnsupportedLeBeArgument(index));
                }
            }
            Rule::Lddw => {
                if index + 1 >= slots {
                    return Err(SbpfVerifierError::LddwCannotBeLast);
                }
                if slot(program, index + 1).opc != 0 {
                    return Err(SbpfVerifierError::IncompleteLddw(index));
                }
            }
            Rule::Jump => {
                let target = index as isize + 1 + isize::from(insn.off);
                if target < 0 || target as usize >= slots {
                    return Err(SbpfVerifierError::JumpOutOfCode(target as usize, index));
                }
                if slot(program, target as usize).opc == 0 {
                    return Err(SbpfVerifierError::JumpToMiddleOfLddw(
                        target as usize,
                        index,
                    ));
                }
            }
            Rule::CallReg => {
                let register = if version.callx_uses_src_reg() {
                    i64::from(insn.src)
                } else if version.callx_uses_dst_reg() {
                    i64::from(insn.dst)
                } else {
                    insn.imm
                };
                if !(0..10).contains(&register) {
                    return Err(SbpfVerifierError::InvalidRegister(index));
                }
            }
            Rule::Add64Imm => {
                let manual_bump = insn.dst == FRAME_PTR_REG && version.manual_stack_frame_bump();
                if manual_bump && insn.imm & 63 != 0 {
                    return Err(SbpfVerifierError::UnalignedImmediate(index));
                }
            }
        }

        // LDDW occupies two slots. The reference advances past the first
        // before checking registers, so an LDDW register error reports the
        // index of its second slot.
        if matches!(rule, Rule::Lddw) {
            index += 1;
        }

        // check_registers: the source must name r0..=r10; the destination
        // may be r10 only for stores and the V1/V2 manual frame bump.
        if insn.src > FRAME_PTR_REG {
            return Err(SbpfVerifierError::InvalidSourceRegister(index));
        }
        match insn.dst {
            0..=9 => {}
            FRAME_PTR_REG if store || (insn.opc == 0x07 && version.manual_stack_frame_bump()) => {}
            FRAME_PTR_REG => return Err(SbpfVerifierError::CannotWriteR10(index)),
            _ => return Err(SbpfVerifierError::InvalidDestinationRegister(index)),
        }
        index += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use SbpfVerifierError as E;
    use SbpfVersion::{V0, V1, V2, V3, V4};

    fn insn(opc: u8, dst: u8, src: u8, off: i16, imm: i32) -> [u8; 8] {
        let off = off.to_le_bytes();
        let imm = imm.to_le_bytes();
        [
            opc,
            (src << 4) | dst,
            off[0],
            off[1],
            imm[0],
            imm[1],
            imm[2],
            imm[3],
        ]
    }

    const EXIT: [u8; 8] = [0x95, 0, 0, 0, 0, 0, 0, 0];

    /// Each rule of the pinned reference `RequisiteVerifier`; the exhaustive
    /// upstream differential lives in `tools/ebpf-reference`.
    #[test]
    fn every_reference_rule_reports_its_error_and_slot() {
        let lddw = [insn(0x18, 1, 0, 0, 7), [0; 8]].concat();
        let cases: Vec<(&[SbpfVersion], Vec<u8>, Result<(), E>)> = vec![
            (&[V0, V3], vec![], Err(E::NoProgram)),
            (&[V0, V3], vec![0x95; 12], Err(E::ProgramLengthNotMultiple)),
            (&[V0, V1, V2, V3, V4], EXIT.to_vec(), Ok(())),
            // Version-gated opcodes.
            (
                &[V2],
                [lddw.clone(), EXIT.to_vec()].concat(),
                Err(E::UnknownOpCode(0x18, 0)),
            ),
            (
                &[V0, V3],
                [insn(0xf7, 1, 0, 0, 1), EXIT].concat(),
                Err(E::UnknownOpCode(0xf7, 0)),
            ),
            (&[V2], [insn(0xf7, 1, 0, 0, 1), EXIT].concat(), Ok(())),
            (
                &[V0, V2],
                [insn(0x16, 1, 0, 0, 1), EXIT].concat(),
                Err(E::UnknownOpCode(0x16, 0)),
            ),
            (
                &[V2],
                [insn(0x84, 1, 0, 0, 0), EXIT].concat(),
                Err(E::UnknownOpCode(0x84, 0)),
            ),
            (&[V2], [insn(0x27, 10, 1, -8, 0), EXIT].concat(), Ok(())),
            // Immediate operands.
            (
                &[V0],
                [insn(0x37, 1, 0, 0, 0), EXIT].concat(),
                Err(E::DivisionByZero(0)),
            ),
            (
                &[V2],
                [insn(0xd6, 1, 0, 0, 0), EXIT].concat(),
                Err(E::DivisionByZero(0)),
            ),
            (
                &[V3],
                [insn(0x64, 1, 0, 0, 32), EXIT].concat(),
                Err(E::ShiftWithOverflow(32, 32, 0)),
            ),
            (
                &[V3],
                [insn(0x67, 1, 0, 0, -1), EXIT].concat(),
                Err(E::ShiftWithOverflow(u64::MAX, 64, 0)),
            ),
            (
                &[V0],
                [insn(0xdc, 1, 0, 0, 8), EXIT].concat(),
                Err(E::UnsupportedLeBeArgument(0)),
            ),
            (
                &[V1],
                [insn(0x07, 10, 0, 0, -32), EXIT].concat(),
                Err(E::UnalignedImmediate(0)),
            ),
            (
                &[V1, V2],
                [insn(0x07, 10, 0, 0, -64), EXIT].concat(),
                Ok(()),
            ),
            (
                &[V0, V3],
                [insn(0x07, 10, 0, 0, -64), EXIT].concat(),
                Err(E::CannotWriteR10(0)),
            ),
            // LDDW pairing; register errors report the second slot.
            // (index + 1) * 8 >= length: the first slot is the final slot.
            (
                &[V0, V3],
                insn(0x18, 1, 0, 0, 7).to_vec(),
                Err(E::LddwCannotBeLast),
            ),
            (&[V0, V3], lddw.clone(), Ok(())),
            (
                &[V0, V3],
                [insn(0x18, 1, 0, 0, 7), EXIT, EXIT].concat(),
                Err(E::IncompleteLddw(0)),
            ),
            (
                &[V0, V3],
                [insn(0x18, 11, 0, 0, 7), [0; 8], EXIT].concat(),
                Err(E::InvalidDestinationRegister(1)),
            ),
            // Jump targets.
            (
                &[V3],
                [insn(0x05, 0, 0, 1, 0), EXIT].concat(),
                Err(E::JumpOutOfCode(2, 0)),
            ),
            (
                &[V3],
                [insn(0x05, 0, 0, -2, 0), EXIT].concat(),
                Err(E::JumpOutOfCode(usize::MAX, 0)),
            ),
            (
                &[V3],
                [insn(0x05, 0, 0, 1, 0).to_vec(), lddw.clone(), EXIT.to_vec()].concat(),
                Err(E::JumpToMiddleOfLddw(2, 0)),
            ),
            (
                &[V3],
                [insn(0x15, 10, 0, 0, 0), EXIT].concat(),
                Err(E::CannotWriteR10(0)),
            ),
            // CALLX register field per version.
            (
                &[V0],
                [insn(0x8d, 0, 0, 0, 10), EXIT].concat(),
                Err(E::InvalidRegister(0)),
            ),
            (
                &[V2],
                [insn(0x8d, 0, 10, 0, 0), EXIT].concat(),
                Err(E::InvalidRegister(0)),
            ),
            (&[V3], [insn(0x8d, 9, 0, 0, 0), EXIT].concat(), Ok(())),
            // Register policy.
            (
                &[V3],
                [insn(0xbf, 1, 11, 0, 0), EXIT].concat(),
                Err(E::InvalidSourceRegister(0)),
            ),
            (
                &[V3],
                [insn(0xbf, 12, 1, 0, 0), EXIT].concat(),
                Err(E::InvalidDestinationRegister(0)),
            ),
            (&[V3], [insn(0x7b, 10, 1, -8, 0), EXIT].concat(), Ok(())),
            (
                &[V3],
                [EXIT, insn(0xff, 0, 0, 0, 0)].concat(),
                Err(E::UnknownOpCode(0xff, 1)),
            ),
        ];
        for (versions, program, expected) in cases {
            for &version in versions {
                assert_eq!(
                    verify_sbpf(&program, version),
                    expected,
                    "{version:?} {program:02x?}"
                );
            }
        }
    }
}
