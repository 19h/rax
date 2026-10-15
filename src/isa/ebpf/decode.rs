//! eBPF (RFC 9669) and Solana SBPF (V0–V4) instruction decoder.
//!
//! Reference specifications:
//! - IETF RFC 9669: BPF Instruction Set Architecture (ISA), October 2024
//!   (`docs/specifications/ebpf/rfc9669.txt`)
//! - Linux kernel BPF ISA specification v1.0
//!   (`docs/specifications/ebpf/instruction-set.rst`)
//! - Anza / Solana SBPF canonical reference (`anza-xyz/sbpf`)
//!   (`docs/specifications/ebpf/sbpf/{ebpf,program,interpreter,verifier}.rs`)

use std::fmt;

use crate::smir::ir::types::Endian;

/// Size in bytes of a basic eBPF instruction slot (64 bits).
pub const INSN_SIZE: usize = 8;

/// Size in bytes of a wide 128-bit eBPF instruction (`LD_DW_IMM`).
pub const WIDE_INSN_SIZE: usize = 16;

/// Architectural frame-pointer register index (`r10`).
pub const FRAME_PTR_REG: u8 = 10;

/// Solana SBPF ISA version (`V0` through `V4`).
///
/// Each version toggles the exact architectural feature gates defined in
/// `anza-xyz/sbpf`'s `SBPFVersion`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum SbpfVersion {
    /// Legacy Solana BPF format (`SBPFVersion::V0`).
    V0,
    /// Dynamic stack frames (`SBPFVersion::V1`).
    V1,
    /// SIMD-0174 (`SBPFVersion::V2`): PQR class, explicit sign extension on
    /// `MOV32_REG`, swapped `SUB_IMM` operands, relocated memory opcodes,
    /// `HOR64_IMM` instead of `LDDW`, `CALLX` in `src`.
    V2,
    /// SIMD-0178 (`SBPFVersion::V3`): static syscalls (`src == 0` vs `src == 1`),
    /// `JMP32`, `CALLX` in `dst`, restored standard memory classes and `LDDW`.
    #[default]
    V3,
    /// Latest Solana SBPF revision (`SBPFVersion::V4`).
    V4,
}

impl SbpfVersion {
    /// SIMD-0166: manual stack frame bump (`add64 r10, imm`) instead of fixed
    /// per-call frame pointer advances.
    #[inline]
    pub const fn manual_stack_frame_bump(self) -> bool {
        !matches!(self, Self::V0)
    }

    /// Fixed stack frame gaps between call frames (`V0` only).
    #[inline]
    pub const fn stack_frame_gaps(self) -> bool {
        matches!(self, Self::V0)
    }

    /// SIMD-0174: Product / Quotient / Remainder (`BPF_PQR = 0x06`) instruction
    /// class enabled (`V2` only).
    #[inline]
    pub const fn enable_pqr(self) -> bool {
        matches!(self, Self::V2)
    }

    /// SIMD-0174: In `V2`, `ADD32` and `SUB32` zero-extend to 64 bits while
    /// `MOV32_REG` sign-extends to 64 bits. In `V0`, `V1`, `V3`, `V4`,
    /// `ADD32`, `SUB32`, and `MUL32` sign-extend their 32-bit result to 64 bits
    /// while `MOV32_REG` zero-extends.
    #[inline]
    pub const fn explicit_sign_extension_of_results(self) -> bool {
        matches!(self, Self::V2)
    }

    /// SIMD-0174: In `V2`, `SUB32_IMM` and `SUB64_IMM` compute `dst = imm - dst`.
    #[inline]
    pub const fn swap_sub_reg_imm_operands(self) -> bool {
        matches!(self, Self::V2)
    }

    /// SIMD-0174: `NEG32` and `NEG64` are disabled in `V2`.
    #[inline]
    pub const fn disable_neg(self) -> bool {
        matches!(self, Self::V2)
    }

    /// SIMD-0174: `CALLX` (`0x8d`) reads the target register index from `src`
    /// in `V2`.
    #[inline]
    pub const fn callx_uses_src_reg(self) -> bool {
        matches!(self, Self::V2)
    }

    /// SIMD-0178: `CALLX` (`0x8d`) reads the target register index from `dst`
    /// in `V3` and `V4` (in `V0`/`V1`, `CALLX` reads the register index from `imm`).
    #[inline]
    pub const fn callx_uses_dst_reg(self) -> bool {
        matches!(self, Self::V3 | Self::V4)
    }

    /// SIMD-0174: `LD_DW_IMM` (`0x18`) is disabled in `V2` and replaced by
    /// `HOR64_IMM` (`0xf7`).
    #[inline]
    pub const fn disable_lddw(self) -> bool {
        matches!(self, Self::V2)
    }

    /// SIMD-0174: `LE` (`0xd4`) is disabled in `V2`.
    #[inline]
    pub const fn disable_le(self) -> bool {
        matches!(self, Self::V2)
    }

    /// SIMD-0174: Standard `LDX`/`ST`/`STX` classes are disabled in `V2` and
    /// moved into `BPF_ALU32_LOAD` (`0x04`) and `BPF_ALU64_STORE` (`0x07`).
    #[inline]
    pub const fn move_memory_instruction_classes(self) -> bool {
        matches!(self, Self::V2)
    }

    /// SIMD-0178: 32-bit conditional jumps (`BPF_JMP32 = 0x06`) are enabled in
    /// `V3` and `V4`.
    #[inline]
    pub const fn enable_jmp32(self) -> bool {
        matches!(self, Self::V3 | Self::V4)
    }

    /// SIMD-0178: `CALL_IMM` (`0x85`) uses `src == 0` for external syscalls and
    /// `src == 1` for PC-relative internal calls in `V3` and `V4`.
    #[inline]
    pub const fn static_syscalls(self) -> bool {
        matches!(self, Self::V3 | Self::V4)
    }
}

/// Architectural dialect of the BPF engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EbpfDialect {
    /// IETF RFC 9669 / Linux kernel standard eBPF (CPU v1–v4).
    #[default]
    Rfc9669,
    /// Solana Bytecode Format (`SBPF` V0–V4).
    Solana(SbpfVersion),
}

impl EbpfDialect {
    /// Returns `true` if this dialect is Solana SBPF.
    #[inline]
    pub const fn is_solana(self) -> bool {
        matches!(self, Self::Solana(_))
    }

    /// Returns the Solana SBPF version if this dialect is `Solana`.
    #[inline]
    pub const fn sbpf_version(self) -> Option<SbpfVersion> {
        match self {
            Self::Rfc9669 => None,
            Self::Solana(v) => Some(v),
        }
    }
}

/// Operand width for ALU and jump operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AluWidth {
    /// 32-bit operation (`BPF_ALU` / `BPF_JMP32`).
    W32,
    /// 64-bit operation (`BPF_ALU64` / `BPF_JMP64`).
    W64,
}

/// Source operand for ALU and jump instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EbpfSrc {
    /// 32-bit signed immediate field (`BPF_K`).
    Imm(i32),
    /// Source register (`BPF_X`, `r0..r10`).
    Reg(u8),
}

/// Memory access size in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EbpfMemWidth {
    /// 1-byte access (`BPF_B` / `BPF_1B`).
    B1,
    /// 2-byte access (`BPF_H` / `BPF_2B`).
    B2,
    /// 4-byte access (`BPF_W` / `BPF_4B`).
    B4,
    /// 8-byte access (`BPF_DW` / `BPF_8B`).
    B8,
}

impl EbpfMemWidth {
    /// Width in bytes (`1`, `2`, `4`, or `8`).
    #[inline]
    pub const fn bytes(self) -> usize {
        match self {
            Self::B1 => 1,
            Self::B2 => 2,
            Self::B4 => 4,
            Self::B8 => 8,
        }
    }
}

/// Binary arithmetic / logical operation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinAluOp {
    /// `dst += src` (zero-extended for 32-bit in RFC 9669 and SBPF V2;
    /// sign-extended to 64 bits for 32-bit in SBPF V0/V1/V3/V4).
    Add { sign_extend_32: bool },
    /// `dst = dst - src` (or `dst = src - dst` when `swap_operands` is set in SBPF V2).
    Sub {
        sign_extend_32: bool,
        swap_operands: bool,
    },
    /// `dst *= src` (low product; sign-extended to 64 bits for 32-bit in SBPF V0/V1/V3/V4).
    Mul { sign_extend_32: bool },
    /// Unsigned high 64 bits of 64x64->128 product (`UHMUL64` in SBPF V2 PQR;
    /// immediate is zero-extended from `u32`).
    Uhmul64,
    /// Signed high 64 bits of 64x64->128 product (`SHMUL64` in SBPF V2 PQR;
    /// immediate is sign-extended from `i32`).
    Shmul64,
    /// Unsigned division (`DIV` / `UDIV`).
    /// `imm_zero_extended_64` is `true` for SBPF V2 `UDIV64_IMM` and `false` for
    /// RFC 9669 / SBPF V0–V1 `DIV64_IMM`.
    Udiv {
        trap_on_zero: bool,
        imm_zero_extended_64: bool,
    },
    /// Signed division (`SDIV` in RFC 9669 CPU v4 and SBPF V2 PQR).
    Sdiv { trap_on_zero_or_overflow: bool },
    /// Unsigned remainder (`MOD` / `UREM`).
    Urem {
        trap_on_zero: bool,
        imm_zero_extended_64: bool,
    },
    /// Signed remainder (`SMOD` in RFC 9669 CPU v4 / `SREM` in SBPF V2 PQR).
    Srem { trap_on_zero_or_overflow: bool },
    /// Bitwise OR (`dst |= src`).
    Or,
    /// Bitwise AND (`dst &= src`).
    And,
    /// Logical shift left (`dst <<= (src & mask)`).
    Lsh,
    /// Logical shift right (`dst >>= (src & mask)`).
    Rsh,
    /// Bitwise XOR (`dst ^= src`).
    Xor,
    /// Register/immediate move (`dst = src`).
    Mov {
        /// For `MOVSX` (`off` in `{8, 16, 32}`) or SBPF V2 `MOV32_REG` (`Some(32)`),
        /// sign-extends from the specified bit width (`8`, `16`, or `32`) to the
        /// operation width (`32` or `64`), then zero-extends `W32` to 64 bits
        /// unless `sign_extend_to_64` is set.
        sign_extend_from_bits: Option<u8>,
        /// When `true` (SBPF V2 `MOV32_REG` or `ALU64` `MOVSX`), sign-extends
        /// all the way to 64 bits.
        sign_extend_to_64: bool,
    },
    /// Arithmetic shift right (`dst = (dst as signed) >> (src & mask)`).
    Arsh,
    /// High-half bitwise OR (`HOR64_IMM` in SBPF V2: `dst |= (imm as u32 as u64) << 32`).
    Hor64Imm,
}

/// Byte-order / endianness conversion kind (`BPF_END` / `BSWAP`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndianOp {
    /// Convert between host endianness and little-endian (`BPF_TO_LE`, `0xd4`).
    ToLe(u8),
    /// Convert between host endianness and big-endian (`BPF_TO_BE`, `0xdc`).
    ToBe(u8),
    /// Unconditional byte swap (`BSWAP`, `0xd7` in RFC 9669 CPU v4).
    Bswap(u8),
}

/// Conditional branch comparison kind (`BPF_JMP` / `BPF_JMP32`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JmpCond {
    /// `dst == src` (`BPF_JEQ`, `0x10`).
    Jeq,
    /// `dst > src` unsigned (`BPF_JGT`, `0x20`).
    Jgt,
    /// `dst >= src` unsigned (`BPF_JGE`, `0x30`).
    Jge,
    /// `(dst & src) != 0` (`BPF_JSET`, `0x40`).
    Jset,
    /// `dst != src` (`BPF_JNE`, `0x50`).
    Jne,
    /// `dst > src` signed (`BPF_JSGT`, `0x60`).
    Jsgt,
    /// `dst >= src` signed (`BPF_JSGE`, `0x70`).
    Jsge,
    /// `dst < src` unsigned (`BPF_JLT`, `0xa0`).
    Jlt,
    /// `dst <= src` unsigned (`BPF_JLE`, `0xb0`).
    Jle,
    /// `dst < src` signed (`BPF_JSLT`, `0xc0`).
    Jslt,
    /// `dst <= src` signed (`BPF_JSLE`, `0xd0`).
    Jsle,
}

/// Atomic memory operation (`BPF_ATOMIC`, RFC 9669 §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomicKind {
    /// Atomic add (`*(dst + off) += src`, optionally fetching old value into `src`).
    Add { fetch: bool },
    /// Atomic bitwise OR (`*(dst + off) |= src`, optionally fetching old value into `src`).
    Or { fetch: bool },
    /// Atomic bitwise AND (`*(dst + off) &= src`, optionally fetching old value into `src`).
    And { fetch: bool },
    /// Atomic bitwise XOR (`*(dst + off) ^= src`, optionally fetching old value into `src`).
    Xor { fetch: bool },
    /// Atomic exchange (`src = atomic_xchg(dst + off, src)`).
    Xchg,
    /// Atomic compare-and-exchange (`r0 = atomic_cmpxchg(dst + off, r0, src)`).
    Cmpxchg,
}

/// Decoded semantic operation for an eBPF or Solana SBPF instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EbpfOp {
    /// 128-bit load immediate (`LD_DW_IMM`, opcode `0x18`, 16 bytes).
    /// `src_type` holds `insn.src` (`0x0` for standard 64-bit constant load,
    /// `0x1..0x6` for RFC 9669 map/btf pseudo-loads).
    LoadImm64 { dst: u8, src_type: u8, imm64: u64 },
    /// Memory load into register (`LDX` / `LDX_MEMSX` / SBPF V2 `LD_*B_REG`).
    LoadReg {
        dst: u8,
        base: u8,
        off: i16,
        width: EbpfMemWidth,
        sign_extend: bool,
    },
    /// Memory store of immediate (`ST` / SBPF V2 `ST_*B_IMM`).
    StoreImm {
        base: u8,
        off: i16,
        imm: i32,
        width: EbpfMemWidth,
    },
    /// Memory store of register (`STX` / SBPF V2 `ST_*B_REG`).
    StoreReg {
        base: u8,
        off: i16,
        src: u8,
        width: EbpfMemWidth,
    },
    /// Atomic memory read-modify-write (`BPF_ATOMIC`, RFC 9669 §5.3).
    Atomic {
        base: u8,
        off: i16,
        src: u8,
        width: EbpfMemWidth,
        kind: AtomicKind,
    },
    /// Binary ALU operation (`BPF_ALU`, `BPF_ALU64`, or SBPF V2 `BPF_PQR`).
    BinAlu {
        width: AluWidth,
        op: BinAluOp,
        dst: u8,
        src: EbpfSrc,
    },
    /// Unary negation (`NEG32` / `NEG64`).
    Neg { width: AluWidth, dst: u8 },
    /// Byte-order conversion (`LE`, `BE`, `BSWAP`).
    Endian { dst: u8, op: EndianOp },
    /// Unconditional PC-relative jump (`JA` 16-bit offset or `JA32` 32-bit offset in 8-byte slots).
    Jump { offset_insns: i32 },
    /// Conditional PC-relative jump (`JEQ`, `JGT`, etc.).
    JumpCond {
        width: AluWidth,
        cond: JmpCond,
        dst: u8,
        src: EbpfSrc,
        offset_insns: i16,
    },
    /// Helper / syscall or PC-relative local call (`CALL_IMM`, `0x85`).
    CallImm { src_kind: u8, imm: i32 },
    /// Indirect register call (`CALLX` / `CALL_REG`, `0x8d`).
    CallReg { target_reg: u8 },
    /// Function return / program exit (`EXIT`, `0x95`).
    Exit,
}

/// Raw 8-byte (or 16-byte) eBPF instruction fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EbpfInsn {
    /// Primary opcode byte (`opc`).
    pub opc: u8,
    /// Destination register number (`0..=10`).
    pub dst: u8,
    /// Source register number (`0..=10`).
    pub src: u8,
    /// Signed 16-bit offset field (`off`).
    pub off: i16,
    /// Signed 32-bit immediate field (`imm`) from the first 8-byte slot.
    pub imm: i32,
    /// Second 32-bit immediate field (`imm2`) when `size == 16` (`LD_DW_IMM`).
    pub imm2: i32,
    /// Instruction size in bytes (`8` or `16`).
    pub size: u8,
    /// Decoded semantic operation.
    pub op: EbpfOp,
}

impl EbpfInsn {
    /// Encode this instruction into 8 (or 16) bytes in the given endianness.
    pub fn encode(&self, endian: Endian) -> Vec<u8> {
        let regs = match endian {
            Endian::Little => (self.src << 4) | (self.dst & 0x0f),
            Endian::Big => (self.dst << 4) | (self.src & 0x0f),
        };
        let off_bytes = match endian {
            Endian::Little => self.off.to_le_bytes(),
            Endian::Big => self.off.to_be_bytes(),
        };
        let imm_bytes = match endian {
            Endian::Little => self.imm.to_le_bytes(),
            Endian::Big => self.imm.to_be_bytes(),
        };
        let mut out = Vec::with_capacity(self.size as usize);
        out.push(self.opc);
        out.push(regs);
        out.extend_from_slice(&off_bytes);
        out.extend_from_slice(&imm_bytes);
        if self.size == 16 {
            let imm2_bytes = match endian {
                Endian::Little => self.imm2.to_le_bytes(),
                Endian::Big => self.imm2.to_be_bytes(),
            };
            out.extend_from_slice(&[0, 0, 0, 0]);
            out.extend_from_slice(&imm2_bytes);
        }
        out
    }
}

/// Errors produced while decoding an eBPF or Solana SBPF instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EbpfDecodeError {
    /// Fewer than 8 bytes available for a basic instruction, or fewer than 16
    /// bytes available for a wide `LD_DW_IMM` instruction.
    UnexpectedEof { needed: usize, available: usize },
    /// Register index outside `0..=10`.
    InvalidRegister { reg: u8 },
    /// Unknown or dialect-disabled opcode byte.
    InvalidOpcode { opc: u8 },
    /// Non-zero reserved fields (`src`, `off`, `imm`, or second slot of `LD_DW_IMM`).
    ReservedField { opc: u8 },
    /// Invalid atomic operation immediate (`imm`) on `BPF_ATOMIC`.
    InvalidAtomicImm { opc: u8, imm: i32 },
    /// Invalid endianness width immediate (must be `16`, `32`, or `64`).
    InvalidEndianImm { opc: u8, imm: i32 },
}

impl fmt::Display for EbpfDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { needed, available } => write!(
                f,
                "truncated eBPF instruction: needed {needed} bytes, got {available}"
            ),
            Self::InvalidRegister { reg } => {
                write!(f, "invalid eBPF register index r{reg} (must be 0..=10)")
            }
            Self::InvalidOpcode { opc } => write!(f, "invalid or disabled eBPF opcode 0x{opc:02x}"),
            Self::ReservedField { opc } => {
                write!(f, "non-zero reserved field in eBPF opcode 0x{opc:02x}")
            }
            Self::InvalidAtomicImm { opc, imm } => write!(
                f,
                "invalid atomic immediate 0x{imm:x} for eBPF opcode 0x{opc:02x}"
            ),
            Self::InvalidEndianImm { opc, imm } => write!(
                f,
                "invalid endian immediate {imm} for eBPF opcode 0x{opc:02x}"
            ),
        }
    }
}

impl std::error::Error for EbpfDecodeError {}

#[inline]
fn validate_reg(reg: u8) -> Result<u8, EbpfDecodeError> {
    if reg <= FRAME_PTR_REG {
        Ok(reg)
    } else {
        Err(EbpfDecodeError::InvalidRegister { reg })
    }
}

/// Decode one eBPF or Solana SBPF instruction from `bytes`.
pub fn decode_insn(
    bytes: &[u8],
    dialect: EbpfDialect,
    endian: Endian,
) -> Result<EbpfInsn, EbpfDecodeError> {
    if bytes.len() < INSN_SIZE {
        return Err(EbpfDecodeError::UnexpectedEof {
            needed: INSN_SIZE,
            available: bytes.len(),
        });
    }

    let opc = bytes[0];
    let regs = bytes[1];
    let (dst, src) = match endian {
        Endian::Little => (regs & 0x0f, regs >> 4),
        Endian::Big => (regs >> 4, regs & 0x0f),
    };
    let off = match endian {
        Endian::Little => i16::from_le_bytes([bytes[2], bytes[3]]),
        Endian::Big => i16::from_be_bytes([bytes[2], bytes[3]]),
    };
    let imm = match endian {
        Endian::Little => i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        Endian::Big => i32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
    };

    let sbpf = dialect.sbpf_version();

    // 1. Wide 16-byte instruction: LD_DW_IMM (0x18)
    if opc == 0x18 {
        if sbpf.is_some_and(|v| v.disable_lddw()) {
            return Err(EbpfDecodeError::InvalidOpcode { opc });
        }
        if bytes.len() < WIDE_INSN_SIZE {
            return Err(EbpfDecodeError::UnexpectedEof {
                needed: WIDE_INSN_SIZE,
                available: bytes.len(),
            });
        }
        validate_reg(dst)?;
        if dialect == EbpfDialect::Rfc9669 {
            // RFC 9669 §5.4: src in 0..=6, second slot opcode must be 0.
            if src > 6 || bytes[8] != 0 || bytes[9] != 0 || bytes[10] != 0 || bytes[11] != 0 {
                return Err(EbpfDecodeError::ReservedField { opc });
            }
        } else if bytes[8] != 0 {
            return Err(EbpfDecodeError::ReservedField { opc });
        }
        let imm2 = match endian {
            Endian::Little => i32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
            Endian::Big => i32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        };
        let imm64 = ((imm2 as u32 as u64) << 32) | (imm as u32 as u64);
        return Ok(EbpfInsn {
            opc,
            dst,
            src,
            off,
            imm,
            imm2,
            size: 16,
            op: EbpfOp::LoadImm64 {
                dst,
                src_type: src,
                imm64,
            },
        });
    }

    // 2. Solana SBPF V2 relocated memory instructions and HOR64_IMM / PQR class
    if let Some(ver) = sbpf {
        if ver.move_memory_instruction_classes() {
            match opc {
                0x2c | 0x3c | 0x8c | 0x9c => {
                    validate_reg(dst)?;
                    validate_reg(src)?;
                    let width = match opc {
                        0x2c => EbpfMemWidth::B1,
                        0x3c => EbpfMemWidth::B2,
                        0x8c => EbpfMemWidth::B4,
                        0x9c => EbpfMemWidth::B8,
                        _ => unreachable!(),
                    };
                    return Ok(EbpfInsn {
                        opc,
                        dst,
                        src,
                        off,
                        imm,
                        imm2: 0,
                        size: 8,
                        op: EbpfOp::LoadReg {
                            dst,
                            base: src,
                            off,
                            width,
                            sign_extend: false,
                        },
                    });
                }
                0x27 | 0x37 | 0x87 | 0x97 => {
                    validate_reg(dst)?;
                    let width = match opc {
                        0x27 => EbpfMemWidth::B1,
                        0x37 => EbpfMemWidth::B2,
                        0x87 => EbpfMemWidth::B4,
                        0x97 => EbpfMemWidth::B8,
                        _ => unreachable!(),
                    };
                    return Ok(EbpfInsn {
                        opc,
                        dst,
                        src,
                        off,
                        imm,
                        imm2: 0,
                        size: 8,
                        op: EbpfOp::StoreImm {
                            base: dst,
                            off,
                            imm,
                            width,
                        },
                    });
                }
                0x2f | 0x3f | 0x8f | 0x9f => {
                    validate_reg(dst)?;
                    validate_reg(src)?;
                    let width = match opc {
                        0x2f => EbpfMemWidth::B1,
                        0x3f => EbpfMemWidth::B2,
                        0x8f => EbpfMemWidth::B4,
                        0x9f => EbpfMemWidth::B8,
                        _ => unreachable!(),
                    };
                    return Ok(EbpfInsn {
                        opc,
                        dst,
                        src,
                        off,
                        imm,
                        imm2: 0,
                        size: 8,
                        op: EbpfOp::StoreReg {
                            base: dst,
                            off,
                            src,
                            width,
                        },
                    });
                }
                _ => {}
            }
        }

        if ver.disable_lddw() && opc == 0xf7 {
            validate_reg(dst)?;
            return Ok(EbpfInsn {
                opc,
                dst,
                src,
                off,
                imm,
                imm2: 0,
                size: 8,
                op: EbpfOp::BinAlu {
                    width: AluWidth::W64,
                    op: BinAluOp::Hor64Imm,
                    dst,
                    src: EbpfSrc::Imm(imm),
                },
            });
        }

        if ver.enable_pqr() && (opc & 0x07) == 0x06 {
            validate_reg(dst)?;
            let is_reg = (opc & 0x08) != 0;
            let src_op = if is_reg {
                validate_reg(src)?;
                EbpfSrc::Reg(src)
            } else {
                if !is_reg
                    && imm == 0
                    && matches!(opc, 0x46 | 0x56 | 0x66 | 0x76 | 0xc6 | 0xd6 | 0xe6 | 0xf6)
                {
                    // Immediate division/remainder by 0 is rejected by the SBPF verifier,
                    // or traps at runtime; we allow decoding and set trap_on_zero = true.
                }
                EbpfSrc::Imm(imm)
            };
            let (width, alu_op) = match opc {
                0x86 | 0x8e => (
                    AluWidth::W32,
                    BinAluOp::Mul {
                        sign_extend_32: false,
                    },
                ),
                0x96 | 0x9e => (
                    AluWidth::W64,
                    BinAluOp::Mul {
                        sign_extend_32: false,
                    },
                ),
                0x36 | 0x3e => (AluWidth::W64, BinAluOp::Uhmul64),
                0xb6 | 0xbe => (AluWidth::W64, BinAluOp::Shmul64),
                0x46 | 0x4e => (
                    AluWidth::W32,
                    BinAluOp::Udiv {
                        trap_on_zero: true,
                        imm_zero_extended_64: true,
                    },
                ),
                0x56 | 0x5e => (
                    AluWidth::W64,
                    BinAluOp::Udiv {
                        trap_on_zero: true,
                        imm_zero_extended_64: true,
                    },
                ),
                0x66 | 0x6e => (
                    AluWidth::W32,
                    BinAluOp::Urem {
                        trap_on_zero: true,
                        imm_zero_extended_64: true,
                    },
                ),
                0x76 | 0x7e => (
                    AluWidth::W64,
                    BinAluOp::Urem {
                        trap_on_zero: true,
                        imm_zero_extended_64: true,
                    },
                ),
                0xc6 | 0xce => (
                    AluWidth::W32,
                    BinAluOp::Sdiv {
                        trap_on_zero_or_overflow: true,
                    },
                ),
                0xd6 | 0xde => (
                    AluWidth::W64,
                    BinAluOp::Sdiv {
                        trap_on_zero_or_overflow: true,
                    },
                ),
                0xe6 | 0xee => (
                    AluWidth::W32,
                    BinAluOp::Srem {
                        trap_on_zero_or_overflow: true,
                    },
                ),
                0xf6 | 0xfe => (
                    AluWidth::W64,
                    BinAluOp::Srem {
                        trap_on_zero_or_overflow: true,
                    },
                ),
                _ => return Err(EbpfDecodeError::InvalidOpcode { opc }),
            };
            return Ok(EbpfInsn {
                opc,
                dst,
                src,
                off,
                imm,
                imm2: 0,
                size: 8,
                op: EbpfOp::BinAlu {
                    width,
                    op: alu_op,
                    dst,
                    src: src_op,
                },
            });
        }
    }

    let class = opc & 0x07;
    match class {
        // BPF_LDX (0x01), BPF_ST (0x02), BPF_STX (0x03)
        0x01..=0x03 => {
            if sbpf.is_some_and(|v| v.move_memory_instruction_classes()) {
                return Err(EbpfDecodeError::InvalidOpcode { opc });
            }
            let mode = opc & 0xe0;
            let size_bits = opc & 0x18;
            let width = match size_bits {
                0x00 => EbpfMemWidth::B4,
                0x08 => EbpfMemWidth::B2,
                0x10 => EbpfMemWidth::B1,
                0x18 => EbpfMemWidth::B8,
                _ => unreachable!(),
            };

            match (class, mode) {
                // Standard zero-extending load: BPF_LDX | BPF_MEM
                (0x01, 0x60) => {
                    validate_reg(dst)?;
                    validate_reg(src)?;
                    Ok(EbpfInsn {
                        opc,
                        dst,
                        src,
                        off,
                        imm,
                        imm2: 0,
                        size: 8,
                        op: EbpfOp::LoadReg {
                            dst,
                            base: src,
                            off,
                            width,
                            sign_extend: false,
                        },
                    })
                }
                // RFC 9669 CPU v4 sign-extending load: BPF_LDX | BPF_MEMSX (0x80) for B1/B2/B4
                (0x01, 0x80) if dialect == EbpfDialect::Rfc9669 && width != EbpfMemWidth::B8 => {
                    validate_reg(dst)?;
                    validate_reg(src)?;
                    Ok(EbpfInsn {
                        opc,
                        dst,
                        src,
                        off,
                        imm,
                        imm2: 0,
                        size: 8,
                        op: EbpfOp::LoadReg {
                            dst,
                            base: src,
                            off,
                            width,
                            sign_extend: true,
                        },
                    })
                }
                // Standard store immediate: BPF_ST | BPF_MEM
                (0x02, 0x60) => {
                    validate_reg(dst)?;
                    Ok(EbpfInsn {
                        opc,
                        dst,
                        src,
                        off,
                        imm,
                        imm2: 0,
                        size: 8,
                        op: EbpfOp::StoreImm {
                            base: dst,
                            off,
                            imm,
                            width,
                        },
                    })
                }
                // Standard store register: BPF_STX | BPF_MEM
                (0x03, 0x60) => {
                    validate_reg(dst)?;
                    validate_reg(src)?;
                    Ok(EbpfInsn {
                        opc,
                        dst,
                        src,
                        off,
                        imm,
                        imm2: 0,
                        size: 8,
                        op: EbpfOp::StoreReg {
                            base: dst,
                            off,
                            src,
                            width,
                        },
                    })
                }
                // RFC 9669 §5.3 atomic operations: BPF_STX | BPF_ATOMIC (0xc0) for B4 (0xc3) and B8 (0xdb)
                (0x03, 0xc0)
                    if dialect == EbpfDialect::Rfc9669
                        && matches!(width, EbpfMemWidth::B4 | EbpfMemWidth::B8) =>
                {
                    validate_reg(dst)?;
                    validate_reg(src)?;
                    let kind = match imm {
                        0x00 => AtomicKind::Add { fetch: false },
                        0x01 => AtomicKind::Add { fetch: true },
                        0x40 => AtomicKind::Or { fetch: false },
                        0x41 => AtomicKind::Or { fetch: true },
                        0x50 => AtomicKind::And { fetch: false },
                        0x51 => AtomicKind::And { fetch: true },
                        0xa0 => AtomicKind::Xor { fetch: false },
                        0xa1 => AtomicKind::Xor { fetch: true },
                        0xe1 => AtomicKind::Xchg,
                        0xf1 => AtomicKind::Cmpxchg,
                        _ => return Err(EbpfDecodeError::InvalidAtomicImm { opc, imm }),
                    };
                    Ok(EbpfInsn {
                        opc,
                        dst,
                        src,
                        off,
                        imm,
                        imm2: 0,
                        size: 8,
                        op: EbpfOp::Atomic {
                            base: dst,
                            off,
                            src,
                            width,
                            kind,
                        },
                    })
                }
                _ => Err(EbpfDecodeError::InvalidOpcode { opc }),
            }
        }

        // BPF_ALU (0x04) and BPF_ALU64 (0x07)
        0x04 | 0x07 => {
            validate_reg(dst)?;
            let width = if class == 0x07 {
                AluWidth::W64
            } else {
                AluWidth::W32
            };
            let code = opc & 0xf0;
            let is_reg = (opc & 0x08) != 0;

            // Endianness / byte-swap instructions (code == 0xd0)
            if code == 0xd0 {
                let width_bits = match imm {
                    16 => 16u8,
                    32 => 32u8,
                    64 => 64u8,
                    _ => return Err(EbpfDecodeError::InvalidEndianImm { opc, imm }),
                };
                let endian_op = match (class, is_reg) {
                    (0x04, false) => {
                        if sbpf.is_some_and(|v| v.disable_le()) {
                            return Err(EbpfDecodeError::InvalidOpcode { opc });
                        }
                        EndianOp::ToLe(width_bits)
                    }
                    (0x04, true) => EndianOp::ToBe(width_bits),
                    (0x07, false) if dialect == EbpfDialect::Rfc9669 => EndianOp::Bswap(width_bits),
                    _ => return Err(EbpfDecodeError::InvalidOpcode { opc }),
                };
                return Ok(EbpfInsn {
                    opc,
                    dst,
                    src,
                    off,
                    imm,
                    imm2: 0,
                    size: 8,
                    op: EbpfOp::Endian { dst, op: endian_op },
                });
            }

            // Unary NEG (code == 0x80)
            if code == 0x80 {
                if is_reg || sbpf.is_some_and(|v| v.disable_neg()) {
                    return Err(EbpfDecodeError::InvalidOpcode { opc });
                }
                return Ok(EbpfInsn {
                    opc,
                    dst,
                    src,
                    off,
                    imm,
                    imm2: 0,
                    size: 8,
                    op: EbpfOp::Neg { width, dst },
                });
            }

            let src_op = if is_reg {
                validate_reg(src)?;
                EbpfSrc::Reg(src)
            } else {
                EbpfSrc::Imm(imm)
            };

            let sign_ext_alu32 = sbpf.is_some_and(|v| !v.explicit_sign_extension_of_results());
            let swap_sub = !is_reg && sbpf.is_some_and(|v| v.swap_sub_reg_imm_operands());

            let alu_op = match code {
                0x00 => BinAluOp::Add {
                    sign_extend_32: sign_ext_alu32,
                },
                0x10 => BinAluOp::Sub {
                    sign_extend_32: sign_ext_alu32,
                    swap_operands: swap_sub,
                },
                0x20 => {
                    if sbpf.is_some_and(|v| v.enable_pqr()) {
                        return Err(EbpfDecodeError::InvalidOpcode { opc });
                    }
                    BinAluOp::Mul {
                        sign_extend_32: sign_ext_alu32,
                    }
                }
                0x30 => {
                    if sbpf.is_some_and(|v| v.enable_pqr()) {
                        return Err(EbpfDecodeError::InvalidOpcode { opc });
                    }
                    match (dialect, off) {
                        (EbpfDialect::Rfc9669, 0) => BinAluOp::Udiv {
                            trap_on_zero: false,
                            imm_zero_extended_64: false,
                        },
                        (EbpfDialect::Rfc9669, 1) => BinAluOp::Sdiv {
                            trap_on_zero_or_overflow: false,
                        },
                        (EbpfDialect::Rfc9669, _) => {
                            return Err(EbpfDecodeError::ReservedField { opc });
                        }
                        (EbpfDialect::Solana(_), _) => BinAluOp::Udiv {
                            trap_on_zero: true,
                            imm_zero_extended_64: false,
                        },
                    }
                }
                0x40 => BinAluOp::Or,
                0x50 => BinAluOp::And,
                0x60 => BinAluOp::Lsh,
                0x70 => BinAluOp::Rsh,
                0x90 => {
                    if sbpf.is_some_and(|v| v.enable_pqr()) {
                        return Err(EbpfDecodeError::InvalidOpcode { opc });
                    }
                    match (dialect, off) {
                        (EbpfDialect::Rfc9669, 0) => BinAluOp::Urem {
                            trap_on_zero: false,
                            imm_zero_extended_64: false,
                        },
                        (EbpfDialect::Rfc9669, 1) => BinAluOp::Srem {
                            trap_on_zero_or_overflow: false,
                        },
                        (EbpfDialect::Rfc9669, _) => {
                            return Err(EbpfDecodeError::ReservedField { opc });
                        }
                        (EbpfDialect::Solana(_), _) => BinAluOp::Urem {
                            trap_on_zero: true,
                            imm_zero_extended_64: false,
                        },
                    }
                }
                0xa0 => BinAluOp::Xor,
                0xb0 => match (dialect, is_reg, off) {
                    // RFC 9669 MOVSX (CPU v4): register source only, off in {8, 16} for ALU32
                    // and {8, 16, 32} for ALU64.
                    (EbpfDialect::Rfc9669, true, 8 | 16) => BinAluOp::Mov {
                        sign_extend_from_bits: Some(off as u8),
                        sign_extend_to_64: width == AluWidth::W64,
                    },
                    (EbpfDialect::Rfc9669, true, 32) if width == AluWidth::W64 => BinAluOp::Mov {
                        sign_extend_from_bits: Some(32),
                        sign_extend_to_64: true,
                    },
                    (EbpfDialect::Rfc9669, _, 0) => BinAluOp::Mov {
                        sign_extend_from_bits: None,
                        sign_extend_to_64: false,
                    },
                    (EbpfDialect::Rfc9669, _, _) => {
                        return Err(EbpfDecodeError::ReservedField { opc });
                    }
                    // Solana SBPF V2 MOV32_REG sign-extends 32->64.
                    (EbpfDialect::Solana(ver), true, _)
                        if width == AluWidth::W32 && ver.explicit_sign_extension_of_results() =>
                    {
                        BinAluOp::Mov {
                            sign_extend_from_bits: Some(32),
                            sign_extend_to_64: true,
                        }
                    }
                    (EbpfDialect::Solana(_), _, _) => BinAluOp::Mov {
                        sign_extend_from_bits: None,
                        sign_extend_to_64: false,
                    },
                },
                0xc0 => BinAluOp::Arsh,
                _ => return Err(EbpfDecodeError::InvalidOpcode { opc }),
            };

            Ok(EbpfInsn {
                opc,
                dst,
                src,
                off,
                imm,
                imm2: 0,
                size: 8,
                op: EbpfOp::BinAlu {
                    width,
                    op: alu_op,
                    dst,
                    src: src_op,
                },
            })
        }

        // BPF_JMP64 (0x05) and BPF_JMP32 (0x06)
        0x05 | 0x06 => {
            if class == 0x06 {
                if let Some(ver) = sbpf
                    && !ver.enable_jmp32()
                {
                    return Err(EbpfDecodeError::InvalidOpcode { opc });
                }
            }
            let width = if class == 0x05 {
                AluWidth::W64
            } else {
                AluWidth::W32
            };
            let code = opc & 0xf0;
            let is_reg = (opc & 0x08) != 0;

            match (class, code, is_reg) {
                // JA (0x05): unconditional 16-bit offset jump
                (0x05, 0x00, false) => Ok(EbpfInsn {
                    opc,
                    dst,
                    src,
                    off,
                    imm,
                    imm2: 0,
                    size: 8,
                    op: EbpfOp::Jump {
                        offset_insns: i32::from(off),
                    },
                }),
                // JA32 (0x06): unconditional 32-bit immediate offset jump (RFC 9669 CPU v4)
                (0x06, 0x00, false) if dialect == EbpfDialect::Rfc9669 => Ok(EbpfInsn {
                    opc,
                    dst,
                    src,
                    off,
                    imm,
                    imm2: 0,
                    size: 8,
                    op: EbpfOp::Jump { offset_insns: imm },
                }),
                // CALL_IMM (0x85)
                (0x05, 0x80, false) => {
                    if dialect == EbpfDialect::Rfc9669 && src > 2 {
                        return Err(EbpfDecodeError::ReservedField { opc });
                    }
                    Ok(EbpfInsn {
                        opc,
                        dst,
                        src,
                        off,
                        imm,
                        imm2: 0,
                        size: 8,
                        op: EbpfOp::CallImm { src_kind: src, imm },
                    })
                }
                // CALLX / CALL_REG (0x8d)
                (0x05, 0x80, true) => {
                    let target_reg = match sbpf {
                        Some(ver) if ver.callx_uses_src_reg() => validate_reg(src)?,
                        Some(ver) if ver.callx_uses_dst_reg() => validate_reg(dst)?,
                        Some(_) => {
                            if !(0..=10).contains(&imm) {
                                return Err(EbpfDecodeError::InvalidRegister { reg: imm as u8 });
                            }
                            imm as u8
                        }
                        None => validate_reg(dst)?,
                    };
                    Ok(EbpfInsn {
                        opc,
                        dst,
                        src,
                        off,
                        imm,
                        imm2: 0,
                        size: 8,
                        op: EbpfOp::CallReg { target_reg },
                    })
                }
                // EXIT (0x95)
                (0x05, 0x90, false) => Ok(EbpfInsn {
                    opc,
                    dst,
                    src,
                    off,
                    imm,
                    imm2: 0,
                    size: 8,
                    op: EbpfOp::Exit,
                }),
                // Conditional jumps (0x10..=0x70, 0xa0..=0xd0)
                (_, 0x10..=0x70 | 0xa0..=0xd0, _) => {
                    validate_reg(dst)?;
                    let src_op = if is_reg {
                        validate_reg(src)?;
                        EbpfSrc::Reg(src)
                    } else {
                        EbpfSrc::Imm(imm)
                    };
                    let cond = match code {
                        0x10 => JmpCond::Jeq,
                        0x20 => JmpCond::Jgt,
                        0x30 => JmpCond::Jge,
                        0x40 => JmpCond::Jset,
                        0x50 => JmpCond::Jne,
                        0x60 => JmpCond::Jsgt,
                        0x70 => JmpCond::Jsge,
                        0xa0 => JmpCond::Jlt,
                        0xb0 => JmpCond::Jle,
                        0xc0 => JmpCond::Jslt,
                        0xd0 => JmpCond::Jsle,
                        _ => unreachable!(),
                    };
                    Ok(EbpfInsn {
                        opc,
                        dst,
                        src,
                        off,
                        imm,
                        imm2: 0,
                        size: 8,
                        op: EbpfOp::JumpCond {
                            width,
                            cond,
                            dst,
                            src: src_op,
                            offset_insns: off,
                        },
                    })
                }
                _ => Err(EbpfDecodeError::InvalidOpcode { opc }),
            }
        }

        _ => Err(EbpfDecodeError::InvalidOpcode { opc }),
    }
}
