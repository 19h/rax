//! Independent RFC 9669 reference semantics of one instruction.
//!
//! Written from the RFC text (sections 2.2, 2.3, 3, 4, and 5) and the
//! registry of Appendix A, which `gen_registry.py` parses into
//! [`crate::registry::REGISTRY`]. It shares no code with the engine: the
//! proofs compare the engine against these definitions for every field value
//! and every register and memory state.
//!
//! Admission is the registry plus the platform contract that the engine
//! documents (RFC 9669 section 6 leaves verification to the platform):
//!
//! - the optional, deprecated "packet" group is not provided;
//! - `r10` is the read-only frame pointer, as on Linux;
//! - unused `dst_reg` fields are zero (section 3.1: "Unused fields SHALL be
//!   cleared to zero"); the registry lists the other fields.
//!
//! The platform memory is one flat byte array at address 0; an access that
//! does not fit in it faults with its start address, width, and direction.

use crate::registry::REGISTRY;

/// A registry field constraint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sel {
    Any,
    Exact(u32),
}

/// RFC 9669 section 2.4 conformance groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Base32,
    Base64,
    Atomic32,
    Atomic64,
    Divmul32,
    Divmul64,
    Packet,
}

/// One row of Appendix A, Table 18.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub opcode: u8,
    pub src: Sel,
    pub offset: Sel,
    pub imm: Sel,
    pub group: Group,
    pub description: &'static str,
    pub reference: &'static str,
}

/// Host byte order of the BPF program (section 3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Little,
    Big,
}

/// The fields of a basic instruction (section 3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fields {
    pub opcode: u8,
    pub dst: u8,
    pub src: u8,
    pub offset: i16,
    pub imm: i32,
}

/// Split an 8-byte basic instruction into its fields. The register nibbles
/// and the multi-byte fields follow the host byte order.
pub fn fields(bytes: [u8; 8], order: Order) -> Fields {
    let (dst, src) = match order {
        Order::Little => (bytes[1] & 0x0f, bytes[1] >> 4),
        Order::Big => (bytes[1] >> 4, bytes[1] & 0x0f),
    };
    let offset = [bytes[2], bytes[3]];
    let imm = [bytes[4], bytes[5], bytes[6], bytes[7]];
    match order {
        Order::Little => Fields {
            opcode: bytes[0],
            dst,
            src,
            offset: i16::from_le_bytes(offset),
            imm: i32::from_le_bytes(imm),
        },
        Order::Big => Fields {
            opcode: bytes[0],
            dst,
            src,
            offset: i16::from_be_bytes(offset),
            imm: i32::from_be_bytes(imm),
        },
    }
}

fn matches(sel: Sel, value: i64) -> bool {
    match sel {
        Sel::Any => true,
        Sel::Exact(expected) => value == i64::from(expected),
    }
}

/// Instructions RFC 9669 defines but Appendix A's Table 18 omits.
///
/// Section 5.2 defines `{MEMSX, <size>, LDX}` for B, H, and W ("dst = *(signed
/// size *) (src + offset)"), and section 2.4 places every instruction "defined
/// in this specification unless otherwise noted" in base32; Table 18 lists no
/// opcode 0x81, 0x89, or 0x91. These rows follow the Table 18 rows of the
/// unsigned loads (0x61, 0x69, 0x71): any registers and offset, and a zero
/// immediate, which they do not use (section 3.1).
pub const SECTION_5_2: [Entry; 3] = [
    Entry {
        opcode: 0x81,
        src: Sel::Any,
        offset: Sel::Any,
        imm: Sel::Exact(0),
        group: Group::Base32,
        description: "dst = *(s32 *)(src + offset)",
        reference: "RFC 9669, Section 5.2 (absent from Table 18)",
    },
    Entry {
        opcode: 0x89,
        src: Sel::Any,
        offset: Sel::Any,
        imm: Sel::Exact(0),
        group: Group::Base32,
        description: "dst = *(s16 *)(src + offset)",
        reference: "RFC 9669, Section 5.2 (absent from Table 18)",
    },
    Entry {
        opcode: 0x91,
        src: Sel::Any,
        offset: Sel::Any,
        imm: Sel::Exact(0),
        group: Group::Base32,
        description: "dst = *(s8 *)(src + offset)",
        reference: "RFC 9669, Section 5.2 (absent from Table 18)",
    },
];

/// The registry row an instruction matches, ignoring registers: Table 18,
/// then the section 5.2 rows it omits.
pub fn registry_entry(f: Fields) -> Option<&'static Entry> {
    let matching = |entry: &&Entry| {
        entry.opcode == f.opcode
            && matches(entry.src, i64::from(f.src))
            && matches(entry.offset, i64::from(f.offset))
            && matches(entry.imm, i64::from(f.imm))
    };
    REGISTRY
        .iter()
        .find(matching)
        .or_else(|| SECTION_5_2.iter().find(matching))
}

const CLASS_LD: u8 = 0x0;
const CLASS_LDX: u8 = 0x1;
const CLASS_ST: u8 = 0x2;
const CLASS_STX: u8 = 0x3;
const CLASS_ALU: u8 = 0x4;
const CLASS_JMP: u8 = 0x5;
const CLASS_JMP32: u8 = 0x6;
const CLASS_ALU64: u8 = 0x7;

const MODE_MEMSX: u8 = 0x4;
const MODE_ATOMIC: u8 = 0x6;
const FETCH: i32 = 0x01;

fn class(opcode: u8) -> u8 {
    opcode & 0x07
}

/// Whether the instruction uses `dst_reg` as a register (section 3.1 and the
/// per-class semantics): all but JA, CALL, EXIT, and the LDDW continuation.
fn uses_dst(opcode: u8) -> bool {
    !matches!(opcode, 0x00 | 0x05 | 0x06 | 0x85 | 0x95)
}

/// Whether the `src_reg` field names a register (registry `any`).
fn src_is_register(entry: &Entry) -> bool {
    entry.src == Sel::Any
}

/// The register an admitted instruction writes, if any.
fn written_register(f: Fields) -> Option<u8> {
    match class(f.opcode) {
        CLASS_ALU | CLASS_ALU64 | CLASS_LDX => Some(f.dst),
        CLASS_LD if f.opcode == 0x18 => Some(f.dst),
        CLASS_STX if f.opcode >> 5 == MODE_ATOMIC => {
            if f.imm == 0xf1 {
                Some(0)
            } else if f.imm & FETCH != 0 {
                Some(f.src)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Admission of a first slot under the documented platform contract.
pub fn admitted(f: Fields) -> Option<&'static Entry> {
    let entry = registry_entry(f)?;
    if entry.group == Group::Packet || f.opcode == 0x00 {
        return None;
    }
    if f.dst > 10 || (!uses_dst(f.opcode) && f.dst != 0) {
        return None;
    }
    if src_is_register(entry) && f.src > 10 {
        return None;
    }
    if written_register(f) == Some(10) {
        return None;
    }
    Some(entry)
}

/// Admission of the second slot of a wide instruction `first`: the
/// registry's `0x00` row with an unused (zero) `dst_reg`, and a zero
/// `next_imm` where Table 12 does not use it (subtypes 0x1, 0x3, 0x4, 0x5;
/// section 3.1: "Unused fields SHALL be cleared to zero").
pub fn continuation_admitted(first: Fields, next: Fields) -> bool {
    let uses_next_imm = matches!(first.src, 0x0 | 0x2 | 0x6);
    registry_entry(next).is_some_and(|entry| entry.opcode == 0x00)
        && next.dst == 0
        && (uses_next_imm || next.imm == 0)
}

/// The machine state one instruction observes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State<const N: usize> {
    pub regs: [u64; 11],
    pub pc: u64,
    pub memory: [u8; N],
}

/// The result of executing one admitted instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome<const N: usize> {
    /// Execution continues with this state.
    Next(State<N>),
    /// A memory access outside the platform memory.
    Fault { addr: u64, size: u64, write: bool },
    /// `EXIT` with an empty call stack.
    Exit { r0: u64 },
    /// A program-local call to `target`; the caller's state is unchanged.
    LocalCall { target: u64, return_pc: u64 },
    /// A helper call by static ID (`by_btf` false) or BTF ID.
    Helper { id: u32, by_btf: bool, next_pc: u64 },
    /// An LDDW subtype naming a map or platform variable (0x1, 0x2, 0x3,
    /// 0x5, 0x6); its value is the platform binding.
    PseudoLoad { src_type: u8 },
}

fn sext32(value: i32) -> u64 {
    i64::from(value) as u64
}

fn zext32(value: u64) -> u64 {
    value & 0xffff_ffff
}

/// Sign extend the low `bits` bits of `value` to 64 bits (section 2.3).
fn sign_extend(value: u64, bits: u32) -> u64 {
    let shift = 64 - bits;
    (((value << shift) as i64) >> shift) as u64
}

fn width_bytes(size: u8) -> u64 {
    match size {
        0 => 4,
        1 => 2,
        2 => 1,
        _ => 8,
    }
}

fn read<const N: usize>(memory: &[u8; N], addr: u64, size: u64, order: Order) -> Option<u64> {
    let end = addr.checked_add(size)?;
    if end > N as u64 {
        return None;
    }
    let mut value = 0u64;
    for index in 0..size {
        let byte = u64::from(memory[(addr + index) as usize]);
        value |= match order {
            Order::Little => byte << (8 * index),
            Order::Big => byte << (8 * (size - 1 - index)),
        };
    }
    Some(value)
}

fn write<const N: usize>(
    memory: &mut [u8; N],
    addr: u64,
    size: u64,
    value: u64,
    order: Order,
) -> bool {
    let Some(end) = addr.checked_add(size) else {
        return false;
    };
    if end > N as u64 {
        return false;
    }
    for index in 0..size {
        let shift = match order {
            Order::Little => 8 * index,
            Order::Big => 8 * (size - 1 - index),
        };
        memory[(addr + index) as usize] = (value >> shift) as u8;
    }
    true
}

/// Byte swap of the low `bits` bits; the result is zero-extended.
fn bswap(value: u64, bits: i32) -> u64 {
    match bits {
        16 => u64::from((value as u16).swap_bytes()),
        32 => u64::from((value as u32).swap_bytes()),
        _ => value.swap_bytes(),
    }
}

/// `le<bits>` / `be<bits>` (section 2.2): conversion between the host byte
/// order and the named one, of an unsigned `bits`-bit number.
fn convert(value: u64, bits: i32, to_big: bool, order: Order) -> u64 {
    let host_is_big = order == Order::Big;
    if to_big == host_is_big {
        match bits {
            16 => u64::from(value as u16),
            32 => zext32(value),
            _ => value,
        }
    } else {
        bswap(value, bits)
    }
}

/// The integer division and remainder of section 4.1 on operands for which
/// the Rust operators are defined (non-zero divisor, no signed overflow).
/// The proofs replace these, and the engine's identical primitives, with
/// one functionally consistent uninterpreted operator (`proofs::operators`).
pub mod ops {
    pub fn udiv32(dividend: u32, divisor: u32) -> u32 {
        dividend / divisor
    }

    pub fn urem32(dividend: u32, divisor: u32) -> u32 {
        dividend % divisor
    }

    pub fn udiv64(dividend: u64, divisor: u64) -> u64 {
        dividend / divisor
    }

    pub fn urem64(dividend: u64, divisor: u64) -> u64 {
        dividend % divisor
    }

    pub fn sdiv32(dividend: i32, divisor: i32) -> i32 {
        dividend / divisor
    }

    pub fn srem32(dividend: i32, divisor: i32) -> i32 {
        dividend % divisor
    }

    pub fn sdiv64(dividend: i64, divisor: i64) -> i64 {
        dividend / divisor
    }

    pub fn srem64(dividend: i64, divisor: i64) -> i64 {
        dividend % divisor
    }
}

/// Arithmetic of section 4.1 on 64-bit operands.
fn alu64(code: u8, offset: i16, dst: u64, src: u64) -> u64 {
    match code {
        0x0 => dst.wrapping_add(src),
        0x1 => dst.wrapping_sub(src),
        0x2 => dst.wrapping_mul(src),
        0x3 if offset == 0 => {
            if src != 0 {
                ops::udiv64(dst, src)
            } else {
                0
            }
        }
        0x3 => {
            let (dividend, divisor) = (dst as i64, src as i64);
            if divisor == 0 {
                0
            } else if dividend == i64::MIN && divisor == -1 {
                // The quotient 2^63 wraps (section 4.1).
                i64::MIN as u64
            } else {
                ops::sdiv64(dividend, divisor) as u64
            }
        }
        0x4 => dst | src,
        0x5 => dst & src,
        0x6 => dst << (src & 0x3f),
        0x7 => dst >> (src & 0x3f),
        0x8 => dst.wrapping_neg(),
        0x9 if offset == 0 => {
            if src != 0 {
                ops::urem64(dst, src)
            } else {
                dst
            }
        }
        // Truncated division: a % n = a - n * trunc(a / n), so
        // MIN % -1 = MIN - (-1 * MIN) = 0 with wrapping.
        0x9 => {
            let (dividend, divisor) = (dst as i64, src as i64);
            if divisor == 0 {
                dst
            } else if dividend == i64::MIN && divisor == -1 {
                0
            } else {
                ops::srem64(dividend, divisor) as u64
            }
        }
        0xa => dst ^ src,
        0xb if offset == 0 => src,
        0xb => sign_extend(src, offset as u32),
        _ => ((dst as i64) >> (src & 0x3f)) as u64,
    }
}

/// Arithmetic of section 4.1 on 32-bit operands; the result is zero-extended.
fn alu32(code: u8, offset: i16, dst: u64, src: u64) -> u64 {
    let d = dst as u32;
    let s = src as u32;
    let value = match code {
        0x0 => d.wrapping_add(s),
        0x1 => d.wrapping_sub(s),
        0x2 => d.wrapping_mul(s),
        0x3 if offset == 0 => {
            if s != 0 {
                ops::udiv32(d, s)
            } else {
                0
            }
        }
        0x3 => {
            let (dividend, divisor) = (d as i32, s as i32);
            if divisor == 0 {
                0
            } else if dividend == i32::MIN && divisor == -1 {
                i32::MIN as u32
            } else {
                ops::sdiv32(dividend, divisor) as u32
            }
        }
        0x4 => d | s,
        0x5 => d & s,
        0x6 => d << (s & 0x1f),
        0x7 => d >> (s & 0x1f),
        0x8 => d.wrapping_neg(),
        // Modulo by zero keeps dst but zeroes its upper 32 bits.
        0x9 if offset == 0 => {
            if s != 0 {
                ops::urem32(d, s)
            } else {
                d
            }
        }
        0x9 => {
            let (dividend, divisor) = (d as i32, s as i32);
            if divisor == 0 {
                d
            } else if dividend == i32::MIN && divisor == -1 {
                0
            } else {
                ops::srem32(dividend, divisor) as u32
            }
        }
        0xa => d ^ s,
        0xb if offset == 0 => s,
        0xb => sign_extend(u64::from(s), offset as u32) as u32,
        _ => ((d as i32) >> (s & 0x1f)) as u32,
    };
    u64::from(value)
}

fn condition(code: u8, wide: bool, dst: u64, src: u64) -> bool {
    let (d, s) = if wide {
        (dst, src)
    } else {
        (zext32(dst), zext32(src))
    };
    let (sd, ss) = if wide {
        (dst as i64, src as i64)
    } else {
        (i64::from(dst as i32), i64::from(src as i32))
    };
    match code {
        0x1 => d == s,
        0x2 => d > s,
        0x3 => d >= s,
        0x4 => d & s != 0,
        0x5 => d != s,
        0x6 => sd > ss,
        0x7 => sd >= ss,
        0xa => d < s,
        0xb => d <= s,
        0xc => sd < ss,
        _ => sd <= ss,
    }
}

/// Execute an admitted instruction. `next` is the second slot of a wide
/// instruction, already admitted by [`continuation_admitted`].
pub fn execute<const N: usize>(
    f: Fields,
    next: Fields,
    state: State<N>,
    order: Order,
) -> Outcome<N> {
    let mut out = state;
    let regs = state.regs;
    let dst = regs[usize::from(f.dst)];
    let src_reg = || regs[usize::from(f.src)];
    let next_pc = state.pc.wrapping_add(8);
    out.pc = next_pc;
    let source_is_reg = f.opcode & 0x08 != 0;
    match class(f.opcode) {
        CLASS_ALU64 | CLASS_ALU => {
            let code = f.opcode >> 4;
            let wide = class(f.opcode) == CLASS_ALU64;
            let value = if code == 0xd {
                if wide {
                    bswap(dst, f.imm)
                } else {
                    convert(dst, f.imm, source_is_reg, order)
                }
            } else {
                // ALU64 sign-extends imm; ALU uses its low 32 bits.
                let operand = if source_is_reg {
                    src_reg()
                } else {
                    sext32(f.imm)
                };
                if wide {
                    alu64(code, f.offset, dst, operand)
                } else {
                    alu32(code, f.offset, dst, operand)
                }
            };
            out.regs[usize::from(f.dst)] = value;
            Outcome::Next(out)
        }
        CLASS_JMP | CLASS_JMP32 => {
            let code = f.opcode >> 4;
            let wide = class(f.opcode) == CLASS_JMP;
            let slots = |count: i64| next_pc.wrapping_add(count.wrapping_mul(8) as u64);
            match code {
                0x0 if wide => out.pc = slots(i64::from(f.offset)),
                0x0 => out.pc = slots(i64::from(f.imm)),
                0x8 => {
                    return match f.src {
                        1 => Outcome::LocalCall {
                            target: slots(i64::from(f.imm)),
                            return_pc: next_pc,
                        },
                        kind => Outcome::Helper {
                            id: f.imm as u32,
                            by_btf: kind == 2,
                            next_pc,
                        },
                    };
                }
                0x9 => return Outcome::Exit { r0: regs[0] },
                _ => {
                    let operand = if source_is_reg {
                        src_reg()
                    } else {
                        sext32(f.imm)
                    };
                    if condition(code, wide, dst, operand) {
                        out.pc = slots(i64::from(f.offset));
                    }
                }
            }
            Outcome::Next(out)
        }
        CLASS_LD => {
            // Section 5.4, Table 12. code_addr(imm) is the instruction imm + 1
            // slots after the LDDW's first slot (the Linux verifier and libbpf
            // index i + imm + 1, the base of a program-local call).
            let value = match f.src {
                0 => (u64::from(next.imm as u32) << 32) | u64::from(f.imm as u32),
                4 => state
                    .pc
                    .wrapping_add((i64::from(f.imm) + 1).wrapping_mul(8) as u64),
                src_type => return Outcome::PseudoLoad { src_type },
            };
            out.regs[usize::from(f.dst)] = value;
            out.pc = state.pc.wrapping_add(16);
            Outcome::Next(out)
        }
        CLASS_LDX => {
            let size = width_bytes((f.opcode >> 3) & 0x3);
            let addr = src_reg().wrapping_add(i64::from(f.offset) as u64);
            let Some(value) = read(&state.memory, addr, size, order) else {
                return Outcome::Fault {
                    addr,
                    size,
                    write: false,
                };
            };
            out.regs[usize::from(f.dst)] = if f.opcode >> 5 == MODE_MEMSX {
                sign_extend(value, (size * 8) as u32)
            } else {
                value
            };
            Outcome::Next(out)
        }
        CLASS_ST | CLASS_STX => {
            let size = width_bytes((f.opcode >> 3) & 0x3);
            let addr = dst.wrapping_add(i64::from(f.offset) as u64);
            let fault = Outcome::Fault {
                addr,
                size,
                write: true,
            };
            if class(f.opcode) == CLASS_ST {
                if !write(&mut out.memory, addr, size, sext32(f.imm), order) {
                    return fault;
                }
                return Outcome::Next(out);
            }
            let src = src_reg();
            if f.opcode >> 5 != MODE_ATOMIC {
                if !write(&mut out.memory, addr, size, src, order) {
                    return fault;
                }
                return Outcome::Next(out);
            }
            let Some(old) = read(&state.memory, addr, size, order) else {
                return fault;
            };
            let truncate = |value: u64| if size == 4 { zext32(value) } else { value };
            let operand = truncate(src);
            let (new, loaded) = match f.imm & !FETCH {
                0x00 => (old.wrapping_add(operand), Some(f.src)),
                0x40 => (old | operand, Some(f.src)),
                0x50 => (old & operand, Some(f.src)),
                0xa0 => (old ^ operand, Some(f.src)),
                0xe0 => (operand, Some(f.src)),
                _ => {
                    // CMPXCHG compares the operand width of R0.
                    let replace = old == truncate(regs[0]);
                    (if replace { operand } else { old }, Some(0))
                }
            };
            write(&mut out.memory, addr, size, truncate(new), order);
            if f.imm & FETCH != 0
                && let Some(register) = loaded
            {
                out.regs[usize::from(register)] = old;
            }
            Outcome::Next(out)
        }
        _ => unreachable!("admitted classes are exhaustive"),
    }
}
