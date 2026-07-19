//! decode/prefix.rs

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
// Prefix Decoding
// ============================================================================

/// Lookup table for prefix detection
pub(crate) static PREFIX_LUT: [u8; 256] = {
    let mut lut = [0u8; 256];
    // Segment overrides
    lut[0x26] = 1; // ES
    lut[0x2E] = 1; // CS
    lut[0x36] = 1; // SS
    lut[0x3E] = 1; // DS
    lut[0x64] = 1; // FS
    lut[0x65] = 1; // GS
    // Operand/address size
    lut[0x66] = 1;
    lut[0x67] = 1;
    // LOCK, REP
    lut[0xF0] = 1;
    lut[0xF2] = 1;
    lut[0xF3] = 1;
    // REX (0x40-0x4F)
    let mut i = 0x40u8;
    while i <= 0x4F {
        lut[i as usize] = 1;
        i += 1;
    }
    // REX2 (APX)
    lut[0xD5] = 1;
    lut
};

/// Decoded APX REX2 prefix state.
///
/// Payload layout follows LLVM/Intel APX encoding: `M R4 X4 B4 W R3 X3 B3`.
/// The `*_hi` bits add 16 and the `*_lo` bits add 8 to the corresponding
/// ModR/M, SIB, or opcode-register field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rex2Prefix {
    pub m: bool,
    pub w: bool,
    pub r_hi: bool,
    pub x_hi: bool,
    pub b_hi: bool,
    pub r_lo: bool,
    pub x_lo: bool,
    pub b_lo: bool,
}

impl Rex2Prefix {
    #[inline]
    pub(crate) fn r_ext(self) -> u8 {
        (if self.r_hi { 16 } else { 0 }) | (if self.r_lo { 8 } else { 0 })
    }

    #[inline]
    pub(crate) fn x_ext(self) -> u8 {
        (if self.x_hi { 16 } else { 0 }) | (if self.x_lo { 8 } else { 0 })
    }

    #[inline]
    pub(crate) fn b_ext(self) -> u8 {
        (if self.b_hi { 16 } else { 0 }) | (if self.b_lo { 8 } else { 0 })
    }
}

/// Decoded x86 instruction prefix state
#[derive(Clone, Debug, Default)]
pub struct X86Prefix {
    /// REX prefix if present
    pub rex: Option<u8>,
    /// REX2 prefix if present (APX)
    pub rex2: Option<Rex2Prefix>,
    /// Operand size override (0x66)
    pub operand_size_override: bool,
    /// Address size override (0x67)
    pub address_size_override: bool,
    /// REP/REPNE prefix
    pub rep_prefix: Option<u8>,
    /// Segment override
    pub segment_override: Option<u8>,
    /// LOCK prefix
    pub lock: bool,
    /// Cursor position after prefixes
    pub cursor: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VecEncodingKind {
    Vex,
    Evex,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VecPrefix {
    pub(crate) encoding: VecEncodingKind,
    pub(crate) map: X86VecMap,
    pub(crate) pp: X86SsePrefix,
    pub(crate) width: VecWidth,
    pub(crate) l_bits: u8,
    pub(crate) w: bool,
    pub(crate) vvvv: u8,
    pub(crate) rex: Option<u8>,
    pub(crate) aaa: u8,
    pub(crate) zeroing: bool,
    pub(crate) b: bool,
    pub(crate) reg_high: bool,
    pub(crate) rm_high: bool,
    pub(crate) v_high: bool,
    pub(crate) address_size_override: bool,
    pub(crate) segment_override: Option<u8>,
    pub(crate) bytes: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ApxEvexPrefix {
    pub(crate) bytes: usize,
    pub(crate) r: bool,
    pub(crate) x: bool,
    pub(crate) vvvv: u8,
    pub(crate) r_prime: bool,
    pub(crate) v_prime: bool,
    pub(crate) w: bool,
    pub(crate) pp: u8,
    pub(crate) operand_size_override: bool,
    pub(crate) nd: bool,
    pub(crate) nf: bool,
    pub(crate) z: bool,
    pub(crate) ll: u8,
    pub(crate) aaa: u8,
    pub(crate) b: bool,
    pub(crate) b4: bool,
    pub(crate) x4: bool,
}

impl ApxEvexPrefix {
    pub(crate) fn rm_ext(self) -> u8 {
        let b_ext = if self.b { 0 } else { 8 };
        let b4_ext = if self.b4 { 16 } else { 0 };
        b_ext | b4_ext
    }

    pub(crate) fn reg_ext(self) -> u8 {
        let r_ext = if self.r { 0 } else { 8 };
        let r_prime_ext = if self.r_prime { 0 } else { 16 };
        r_ext | r_prime_ext
    }

    pub(crate) fn index_ext(self) -> u8 {
        let x_ext = if self.x { 0 } else { 8 };
        let x4_ext = if self.x4 { 0 } else { 16 };
        x_ext | x4_ext
    }

    pub(crate) fn vvvv_reg(self) -> u8 {
        let v_prime_ext = if self.v_prime { 0 } else { 16 };
        (self.vvvv ^ 0x0F) | v_prime_ext
    }

    pub(crate) fn op_size(self, is_byte: bool) -> u8 {
        if is_byte {
            1
        } else if self.w {
            8
        } else if self.operand_size_override {
            2
        } else {
            4
        }
    }

    pub(crate) fn flags(self) -> FlagUpdate {
        if self.nf {
            FlagUpdate::None
        } else {
            FlagUpdate::All
        }
    }

    pub(crate) fn ccmp_cond(self) -> u8 {
        ((self.v_prime as u8) << 3) | self.aaa
    }

    pub(crate) fn ccmp_default_flags(self) -> u8 {
        self.vvvv
    }

    pub(crate) fn as_modrm_prefix(self, cursor: usize) -> X86Prefix {
        X86Prefix {
            rex2: Some(Rex2Prefix {
                m: false,
                w: self.w,
                r_hi: (self.reg_ext() & 16) != 0,
                x_hi: (self.index_ext() & 16) != 0,
                b_hi: (self.rm_ext() & 16) != 0,
                r_lo: (self.reg_ext() & 8) != 0,
                x_lo: (self.index_ext() & 8) != 0,
                b_lo: (self.rm_ext() & 8) != 0,
            }),
            cursor,
            operand_size_override: self.operand_size_override,
            ..X86Prefix::default()
        }
    }
}

impl X86Prefix {
    /// Get REX.W flag
    #[inline]
    pub fn rex_w(&self) -> bool {
        self.rex2
            .map_or_else(|| self.rex.map_or(false, |r| r & 0x08 != 0), |r| r.w)
    }

    /// Get REX.R flag (extends ModR/M reg field)
    #[inline]
    pub fn rex_r(&self) -> u8 {
        self.rex2
            .map_or_else(|| self.rex.map_or(0, |r| (r & 0x04) << 1), |r| r.r_ext())
    }

    /// Get REX.X flag (extends SIB index field)
    #[inline]
    pub fn rex_x(&self) -> u8 {
        self.rex2
            .map_or_else(|| self.rex.map_or(0, |r| (r & 0x02) << 2), |r| r.x_ext())
    }

    /// Get REX.B flag (extends ModR/M r/m or opcode reg)
    #[inline]
    pub fn rex_b(&self) -> u8 {
        self.rex2
            .map_or_else(|| self.rex.map_or(0, |r| (r & 0x01) << 3), |r| r.b_ext())
    }

    /// Check if any REX prefix is present
    #[inline]
    pub fn has_rex(&self) -> bool {
        self.rex.is_some() || self.rex2.is_some()
    }

    /// Check if REX2 selects the compressed 0F opcode map.
    #[inline]
    pub fn rex2_m(&self) -> bool {
        self.rex2.map_or(false, |r| r.m)
    }

    /// Compute operand size for 64-bit mode
    #[inline]
    pub fn op_size(&self) -> u8 {
        if self.rex_w() {
            8
        } else if self.operand_size_override {
            2
        } else {
            4
        }
    }

    /// Compute operand width for SMIR
    #[inline]
    pub fn op_width(&self) -> OpWidth {
        match self.op_size() {
            1 => OpWidth::W8,
            2 => OpWidth::W16,
            4 => OpWidth::W32,
            8 => OpWidth::W64,
            _ => OpWidth::W32,
        }
    }
}

/// Decode instruction prefixes
pub(crate) fn decode_prefixes(bytes: &[u8]) -> Result<X86Prefix, LiftError> {
    if bytes.is_empty() {
        return Err(LiftError::Incomplete {
            addr: 0,
            have: 0,
            need: 1,
        });
    }

    let mut prefix = X86Prefix::default();
    let mut cursor = 0;

    while cursor < bytes.len() {
        let b = bytes[cursor];
        if PREFIX_LUT[b as usize] == 0 {
            break;
        }

        match b {
            0x66 => {
                prefix.rex = None;
                prefix.operand_size_override = true;
            }
            0x67 => {
                prefix.rex = None;
                prefix.address_size_override = true;
            }
            0x40..=0x4F => prefix.rex = Some(b),
            0xD5 => {
                cursor += 1;
                if cursor >= bytes.len() {
                    return Err(LiftError::Incomplete {
                        addr: 0,
                        have: bytes.len(),
                        need: cursor + 1,
                    });
                }
                let payload = bytes[cursor];
                prefix.rex2 = Some(Rex2Prefix {
                    m: (payload & 0x80) != 0,
                    r_hi: (payload & 0x40) != 0,
                    x_hi: (payload & 0x20) != 0,
                    b_hi: (payload & 0x10) != 0,
                    w: (payload & 0x08) != 0,
                    r_lo: (payload & 0x04) != 0,
                    x_lo: (payload & 0x02) != 0,
                    b_lo: (payload & 0x01) != 0,
                });
                cursor += 1;
                break;
            }
            0xF0 => {
                prefix.rex = None;
                prefix.lock = true;
            }
            0xF2 | 0xF3 => {
                prefix.rex = None;
                prefix.rep_prefix = Some(b);
            }
            0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 => {
                prefix.rex = None;
                prefix.segment_override = Some(b);
            }
            _ => break,
        }
        cursor += 1;
    }

    prefix.cursor = cursor;
    Ok(prefix)
}

pub(crate) fn vex_pp_to_prefix(pp: u8) -> X86SsePrefix {
    match pp & 0x3 {
        0 => X86SsePrefix::None,
        1 => X86SsePrefix::OpSize,
        2 => X86SsePrefix::Rep,
        _ => X86SsePrefix::Repne,
    }
}

pub(crate) fn vec_map_from_bits(map: u8) -> Option<X86VecMap> {
    match map {
        0x01 => Some(X86VecMap::Map0F),
        0x02 => Some(X86VecMap::Map0F38),
        0x03 => Some(X86VecMap::Map0F3A),
        0x05 => Some(X86VecMap::Map5),
        0x06 => Some(X86VecMap::Map6),
        _ => None,
    }
}

pub(crate) fn build_rex(r: u8, x: u8, b: u8, w: bool) -> Option<u8> {
    let mut rex = 0x40;
    if w {
        rex |= 0x08;
    }
    if r != 0 {
        rex |= 0x04;
    }
    if x != 0 {
        rex |= 0x02;
    }
    if b != 0 {
        rex |= 0x01;
    }
    if rex == 0x40 { None } else { Some(rex) }
}

pub(crate) fn decode_vex_prefix(bytes: &[u8], addr: u64) -> Result<VecPrefix, LiftError> {
    if bytes.is_empty() {
        return Err(LiftError::Incomplete {
            addr,
            have: 0,
            need: 1,
        });
    }

    match bytes[0] {
        0xC5 => {
            if bytes.len() < 2 {
                return Err(LiftError::Incomplete {
                    addr,
                    have: bytes.len(),
                    need: 2,
                });
            }
            let b1 = bytes[1];
            let r = ((b1 >> 7) & 1) ^ 1;
            let vvvv = (!b1 >> 3) & 0x0F;
            let l = (b1 >> 2) & 1;
            let pp = vex_pp_to_prefix(b1 & 0x3);

            Ok(VecPrefix {
                encoding: VecEncodingKind::Vex,
                map: X86VecMap::Map0F,
                pp,
                width: if l == 1 {
                    VecWidth::V256
                } else {
                    VecWidth::V128
                },
                l_bits: l,
                w: false,
                vvvv,
                rex: build_rex(r, 0, 0, false),
                aaa: 0,
                zeroing: false,
                b: false,
                reg_high: false,
                rm_high: false,
                v_high: false,
                address_size_override: false,
                segment_override: None,
                bytes: 2,
            })
        }
        0xC4 => {
            if bytes.len() < 3 {
                return Err(LiftError::Incomplete {
                    addr,
                    have: bytes.len(),
                    need: 3,
                });
            }
            let b1 = bytes[1];
            let b2 = bytes[2];
            let r = ((b1 >> 7) & 1) ^ 1;
            let x = ((b1 >> 6) & 1) ^ 1;
            let b = ((b1 >> 5) & 1) ^ 1;
            let map = vec_map_from_bits(b1 & 0x1F).ok_or_else(|| LiftError::Unsupported {
                addr,
                mnemonic: format!("VEX map 0x{:02X}", b1 & 0x1F),
            })?;
            let w = (b2 >> 7) & 1 != 0;
            let vvvv = (!b2 >> 3) & 0x0F;
            let l = (b2 >> 2) & 1;
            let pp = vex_pp_to_prefix(b2 & 0x3);

            Ok(VecPrefix {
                encoding: VecEncodingKind::Vex,
                map,
                pp,
                width: if l == 1 {
                    VecWidth::V256
                } else {
                    VecWidth::V128
                },
                l_bits: l,
                w,
                vvvv,
                rex: build_rex(r, x, b, w),
                aaa: 0,
                zeroing: false,
                b: false,
                reg_high: false,
                rm_high: false,
                v_high: false,
                address_size_override: false,
                segment_override: None,
                bytes: 3,
            })
        }
        _ => Err(LiftError::Unsupported {
            addr,
            mnemonic: "VEX prefix".to_string(),
        }),
    }
}

pub(crate) fn decode_evex_prefix(bytes: &[u8], addr: u64) -> Result<VecPrefix, LiftError> {
    if bytes.len() < 4 {
        return Err(LiftError::Incomplete {
            addr,
            have: bytes.len(),
            need: 4,
        });
    }

    let b1 = bytes[1];
    let b2 = bytes[2];
    let b3 = bytes[3];

    // EVEX prefix decoding is structural. Opcode-family lifters consume or
    // reject write masks, zeroing, broadcast, SAE, and embedded rounding so
    // adding a family does not require a second semantic opcode allowlist.

    let r = ((b1 >> 7) & 1) ^ 1;
    let r_prime = ((b1 >> 4) & 1) ^ 1;
    let x = ((b1 >> 6) & 1) ^ 1;
    let b = ((b1 >> 5) & 1) ^ 1;
    let map_bits = b1 & 0x07;
    let map = vec_map_from_bits(map_bits).ok_or_else(|| LiftError::Unsupported {
        addr,
        mnemonic: format!("EVEX map 0x{map_bits:02X}"),
    })?;

    let w = (b2 >> 7) & 1 != 0;
    let vvvv = (!b2 >> 3) & 0x0F;
    let v_prime = ((b3 >> 3) & 1) ^ 1;
    let pp = vex_pp_to_prefix(b2 & 0x3);

    let l_bits = (b3 >> 5) & 0x3;
    let width = match l_bits {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => VecWidth::V512,
    };

    Ok(VecPrefix {
        encoding: VecEncodingKind::Evex,
        map,
        pp,
        width,
        l_bits,
        w,
        vvvv,
        rex: build_rex(r, x, b, w),
        aaa: b3 & 0x07,
        zeroing: b3 & 0x80 != 0,
        b: b3 & 0x10 != 0,
        reg_high: r_prime != 0,
        rm_high: x != 0,
        v_high: v_prime != 0,
        address_size_override: false,
        segment_override: None,
        bytes: 4,
    })
}

pub(crate) fn decode_apx_evex_prefix(bytes: &[u8], addr: u64) -> Result<ApxEvexPrefix, LiftError> {
    decode_apx_evex_prefix_for_map(bytes, addr, 4)
}

pub(crate) fn decode_apx_evex_prefix_for_map(
    bytes: &[u8],
    addr: u64,
    expected_mm: u8,
) -> Result<ApxEvexPrefix, LiftError> {
    if bytes.len() < 4 {
        return Err(LiftError::Incomplete {
            addr,
            have: bytes.len(),
            need: 4,
        });
    }

    let p0 = bytes[1];
    let p1 = bytes[2];
    let p2 = bytes[3];
    let mm = p0 & 0x07;
    if mm != expected_mm {
        return Err(LiftError::Unsupported {
            addr,
            mnemonic: format!("EVEX map 0x{mm:02X}"),
        });
    }

    Ok(ApxEvexPrefix {
        bytes: 4,
        r: (p0 & 0x80) != 0,
        x: (p0 & 0x40) != 0,
        vvvv: (p1 >> 3) & 0x0F,
        r_prime: (p0 & 0x10) != 0,
        v_prime: (p2 & 0x08) != 0,
        w: (p1 & 0x80) != 0,
        pp: p1 & 0x03,
        operand_size_override: (p1 & 0x03) == 0x01,
        nd: (p2 & 0x10) != 0,
        nf: (p2 & 0x04) != 0,
        z: (p2 & 0x80) != 0,
        ll: (p2 >> 5) & 0x03,
        aaa: p2 & 0x07,
        b: (p0 & 0x20) != 0,
        b4: (p0 & 0x08) != 0,
        x4: (p1 & 0x04) != 0,
    })
}
