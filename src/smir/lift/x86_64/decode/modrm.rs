//! decode/modrm.rs

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind, X86VecAlign, X86VecMap,
    X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant,
    X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth,
    X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
    X86InstructionBytes,
};
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

// ============================================================================
// ModR/M and SIB Decoding
// ============================================================================

/// Decoded ModR/M result
#[derive(Clone, Debug)]
pub struct ModRm {
    /// ModR/M byte value
    pub byte: u8,
    /// mod field (0-3)
    pub mod_bits: u8,
    /// reg field with REX.R (0-15)
    pub reg: u8,
    /// r/m field with REX.B (0-15)
    pub rm: u8,
    /// Is this a memory operand (mod != 3)?
    pub is_memory: bool,
    /// Decoded memory address (if is_memory)
    pub addr: Option<X86Address>,
    /// Total bytes consumed (including SIB and displacement)
    pub bytes_consumed: usize,
}

/// x86 memory address representation for lifting
#[derive(Clone, Debug)]
pub struct X86Address {
    /// Base register (None for absolute addresses)
    pub base: Option<u8>,
    /// Index register (None if no index)
    pub index: Option<u8>,
    /// Scale (1, 2, 4, or 8)
    pub scale: u8,
    /// Displacement
    pub disp: i64,
    /// RIP-relative addressing?
    pub rip_relative: bool,
    /// Address calculation width. In 64-bit mode a `67h` override selects
    /// modulo-2^32 base/index/displacement arithmetic whose result is
    /// zero-extended before an FS/GS segment base is added.
    pub address_width: OpWidth,
    /// Displacement size hint
    pub disp_size: DispSize,
    /// FS/GS segment override, if any (`X86Reg::FsBase` / `X86Reg::GsBase`). In
    /// 64-bit mode CS/DS/ES/SS are flat (base 0) and recorded as `None`.
    pub segment: Option<X86Reg>,
}

/// Decode ModR/M byte and any following SIB/displacement
pub(crate) fn decode_modrm(
    bytes: &[u8],
    prefix: &X86Prefix,
    addr: u64,
) -> Result<ModRm, LiftError> {
    if bytes.is_empty() {
        return Err(LiftError::Incomplete {
            addr,
            have: 0,
            need: 1,
        });
    }

    let modrm = bytes[0];
    let mod_bits = modrm >> 6;
    let reg_field = (modrm >> 3) & 0x07;
    let rm_field = modrm & 0x07;

    let reg = reg_field | prefix.rex_r();
    let rm = rm_field | prefix.rex_b();

    if mod_bits == 3 {
        // Register operand
        return Ok(ModRm {
            byte: modrm,
            mod_bits,
            reg,
            rm,
            is_memory: false,
            addr: None,
            bytes_consumed: 1,
        });
    }

    // FS (0x64) / GS (0x65) overrides carry a non-zero segment base in long mode
    // (TLS / per-CPU data); the lifted memory operand becomes an
    // `Address::SegmentRel` that adds the FsBase/GsBase register. CS/DS/ES/SS
    // overrides are flat/zero-based in long mode and carry no base, so they are
    // left as ordinary addresses (segment = None).
    let segment = match prefix.segment_override {
        Some(0x64) => Some(X86Reg::FsBase),
        Some(0x65) => Some(X86Reg::GsBase),
        _ => None,
    };

    // Memory operand - decode SIB and displacement
    let mut consumed = 1;
    let mut x86_addr = X86Address {
        base: None,
        index: None,
        scale: 1,
        disp: 0,
        rip_relative: false,
        address_width: if prefix.address_size_override {
            OpWidth::W32
        } else {
            OpWidth::W64
        },
        disp_size: DispSize::Auto,
        segment,
    };

    if rm_field == 4 {
        // SIB byte follows
        if bytes.len() < 2 {
            return Err(LiftError::Incomplete {
                addr,
                have: bytes.len(),
                need: 2,
            });
        }
        let sib = bytes[1];
        consumed += 1;

        let scale = 1u8 << (sib >> 6);
        let index_field = (sib >> 3) & 0x07;
        let base_field = sib & 0x07;

        let index = index_field | prefix.rex_x();
        let base = base_field | prefix.rex_b();

        x86_addr.scale = scale;

        // Index = 4 means no index
        if index != 4 {
            x86_addr.index = Some(index);
        }

        // Handle base
        if base_field == 5 && mod_bits == 0 {
            // No base, disp32 follows
            if bytes.len() < consumed + 4 {
                return Err(LiftError::Incomplete {
                    addr,
                    have: bytes.len(),
                    need: consumed + 4,
                });
            }
            let disp = i32::from_le_bytes([
                bytes[consumed],
                bytes[consumed + 1],
                bytes[consumed + 2],
                bytes[consumed + 3],
            ]) as i64;
            consumed += 4;
            x86_addr.disp = disp;
            x86_addr.disp_size = DispSize::Disp32;
        } else {
            x86_addr.base = Some(base);
        }
    } else if rm_field == 5 && mod_bits == 0 {
        // In the default 64-bit address size this is RIP-relative. Under a
        // 67h override the same encoding is a zero-extended absolute disp32.
        if bytes.len() < 5 {
            return Err(LiftError::Incomplete {
                addr,
                have: bytes.len(),
                need: 5,
            });
        }
        let disp = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as i64;
        consumed += 4;
        x86_addr.disp = disp;
        x86_addr.rip_relative = !prefix.address_size_override;
        x86_addr.disp_size = DispSize::Disp32;
    } else {
        // Regular register indirect
        x86_addr.base = Some(rm);
    }

    // Handle displacement for mod=1 (disp8) and mod=2 (disp32)
    match mod_bits {
        1 => {
            if bytes.len() < consumed + 1 {
                return Err(LiftError::Incomplete {
                    addr,
                    have: bytes.len(),
                    need: consumed + 1,
                });
            }
            x86_addr.disp = bytes[consumed] as i8 as i64;
            consumed += 1;
            x86_addr.disp_size = DispSize::Disp8;
        }
        2 => {
            if bytes.len() < consumed + 4 {
                return Err(LiftError::Incomplete {
                    addr,
                    have: bytes.len(),
                    need: consumed + 4,
                });
            }
            x86_addr.disp = i32::from_le_bytes([
                bytes[consumed],
                bytes[consumed + 1],
                bytes[consumed + 2],
                bytes[consumed + 3],
            ]) as i64;
            consumed += 4;
            x86_addr.disp_size = DispSize::Disp32;
        }
        _ => {}
    }

    Ok(ModRm {
        byte: modrm,
        mod_bits,
        reg,
        rm,
        is_memory: true,
        addr: Some(x86_addr),
        bytes_consumed: consumed,
    })
}
