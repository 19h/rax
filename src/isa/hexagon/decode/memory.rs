//! Scalar memory operands, with unscaled constant-extended byte offsets.

use super::*;

pub(super) fn load_io(
    decoded: &DecodedOp,
    width: MemWidth,
    sign: MemSign,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let dst = field_u8(decoded, b'd')?;
    let (offset, used) = decode_field_scaled_simm(decoded, b'i', immext, width_shift(width))?;
    Some((
        DecodedInsn::Load {
            dst,
            addr: AddrMode::Offset { base, offset },
            width,
            sign,
            pred: None,
        },
        used,
    ))
}

pub(super) fn load_gp(
    decoded: &DecodedOp,
    width: MemWidth,
    sign: MemSign,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let dst = field_u8(decoded, b'd')?;
    let (imm, used) = decode_field_uimm(decoded, b'i', immext)?;
    // A constant extender turns the GP-relative form into an *absolute* access
    // (the extended immediate is the full byte address; GP is not added).
    let addr = if used {
        AddrMode::Abs { addr: imm }
    } else {
        AddrMode::GpOffset {
            offset: (imm << width_shift(width)) as i32,
        }
    };
    Some((
        DecodedInsn::Load {
            dst,
            addr,
            width,
            sign,
            pred: None,
        },
        used,
    ))
}

pub(super) fn load_pi(
    decoded: &DecodedOp,
    width: MemWidth,
    sign: MemSign,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let dst = field_u8(decoded, b'd')?;
    let (imm, _) = decode_field_simm(decoded, b'i', None)?;
    let offset = imm.wrapping_shl(width_shift(width) as u32);
    Some((
        DecodedInsn::Load {
            dst,
            addr: AddrMode::PostIncImm { base, offset },
            width,
            sign,
            pred: None,
        },
        false,
    ))
}

pub(super) fn store_io(
    decoded: &DecodedOp,
    width: MemWidth,
    immext: Option<u32>,
    src_new: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let src = field_u8(decoded, b't')?;
    let (offset, used) = decode_field_scaled_simm(decoded, b'i', immext, width_shift(width))?;
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::Offset { base, offset },
            width,
            pred: None,
            src_new,
            high_half: false,
        },
        used,
    ))
}

/// `storerf` high-half store with register+immediate offset (`memh(Rs+#s11:1)
/// =Rt.h`, `S2_storerf_io`). Always `MemWidth::Half`; stores `Rt[31:16]`.
pub(super) fn store_io_high(
    decoded: &DecodedOp,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let src = field_u8(decoded, b't')?;
    let (offset, used) =
        decode_field_scaled_simm(decoded, b'i', immext, width_shift(MemWidth::Half))?;
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::Offset { base, offset },
            width: MemWidth::Half,
            pred: None,
            src_new: false,
            high_half: true,
        },
        used,
    ))
}

/// New-value store with register+immediate offset (`memX(Rs+#s11:N)=Nt8.new`).
pub(super) fn store_new_io(
    decoded: &DecodedOp,
    width: MemWidth,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let nt = field_u8(decoded, b't')?;
    let (offset, used) = decode_field_scaled_simm(decoded, b'i', immext, width_shift(width))?;
    Some((
        DecodedInsn::StoreNew {
            nt,
            addr: AddrMode::Offset { base, offset },
            width,
            pred: None,
        },
        used,
    ))
}

/// Predicated new-value store (`if ([!]Pv[.new]) memX(Rs+#u6:N)=Nt8.new`). The
/// store data is the `Nt8` producer (field `t`), resolved against the packet's
/// producers, not a direct register read — hence `StoreNew` with a predicate.
/// `pred_new` selects whether the guarding predicate is `.new` (tnew/fnew).
pub(super) fn pred_store_new_io(
    decoded: &DecodedOp,
    width: MemWidth,
    sense: bool,
    pred_new: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let nt = field_u8(decoded, b't')?;
    let pred = field_u8(decoded, b'v')?;
    let (imm, _) = decode_field_uimm(decoded, b'i', None)?;
    let offset = (imm << width_shift(width)) as i32;
    Some((
        DecodedInsn::StoreNew {
            nt,
            addr: AddrMode::Offset { base, offset },
            width,
            pred: Some(pred_cond(pred, sense, pred_new)),
        },
        false,
    ))
}

pub(super) fn store_gp(
    decoded: &DecodedOp,
    width: MemWidth,
    src_new: bool,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let src = field_u8(decoded, b't')?;
    let (imm, used) = decode_field_uimm(decoded, b'i', immext)?;
    let addr = if used {
        AddrMode::Abs { addr: imm }
    } else {
        AddrMode::GpOffset {
            offset: (imm << width_shift(width)) as i32,
        }
    };
    Some((
        DecodedInsn::Store {
            src,
            addr,
            width,
            pred: None,
            src_new,
            high_half: false,
        },
        used,
    ))
}

pub(super) fn store_pi(
    decoded: &DecodedOp,
    width: MemWidth,
    src_new: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let src = field_u8(decoded, b't')?;
    let (imm, _) = decode_field_simm(decoded, b'i', None)?;
    let offset = imm.wrapping_shl(width_shift(width) as u32);
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::PostIncImm { base, offset },
            width,
            pred: None,
            src_new,
            high_half: false,
        },
        false,
    ))
}

/// Base register-offset store (`memX(Rs+Ru<<#u2)=Rt[.h]`, `S4_storer*_rr`).
/// Fields: `s` = Rs base, `u` = Ru index, `i` = #u2 shift, `t` = data Rt. (Note
/// the load form `L4_loadr*_rr` instead carries the index in field `t`.)
pub(super) fn store_rr(
    decoded: &DecodedOp,
    width: MemWidth,
    high_half: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let index = field_u8(decoded, b'u')?;
    let src = field_u8(decoded, b't')?;
    let shift = decode_field_uimm(decoded, b'i', None)?.0 as u8;
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::RegScaled { base, index, shift },
            width,
            pred: None,
            src_new: false,
            high_half,
        },
        false,
    ))
}

/// NEW-VALUE register-offset store (`S4_storer*new_rr`). The data source is the
/// `Nt8` producer (field `t`); index is `u`, base is `s`, shift is `i`.
pub(super) fn store_new_rr(decoded: &DecodedOp, width: MemWidth) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let index = field_u8(decoded, b'u')?;
    let nt = field_u8(decoded, b't')?;
    let shift = decode_field_uimm(decoded, b'i', None)?.0 as u8;
    Some((
        DecodedInsn::StoreNew {
            nt,
            addr: AddrMode::RegScaled { base, index, shift },
            width,
            pred: None,
        },
        false,
    ))
}

/// Absolute-set store (`memX(Re=##U6)=Rt[.h]`, `S4_storer*_ap`). The EA is the
/// constant-extended `I` field; the address register `Re` (field `e`) is also
/// written with that absolute. Data is `Rt` (field `t`). Requires an extender.
pub(super) fn store_ap(
    decoded: &DecodedOp,
    width: MemWidth,
    high_half: bool,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let src = field_u8(decoded, b't')?;
    let areg = field_u8(decoded, b'e')?;
    let (addr, used) = decode_field_uimm(decoded, b'I', immext)?;
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::AbsSet { areg, addr },
            width,
            pred: None,
            src_new: false,
            high_half,
        },
        used,
    ))
}

/// NEW-VALUE absolute-set store (`S4_storer*new_ap`). Data source is the `Nt8`
/// producer (field `t`); the address register `Re` is also written.
pub(super) fn store_new_ap(
    decoded: &DecodedOp,
    width: MemWidth,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let nt = field_u8(decoded, b't')?;
    let areg = field_u8(decoded, b'e')?;
    let (addr, used) = decode_field_uimm(decoded, b'I', immext)?;
    Some((
        DecodedInsn::StoreNew {
            nt,
            addr: AddrMode::AbsSet { areg, addr },
            width,
            pred: None,
        },
        used,
    ))
}

/// Scaled-index absolute store (`memX(Ru<<#u2+##U6)=Rt[.h]`, `S4_storer*_ur`).
/// The EA is `addr + (Ru << shift)` with `addr` the constant-extended `I` field.
/// Fields: `u` = Ru index, `i` = #u2 shift, `t` = data Rt. (The load form
/// `L4_loadr*_ur` instead carries the index in field `t`.)
pub(super) fn store_ur(
    decoded: &DecodedOp,
    width: MemWidth,
    high_half: bool,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let src = field_u8(decoded, b't')?;
    let index = field_u8(decoded, b'u')?;
    let shift = decode_field_uimm(decoded, b'i', None)?.0 as u8;
    let (addr, used) = decode_field_uimm(decoded, b'I', immext)?;
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::IndexAbs { index, shift, addr },
            width,
            pred: None,
            src_new: false,
            high_half,
        },
        used,
    ))
}

/// NEW-VALUE scaled-index absolute store (`S4_storer*new_ur`). Data source is
/// the `Nt8` producer (field `t`); index is `u`, shift is `i`.
pub(super) fn store_new_ur(
    decoded: &DecodedOp,
    width: MemWidth,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let nt = field_u8(decoded, b't')?;
    let index = field_u8(decoded, b'u')?;
    let shift = decode_field_uimm(decoded, b'i', None)?.0 as u8;
    let (addr, used) = decode_field_uimm(decoded, b'I', immext)?;
    Some((
        DecodedInsn::StoreNew {
            nt,
            addr: AddrMode::IndexAbs { index, shift, addr },
            width,
            pred: None,
        },
        used,
    ))
}

/// `storerf` post-increment-by-immediate store (`memh(Rx++#s4:1)=Rt.h`,
/// `S2_storerf_pi`). Like `store_pi` but stores the high halfword `Rt[31:16]`.
pub(super) fn store_high_pi(decoded: &DecodedOp, width: MemWidth) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let src = field_u8(decoded, b't')?;
    let (imm, _) = decode_field_simm(decoded, b'i', None)?;
    let offset = imm.wrapping_shl(width_shift(width) as u32);
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::PostIncImm { base, offset },
            width,
            pred: None,
            src_new: false,
            high_half: true,
        },
        false,
    ))
}

/// `storerf` register / bit-reverse post-increment store (`S2_storerf_pr` /
/// `_pbr`). `x` = base (post-incremented), `u` = Mu select, `t` = data Rt.
pub(super) fn store_high_pr(
    decoded: &DecodedOp,
    width: MemWidth,
    brev: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let modsel = field_u8(decoded, b'u')?;
    let src = field_u8(decoded, b't')?;
    let addr = if brev {
        AddrMode::PostIncBrev { base, modsel }
    } else {
        AddrMode::PostIncReg { base, modsel }
    };
    Some((
        DecodedInsn::Store {
            src,
            addr,
            width,
            pred: None,
            src_new: false,
            high_half: true,
        },
        false,
    ))
}

/// `storerf` circular post-increment store by immediate (`S2_storerf_pci`).
/// `x` = base, `u` = Mu select, `i` = signed #s4 increment (scaled), `t` = data.
pub(super) fn store_high_pci(decoded: &DecodedOp, width: MemWidth) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let modsel = field_u8(decoded, b'u')?;
    let src = field_u8(decoded, b't')?;
    let (imm, _) = decode_field_simm(decoded, b'i', None)?;
    let incr = imm.wrapping_shl(width_shift(width) as u32);
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::PostIncCircImm { base, modsel, incr },
            width,
            pred: None,
            src_new: false,
            high_half: true,
        },
        false,
    ))
}

/// `storerf` circular post-increment store by the M I-field (`S2_storerf_pcr`).
/// `x` = base, `u` = Mu select, `t` = data Rt.
pub(super) fn store_high_pcr(decoded: &DecodedOp, width: MemWidth) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let modsel = field_u8(decoded, b'u')?;
    let src = field_u8(decoded, b't')?;
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::PostIncCircReg {
                base,
                modsel,
                shift: width_shift(width),
            },
            width,
            pred: None,
            src_new: false,
            high_half: true,
        },
        false,
    ))
}

/// NEW-VALUE GP-relative store (`memX(gp+#u16:N)=Nt8.new`, `S2_storer*newgp`).
/// Data is the `Nt8` producer (field `t`); offset is `i` (scaled by access
/// size, or the full byte address if a constant extender is present).
pub(super) fn store_new_gp(
    decoded: &DecodedOp,
    width: MemWidth,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let nt = field_u8(decoded, b't')?;
    let (imm, used) = decode_field_uimm(decoded, b'i', immext)?;
    let addr = if used {
        AddrMode::Abs { addr: imm }
    } else {
        AddrMode::GpOffset {
            offset: (imm << width_shift(width)) as i32,
        }
    };
    Some((
        DecodedInsn::StoreNew {
            nt,
            addr,
            width,
            pred: None,
        },
        used,
    ))
}

/// `storerfgp` GP-relative high-half store (`memh(gp+#u16:1)=Rt.h`). Like
/// `store_gp` but stores `Rt[31:16]`.
pub(super) fn store_high_gp(
    decoded: &DecodedOp,
    width: MemWidth,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let src = field_u8(decoded, b't')?;
    let (imm, used) = decode_field_uimm(decoded, b'i', immext)?;
    let addr = if used {
        AddrMode::Abs { addr: imm }
    } else {
        AddrMode::GpOffset {
            offset: (imm << width_shift(width)) as i32,
        }
    };
    Some((
        DecodedInsn::Store {
            src,
            addr,
            width,
            pred: None,
            src_new: false,
            high_half: true,
        },
        used,
    ))
}

/// Store-conditional / store-release (`memw_locked(Rs,Pd)=Rt`,
/// `memd_locked(Rs,Pd)=Rtt`, and the `memX_rl(Rs):at/:st` release stores). The
/// EA is always `Rs+0` (field `s`); data is `Rt`/`Rtt` (field `t`). For the
/// `_locked` forms field `d` is the success predicate Pd, which in single-thread
/// user mode is always set TRUE (the reservation never fails). Release stores
/// have no success predicate.
pub(super) fn store_cond(
    decoded: &DecodedOp,
    width: MemWidth,
    set_pred: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let src = field_u8(decoded, b't')?;
    let success_pred = if set_pred {
        Some(field_u8(decoded, b'd')?)
    } else {
        None
    };
    Some((
        DecodedInsn::StoreCond {
            src,
            base,
            width,
            success_pred,
        },
        false,
    ))
}

/// Vector byte splice (`Rdd=vspliceb(Rss,Rtt,#u3|Pu)`, `S2_vsplice{ib,rb}`). The
/// low `N` bytes come from Rss and the high `8-N` bytes from Rtt, where `N` is
/// the `#u3` immediate (`ib`) or `Pu & 7` (`rb`). A pure register-pair write.
pub(super) fn vsplice(decoded: &DecodedOp, imm_n: bool) -> Option<(DecodedInsn, bool)> {
    let dst = field_u8(decoded, b'd')?;
    let src_low = field_u8(decoded, b's')?; // Rss
    let src_high = field_u8(decoded, b't')?; // Rtt
    let amount = if imm_n {
        SpliceAmount::Imm(decode_field_uimm(decoded, b'i', None)?.0 as u8)
    } else {
        SpliceAmount::Pred(field_u8(decoded, b'u')?)
    };
    Some((
        DecodedInsn::Vsplice {
            dst,
            src_low,
            src_high,
            amount,
        },
        false,
    ))
}

/// `memX(Rx++Mu)` post-increment by the M register (`L2_load*_pr`).
pub(super) fn load_pr(
    decoded: &DecodedOp,
    width: MemWidth,
    sign: MemSign,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let dst = field_u8(decoded, b'd')?;
    let modsel = field_u8(decoded, b'u')?;
    Some((
        DecodedInsn::Load {
            dst,
            addr: AddrMode::PostIncReg { base, modsel },
            width,
            sign,
            pred: None,
        },
        false,
    ))
}

/// `memX(Rx++Mu:brev)` bit-reverse post-increment (`L2_load*_pbr`).
pub(super) fn load_pbr(
    decoded: &DecodedOp,
    width: MemWidth,
    sign: MemSign,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let dst = field_u8(decoded, b'd')?;
    let modsel = field_u8(decoded, b'u')?;
    Some((
        DecodedInsn::Load {
            dst,
            addr: AddrMode::PostIncBrev { base, modsel },
            width,
            sign,
            pred: None,
        },
        false,
    ))
}

/// `memX(Rx++#s4:N:circ(Mu))` circular post-increment by immediate
/// (`L2_load*_pci`). The s4 field is scaled by the access size.
pub(super) fn load_pci(
    decoded: &DecodedOp,
    width: MemWidth,
    sign: MemSign,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let dst = field_u8(decoded, b'd')?;
    let modsel = field_u8(decoded, b'u')?;
    let (imm, _) = decode_field_simm(decoded, b'i', None)?;
    let incr = imm.wrapping_shl(width_shift(width) as u32);
    Some((
        DecodedInsn::Load {
            dst,
            addr: AddrMode::PostIncCircImm { base, modsel, incr },
            width,
            sign,
            pred: None,
        },
        false,
    ))
}

/// `memX(Rx++I:circ(Mu))` circular post-increment by the M register's I field
/// (`L2_load*_pcr`).
pub(super) fn load_pcr(
    decoded: &DecodedOp,
    width: MemWidth,
    sign: MemSign,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let dst = field_u8(decoded, b'd')?;
    let modsel = field_u8(decoded, b'u')?;
    Some((
        DecodedInsn::Load {
            dst,
            addr: AddrMode::PostIncCircReg {
                base,
                modsel,
                shift: width_shift(width),
            },
            width,
            sign,
            pred: None,
        },
        false,
    ))
}

// ====================================================================
// Shared addressing-mode decoders for the L2/L4 LOADALIGN / BSW / BZW and
// predicated-load families. Each returns the resolved `AddrMode` plus whether
// a constant extender was consumed. `scale_shift` is the immediate / I-field
// scale (the `:N` in `Rx++#s4:N`): 0 for byte FIFO, 1 for half FIFO and the
// 2-byte unpack, 2 for the 4-byte unpack.
// ====================================================================

/// `mem...(Rs+#s11:N)` — register + scaled signed immediate. Field `s` = base,
/// `i` = signed immediate (scaled by `scale_shift`).
pub(super) fn addr_io_n(
    decoded: &DecodedOp,
    scale_shift: u8,
    immext: Option<u32>,
) -> Option<(AddrMode, bool)> {
    let base = field_u8(decoded, b's')?;
    let (offset, used) = decode_field_scaled_simm(decoded, b'i', immext, scale_shift)?;
    Some((AddrMode::Offset { base, offset }, used))
}

/// `mem...(Rx++#s4:N)` — post-increment by scaled signed immediate. Field `x`
/// = base (post-incremented), `i` = signed immediate (scaled by `scale_shift`).
pub(super) fn addr_pi_n(decoded: &DecodedOp, scale_shift: u8) -> Option<AddrMode> {
    let base = field_u8(decoded, b'x')?;
    let (imm, _) = decode_field_simm(decoded, b'i', None)?;
    let offset = imm.wrapping_shl(scale_shift as u32);
    Some(AddrMode::PostIncImm { base, offset })
}

/// `mem...(Rx++Mu)` — post-increment by the modifier register. Fields `x`, `u`.
pub(super) fn addr_pr(decoded: &DecodedOp) -> Option<AddrMode> {
    let base = field_u8(decoded, b'x')?;
    let modsel = field_u8(decoded, b'u')?;
    Some(AddrMode::PostIncReg { base, modsel })
}

/// `mem...(Rx++Mu:brev)` — bit-reverse post-increment. Fields `x`, `u`.
pub(super) fn addr_pbr(decoded: &DecodedOp) -> Option<AddrMode> {
    let base = field_u8(decoded, b'x')?;
    let modsel = field_u8(decoded, b'u')?;
    Some(AddrMode::PostIncBrev { base, modsel })
}

/// `mem...(Rx++#s4:N:circ(Mu))` — circular post-increment by scaled immediate.
pub(super) fn addr_pci_n(decoded: &DecodedOp, scale_shift: u8) -> Option<AddrMode> {
    let base = field_u8(decoded, b'x')?;
    let modsel = field_u8(decoded, b'u')?;
    let (imm, _) = decode_field_simm(decoded, b'i', None)?;
    let incr = imm.wrapping_shl(scale_shift as u32);
    Some(AddrMode::PostIncCircImm { base, modsel, incr })
}

/// `mem...(Rx++I:circ(Mu))` — circular post-increment by the I field, scaled.
pub(super) fn addr_pcr_n(decoded: &DecodedOp, scale_shift: u8) -> Option<AddrMode> {
    let base = field_u8(decoded, b'x')?;
    let modsel = field_u8(decoded, b'u')?;
    Some(AddrMode::PostIncCircReg {
        base,
        modsel,
        shift: scale_shift,
    })
}

/// `mem...(Re=##U6)` — absolute-set (`L4_*_ap`). Field `e` = address register
/// Re (also written), `I` = the 6 low bits of the extended absolute address.
/// Requires a constant extender; without one the address is just `I`.
pub(super) fn addr_ap(decoded: &DecodedOp, immext: Option<u32>) -> Option<(AddrMode, bool)> {
    let areg = field_u8(decoded, b'e')?;
    let (addr, used) = decode_field_uimm(decoded, b'I', immext)?;
    Some((AddrMode::AbsSet { areg, addr }, used))
}

/// `mem...(Ru<<#u2+##U6)` — scaled-index absolute (`L4_*_ur`). Field `t` = Ru
/// index, `i` = #u2 shift, `I` = low 6 bits of the extended absolute address.
pub(super) fn addr_ur(decoded: &DecodedOp, immext: Option<u32>) -> Option<(AddrMode, bool)> {
    let index = field_u8(decoded, b't')?;
    let shift = decode_field_uimm(decoded, b'i', None)?.0 as u8;
    let (addr, used) = decode_field_uimm(decoded, b'I', immext)?;
    Some((AddrMode::IndexAbs { index, shift, addr }, used))
}

/// Build a `LoadAlign` (FIFO) insn from a resolved address. `width` is Byte
/// (shift 8) or Half (shift 16); the read-modify dest pair Ryy is field `y`.
pub(super) fn loadalign(
    decoded: &DecodedOp,
    addr: AddrMode,
    width: MemWidth,
    used: bool,
) -> Option<(DecodedInsn, bool)> {
    let dst_pair = field_u8(decoded, b'y')?;
    Some((
        DecodedInsn::LoadAlign {
            dst_pair,
            addr,
            width,
            pred: None,
        },
        used,
    ))
}

/// Build a `LoadUnpack` (bsw/bzw) insn from a resolved address. `count` is 2 or
/// 4 bytes; `sign` selects membh (Signed) vs memubh (Unsigned). Dest is `d`.
pub(super) fn loadunpack(
    decoded: &DecodedOp,
    addr: AddrMode,
    count: u8,
    sign: MemSign,
    used: bool,
) -> Option<(DecodedInsn, bool)> {
    let dst = field_u8(decoded, b'd')?;
    Some((
        DecodedInsn::LoadUnpack {
            dst,
            addr,
            count,
            sign,
            pred: None,
        },
        used,
    ))
}

/// Base register-offset load (`Rd=memX(Rs+Ru<<#u2)`, `L4_loadr*_rr`). Fields:
/// `s` = Rs base, `t` = Ru index, `i` = #u2 shift, `d` = dest.
pub(super) fn load_rr(
    decoded: &DecodedOp,
    width: MemWidth,
    sign: MemSign,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let index = field_u8(decoded, b't')?;
    let dst = field_u8(decoded, b'd')?;
    let shift = decode_field_uimm(decoded, b'i', None)?.0 as u8;
    Some((
        DecodedInsn::Load {
            dst,
            addr: AddrMode::RegScaled { base, index, shift },
            width,
            sign,
            pred: None,
        },
        false,
    ))
}

/// Build a plain (non-predicated) `Load` from an already-resolved address
/// `(AddrMode, extender_used)`. Dest is field `d` (the `_ap`/`_ur` L4 forms).
pub(super) fn load_simple(
    decoded: &DecodedOp,
    width: MemWidth,
    sign: MemSign,
    addr_used: (AddrMode, bool),
) -> Option<(DecodedInsn, bool)> {
    let dst = field_u8(decoded, b'd')?;
    Some((
        DecodedInsn::Load {
            dst,
            addr: addr_used.0,
            width,
            sign,
            pred: None,
        },
        addr_used.1,
    ))
}

/// Atomic load (`L2_loadw_locked`/`_aq`, `L4_loadd_locked`/`_aq`). In
/// single-thread user mode these are plain word / doubleword loads
/// (`Rd=memw(Rs)` / `Rdd=memd(Rs)`); the acquire-barrier has no observable
/// effect on register / memory state. The `_locked` forms additionally set a
/// reservation on the accessed address (consumed by a later store-conditional),
/// so they decode to `LoadLocked`; the `_aq` forms are plain `Load`. Fields:
/// `s` = base, `d` = dest. Always reads at `Rs+0`.
pub(super) fn load_atomic(
    decoded: &DecodedOp,
    width: MemWidth,
    locked: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let dst = field_u8(decoded, b'd')?;
    if locked {
        Some((DecodedInsn::LoadLocked { dst, base, width }, false))
    } else {
        Some((
            DecodedInsn::Load {
                dst,
                addr: AddrMode::Offset { base, offset: 0 },
                width,
                sign: MemSign::Unsigned,
                pred: None,
            },
            false,
        ))
    }
}

/// Predicated post-increment load (`if ([!]Pv[.new]) Rd=memX(Rx++#s4:N)`,
/// `L2_ploadr*_pi`). Fields: `x` = base, `d` = dest, `t` = Pv, `i` = signed
/// increment (scaled by the access width). A not-taken load CANCELS: no Rd
/// write and no post-increment.
pub(super) fn pred_load_pi(
    decoded: &DecodedOp,
    width: MemWidth,
    sign: MemSign,
    sense: bool,
    pred_new: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let dst = field_u8(decoded, b'd')?;
    let pred = field_u8(decoded, b't')?;
    let (imm, _) = decode_field_simm(decoded, b'i', None)?;
    let offset = imm.wrapping_shl(width_shift(width) as u32);
    Some((
        DecodedInsn::Load {
            dst,
            addr: AddrMode::PostIncImm { base, offset },
            width,
            sign,
            pred: Some(pred_cond(pred, sense, pred_new)),
        },
        false,
    ))
}

/// Predicated register-offset load (`if ([!]Pv[.new]) Rd=memX(Rs+Ru<<#u2)`,
/// `L4_ploadr*_rr`). Fields: `s` = Rs base, `t` = Ru index, `i` = #u2 shift,
/// `d` = dest, `v` = Pv.
pub(super) fn pred_load_rr(
    decoded: &DecodedOp,
    width: MemWidth,
    sign: MemSign,
    sense: bool,
    pred_new: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let index = field_u8(decoded, b't')?;
    let dst = field_u8(decoded, b'd')?;
    let pred = field_u8(decoded, b'v')?;
    let shift = decode_field_uimm(decoded, b'i', None)?.0 as u8;
    Some((
        DecodedInsn::Load {
            dst,
            addr: AddrMode::RegScaled { base, index, shift },
            width,
            sign,
            pred: Some(pred_cond(pred, sense, pred_new)),
        },
        false,
    ))
}

/// Predicated absolute load (`if ([!]Pv[.new]) Rd=memX(##addr)`,
/// `L4_ploadr*_abs`). Fields: `d` = dest, `t` = Pv, `i` = absolute address
/// (low 6 bits, completed by the constant extender).
pub(super) fn pred_load_abs(
    decoded: &DecodedOp,
    width: MemWidth,
    sign: MemSign,
    sense: bool,
    pred_new: bool,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let dst = field_u8(decoded, b'd')?;
    let pred = field_u8(decoded, b't')?;
    let (addr, used) = decode_field_uimm(decoded, b'i', immext)?;
    Some((
        DecodedInsn::Load {
            dst,
            addr: AddrMode::Abs { addr },
            width,
            sign,
            pred: Some(pred_cond(pred, sense, pred_new)),
        },
        used,
    ))
}

/// `memX(Rx++Mu)=Rt` post-increment store by the M register (`S2_store*_pr`).
pub(super) fn store_pr(
    decoded: &DecodedOp,
    width: MemWidth,
    src_new: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let src = field_u8(decoded, b't')?;
    let modsel = field_u8(decoded, b'u')?;
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::PostIncReg { base, modsel },
            width,
            pred: None,
            src_new,
            high_half: false,
        },
        false,
    ))
}

/// `memX(Rx++Mu:brev)=Rt` bit-reverse post-increment store (`S2_store*_pbr`).
pub(super) fn store_pbr(
    decoded: &DecodedOp,
    width: MemWidth,
    src_new: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let src = field_u8(decoded, b't')?;
    let modsel = field_u8(decoded, b'u')?;
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::PostIncBrev { base, modsel },
            width,
            pred: None,
            src_new,
            high_half: false,
        },
        false,
    ))
}

/// `memX(Rx++#s4:N:circ(Mu))=Rt` circular post-increment store by immediate
/// (`S2_store*_pci`). The store source for *_pci uses the `t`/`SRC` field.
pub(super) fn store_pci(
    decoded: &DecodedOp,
    width: MemWidth,
    src_new: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let src = field_u8(decoded, b't')?;
    let modsel = field_u8(decoded, b'u')?;
    let (imm, _) = decode_field_simm(decoded, b'i', None)?;
    let incr = imm.wrapping_shl(width_shift(width) as u32);
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::PostIncCircImm { base, modsel, incr },
            width,
            pred: None,
            src_new,
            high_half: false,
        },
        false,
    ))
}

/// `memX(Rx++I:circ(Mu))=Rt` circular post-increment store by register
/// (`S2_store*_pcr`).
pub(super) fn store_pcr(
    decoded: &DecodedOp,
    width: MemWidth,
    src_new: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let src = field_u8(decoded, b't')?;
    let modsel = field_u8(decoded, b'u')?;
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::PostIncCircReg {
                base,
                modsel,
                shift: width_shift(width),
            },
            width,
            pred: None,
            src_new,
            high_half: false,
        },
        false,
    ))
}

/// New-value store with circular/post-increment register addressing. The
/// new-value source register comes from `t` (the `Nt8` field).
fn store_new_addr(
    decoded: &DecodedOp,
    width: MemWidth,
    addr: AddrMode,
) -> Option<(DecodedInsn, bool)> {
    let nt = field_u8(decoded, b't')?;
    Some((
        DecodedInsn::StoreNew {
            nt,
            addr,
            width,
            pred: None,
        },
        false,
    ))
}

pub(super) fn store_new_pi(decoded: &DecodedOp, width: MemWidth) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let (imm, _) = decode_field_simm(decoded, b'i', None)?;
    let offset = imm.wrapping_shl(width_shift(width) as u32);
    store_new_addr(decoded, width, AddrMode::PostIncImm { base, offset })
}

pub(super) fn store_new_pr(decoded: &DecodedOp, width: MemWidth) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let modsel = field_u8(decoded, b'u')?;
    store_new_addr(decoded, width, AddrMode::PostIncReg { base, modsel })
}

pub(super) fn store_new_pbr(decoded: &DecodedOp, width: MemWidth) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let modsel = field_u8(decoded, b'u')?;
    store_new_addr(decoded, width, AddrMode::PostIncBrev { base, modsel })
}

pub(super) fn store_new_pci(decoded: &DecodedOp, width: MemWidth) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let modsel = field_u8(decoded, b'u')?;
    let (imm, _) = decode_field_simm(decoded, b'i', None)?;
    let incr = imm.wrapping_shl(width_shift(width) as u32);
    store_new_addr(
        decoded,
        width,
        AddrMode::PostIncCircImm { base, modsel, incr },
    )
}

pub(super) fn store_new_pcr(decoded: &DecodedOp, width: MemWidth) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let modsel = field_u8(decoded, b'u')?;
    store_new_addr(
        decoded,
        width,
        AddrMode::PostIncCircReg {
            base,
            modsel,
            shift: width_shift(width),
        },
    )
}

pub(super) fn pred_load_io(
    decoded: &DecodedOp,
    width: MemWidth,
    sign: MemSign,
    sense: bool,
    pred_new: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let dst = field_u8(decoded, b'd')?;
    let pred = field_u8(decoded, b't')?;
    let (imm, _) = decode_field_uimm(decoded, b'i', None)?;
    let offset = (imm << width_shift(width)) as i32;
    Some((
        DecodedInsn::Load {
            dst,
            addr: AddrMode::Offset { base, offset },
            width,
            sign,
            pred: Some(pred_cond(pred, sense, pred_new)),
        },
        false,
    ))
}

pub(super) fn pred_store_io(
    decoded: &DecodedOp,
    width: MemWidth,
    sense: bool,
    pred_new: bool,
    src_new: bool,
    high_half: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let src = field_u8(decoded, b't')?;
    let pred = field_u8(decoded, b'v')?;
    let (imm, _) = decode_field_uimm(decoded, b'i', None)?;
    let offset = (imm << width_shift(width)) as i32;
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::Offset { base, offset },
            width,
            pred: Some(pred_cond(pred, sense, pred_new)),
            src_new,
            high_half,
        },
        false,
    ))
}

/// Predicated post-increment store (`if ([!]Pv[.new]) memX(Rx++#s4:N)=Rt[.h]`).
/// Fields: `x` = base (post-incremented), `t` = data Rt, `v` = Pv, `i` = signed
/// increment (scaled by the access size). A not-taken store performs NO
/// post-increment (full cancel).
pub(super) fn pred_store_pi(
    decoded: &DecodedOp,
    width: MemWidth,
    sense: bool,
    pred_new: bool,
    high_half: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let src = field_u8(decoded, b't')?;
    let pred = field_u8(decoded, b'v')?;
    let (imm, _) = decode_field_simm(decoded, b'i', None)?;
    let offset = imm.wrapping_shl(width_shift(width) as u32);
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::PostIncImm { base, offset },
            width,
            pred: Some(pred_cond(pred, sense, pred_new)),
            src_new: false,
            high_half,
        },
        false,
    ))
}

/// Predicated NEW-VALUE post-increment store (`if ([!]Pv[.new]) memX(Rx++#s4:N)
/// =Nt8.new`). Like `pred_store_pi` but the data source is the `Nt8` producer
/// (resolved against the packet's producers), so we emit `StoreNew`.
pub(super) fn pred_store_new_pi(
    decoded: &DecodedOp,
    width: MemWidth,
    sense: bool,
    pred_new: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b'x')?;
    let nt = field_u8(decoded, b't')?;
    let pred = field_u8(decoded, b'v')?;
    let (imm, _) = decode_field_simm(decoded, b'i', None)?;
    let offset = imm.wrapping_shl(width_shift(width) as u32);
    Some((
        DecodedInsn::StoreNew {
            nt,
            addr: AddrMode::PostIncImm { base, offset },
            width,
            pred: Some(pred_cond(pred, sense, pred_new)),
        },
        false,
    ))
}

/// Predicated register-offset store (`if ([!]Pv[.new]) memX(Rs+Ru<<#u2)=Rt[.h]`,
/// `S4_pstorer*_rr`). Fields: `s` = Rs base, `u` = Ru index, `i` = #u2 shift,
/// `t` = data Rt, `v` = Pv.
pub(super) fn pred_store_rr(
    decoded: &DecodedOp,
    width: MemWidth,
    sense: bool,
    pred_new: bool,
    high_half: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let index = field_u8(decoded, b'u')?;
    let src = field_u8(decoded, b't')?;
    let pred = field_u8(decoded, b'v')?;
    let shift = decode_field_uimm(decoded, b'i', None)?.0 as u8;
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::RegScaled { base, index, shift },
            width,
            pred: Some(pred_cond(pred, sense, pred_new)),
            src_new: false,
            high_half,
        },
        false,
    ))
}

/// Predicated NEW-VALUE register-offset store (`S4_pstorer*new_rr`). The data
/// source is the `Nt8` producer (`t`), so we emit `StoreNew`.
pub(super) fn pred_store_new_rr(
    decoded: &DecodedOp,
    width: MemWidth,
    sense: bool,
    pred_new: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let index = field_u8(decoded, b'u')?;
    let nt = field_u8(decoded, b't')?;
    let pred = field_u8(decoded, b'v')?;
    let shift = decode_field_uimm(decoded, b'i', None)?.0 as u8;
    Some((
        DecodedInsn::StoreNew {
            nt,
            addr: AddrMode::RegScaled { base, index, shift },
            width,
            pred: Some(pred_cond(pred, sense, pred_new)),
        },
        false,
    ))
}

/// Predicated absolute store (`if ([!]Pv[.new]) memX(##addr)=Rt[.h]`,
/// `S4_pstorer*_abs`). The address is the constant-extended `i` field. Fields:
/// `i` = absolute address (extended), `t` = data Rt, `v` = Pv.
pub(super) fn pred_store_abs(
    decoded: &DecodedOp,
    width: MemWidth,
    sense: bool,
    pred_new: bool,
    high_half: bool,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let src = field_u8(decoded, b't')?;
    let pred = field_u8(decoded, b'v')?;
    let (addr, used) = decode_field_uimm(decoded, b'i', immext)?;
    Some((
        DecodedInsn::Store {
            src,
            addr: AddrMode::Abs { addr },
            width,
            pred: Some(pred_cond(pred, sense, pred_new)),
            src_new: false,
            high_half,
        },
        used,
    ))
}

/// Predicated NEW-VALUE absolute store (`S4_pstorer*new_abs`). Data source is
/// the `Nt8` producer (`t`), so we emit `StoreNew`.
pub(super) fn pred_store_new_abs(
    decoded: &DecodedOp,
    width: MemWidth,
    sense: bool,
    pred_new: bool,
    immext: Option<u32>,
) -> Option<(DecodedInsn, bool)> {
    let nt = field_u8(decoded, b't')?;
    let pred = field_u8(decoded, b'v')?;
    let (addr, used) = decode_field_uimm(decoded, b'i', immext)?;
    Some((
        DecodedInsn::StoreNew {
            nt,
            addr: AddrMode::Abs { addr },
            width,
            pred: Some(pred_cond(pred, sense, pred_new)),
        },
        used,
    ))
}

/// Store-immediate `memX(Rs+#u6:N)=#s6` (`S4_storeir<W>[cond]_io`). Fields:
/// `s` = base, `i` = unsigned offset (scaled by access size), `I` = signed #s6
/// value, `v` = Pv (only for the predicated forms). The value is sign-extended
/// from 6 bits. `pred` is `None` for the unconditional `S4_storeir<W>_io`.
pub(super) fn store_imm_io(
    decoded: &DecodedOp,
    width: MemWidth,
    pred: Option<(bool, bool)>,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let (imm, _) = decode_field_uimm(decoded, b'i', None)?;
    let offset = (imm << width_shift(width)) as i32;
    let raw = decode_field_uimm(decoded, b'I', None)?.0;
    let value = sign_extend(raw, 6) as u32;
    let pred = match pred {
        Some((sense, pred_new)) => Some(pred_cond(field_u8(decoded, b'v')?, sense, pred_new)),
        None => None,
    };
    Some((
        DecodedInsn::StoreImm {
            value,
            addr: AddrMode::Offset { base, offset },
            width,
            pred,
        },
        false,
    ))
}

/// Decode a read-modify-write memop (`memX(Rs+#u6:N) OP= Rt|#U5`).
/// `op` selects the operation; `imm_src` chooses the `#U5` immediate operand
/// (field `I`) over the register operand (field `t`).
pub(super) fn memop(
    decoded: &DecodedOp,
    width: MemWidth,
    op: MemOpKind,
    imm_src: bool,
) -> Option<(DecodedInsn, bool)> {
    let base = field_u8(decoded, b's')?;
    let imm = decode_field_uimm(decoded, b'i', None)?.0;
    let offset = (imm << width_shift(width)) as i32;
    let src = if imm_src {
        MemOpSrc::Imm(decode_field_uimm(decoded, b'I', None)?.0)
    } else {
        MemOpSrc::Reg(field_u8(decoded, b't')?)
    };
    Some((
        DecodedInsn::MemOp {
            base,
            offset,
            width,
            op,
            src,
        },
        false,
    ))
}
