//! evex::permute tests

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_rex2_m_compressed_0f_map_uses_llvm_encoding() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `imul r16, rax` as REX2.M compressed 0F AF.
    let result = lifter
        .lift_insn(0x1000, &[0xD5, 0xC8, 0xAF, 0xC0], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 4);
    let ops = assert_rex2_guarded_ops(&result, 1);
    match &ops[0].kind {
        OpKind::MulS {
            dst_lo,
            dst_hi: None,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst_lo, x86_gpr(16));
            assert_eq!(*src1, x86_gpr(16));
            assert_eq!(*src2, x86_gpr(0));
        }
        other => panic!("expected imul r16, rax, got {other:?}"),
    }
}
#[test]
fn lift_evex_two_table_permute_covers_overwrite_direction_shapes_and_memory() {
    for (bytes, elem, overwrite_table) in [
        (
            &[0x62, 0xA2, 0x6D, 0x82, 0x75, 0xCB][..],
            VecElementType::I8,
            false,
        ),
        (
            &[0x62, 0xA2, 0xD5, 0x23, 0x75, 0xE6][..],
            VecElementType::I16,
            false,
        ),
        (
            &[0x62, 0x82, 0x3D, 0xC4, 0x76, 0xF9][..],
            VecElementType::I32,
            false,
        ),
        (
            &[0x62, 0x62, 0xA5, 0x15, 0x76, 0x10][..],
            VecElementType::I64,
            false,
        ),
        (
            &[0x62, 0xA2, 0x6D, 0x26, 0x77, 0xCB][..],
            VecElementType::F32,
            false,
        ),
        (
            &[0x62, 0xA2, 0xD5, 0xC7, 0x77, 0xE6][..],
            VecElementType::F64,
            false,
        ),
        (
            &[0x62, 0xA2, 0x6D, 0x82, 0x7D, 0xCB][..],
            VecElementType::I8,
            true,
        ),
        (
            &[0x62, 0xA2, 0xD5, 0x23, 0x7D, 0xE6][..],
            VecElementType::I16,
            true,
        ),
        (
            &[0x62, 0x82, 0x3D, 0xC4, 0x7E, 0xF9][..],
            VecElementType::I32,
            true,
        ),
        (
            &[0x62, 0x62, 0xA5, 0x15, 0x7E, 0x10][..],
            VecElementType::I64,
            true,
        ),
        (
            &[0x62, 0xA2, 0x6D, 0x26, 0x7F, 0xCB][..],
            VecElementType::F32,
            true,
        ),
        (
            &[0x62, 0xA2, 0xD5, 0xC7, 0x7F, 0xE6][..],
            VecElementType::F64,
            true,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(
            lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VPermute {
                    src2: Some(_),
                    elem: actual_elem,
                    overwrite_table: actual_overwrite,
                    ..
                } | OpKind::X86PermuteBytesWords {
                    table2: Some(_),
                    elem: actual_elem,
                    overwrite_table: actual_overwrite,
                    ..
                } if actual_elem == elem && actual_overwrite == overwrite_table
            )),
            "missing two-table permutation for {bytes:02X?}"
        );
    }

    let direct_index = lift_single(&[0x62, 0xA2, 0x6D, 0x82, 0x75, 0xCB]).unwrap();
    assert_eq!(direct_index.ops.len(), 1);
    assert!(matches!(
        direct_index.ops[0].kind,
        OpKind::X86PermuteBytesWords {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            table1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            table2: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(19)))),
            indices: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
            elem: VecElementType::I8,
            width: VecWidth::V128,
            overwrite_table: false,
            zeroing: true,
        }
    ));
    let direct_table = lift_single(&[0x62, 0xA2, 0xD5, 0x23, 0x7D, 0xE6]).unwrap();
    assert_eq!(direct_table.ops.len(), 1);
    assert!(matches!(
        direct_table.ops[0].kind,
        OpKind::X86PermuteBytesWords {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(20))),
            table1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(20))),
            table2: Some(VReg::Arch(ArchReg::X86(X86Reg::Ymm(22)))),
            indices: VReg::Arch(ArchReg::X86(X86Reg::Ymm(21))),
            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
            elem: VecElementType::I16,
            width: VecWidth::V256,
            overwrite_table: true,
            zeroing: false,
        }
    ));

    let index_overwrite = lift_single(&[0x62, 0x82, 0x3D, 0xC4, 0x76, 0xF9]).unwrap();
    assert!(index_overwrite.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VPermute {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(24))),
            indices: VReg::Arch(ArchReg::X86(X86Reg::Zmm(23))),
            ..
        }
    )));
    let table_overwrite = lift_single(&[0x62, 0x82, 0x3D, 0xC4, 0x7E, 0xF9]).unwrap();
    assert!(table_overwrite.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VPermute {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(23))),
            indices: VReg::Arch(ArchReg::X86(X86Reg::Zmm(24))),
            ..
        }
    )));

    let memory = lift_single(&[0x62, 0xF2, 0x75, 0x89, 0x76, 0x00]).unwrap();
    assert_eq!(
        memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        4
    );
    assert!(
        !memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::VLoad { .. }))
    );

    for bytes in [
        &[0xC4, 0xE2, 0x75, 0x76, 0xC2][..],
        &[0x62, 0xF2, 0x75, 0x80, 0x76, 0xC2][..],
        &[0x62, 0xF2, 0x75, 0x99, 0x75, 0x00][..],
        &[0x62, 0xF2, 0x75, 0x19, 0x7D, 0x00][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "invalid two-table permutation accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_evex_compress_expand_covers_all_element_families_memory_and_invalids() {
    for (bytes, compress, elem, width, zeroing) in [
        (
            &[0x62, 0xF2, 0x7D, 0x09, 0x63, 0xD1][..],
            true,
            VecElementType::I8,
            VecWidth::V128,
            false,
        ),
        (
            &[0x62, 0xF2, 0xFD, 0xAB, 0x63, 0xEC][..],
            true,
            VecElementType::I16,
            VecWidth::V256,
            true,
        ),
        (
            &[0x62, 0xA2, 0x7D, 0x4A, 0x8B, 0xD1][..],
            true,
            VecElementType::I32,
            VecWidth::V512,
            false,
        ),
        (
            &[0x62, 0xF2, 0xFD, 0x89, 0x8B, 0xD9][..],
            true,
            VecElementType::I64,
            VecWidth::V128,
            true,
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x2B, 0x8A, 0xF4][..],
            true,
            VecElementType::F32,
            VecWidth::V256,
            false,
        ),
        (
            &[0x62, 0xA2, 0xFD, 0xCA, 0x8A, 0xD9][..],
            true,
            VecElementType::F64,
            VecWidth::V512,
            true,
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x09, 0x62, 0xCA][..],
            false,
            VecElementType::I8,
            VecWidth::V128,
            false,
        ),
        (
            &[0x62, 0xF2, 0xFD, 0xAB, 0x62, 0xE5][..],
            false,
            VecElementType::I16,
            VecWidth::V256,
            true,
        ),
        (
            &[0x62, 0xA2, 0x7D, 0x4A, 0x89, 0xCA][..],
            false,
            VecElementType::I32,
            VecWidth::V512,
            false,
        ),
        (
            &[0x62, 0xF2, 0xFD, 0x89, 0x89, 0xCB][..],
            false,
            VecElementType::I64,
            VecWidth::V128,
            true,
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x2B, 0x88, 0xE6][..],
            false,
            VecElementType::F32,
            VecWidth::V256,
            false,
        ),
        (
            &[0x62, 0xA2, 0xFD, 0xCA, 0x88, 0xCB][..],
            false,
            VecElementType::F64,
            VecWidth::V512,
            true,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(lifted.ops.iter().any(|op| match op.kind {
            OpKind::VCompress {
                elem: actual_elem,
                width: actual_width,
                zeroing: actual_zeroing,
                ..
            } if compress =>
                actual_elem == elem && actual_width == width && actual_zeroing == zeroing,
            OpKind::VExpand {
                elem: actual_elem,
                width: actual_width,
                zeroing: actual_zeroing,
                ..
            } if !compress =>
                actual_elem == elem && actual_width == width && actual_zeroing == zeroing,
            _ => false,
        }));
    }

    let store = lift_single(&[0x62, 0xE2, 0x7D, 0x4A, 0x8B, 0x50, 0x01]).unwrap();
    assert_eq!(
        store
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredStore {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );
    assert!(
        !store
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::VCompress { .. }))
    );
    let load = lift_single(&[0x62, 0xE2, 0x7D, 0xCA, 0x89, 0x48, 0x01]).unwrap();
    assert_eq!(
        load.ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );
    let last_load = load
        .ops
        .iter()
        .rposition(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .unwrap();
    let architectural_write = load
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    ..
                }
            )
        })
        .unwrap();
    assert!(last_load < architectural_write);

    for bytes in [
        &[0xC4, 0xE2, 0x7D, 0x8B, 0xD1][..],       // EVEX-only
        &[0x62, 0xF2, 0x7C, 0x09, 0x63, 0xD1][..], // mandatory 66 absent
        &[0x62, 0xF2, 0x75, 0x09, 0x63, 0xD1][..], // EVEX.vvvv reserved
        &[0x62, 0xF2, 0x7D, 0x19, 0x63, 0xD1][..], // EVEX.b reserved
        &[0x62, 0xF2, 0x7D, 0x69, 0x63, 0xD1][..], // L'L=3
        &[0x62, 0xF2, 0x7D, 0x88, 0x63, 0xD1][..], // {z} with k0
        &[0x62, 0xE2, 0x7D, 0xCA, 0x8B, 0x10][..], // memory compress cannot use {z}
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_evex_vector_align_covers_elements_widths_e4nf_memory_and_invalids() {
    for bytes in [
        &[0x62, 0xF3, 0x6D, 0x08, 0x03, 0xCB, 0x01][..],
        &[0x62, 0xA3, 0xD5, 0xA3, 0x03, 0xE6, 0x07][..],
        &[0x62, 0xC3, 0x6D, 0x47, 0x03, 0x4D, 0x01, 0x1F][..],
        &[0x62, 0x03, 0x8D, 0xC1, 0x03, 0xFD, 0x0F][..],
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(
            lifted
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
        );
    }

    let memory = lift_single(&[0x62, 0xC3, 0x6D, 0x47, 0x03, 0x4D, 0x01, 0x1F]).unwrap();
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V512,
            ..
        }
    )));
    assert!(
        !memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );

    for bytes in [
        &[0xC4, 0xE3, 0x6D, 0x03, 0xCB, 1][..],       // EVEX-only
        &[0x62, 0xF3, 0x6C, 0x08, 0x03, 0xCB, 1][..], // mandatory 66 absent
        &[0x62, 0xF3, 0x6D, 0x68, 0x03, 0xCB, 1][..], // L'L=3
        &[0x62, 0xF3, 0x6D, 0x88, 0x03, 0xCB, 1][..], // {z} with k0
        &[0x62, 0xF3, 0x6D, 0x18, 0x03, 0xCB, 1][..], // EVEX.b on register
        &[0x62, 0xF3, 0x6D, 0x08, 0x03, 0xCB][..],    // missing immediate
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "accepted reserved vector-align encoding {bytes:02X?}"
        );
    }
}
#[test]
fn lift_two_source_shuffle_covers_legacy_vex_evex_and_invalids() {
    for (bytes, elem, lanes) in [
        (&[0x45, 0x0F, 0xC6, 0xCA, 0xE4][..], VecElementType::F32, 4),
        (
            &[0x66, 0x45, 0x0F, 0xC6, 0xCA, 0x02][..],
            VecElementType::F64,
            2,
        ),
        (
            &[0xC4, 0x41, 0x28, 0xC6, 0xCB, 0xE4][..],
            VecElementType::F32,
            4,
        ),
        (&[0xC5, 0x2C, 0xC6, 0xCA, 0xE4][..], VecElementType::F32, 8),
        (
            &[0xC4, 0x41, 0x29, 0xC6, 0xCB, 0x02][..],
            VecElementType::F64,
            2,
        ),
        (
            &[0xC4, 0x41, 0x2D, 0xC6, 0xCB, 0x0A][..],
            VecElementType::F64,
            4,
        ),
        (
            &[0x62, 0xA1, 0x6C, 0xC3, 0xC6, 0xCB, 0xE4][..],
            VecElementType::F32,
            16,
        ),
        (
            &[0x62, 0xA1, 0xED, 0x43, 0xC6, 0xCB, 0xAA][..],
            VecElementType::F64,
            8,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(
            result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VShuffle {
                src2: Some(_), elem: actual_elem, lanes: actual_lanes, ..
            } if actual_elem == elem && actual_lanes == lanes))
        );
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );
    }

    for bytes in [
        &[0x44, 0x0F, 0xC6, 0x48, 0x11, 0xE4][..],
        &[0xC5, 0x2C, 0xC6, 0x48, 0x11, 0xE4][..],
        &[0x62, 0xE1, 0xED, 0x43, 0xC6, 0x48, 0x7F, 0xAA][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(
            result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VLoad { .. }))
        );
    }
    for bytes in [
        &[0x62, 0xE1, 0x6C, 0x53, 0xC6, 0x48, 0x7F, 0xE4][..],
        &[0x62, 0xE1, 0xED, 0xD3, 0xC6, 0x48, 0x7F, 0xAA][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(
            result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::Load { .. }))
        );
        assert!(
            result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VBroadcast { .. }))
        );
    }

    for bytes in [
        &[0xF3, 0x0F, 0xC6, 0xCA, 0x00][..],
        &[0x66, 0xF2, 0x0F, 0xC6, 0xCA, 0x00][..],
        &[0x0F, 0xC6, 0xCA][..],
        &[0xC4, 0x41, 0x2A, 0xC6, 0xCB, 0x00][..],
        &[0x62, 0xA1, 0xEC, 0x43, 0xC6, 0xCB, 0x00][..],
        &[0x62, 0xA1, 0xED, 0x63, 0xC6, 0xCB, 0x00][..],
        &[0x62, 0xA1, 0xED, 0x80, 0xC6, 0xCB, 0x00][..],
        &[0x62, 0xA1, 0xED, 0x53, 0xC6, 0xCB, 0x00][..],
        &[0xC4, 0x41, 0x28, 0xC6, 0xCB][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Incomplete { .. }
                    | LiftError::Unsupported { .. })
            ),
            "invalid two-source shuffle accepted: {bytes:02X?}"
        );
    }
}
#[test]
fn lift_full_vector_moves_preserves_alignment_width_high_registers_and_evex_disp8() {
    for (bytes, aligned) in [
        (&[0x0F, 0x28, 0x18][..], true),
        (&[0x0F, 0x10, 0x18][..], false),
        (&[0x66, 0x0F, 0x6F, 0x18][..], true),
        (&[0xF3, 0x0F, 0x6F, 0x18][..], false),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                width: VecWidth::V128,
                ..
            }
        )));
        assert_eq!(
            lifted
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. })),
            aligned,
            "legacy move alignment classification: {bytes:02X?}"
        );
    }

    for (bytes, aligned) in [
        (&[0xC5, 0x7C, 0x28, 0x4B, 0x20][..], true),
        (&[0xC5, 0x7D, 0x6F, 0x4B, 0x20][..], true),
        (&[0xC5, 0x7E, 0x6F, 0x4B, 0x20][..], false),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                addr: Address::BaseOffset { offset: 32, .. },
                width: VecWidth::V256,
            }
        )));
        assert_eq!(
            lifted
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 32, .. })),
            aligned,
            "VEX move alignment classification: {bytes:02X?}"
        );
    }

    for (bytes, dst, aligned) in [
        (
            &[0x62, 0xE1, 0x7C, 0x48, 0x28, 0x60, 0x01][..],
            X86Reg::Zmm(20),
            true,
        ),
        (
            &[0x62, 0xE1, 0x7D, 0x48, 0x6F, 0x68, 0x01][..],
            X86Reg::Zmm(21),
            true,
        ),
        (
            &[0x62, 0xE1, 0xFE, 0x48, 0x6F, 0x70, 0x01][..],
            X86Reg::Zmm(22),
            false,
        ),
        (
            &[0x62, 0xE1, 0xFF, 0x48, 0x6F, 0x78, 0x01][..],
            X86Reg::Zmm(23),
            false,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(lifted.ops.iter().any(|op| matches!(
            &op.kind,
            OpKind::VLoad {
                dst: VReg::Arch(ArchReg::X86(actual)),
                addr: Address::BaseOffset { offset: 64, .. },
                width: VecWidth::V512,
            } if *actual == dst
        )));
        assert_eq!(
            lifted
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 64, .. })),
            aligned,
            "EVEX move alignment classification: {bytes:02X?}"
        );
    }

    let store = lift_single(&[0x62, 0xE1, 0x7C, 0x48, 0x29, 0x60, 0x02]).unwrap();
    assert!(store.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VStore {
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(20))),
            addr: Address::BaseOffset { offset: 128, .. },
            width: VecWidth::V512,
        }
    )));
    assert!(
        store
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 64, .. }))
    );

    for invalid in [
        &[0x62, 0xE1, 0x74, 0x48, 0x28, 0x20][..], // reserved vvvv
        &[0x62, 0xE1, 0x7C, 0x58, 0x28, 0x20][..], // EVEX.b reserved
    ] {
        assert!(matches!(
            lift_single(invalid),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
