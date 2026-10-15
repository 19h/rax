//! Disassembler for eBPF (RFC 9669) and Solana SBPF (V0–V4) instructions.

use crate::isa::ebpf::decode::{
    AluWidth, AtomicKind, BinAluOp, EbpfInsn, EbpfMemWidth, EbpfOp, EbpfSrc, EndianOp, JmpCond,
};

impl EbpfInsn {
    /// Canonical instruction mnemonic for this decoded instruction.
    pub fn mnemonic(&self) -> &'static str {
        match self.op {
            EbpfOp::LoadImm64 { .. } => "lddw",
            EbpfOp::LoadReg {
                width,
                sign_extend: false,
                ..
            } => match width {
                EbpfMemWidth::B1 => "ldxb",
                EbpfMemWidth::B2 => "ldxh",
                EbpfMemWidth::B4 => "ldxw",
                EbpfMemWidth::B8 => "ldxdw",
            },
            EbpfOp::LoadReg {
                width,
                sign_extend: true,
                ..
            } => match width {
                EbpfMemWidth::B1 => "ldxsb",
                EbpfMemWidth::B2 => "ldxsh",
                EbpfMemWidth::B4 => "ldxsw",
                EbpfMemWidth::B8 => "ldxdw",
            },
            EbpfOp::StoreImm { width, .. } => match width {
                EbpfMemWidth::B1 => "stb",
                EbpfMemWidth::B2 => "sth",
                EbpfMemWidth::B4 => "stw",
                EbpfMemWidth::B8 => "stdw",
            },
            EbpfOp::StoreReg { width, .. } => match width {
                EbpfMemWidth::B1 => "stxb",
                EbpfMemWidth::B2 => "stxh",
                EbpfMemWidth::B4 => "stxw",
                EbpfMemWidth::B8 => "stxdw",
            },
            EbpfOp::Atomic { width, kind, .. } => match (width, kind) {
                (EbpfMemWidth::B4, AtomicKind::Add { fetch: false }) => "lock_add32",
                (EbpfMemWidth::B4, AtomicKind::Add { fetch: true }) => "lock_fetch_add32",
                (EbpfMemWidth::B4, AtomicKind::Or { fetch: false }) => "lock_or32",
                (EbpfMemWidth::B4, AtomicKind::Or { fetch: true }) => "lock_fetch_or32",
                (EbpfMemWidth::B4, AtomicKind::And { fetch: false }) => "lock_and32",
                (EbpfMemWidth::B4, AtomicKind::And { fetch: true }) => "lock_fetch_and32",
                (EbpfMemWidth::B4, AtomicKind::Xor { fetch: false }) => "lock_xor32",
                (EbpfMemWidth::B4, AtomicKind::Xor { fetch: true }) => "lock_fetch_xor32",
                (EbpfMemWidth::B4, AtomicKind::Xchg) => "xchg32",
                (EbpfMemWidth::B4, AtomicKind::Cmpxchg) => "cmpxchg32",
                (_, AtomicKind::Add { fetch: false }) => "lock_add64",
                (_, AtomicKind::Add { fetch: true }) => "lock_fetch_add64",
                (_, AtomicKind::Or { fetch: false }) => "lock_or64",
                (_, AtomicKind::Or { fetch: true }) => "lock_fetch_or64",
                (_, AtomicKind::And { fetch: false }) => "lock_and64",
                (_, AtomicKind::And { fetch: true }) => "lock_fetch_and64",
                (_, AtomicKind::Xor { fetch: false }) => "lock_xor64",
                (_, AtomicKind::Xor { fetch: true }) => "lock_fetch_xor64",
                (_, AtomicKind::Xchg) => "xchg64",
                (_, AtomicKind::Cmpxchg) => "cmpxchg64",
            },
            EbpfOp::BinAlu { width, op, .. } => match (width, op) {
                (AluWidth::W32, BinAluOp::Add { .. }) => "add32",
                (AluWidth::W64, BinAluOp::Add { .. }) => "add64",
                (AluWidth::W32, BinAluOp::Sub { .. }) => "sub32",
                (AluWidth::W64, BinAluOp::Sub { .. }) => "sub64",
                (AluWidth::W32, BinAluOp::Mul { .. }) => {
                    if (self.opc & 0x07) == 0x06 {
                        "lmul32"
                    } else {
                        "mul32"
                    }
                }
                (AluWidth::W64, BinAluOp::Mul { .. }) => {
                    if (self.opc & 0x07) == 0x06 {
                        "lmul64"
                    } else {
                        "mul64"
                    }
                }
                (_, BinAluOp::Uhmul64) => "uhmul64",
                (_, BinAluOp::Shmul64) => "shmul64",
                (AluWidth::W32, BinAluOp::Udiv { .. }) => {
                    if (self.opc & 0x07) == 0x06 {
                        "udiv32"
                    } else {
                        "div32"
                    }
                }
                (AluWidth::W64, BinAluOp::Udiv { .. }) => {
                    if (self.opc & 0x07) == 0x06 {
                        "udiv64"
                    } else {
                        "div64"
                    }
                }
                (AluWidth::W32, BinAluOp::Sdiv { .. }) => "sdiv32",
                (AluWidth::W64, BinAluOp::Sdiv { .. }) => "sdiv64",
                (AluWidth::W32, BinAluOp::Urem { .. }) => {
                    if (self.opc & 0x07) == 0x06 {
                        "urem32"
                    } else {
                        "mod32"
                    }
                }
                (AluWidth::W64, BinAluOp::Urem { .. }) => {
                    if (self.opc & 0x07) == 0x06 {
                        "urem64"
                    } else {
                        "mod64"
                    }
                }
                (AluWidth::W32, BinAluOp::Srem { .. }) => {
                    if (self.opc & 0x07) == 0x06 {
                        "srem32"
                    } else {
                        "smod32"
                    }
                }
                (AluWidth::W64, BinAluOp::Srem { .. }) => {
                    if (self.opc & 0x07) == 0x06 {
                        "srem64"
                    } else {
                        "smod64"
                    }
                }
                (AluWidth::W32, BinAluOp::Or) => "or32",
                (AluWidth::W64, BinAluOp::Or) => "or64",
                (AluWidth::W32, BinAluOp::And) => "and32",
                (AluWidth::W64, BinAluOp::And) => "and64",
                (AluWidth::W32, BinAluOp::Lsh) => "lsh32",
                (AluWidth::W64, BinAluOp::Lsh) => "lsh64",
                (AluWidth::W32, BinAluOp::Rsh) => "rsh32",
                (AluWidth::W64, BinAluOp::Rsh) => "rsh64",
                (AluWidth::W32, BinAluOp::Xor) => "xor32",
                (AluWidth::W64, BinAluOp::Xor) => "xor64",
                (
                    AluWidth::W32,
                    BinAluOp::Mov {
                        sign_extend_from_bits: Some(8 | 16),
                        ..
                    },
                ) => "movsx32",
                (
                    AluWidth::W64,
                    BinAluOp::Mov {
                        sign_extend_from_bits: Some(_),
                        ..
                    },
                ) => "movsx64",
                (AluWidth::W32, BinAluOp::Mov { .. }) => "mov32",
                (AluWidth::W64, BinAluOp::Mov { .. }) => "mov64",
                (AluWidth::W32, BinAluOp::Arsh) => "arsh32",
                (AluWidth::W64, BinAluOp::Arsh) => "arsh64",
                (_, BinAluOp::Hor64Imm) => "hor64",
            },
            EbpfOp::Neg {
                width: AluWidth::W32,
                ..
            } => "neg32",
            EbpfOp::Neg {
                width: AluWidth::W64,
                ..
            } => "neg64",
            EbpfOp::Endian { op, .. } => match op {
                EndianOp::ToLe(16) => "le16",
                EndianOp::ToLe(32) => "le32",
                EndianOp::ToLe(_) => "le64",
                EndianOp::ToBe(16) => "be16",
                EndianOp::ToBe(32) => "be32",
                EndianOp::ToBe(_) => "be64",
                EndianOp::Bswap(16) => "bswap16",
                EndianOp::Bswap(32) => "bswap32",
                EndianOp::Bswap(_) => "bswap64",
            },
            EbpfOp::Jump { .. } => {
                if self.opc == 0x06 {
                    "ja32"
                } else {
                    "ja"
                }
            }
            EbpfOp::JumpCond { width, cond, .. } => match (width, cond) {
                (AluWidth::W64, JmpCond::Jeq) => "jeq",
                (AluWidth::W64, JmpCond::Jgt) => "jgt",
                (AluWidth::W64, JmpCond::Jge) => "jge",
                (AluWidth::W64, JmpCond::Jset) => "jset",
                (AluWidth::W64, JmpCond::Jne) => "jne",
                (AluWidth::W64, JmpCond::Jsgt) => "jsgt",
                (AluWidth::W64, JmpCond::Jsge) => "jsge",
                (AluWidth::W64, JmpCond::Jlt) => "jlt",
                (AluWidth::W64, JmpCond::Jle) => "jle",
                (AluWidth::W64, JmpCond::Jslt) => "jslt",
                (AluWidth::W64, JmpCond::Jsle) => "jsle",
                (AluWidth::W32, JmpCond::Jeq) => "jeq32",
                (AluWidth::W32, JmpCond::Jgt) => "jgt32",
                (AluWidth::W32, JmpCond::Jge) => "jge32",
                (AluWidth::W32, JmpCond::Jset) => "jset32",
                (AluWidth::W32, JmpCond::Jne) => "jne32",
                (AluWidth::W32, JmpCond::Jsgt) => "jsgt32",
                (AluWidth::W32, JmpCond::Jsge) => "jsge32",
                (AluWidth::W32, JmpCond::Jlt) => "jlt32",
                (AluWidth::W32, JmpCond::Jle) => "jle32",
                (AluWidth::W32, JmpCond::Jslt) => "jslt32",
                (AluWidth::W32, JmpCond::Jsle) => "jsle32",
            },
            EbpfOp::CallImm { .. } => "call",
            EbpfOp::CallReg { .. } => "callx",
            EbpfOp::Exit => "exit",
        }
    }

    /// Format this instruction as assembly text.
    pub fn disassemble(&self) -> String {
        let mnem = self.mnemonic();
        match self.op {
            EbpfOp::LoadImm64 { dst, imm64, .. } => {
                format!("{mnem} r{dst}, 0x{imm64:x}")
            }
            EbpfOp::LoadReg { dst, base, off, .. } => {
                format!("{mnem} r{dst}, [r{base}{off:+}]")
            }
            EbpfOp::StoreImm { base, off, imm, .. } => {
                format!("{mnem} [r{base}{off:+}], {imm}")
            }
            EbpfOp::StoreReg { base, off, src, .. } | EbpfOp::Atomic { base, off, src, .. } => {
                format!("{mnem} [r{base}{off:+}], r{src}")
            }
            EbpfOp::BinAlu { dst, src, .. } => match src {
                EbpfSrc::Imm(imm) => format!("{mnem} r{dst}, {imm}"),
                EbpfSrc::Reg(s) => format!("{mnem} r{dst}, r{s}"),
            },
            EbpfOp::Neg { dst, .. } | EbpfOp::Endian { dst, .. } => {
                format!("{mnem} r{dst}")
            }
            EbpfOp::Jump { offset_insns } => {
                format!("{mnem} {offset_insns:+}")
            }
            EbpfOp::JumpCond {
                dst,
                src,
                offset_insns,
                ..
            } => match src {
                EbpfSrc::Imm(imm) => format!("{mnem} r{dst}, {imm}, {offset_insns:+}"),
                EbpfSrc::Reg(s) => format!("{mnem} r{dst}, r{s}, {offset_insns:+}"),
            },
            EbpfOp::CallImm { src_kind, imm } => {
                if src_kind == 1 {
                    format!("{mnem} pc{imm:+}")
                } else {
                    format!("{mnem} {imm}")
                }
            }
            EbpfOp::CallReg { target_reg } => {
                format!("{mnem} r{target_reg}")
            }
            EbpfOp::Exit => mnem.to_string(),
        }
    }
}
