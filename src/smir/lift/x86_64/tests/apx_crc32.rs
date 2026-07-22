//! Strict lifting coverage for APX-promoted CRC32.

use super::*;
use crate::smir::lift::x86_64::*;

fn assert_guarded_crc32(
    result: &LiftResult,
    instruction_len: usize,
    destination: VReg,
    data: VReg,
    data_width: OpWidth,
) {
    assert_eq!(result.bytes_consumed, instruction_len);
    assert!(matches!(
        result.ops.first(),
        Some(SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::X86RequireApx,
            x86_hint: None,
        })
    ));
    assert!(matches!(
        result.ops.last(),
        Some(SmirOp {
            kind: OpKind::Crc32C {
                dst,
                crc,
                data: got_data,
                data_width: got_width,
            },
            ..
        }) if *dst == destination
            && *crc == destination
            && *got_data == data
            && *got_width == data_width
    ));
    for (index, op) in result.ops.iter().enumerate() {
        assert_eq!(op.id, OpId(index as u16));
        assert!(op.kind.flags_written().is_empty());
    }
}

#[test]
fn apx_crc32_strictly_lifts_every_scalable_width_and_egpr_form() {
    for (bytes, width, name) in [
        (
            &[0x62, 0xEC, 0x7C, 0x08, 0xF0, 0xE1][..],
            OpWidth::W8,
            "F0 W0",
        ),
        (
            &[0x62, 0xEC, 0xFC, 0x08, 0xF0, 0xE1][..],
            OpWidth::W8,
            "F0 W1",
        ),
        (
            &[0x62, 0xEC, 0x7D, 0x08, 0xF1, 0xE1][..],
            OpWidth::W16,
            "F1 W0 66",
        ),
        (
            &[0x62, 0xEC, 0x7C, 0x08, 0xF1, 0xE1][..],
            OpWidth::W32,
            "F1 W0 NP",
        ),
        (
            &[0x62, 0xEC, 0xFC, 0x08, 0xF1, 0xE1][..],
            OpWidth::W64,
            "F1 W1 NP",
        ),
        // SCALABLE gives W precedence over the otherwise legal 66 pp value.
        (
            &[0x62, 0xEC, 0xFD, 0x08, 0xF1, 0xE1][..],
            OpWidth::W64,
            "F1 W1 66",
        ),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_guarded_crc32(&result, bytes.len(), x86_gpr(20), x86_gpr(17), width);
    }
}

#[test]
fn apx_crc32_memory_lifts_guard_before_load_with_exact_addresses() {
    let byte = [0x62, 0xEC, 0x7C, 0x08, 0xF0, 0x61, 0x7F];
    let result = lift_single(&byte).expect("CRC32 r20d,byte ptr [r17+127]");
    assert_eq!(result.bytes_consumed, byte.len());
    assert!(matches!(result.ops[0].kind, OpKind::X86RequireApx));
    let (loaded, crc_data) = match (&result.ops[1].kind, &result.ops[2].kind) {
        (
            OpKind::Load {
                dst,
                addr:
                    Address::BaseOffset {
                        base,
                        offset: 0x7F,
                        disp_size: DispSize::Disp8,
                    },
                width: MemWidth::B1,
                sign: SignExtend::Zero,
            },
            OpKind::Crc32C {
                dst: crc_dst,
                crc,
                data,
                data_width: OpWidth::W8,
            },
        ) => {
            assert_eq!(*base, x86_gpr(17));
            assert_eq!(*crc_dst, x86_gpr(20));
            assert_eq!(*crc, x86_gpr(20));
            (*dst, *data)
        }
        other => panic!("unexpected APX byte-memory CRC32 ops: {other:?}"),
    };
    assert_eq!(loaded, crc_data);

    let fs = [0x64, 0x62, 0xEC, 0xF8, 0x08, 0xF1, 0x64, 0x91, 0x20];
    let result = lift_single(&fs).expect("CRC32 r20,FS:[r17+r18*4+32]");
    assert_eq!(result.bytes_consumed, fs.len());
    assert!(matches!(result.ops[0].kind, OpKind::X86RequireApx));
    assert!(matches!(
        &result.ops[1].kind,
        OpKind::Load {
            addr: Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(base),
                index: Some(index),
                scale: 4,
                disp: 0x20,
            },
            width: MemWidth::B8,
            sign: SignExtend::Zero,
            ..
        } if *base == x86_gpr(17) && *index == x86_gpr(18)
    ));

    let addr32 = [0x67, 0x62, 0x14, 0x7C, 0x08, 0xF1, 0x64, 0x91, 0x20];
    let result = lift_single(&addr32).expect("CRC32 r12d,[r9d+r10d*4+32]");
    assert_eq!(result.bytes_consumed, addr32.len());
    assert!(matches!(result.ops[0].kind, OpKind::X86RequireApx));
    assert!(matches!(
        &result.ops[1].kind,
        OpKind::Load {
            addr: Address::X86Addr32(inner),
            width: MemWidth::B4,
            sign: SignExtend::Zero,
            ..
        } if matches!(
            inner.as_ref(),
            Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x20,
                disp_size: DispSize::Disp8,
            } if *base == x86_gpr(9) && *index == x86_gpr(10)
        )
    ));
    let crc = result.ops.last().expect("addr32 CRC op");
    assert!(matches!(
        crc.kind,
        OpKind::Crc32C {
            dst,
            crc,
            data_width: OpWidth::W32,
            ..
        } if dst == x86_gpr(12) && crc == x86_gpr(12)
    ));
}

#[test]
fn apx_crc32_reserved_fields_are_rejected_fail_closed() {
    for (bytes, name) in [
        (&[0x62, 0xF4, 0x7D, 0x08, 0xF0, 0xC1][..], "F0 with 66"),
        (&[0x62, 0xF4, 0x7E, 0x08, 0xF1, 0xC1][..], "F3 pp"),
        (&[0x62, 0xF4, 0x7F, 0x08, 0xF1, 0xC1][..], "F2 pp"),
        (&[0x62, 0xF4, 0x7C, 0x18, 0xF1, 0xC1][..], "ND"),
        (&[0x62, 0xF4, 0x7C, 0x0C, 0xF1, 0xC1][..], "NF"),
        (&[0x62, 0xF4, 0x7C, 0x88, 0xF1, 0xC1][..], "z"),
        (&[0x62, 0xF4, 0x7C, 0x28, 0xF1, 0xC1][..], "LL"),
        (&[0x62, 0xF4, 0x7C, 0x09, 0xF1, 0xC1][..], "aaa"),
        (&[0x62, 0xF4, 0x74, 0x08, 0xF1, 0xC1][..], "V3:0"),
        (&[0x62, 0xF4, 0x7C, 0x00, 0xF1, 0xC1][..], "V4"),
        (&[0x62, 0xF4, 0x78, 0x08, 0xF1, 0xC1][..], "register U"),
    ] {
        let error = lift_single(bytes).expect_err(name);
        assert!(
            matches!(error, LiftError::InvalidEncoding { .. }),
            "{name}: {error:?}"
        );
    }

    // U/X4 is defined as an EGPR index extension for memory operands and may
    // therefore be encoded zero there.
    assert!(lift_single(&[0x62, 0xF4, 0x78, 0x08, 0xF1, 0x04, 0x08]).is_ok());
}

#[test]
fn apx_crc32_incomplete_lengths_are_absolute_and_exact() {
    assert!(matches!(
        lift_single(&[0x62, 0xF4, 0x7C, 0x08, 0xF0]),
        Err(LiftError::Incomplete {
            have: 5,
            need: 6,
            ..
        })
    ));
    assert!(matches!(
        lift_single(&[0x62, 0xF4, 0x7C, 0x08, 0xF1, 0x84]),
        Err(LiftError::Incomplete {
            have: 6,
            need: 7,
            ..
        })
    ));
    assert!(matches!(
        lift_single(&[0x62, 0xF4, 0x7C, 0x08, 0xF1, 0x84, 0x91]),
        Err(LiftError::Incomplete {
            have: 7,
            need: 11,
            ..
        })
    ));
}
