//! Extended-EVEX legacy-prefix ordering and effective-address tests.

use super::*;
use crate::smir::lift::x86_64::*;

fn assert_addr32_direct(addr: &Address, base: VReg) {
    assert!(matches!(
        addr,
        Address::X86Addr32(inner)
            if matches!(inner.as_ref(), Address::Direct(got_base) if *got_base == base)
    ));
}

fn assert_segment_sib(
    addr: &Address,
    segment: X86Reg,
    base: VReg,
    index: VReg,
    scale: u8,
    disp: i64,
    addr32: bool,
) {
    let inner = if addr32 {
        let Address::X86Addr32(inner) = addr else {
            panic!("expected addr32 wrapper, got {addr:?}");
        };
        inner.as_ref()
    } else {
        addr
    };
    assert!(matches!(
        inner,
        Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(got_segment)),
            base: Some(got_base),
            index: Some(got_index),
            scale: got_scale,
            disp: got_disp,
        } if *got_segment == segment
            && *got_base == base
            && *got_index == index
            && *got_scale == scale
            && *got_disp == disp
    ));
}

#[test]
fn apx_map4_accepts_only_architecturally_permitted_legacy_prefix_groups() {
    for prefix in [0x67, 0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65] {
        let bytes = [prefix, 0x62, 0xF4, 0x7C, 0x08, 0x03, 0xC1];
        let result = lift_single(&bytes).unwrap_or_else(|error| {
            panic!("permitted prefix {prefix:02X} did not lift: {error:?}")
        });
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::Add {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
                ..
            }] if *dst == x86_gpr(0) && *src1 == x86_gpr(0) && *src2 == x86_gpr(1)
        ));
    }

    for (prefixes, name) in [
        (&[0x66][..], "operand-size"),
        (&[0xF2], "REPNE"),
        (&[0xF3], "REP"),
        (&[0xF0], "LOCK"),
        (&[0x40], "REX"),
        (&[0x40, 0x67], "REX hidden by address-size"),
        (&[0x48, 0x64], "REX hidden by segment"),
    ] {
        let mut bytes = prefixes.to_vec();
        bytes.extend_from_slice(&[0x62, 0xF4, 0x7C, 0x08, 0x03, 0xC1]);
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("{name}: expected terminal #UD: {error:?}"));
        assert_invalid_opcode_trap(&result, prefixes.len() + 1);
    }

    // REX2 makes 62H the effective legacy-map opcode, which is a reserved APX
    // row. It therefore becomes an exact #UD trap at the three-byte frontier
    // rather than entering the extended-EVEX decoder or strict fallback.
    let rex2 = lift_single(&[0xD5, 0x00, 0x62, 0xF4, 0x7C, 0x08, 0x03, 0xC1])
        .expect("REX2 62H reservation must be modeled");
    assert_invalid_opcode_trap(&rex2, 3);
}

#[test]
fn shared_evex_router_does_not_hide_a_rex_before_an_allowed_prefix() {
    // The ordinary EVEX path shares the prefix router used to reach prefixed
    // APX forms. A later 67H must not erase the earlier forbidden REX from the
    // router's legality decision.
    let result = lift_single(&[0x40, 0x67, 0x62, 0xF2, 0x7D, 0x49, 0xC6, 0x4C, 0x80, 0x7F])
        .expect("hidden REX before EVEX must be a terminal #UD");
    assert_invalid_opcode_trap(&result, 3);
}

#[test]
fn apx_map4_preserves_addr32_and_fs_gs_effective_addresses() {
    let addr32 = lift_single(&[0x67, 0x62, 0xF4, 0x7C, 0x08, 0x03, 0x43, 0x7F])
        .expect("APX ADD EAX,[EBX+127]");
    assert_eq!(addr32.bytes_consumed, 8);
    assert!(matches!(
        &addr32.ops[0].kind,
        OpKind::Load {
            addr: Address::X86Addr32(inner),
            width: MemWidth::B4,
            sign: SignExtend::Zero,
            ..
        } if matches!(
            inner.as_ref(),
            Address::BaseOffset {
                base,
                offset: 0x7F,
                disp_size: DispSize::Disp8,
            } if *base == x86_gpr(3)
        )
    ));

    let fs_addr32 = lift_single(&[0x64, 0x67, 0x62, 0xF4, 0xFC, 0x08, 0x03, 0x4C, 0x8B, 0x20])
        .expect("APX ADD RCX,FS:[EBX+ECX*4+32] with addr32");
    assert_eq!(fs_addr32.bytes_consumed, 10);
    let OpKind::Load {
        addr,
        width: MemWidth::B8,
        sign: SignExtend::Zero,
        ..
    } = &fs_addr32.ops[0].kind
    else {
        panic!("expected APX FS addr32 load, got {:?}", fs_addr32.ops[0]);
    };
    assert_segment_sib(addr, X86Reg::FsBase, x86_gpr(3), x86_gpr(1), 4, 0x20, true);
}

#[test]
fn apx_promoted_bmi_maps_preserve_prefixes_and_egpr_address_extensions() {
    let bextr = lift_single(&[0x67, 0x62, 0x72, 0xF4, 0x0C, 0xF7, 0x03])
        .expect("APX NF BEXTR R8D,[EBX],ECX with addr32");
    assert_eq!(bextr.bytes_consumed, 7);
    let OpKind::Load { addr, .. } = &bextr.ops[0].kind else {
        panic!("expected APX BEXTR load, got {:?}", bextr.ops[0]);
    };
    assert_addr32_direct(addr, x86_gpr(3));

    let pdep = lift_single(&[0x64, 0x62, 0xEA, 0xE3, 0x00, 0xF5, 0x24, 0x91])
        .expect("APX PDEP R20,R19,FS:[R17+R18*4]");
    assert_eq!(pdep.bytes_consumed, 8);
    let OpKind::Load { addr, .. } = &pdep.ops[0].kind else {
        panic!("expected APX PDEP load, got {:?}", pdep.ops[0]);
    };
    assert_segment_sib(addr, X86Reg::FsBase, x86_gpr(17), x86_gpr(18), 4, 0, false);

    let rorx = lift_single(&[0x65, 0x62, 0xEB, 0xFB, 0x08, 0xF0, 0x64, 0x91, 0x20, 0x0D])
        .expect("APX RORX R20,GS:[R17+R18*4+32],13");
    assert_eq!(rorx.bytes_consumed, 10);
    let OpKind::Load { addr, .. } = &rorx.ops[0].kind else {
        panic!("expected APX RORX load, got {:?}", rorx.ops[0]);
    };
    assert_segment_sib(
        addr,
        X86Reg::GsBase,
        x86_gpr(17),
        x86_gpr(18),
        4,
        0x20,
        false,
    );
}

#[test]
fn apx_shifted_evex_field_validation_uses_the_actual_payload() {
    let rao = lift_single(&[0x67, 0x62, 0xEC, 0xFD, 0x08, 0xFC, 0x08])
        .expect("addr32 APX AAND R17,[R16]");
    assert_eq!(rao.bytes_consumed, 7);
    match &rao.ops[0].kind {
        OpKind::AtomicRmw {
            addr,
            src,
            op: AtomicOp::And,
            width: MemWidth::B8,
            order: MemoryOrder::SeqCst,
            ..
        } => {
            assert_addr32_direct(addr, x86_gpr(16));
            assert_eq!(*src, x86_gpr(17));
        }
        other => panic!("expected prefixed APX RAO-INT op, got {other:?}"),
    }

    let adcx = lift_single(&[0x64, 0x62, 0xF4, 0xBD, 0x18, 0x66, 0xC3])
        .expect("segment-prefixed APX ADCX register form");
    assert_eq!(adcx.bytes_consumed, 7);
    assert!(matches!(
        &adcx.ops[0].kind,
        OpKind::X86Adx {
            dst,
            src1,
            src2,
            width: OpWidth::W64,
            kind: X86AdxKind::Adcx,
            ..
        } if *dst == x86_gpr(8) && *src1 == x86_gpr(0) && *src2 == x86_gpr(3)
    ));

    let cmpccxadd = lift_single(&[0x67, 0x62, 0xEA, 0x61, 0x00, 0xE2, 0x44, 0x91, 0x20])
        .expect("addr32 APX CMPccXADD EGPR memory form");
    assert_eq!(cmpccxadd.bytes_consumed, 9);
    assert!(matches!(cmpccxadd.ops[0].kind, OpKind::X86RequireApx));
    assert!(matches!(
        &cmpccxadd.ops[1].kind,
        OpKind::X86CheckAlignmentAc {
            access_size: 4,
            alignment: 4,
            natural_alignment: true,
            ..
        }
    ));
    match &cmpccxadd.ops[2].kind {
        OpKind::AtomicCmpXadd { addr, .. } => {
            let Address::X86Addr32(inner) = addr else {
                panic!("expected CMPccXADD addr32 wrapper, got {addr:?}");
            };
            assert!(matches!(
                inner.as_ref(),
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x20,
                    ..
                } if *base == x86_gpr(17) && *index == x86_gpr(18)
            ));
        }
        other => panic!("expected prefixed CMPccXADD, got {other:?}"),
    }
}

#[test]
fn apx_prefixed_incomplete_length_is_absolute_from_instruction_start() {
    assert!(matches!(
        lift_single(&[0x67, 0x62, 0xF4, 0x7C]),
        Err(LiftError::Incomplete {
            have: 4,
            need: 5,
            ..
        })
    ));
}
