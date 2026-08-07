//! tests::scalar tests

use super::*;
use crate::smir::ir::ops::{X86CmpxchgOp, X86GprOperand};
use crate::smir::lift::x86_64::*;

/// LEA computes the segment OFFSET and must IGNORE a segment override —
/// `lea rax, fs:[rbx]` yields rbx, so it must NOT lift to a SegmentRel that
/// would add fs_base. (Regression for the segment-base-in-LEA bug.)
#[test]
fn lea_ignores_segment_override() {
    let ops = lift_one(&[0x64, 0x48, 0x8d, 0x03]).expect("lift lea fs:[rbx]"); // lea rax, fs:[rbx]
    let addr = ops
        .iter()
        .find_map(|o| match &o.kind {
            OpKind::X86Lea { addr, .. } => Some(addr),
            _ => None,
        })
        .expect("a Lea op");
    assert!(
        !matches!(addr, Address::SegmentRel { .. }),
        "LEA must NOT add the segment base (got {addr:?})"
    );
}

#[test]
fn lea_lifts_architectural_destination_width() {
    for (name, bytes, width) in [
        ("r16", &[0x66, 0x8d, 0x50, 0x01][..], OpWidth::W16),
        ("r32", &[0x8d, 0x50, 0x01][..], OpWidth::W32),
    ] {
        let ops = lift_one(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(ops.len(), 1, "{name}: one width-aware LEA");
        match ops[0].kind {
            OpKind::X86Lea {
                dst,
                width: got_width,
                ..
            } => {
                assert_eq!(dst, x86_gpr(2), "{name}: architectural destination");
                assert_eq!(got_width, width, "{name}: destination width");
            }
            ref other => panic!("{name}: expected x86 LEA, got {other:?}"),
        }
    }

    let ops = lift_one(&[0x48, 0x8d, 0x50, 0x01]).expect("r64 LEA");
    assert_eq!(ops.len(), 1);
    assert!(matches!(
        ops[0].kind,
        OpKind::X86Lea {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
            width: OpWidth::W64,
            ..
        }
    ));
}
/// A genuine FS/GS LOAD, by contrast, DOES carry the segment base.
#[test]
fn mov_gs_load_produces_segmentrel() {
    let ops = lift_one(&[0x65, 0x48, 0x8b, 0x03]).expect("lift mov rax, gs:[rbx]"); // mov rax, gs:[rbx]
    let addr = ops
        .iter()
        .find_map(|o| match &o.kind {
            OpKind::Load { addr, .. } => Some(addr),
            _ => None,
        })
        .expect("a Load op");
    assert!(
        matches!(
            addr,
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
                ..
            }
        ),
        "mov gs:[rbx] must lift to SegmentRel{{GsBase}} (got {addr:?})"
    );
}
#[test]
fn lift_movx_legacy_high_byte_sources_without_virtual_capture() {
    for (name, bytes, signed, dst, src, to_width) in [
        (
            "movzx eax,ah",
            &[0x0F, 0xB6, 0xC4][..],
            false,
            x86_gpr(0),
            x86_gpr(0),
            OpWidth::W32,
        ),
        (
            "movsx ecx,bh",
            &[0x0F, 0xBE, 0xCF][..],
            true,
            x86_gpr(1),
            x86_gpr(3),
            OpWidth::W32,
        ),
        (
            "movzx dx,ch",
            &[0x66, 0x0F, 0xB6, 0xD5][..],
            false,
            x86_gpr(2),
            x86_gpr(1),
            OpWidth::W16,
        ),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(result.ops.len(), 1, "{name}: no virtual byte capture");
        assert_eq!(
            result.ops[0].x86_hint,
            Some(X86OpHint::LegacyHighByteReg),
            "{name}"
        );
        match (&result.ops[0].kind, signed) {
            (
                OpKind::ZeroExtend {
                    dst: got_dst,
                    src: got_src,
                    from_width: OpWidth::W8,
                    to_width: got_to_width,
                },
                false,
            )
            | (
                OpKind::SignExtend {
                    dst: got_dst,
                    src: got_src,
                    from_width: OpWidth::W8,
                    to_width: got_to_width,
                },
                true,
            ) => {
                assert_eq!(*got_dst, dst, "{name}: destination");
                assert_eq!(*got_src, src, "{name}: high-byte parent");
                assert_eq!(*got_to_width, to_width, "{name}: width");
            }
            (other, _) => panic!("{name}: unexpected operation {other:?}"),
        }
    }

    for bytes in [
        &[0x40, 0x0F, 0xB6, 0xC4][..], // movzx eax,spl
        &[0x40, 0x0F, 0xBE, 0xFE][..], // movsx edi,sil
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.ops.len(), 1);
        assert_eq!(result.ops[0].x86_hint, Some(X86OpHint::RexByteReg));
    }
}
#[test]
fn lift_cmpxchg_accumulator_destination_alias_is_one_exact_operation() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `cmpxchgq %rcx, %rax` => 48 0f b1 c8.
    let result = lifter
        .lift_insn(0x1000, &[0x48, 0x0F, 0xB1, 0xC8], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 4);
    assert_eq!(result.ops.len(), 1);
    let OpKind::X86Cmpxchg(cmpxchg) = &result.ops[0].kind else {
        panic!("expected dedicated CMPXCHG, got {:?}", result.ops[0].kind);
    };
    assert_eq!(cmpxchg.dst, X86GprOperand::low(X86Reg::Rax));
    assert_eq!(cmpxchg.src, X86GprOperand::low(X86Reg::Rcx));
    assert_eq!(cmpxchg.width, OpWidth::W64);
    assert_eq!(cmpxchg.flags, FlagUpdate::All);
}

#[test]
fn lift_lock_cmpxchg_register_rejected_like_spec() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // Intel CMPXCHG specifies #UD when LOCK is used without a memory destination.
    for bytes in [
        &[0xF0, 0x48, 0x0F, 0xB1, 0xC8][..],
        &[0xF0, 0xD5, 0xD8, 0xB1, 0xC8][..],
    ] {
        let err = lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap_err();
        assert!(matches!(err, LiftError::InvalidEncoding { .. }), "{err:?}");
    }
}
#[test]
fn lift_xadd_same_register_writes_sum_last_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `xadd rax, rax` => 48 0f c1 c0.
    let result = lifter
        .lift_insn(0x1000, &[0x48, 0x0F, 0xC1, 0xC0], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 4);
    assert_xadd_register_ops(
        &result,
        "xadd_rax_rax",
        x86_gpr(0),
        x86_gpr(0),
        OpWidth::W64,
    );
}

#[test]
fn lift_xadd_retains_legacy_high_byte_and_rex_low_byte_lanes() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let cases = [
        (
            &[0x0F, 0xC0, 0xFC][..],
            X86GprOperand::high(X86Reg::Rax),
            X86GprOperand::high(X86Reg::Rbx),
        ), // XADD AH,BH
        (
            &[0x0F, 0xC0, 0xE0][..],
            X86GprOperand::low(X86Reg::Rax),
            X86GprOperand::high(X86Reg::Rax),
        ), // XADD AL,AH
        (
            &[0x40, 0x0F, 0xC0, 0xEC][..],
            X86GprOperand::low(X86Reg::Rsp),
            X86GprOperand::low(X86Reg::Rbp),
        ), // XADD SPL,BPL
    ];

    for (bytes, expected_dst, expected_src) in cases {
        let result = lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(result.ops.len(), 1);
        let OpKind::X86Xadd(xadd) = &result.ops[0].kind else {
            panic!("expected dedicated XADD for {bytes:02X?}");
        };
        assert_eq!(xadd.dst, expected_dst);
        assert_eq!(xadd.src, expected_src);
        assert_eq!(xadd.width, OpWidth::W8);
        assert_eq!(xadd.flags, FlagUpdate::All);
    }
}
#[test]
fn lift_lock_xadd_register_rejected_like_spec() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // Intel XADD specifies #UD when LOCK is used without a memory destination.
    for bytes in [
        &[0xF0, 0x48, 0x0F, 0xC1, 0xC0][..],
        &[0xF0, 0xD5, 0xD8, 0xC1, 0xC8][..],
    ] {
        let err = lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap_err();
        assert!(matches!(err, LiftError::InvalidEncoding { .. }), "{err:?}");
    }
}
#[test]
fn lift_legacy_group2_sal_alias_marks_memory_sequence_jit_unsafe() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (name, bytes) in [
        ("imm8", &[0xC0, 0x33, 0x04][..]),
        ("one", &[0xD0, 0x33][..]),
        ("cl", &[0xD2, 0x33][..]),
    ] {
        let result = lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap();
        assert_eq!(result.ops.len(), 4, "{name}");
        for index in [1, 3] {
            assert!(
                matches!(result.ops[index].kind, OpKind::Shl { .. }),
                "{name} op {index}: {:?}",
                result.ops[index].kind
            );
            assert_eq!(
                result.ops[index].x86_hint,
                Some(X86OpHint::ShiftGroup6),
                "{name} op {index}"
            );
            assert!(!result.ops[index].is_jit_safe(), "{name} op {index}");
        }
    }
}
#[test]
fn lift_byte_group3_models_ax_as_the_only_implicit_destination() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (bytes, name, group) in [
        (&[0xF6, 0xE3][..], "mul bl", 4),
        (&[0xF6, 0xEB][..], "imul bl", 5),
        (&[0xF6, 0xF3][..], "div bl", 6),
        (&[0xF6, 0xFB][..], "idiv bl", 7),
        (
            &[0x62, 0xF4, 0xFC, 0x0C, 0xF6, 0xE3][..],
            "APX NF mul bl",
            4,
        ),
        (
            &[0x62, 0xF4, 0xFC, 0x0C, 0xF6, 0xF3][..],
            "APX NF div bl",
            6,
        ),
    ] {
        let lifted = lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap();
        assert_eq!(lifted.ops.len(), 1, "{name}");
        match (&lifted.ops[0].kind, group) {
            (
                OpKind::MulU {
                    dst_lo,
                    dst_hi: None,
                    src1,
                    width: OpWidth::W8,
                    ..
                }
                | OpKind::MulS {
                    dst_lo,
                    dst_hi: None,
                    src1,
                    width: OpWidth::W8,
                    ..
                },
                4 | 5,
            ) => {
                assert_eq!(*dst_lo, x86_gpr(0), "{name}: AX destination");
                assert_eq!(*src1, x86_gpr(0), "{name}: AL multiplicand");
            }
            (
                OpKind::DivU {
                    quot,
                    rem: None,
                    src1,
                    width: OpWidth::W8,
                    ..
                }
                | OpKind::DivS {
                    quot,
                    rem: None,
                    src1,
                    width: OpWidth::W8,
                    ..
                },
                6 | 7,
            ) => {
                assert_eq!(*quot, x86_gpr(0), "{name}: AL:AH destination");
                assert_eq!(*src1, x86_gpr(0), "{name}: AX dividend");
            }
            (other, _) => {
                panic!("expected byte implicit group-3 shape for {name}, got {other:?}")
            }
        }
        assert_eq!(lifted.ops[0].kind.dests(), vec![x86_gpr(0)], "{name}");
    }
}
#[test]
fn test_lift_nop() {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // NOP
    let result = lifter.lift_insn(0x1000, &[0x90], &mut ctx).unwrap();
    assert_eq!(result.bytes_consumed, 1);
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert!(
        result.ops.is_empty(),
        "90 must not become a W32 EAX self-write"
    );

    let result = lifter.lift_insn(0x1000, &[0x48, 0x90], &mut ctx).unwrap();
    assert_eq!(result.bytes_consumed, 2);
    assert!(
        result.ops.is_empty(),
        "REX.W 90 is also an architectural NOP"
    );
}
#[test]
fn test_lift_mov_r_imm() {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // MOV EAX, 0x12345678
    let result = lifter
        .lift_insn(0x1000, &[0xB8, 0x78, 0x56, 0x34, 0x12], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 5);
    assert_eq!(result.ops.len(), 1);

    // MOV RAX, 0x123456789ABCDEF0 (REX.W prefix)
    let result = lifter
        .lift_insn(
            0x1000,
            &[0x48, 0xB8, 0xF0, 0xDE, 0xBC, 0x9A, 0x78, 0x56, 0x34, 0x12],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(result.bytes_consumed, 10);
}
#[test]
fn test_lift_test_acc_imm() {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    let result = lifter.lift_insn(0x1000, &[0xA8, 0x01], &mut ctx).unwrap();
    assert_eq!(result.bytes_consumed, 2);
    match &result.ops[0].kind {
        OpKind::Test { src1, src2, width } => {
            assert_eq!(*src1, lifter.gpr(0));
            assert_eq!(*src2, SrcOperand::Imm(1));
            assert_eq!(*width, OpWidth::W8);
        }
        other => panic!("expected TEST AL, imm8 lift, got {other:?}"),
    }

    let result = lifter
        .lift_insn(0x1000, &[0x48, 0xA9, 0xFF, 0xFF, 0xFF, 0xFF], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 6);
    match &result.ops[0].kind {
        OpKind::Test { src1, src2, width } => {
            assert_eq!(*src1, lifter.gpr(0));
            assert_eq!(*src2, SrcOperand::Imm(-1));
            assert_eq!(*width, OpWidth::W64);
        }
        other => panic!("expected TEST RAX, imm32 lift, got {other:?}"),
    }
}
#[test]
fn lift_cbw_cwde_cdqe_widths() {
    for (bytes, from_width, to_width) in [
        (&[0x66, 0x98][..], OpWidth::W8, OpWidth::W16),
        (&[0x98][..], OpWidth::W16, OpWidth::W32),
        (&[0x48, 0x98][..], OpWidth::W32, OpWidth::W64),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        match result.ops.as_slice() {
            [
                SmirOp {
                    kind:
                        OpKind::SignExtend {
                            dst,
                            src,
                            from_width: got_from,
                            to_width: got_to,
                        },
                    ..
                },
            ] => {
                assert_eq!(*dst, x86_gpr(0));
                assert_eq!(*src, x86_gpr(0));
                assert_eq!(*got_from, from_width);
                assert_eq!(*got_to, to_width);
            }
            other => panic!("unexpected 98 lift: {other:?}"),
        }
    }
}
#[test]
fn lift_bit_test_register_forms_widths_and_partial_destinations() {
    for (opcode, expected) in [(0xA3, "bt"), (0xAB, "bts"), (0xB3, "btr"), (0xBB, "btc")] {
        let result = lift_single(&[0x66, 0x0F, opcode, 0xC8]).unwrap();
        assert_eq!(result.bytes_consumed, 4);
        let matches_kind = match (&result.ops[0].kind, expected) {
            (
                OpKind::Bt {
                    width: OpWidth::W16,
                    ..
                },
                "bt",
            ) => true,
            (
                OpKind::Bts {
                    width: OpWidth::W16,
                    ..
                },
                "bts",
            ) => true,
            (
                OpKind::Btr {
                    width: OpWidth::W16,
                    ..
                },
                "btr",
            ) => true,
            (
                OpKind::Btc {
                    width: OpWidth::W16,
                    ..
                },
                "btc",
            ) => true,
            _ => false,
        };
        assert!(matches_kind, "unexpected {expected} lift: {:?}", result.ops);
    }

    let wide = lift_single(&[0x48, 0x0F, 0xAB, 0xC8]).unwrap();
    assert!(matches!(
        wide.ops[0].kind,
        OpKind::Bts {
            width: OpWidth::W64,
            ..
        }
    ));
}
#[test]
fn lift_bit_test_memory_bit_string_and_group8_forms() {
    let indexed = lift_single(&[0x48, 0x0F, 0xA3, 0x08]).unwrap(); // bt [rax],rcx
    assert!(indexed.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::SignExtend {
            src: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
            from_width: OpWidth::W64,
            to_width: OpWidth::W64,
            ..
        }
    )));
    assert!(indexed.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Sar {
            amount: SrcOperand::Imm(6),
            flags: FlagUpdate::None,
            ..
        }
    )));
    assert!(indexed.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Shl {
            amount: SrcOperand::Imm(3),
            flags: FlagUpdate::None,
            ..
        }
    )));
    assert!(matches!(
        indexed.ops.last().unwrap().kind,
        OpKind::Bt { .. }
    ));

    let immediate = lift_single(&[0x0F, 0xBA, 0x28, 0x25]).unwrap(); // bts dword [rax],37
    assert_eq!(immediate.bytes_consumed, 4);
    let store_idx = immediate
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::Store { .. }))
        .unwrap();
    let bt_idx = immediate
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::Bt { .. }))
        .unwrap();
    assert!(store_idx < bt_idx, "CF must commit after the update store");
    assert!(
        !immediate
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::SignExtend { .. }))
    );

    let riprel = lift_single(&[0x0F, 0xBA, 0x25, 0, 0, 0, 0, 1]).unwrap();
    assert!(riprel.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::PcRel {
                base: Some(0x1008),
                ..
            },
            ..
        }
    )));

    for (modrm, expected) in [(0xE0, "bt"), (0xE8, "bts"), (0xF0, "btr"), (0xF8, "btc")] {
        let result = lift_single(&[0x0F, 0xBA, modrm, 7]).unwrap();
        let found = result.ops.iter().any(|op| match (&op.kind, expected) {
            (
                OpKind::Bt {
                    index: SrcOperand::Imm(7),
                    ..
                },
                "bt",
            ) => true,
            (
                OpKind::Bts {
                    index: SrcOperand::Imm(7),
                    ..
                },
                "bts",
            ) => true,
            (
                OpKind::Btr {
                    index: SrcOperand::Imm(7),
                    ..
                },
                "btr",
            ) => true,
            (
                OpKind::Btc {
                    index: SrcOperand::Imm(7),
                    ..
                },
                "btc",
            ) => true,
            _ => false,
        });
        assert!(found, "missing Group-8 {expected} register lift");
    }
}
#[test]
fn lift_bit_test_lock_atomic_and_invalid_encodings() {
    for (opcode, atomic_op) in [
        (0xAB, AtomicOp::Or),
        (0xB3, AtomicOp::And),
        (0xBB, AtomicOp::Xor),
    ] {
        let result = lift_single(&[0xF0, 0x48, 0x0F, opcode, 0x08]).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::AtomicRmw {
                op,
                width: MemWidth::B8,
                order: MemoryOrder::SeqCst,
                ..
            } if op == atomic_op
        )));
        assert!(matches!(result.ops.last().unwrap().kind, OpKind::Bt { .. }));
    }

    let locked_imm = lift_single(&[0xF0, 0x0F, 0xBA, 0x28, 5]).unwrap();
    assert!(locked_imm.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::AtomicRmw {
            op: AtomicOp::Or,
            width: MemWidth::B4,
            order: MemoryOrder::SeqCst,
            ..
        }
    )));

    assert!(matches!(
        lift_single(&[0xF0, 0x48, 0x0F, 0xA3, 0x08]),
        Err(LiftError::InvalidEncoding { .. })
    ));
    assert!(matches!(
        lift_single(&[0xF0, 0x48, 0x0F, 0xAB, 0xC8]),
        Err(LiftError::InvalidEncoding { .. })
    ));
    assert!(matches!(
        lift_single(&[0x0F, 0xBA, 0xC0, 1]),
        Err(LiftError::InvalidEncoding { .. })
    ));
    assert!(matches!(
        lift_single(&[0x0F, 0xBA, 0xE0]),
        Err(LiftError::Incomplete { .. })
    ));
}
#[test]
fn lift_fwait_and_memory_fences() {
    let wait = lift_single(&[0x9B]).unwrap();
    assert_eq!(wait.bytes_consumed, 1);
    assert!(wait.ops.is_empty());

    for (bytes, expected) in [
        (&[0x0F, 0xAE, 0xE8][..], FenceKind::LoadLoad),
        (&[0x0F, 0xAE, 0xF0][..], FenceKind::Full),
        (&[0x0F, 0xAE, 0xF8][..], FenceKind::StoreStore),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::Fence { kind },
                ..
            }] if *kind == expected
        ));
    }

    for (bytes, expected) in [
        (&[0x0F, 0xAE, 0x38][..], X86CacheControlKind::Clflush),
        (
            &[0x66, 0x0F, 0xAE, 0x38][..],
            X86CacheControlKind::Clflushopt,
        ),
        (&[0x66, 0x0F, 0xAE, 0x30][..], X86CacheControlKind::Clwb),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86CacheControl { kind, .. } if kind == expected
        ));
    }

    assert!(matches!(
        lift_single(&[0x0F, 0xAE, 0x28]).unwrap().ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86XRstor { .. },
            ..
        }]
    ));
    assert_invalid_opcode_trap(
        &lift_single(&[0xF0, 0x0F, 0xAE, 0xF0]).expect("LOCK MFENCE must strictly lift to #UD"),
        4,
    );
    assert!(matches!(
        lift_single(&[0xF0, 0x9B]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}
#[test]
fn lift_xgetbv_xsetbv_fixed_encodings_and_legality() {
    for (bytes, set) in [
        (&[0x0F, 0x01, 0xD0][..], false),
        (&[0x0F, 0x01, 0xD1][..], true),
        (&[0x48, 0x0F, 0x01, 0xD0][..], false),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(
            matches!(
                result.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::X86XSetBv {
                        selector: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
                        src_low: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                        src_high: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
                    },
                    ..
                }] if set
            ) || matches!(
                result.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::X86XGetBv {
                        dst_low: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                        dst_high: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
                        selector: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
                    },
                    ..
                }] if !set
            )
        );
    }

    for bytes in [
        &[0xF0, 0x0F, 0x01, 0xD0][..],
        &[0x66, 0x0F, 0x01, 0xD0][..],
        &[0xF3, 0x0F, 0x01, 0xD1][..],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
    let reserved_neighbor = lift_single(&[0x0F, 0x01, 0xD2]).unwrap();
    assert!(reserved_neighbor.ops.is_empty());
    assert!(matches!(
        reserved_neighbor.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));
    assert!(matches!(
        lift_single(&[0x0F, 0x01]),
        Err(LiftError::Incomplete { .. })
    ));
}
#[test]
fn lift_x87_environment_control_encodings_and_legality() {
    for (bytes, expected, has_memory) in [
        (
            &[0xD9, 0x20][..],
            X86X87ControlKind::LoadEnvironment(X86X87EnvWidth::W32),
            true,
        ),
        (
            &[0x66, 0xD9, 0x20][..],
            X86X87ControlKind::LoadEnvironment(X86X87EnvWidth::W16),
            true,
        ),
        (
            &[0xD9, 0x30][..],
            X86X87ControlKind::StoreEnvironment(X86X87EnvWidth::W32),
            true,
        ),
        (
            &[0x66, 0xD9, 0x30][..],
            X86X87ControlKind::StoreEnvironment(X86X87EnvWidth::W16),
            true,
        ),
        (
            &[0xDD, 0x20][..],
            X86X87ControlKind::RestoreState(X86X87EnvWidth::W32),
            true,
        ),
        (
            &[0x66, 0xDD, 0x20][..],
            X86X87ControlKind::RestoreState(X86X87EnvWidth::W16),
            true,
        ),
        (
            &[0xDD, 0x30][..],
            X86X87ControlKind::SaveState(X86X87EnvWidth::W32),
            true,
        ),
        (
            &[0x66, 0xDD, 0x30][..],
            X86X87ControlKind::SaveState(X86X87EnvWidth::W16),
            true,
        ),
        (&[0xD9, 0x28][..], X86X87ControlKind::LoadControlWord, true),
        (&[0xD9, 0x38][..], X86X87ControlKind::StoreControlWord, true),
        (&[0xDD, 0x38][..], X86X87ControlKind::StoreStatusWord, true),
        (&[0xDB, 0xE2][..], X86X87ControlKind::ClearExceptions, false),
        (&[0xDB, 0xE3][..], X86X87ControlKind::Init, false),
        (&[0xDF, 0xE0][..], X86X87ControlKind::StoreStatusAx, false),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86X87Control { kind, addr },
                ..
            }] if *kind == expected && addr.is_some() == has_memory
        ));
    }

    let fnop = lift_single(&[0xD9, 0xD0]).unwrap();
    assert_eq!(fnop.bytes_consumed, 2);
    assert!(fnop.ops.is_empty());

    let addr32 = lift_single(&[0x67, 0xD9, 0x6B, 0x20]).unwrap();
    assert_eq!(addr32.bytes_consumed, 4);
    let [
        SmirOp {
            kind:
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::LoadControlWord,
                    addr: Some(addr),
                },
            ..
        },
    ] = addr32.ops.as_slice()
    else {
        panic!("expected one addr32 FLDCW operation")
    };
    super::addr32_assertions::base_offset(addr, X86Reg::Rbx, 0x20);

    for bytes in [
        &[0xF0, 0xDB, 0xE3][..],
        &[0xF0, 0xD9, 0x28][..],
        &[0xF0, 0xD9, 0x30][..],
        &[0xF0, 0xDD, 0x20][..],
    ] {
        let result = lift_single(bytes).expect("LOCK x87 control form must strictly lift to #UD");
        assert_invalid_opcode_trap(&result, bytes.len());
    }
    for bytes in [
        &[0xD9, 0xD1][..], // reserved register encoding
        &[0xD9, 0x08][..], // reserved memory /1
    ] {
        let result =
            lift_single(bytes).expect("reserved x87 control form must strictly lift to #UD");
        assert_invalid_opcode_trap(&result, bytes.len());
    }
}
#[test]
fn lift_x87_exact_data_transfer_encodings_addressing_and_legality() {
    for (bytes, expected_kind, expected_st, has_memory, expected_fop) in [
        (
            &[0xD9, 0x00][..],
            X86X87DataKind::LoadSingle,
            0,
            true,
            0x0100,
        ),
        (
            &[0xDD, 0x00][..],
            X86X87DataKind::LoadDouble,
            0,
            true,
            0x0500,
        ),
        (
            &[0xDF, 0x00][..],
            X86X87DataKind::LoadInt16,
            0,
            true,
            0x0700,
        ),
        (
            &[0xDB, 0x00][..],
            X86X87DataKind::LoadInt32,
            0,
            true,
            0x0300,
        ),
        (
            &[0xDF, 0x28][..],
            X86X87DataKind::LoadInt64,
            0,
            true,
            0x0728,
        ),
        (&[0xDF, 0x20][..], X86X87DataKind::LoadBcd, 0, true, 0x0720),
        (
            &[0xD8, 0x00][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Single,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: false,
                reverse: false,
            },
            0,
            true,
            0x0000,
        ),
        (
            &[0xDC, 0x00][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Double,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: false,
                reverse: false,
            },
            0,
            true,
            0x0400,
        ),
        (
            &[0xDE, 0x00][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Int16,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: false,
                reverse: false,
            },
            0,
            true,
            0x0600,
        ),
        (
            &[0xDA, 0x00][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Int32,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: false,
                reverse: false,
            },
            0,
            true,
            0x0200,
        ),
        (
            &[0xD8, 0x20][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Single,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: true,
                reverse: false,
            },
            0,
            true,
            0x0020,
        ),
        (
            &[0xDC, 0x20][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Double,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: true,
                reverse: false,
            },
            0,
            true,
            0x0420,
        ),
        (
            &[0xDE, 0x20][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Int16,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: true,
                reverse: false,
            },
            0,
            true,
            0x0620,
        ),
        (
            &[0xDA, 0x20][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Int32,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: true,
                reverse: false,
            },
            0,
            true,
            0x0220,
        ),
        (
            &[0xD8, 0x28][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Single,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: true,
                reverse: true,
            },
            0,
            true,
            0x0028,
        ),
        (
            &[0xDC, 0x28][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Double,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: true,
                reverse: true,
            },
            0,
            true,
            0x0428,
        ),
        (
            &[0xDE, 0x28][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Int16,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: true,
                reverse: true,
            },
            0,
            true,
            0x0628,
        ),
        (
            &[0xDA, 0x28][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Int32,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: true,
                reverse: true,
            },
            0,
            true,
            0x0228,
        ),
        (
            &[0xD8, 0xC3][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: false,
                reverse: false,
            },
            3,
            false,
            0x00C3,
        ),
        (
            &[0xDC, 0xC4][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::StI,
                pop: false,
                subtract: false,
                reverse: false,
            },
            4,
            false,
            0x04C4,
        ),
        (
            &[0xDE, 0xC1][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::StI,
                pop: true,
                subtract: false,
                reverse: false,
            },
            1,
            false,
            0x06C1,
        ),
        (
            &[0xD8, 0xE3][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: true,
                reverse: false,
            },
            3,
            false,
            0x00E3,
        ),
        (
            &[0xDC, 0xEB][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::StI,
                pop: false,
                subtract: true,
                reverse: false,
            },
            3,
            false,
            0x04EB,
        ),
        (
            &[0xDE, 0xE9][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::StI,
                pop: true,
                subtract: true,
                reverse: false,
            },
            1,
            false,
            0x06E9,
        ),
        (
            &[0xD8, 0xEB][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: true,
                reverse: true,
            },
            3,
            false,
            0x00EB,
        ),
        (
            &[0xDC, 0xE3][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::StI,
                pop: false,
                subtract: true,
                reverse: true,
            },
            3,
            false,
            0x04E3,
        ),
        (
            &[0xDE, 0xE1][..],
            X86X87DataKind::AddSubtract {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::StI,
                pop: true,
                subtract: true,
                reverse: true,
            },
            1,
            false,
            0x06E1,
        ),
        (
            &[0xD8, 0x30][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Single,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                reverse: false,
            },
            0,
            true,
            0x0030,
        ),
        (
            &[0xDC, 0x30][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Double,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                reverse: false,
            },
            0,
            true,
            0x0430,
        ),
        (
            &[0xDE, 0x30][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Int16,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                reverse: false,
            },
            0,
            true,
            0x0630,
        ),
        (
            &[0xDA, 0x30][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Int32,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                reverse: false,
            },
            0,
            true,
            0x0230,
        ),
        (
            &[0xD8, 0x38][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Single,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                reverse: true,
            },
            0,
            true,
            0x0038,
        ),
        (
            &[0xDC, 0x38][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Double,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                reverse: true,
            },
            0,
            true,
            0x0438,
        ),
        (
            &[0xDE, 0x38][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Int16,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                reverse: true,
            },
            0,
            true,
            0x0638,
        ),
        (
            &[0xDA, 0x38][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Int32,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                reverse: true,
            },
            0,
            true,
            0x0238,
        ),
        (
            &[0xD8, 0xF3][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                reverse: false,
            },
            3,
            false,
            0x00F3,
        ),
        (
            &[0xDC, 0xFB][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::StI,
                pop: false,
                reverse: false,
            },
            3,
            false,
            0x04FB,
        ),
        (
            &[0xDE, 0xF9][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::StI,
                pop: true,
                reverse: false,
            },
            1,
            false,
            0x06F9,
        ),
        (
            &[0xD8, 0xFB][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
                reverse: true,
            },
            3,
            false,
            0x00FB,
        ),
        (
            &[0xDC, 0xF3][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::StI,
                pop: false,
                reverse: true,
            },
            3,
            false,
            0x04F3,
        ),
        (
            &[0xDE, 0xF1][..],
            X86X87DataKind::Divide {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::StI,
                pop: true,
                reverse: true,
            },
            1,
            false,
            0x06F1,
        ),
        (
            &[0xD8, 0x08][..],
            X86X87DataKind::Multiply {
                source: X86X87ArithmeticSource::Single,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
            },
            0,
            true,
            0x0008,
        ),
        (
            &[0xDC, 0x08][..],
            X86X87DataKind::Multiply {
                source: X86X87ArithmeticSource::Double,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
            },
            0,
            true,
            0x0408,
        ),
        (
            &[0xDE, 0x08][..],
            X86X87DataKind::Multiply {
                source: X86X87ArithmeticSource::Int16,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
            },
            0,
            true,
            0x0608,
        ),
        (
            &[0xDA, 0x08][..],
            X86X87DataKind::Multiply {
                source: X86X87ArithmeticSource::Int32,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
            },
            0,
            true,
            0x0208,
        ),
        (
            &[0xD8, 0xCB][..],
            X86X87DataKind::Multiply {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::St0,
                pop: false,
            },
            3,
            false,
            0x00CB,
        ),
        (
            &[0xDC, 0xCC][..],
            X86X87DataKind::Multiply {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::StI,
                pop: false,
            },
            4,
            false,
            0x04CC,
        ),
        (
            &[0xDE, 0xC9][..],
            X86X87DataKind::Multiply {
                source: X86X87ArithmeticSource::Register,
                destination: X86X87ArithmeticDestination::StI,
                pop: true,
            },
            1,
            false,
            0x06C9,
        ),
        (
            &[0xD9, 0xC5][..],
            X86X87DataKind::LoadRegister,
            5,
            false,
            0x01C5,
        ),
        (
            &[0xDB, 0x28][..],
            X86X87DataKind::LoadExtended,
            0,
            true,
            0x0328,
        ),
        (
            &[0xDD, 0xD6][..],
            X86X87DataKind::StoreRegister,
            6,
            false,
            0x05D6,
        ),
        (
            &[0xDD, 0xDE][..],
            X86X87DataKind::StorePopRegister,
            6,
            false,
            0x05DE,
        ),
        (
            &[0xDB, 0x38][..],
            X86X87DataKind::StorePopExtended,
            0,
            true,
            0x0338,
        ),
        (&[0xDF, 0x30][..], X86X87DataKind::StoreBcd, 0, true, 0x0730),
        (
            &[0xD9, 0x10][..],
            X86X87DataKind::StoreFloat {
                width: X86X87FloatWidth::F32,
                pop: false,
            },
            0,
            true,
            0x0110,
        ),
        (
            &[0xD9, 0x18][..],
            X86X87DataKind::StoreFloat {
                width: X86X87FloatWidth::F32,
                pop: true,
            },
            0,
            true,
            0x0118,
        ),
        (
            &[0xDD, 0x10][..],
            X86X87DataKind::StoreFloat {
                width: X86X87FloatWidth::F64,
                pop: false,
            },
            0,
            true,
            0x0510,
        ),
        (
            &[0xDD, 0x18][..],
            X86X87DataKind::StoreFloat {
                width: X86X87FloatWidth::F64,
                pop: true,
            },
            0,
            true,
            0x0518,
        ),
        (
            &[0xD9, 0xCC][..],
            X86X87DataKind::Exchange,
            4,
            false,
            0x01CC,
        ),
        (&[0xDD, 0xC3][..], X86X87DataKind::Free, 3, false, 0x05C3),
        (
            &[0xD9, 0xE0][..],
            X86X87DataKind::ChangeSign,
            0,
            false,
            0x01E0,
        ),
        (
            &[0xD9, 0xE1][..],
            X86X87DataKind::Absolute,
            1,
            false,
            0x01E1,
        ),
        (&[0xD9, 0xE5][..], X86X87DataKind::Examine, 5, false, 0x01E5),
        (
            &[0xD9, 0xE4][..],
            X86X87DataKind::TestZero,
            4,
            false,
            0x01E4,
        ),
        (
            &[0xD9, 0xFC][..],
            X86X87DataKind::RoundInteger,
            4,
            false,
            0x01FC,
        ),
        (&[0xD9, 0xF4][..], X86X87DataKind::Extract, 4, false, 0x01F4),
        (
            &[0xD9, 0xF5][..],
            X86X87DataKind::Remainder { nearest: true },
            5,
            false,
            0x01F5,
        ),
        (
            &[0xD9, 0xF8][..],
            X86X87DataKind::Remainder { nearest: false },
            0,
            false,
            0x01F8,
        ),
        (&[0xD9, 0xFD][..], X86X87DataKind::Scale, 5, false, 0x01FD),
        (
            &[0xD9, 0xFA][..],
            X86X87DataKind::SquareRoot,
            2,
            false,
            0x01FA,
        ),
        (
            &[0xD9, 0xF6][..],
            X86X87DataKind::DecrementTop,
            6,
            false,
            0x01F6,
        ),
        (
            &[0xD9, 0xF7][..],
            X86X87DataKind::IncrementTop,
            7,
            false,
            0x01F7,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86X87Data { kind, addr, st, fop },
                ..
            }] if *kind == expected_kind
                && addr.is_some() == has_memory
                && *st == expected_st
                && *fop == expected_fop
        ));
    }

    for (opcode, constant) in [
        (0xE8, X86X87Constant::One),
        (0xE9, X86X87Constant::Log2Ten),
        (0xEA, X86X87Constant::Log2E),
        (0xEB, X86X87Constant::Pi),
        (0xEC, X86X87Constant::Log10Two),
        (0xED, X86X87Constant::LnTwo),
        (0xEE, X86X87Constant::Zero),
    ] {
        let result = lift_single(&[0xD9, opcode]).unwrap();
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86X87Data {
                    kind: X86X87DataKind::LoadConstant(got),
                    addr: None,
                    fop,
                    ..
                },
                ..
            }] if *got == constant && *fop == (0x0100 | opcode as u16)
        ));
    }

    for (bytes, condition, st, fop) in [
        (&[0xDA, 0xC3][..], Condition::Ult, 3, 0x02C3),
        (&[0xDA, 0xCA][..], Condition::Eq, 2, 0x02CA),
        (&[0xDA, 0xD5][..], Condition::Ule, 5, 0x02D5),
        (&[0xDA, 0xDF][..], Condition::Parity, 7, 0x02DF),
        (&[0xDB, 0xC1][..], Condition::Uge, 1, 0x03C1),
        (&[0xDB, 0xCC][..], Condition::Ne, 4, 0x03CC),
        (&[0xDB, 0xD6][..], Condition::Ugt, 6, 0x03D6),
        (&[0xDB, 0xD8][..], Condition::NoParity, 0, 0x03D8),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86X87Data {
                    kind: X86X87DataKind::ConditionalMove(got),
                    addr: None,
                    st: got_st,
                    fop: got_fop,
                },
                ..
            }] if *got == condition && *got_st == st && *got_fop == fop
        ));
    }

    for (bytes, source, unordered, pop, eflags, st, fop, has_memory) in [
        (
            &[0xD8, 0x10][..],
            X86X87CompareSource::Single,
            false,
            0,
            false,
            0,
            0x0010,
            true,
        ),
        (
            &[0xD8, 0x18][..],
            X86X87CompareSource::Single,
            false,
            1,
            false,
            0,
            0x0018,
            true,
        ),
        (
            &[0xDC, 0x10][..],
            X86X87CompareSource::Double,
            false,
            0,
            false,
            0,
            0x0410,
            true,
        ),
        (
            &[0xDC, 0x18][..],
            X86X87CompareSource::Double,
            false,
            1,
            false,
            0,
            0x0418,
            true,
        ),
        (
            &[0xDE, 0x10][..],
            X86X87CompareSource::Int16,
            false,
            0,
            false,
            0,
            0x0610,
            true,
        ),
        (
            &[0xDE, 0x18][..],
            X86X87CompareSource::Int16,
            false,
            1,
            false,
            0,
            0x0618,
            true,
        ),
        (
            &[0xDA, 0x10][..],
            X86X87CompareSource::Int32,
            false,
            0,
            false,
            0,
            0x0210,
            true,
        ),
        (
            &[0xDA, 0x18][..],
            X86X87CompareSource::Int32,
            false,
            1,
            false,
            0,
            0x0218,
            true,
        ),
        (
            &[0xD8, 0xD3][..],
            X86X87CompareSource::Register,
            false,
            0,
            false,
            3,
            0x00D3,
            false,
        ),
        (
            &[0xD8, 0xDB][..],
            X86X87CompareSource::Register,
            false,
            1,
            false,
            3,
            0x00DB,
            false,
        ),
        (
            &[0xDD, 0xE3][..],
            X86X87CompareSource::Register,
            true,
            0,
            false,
            3,
            0x05E3,
            false,
        ),
        (
            &[0xDD, 0xEB][..],
            X86X87CompareSource::Register,
            true,
            1,
            false,
            3,
            0x05EB,
            false,
        ),
        (
            &[0xDE, 0xD9][..],
            X86X87CompareSource::Register,
            false,
            2,
            false,
            1,
            0x06D9,
            false,
        ),
        (
            &[0xDA, 0xE9][..],
            X86X87CompareSource::Register,
            true,
            2,
            false,
            1,
            0x02E9,
            false,
        ),
        (
            &[0xDB, 0xEB][..],
            X86X87CompareSource::Register,
            true,
            0,
            true,
            3,
            0x03EB,
            false,
        ),
        (
            &[0xDB, 0xF3][..],
            X86X87CompareSource::Register,
            false,
            0,
            true,
            3,
            0x03F3,
            false,
        ),
        (
            &[0xDF, 0xEB][..],
            X86X87CompareSource::Register,
            true,
            1,
            true,
            3,
            0x07EB,
            false,
        ),
        (
            &[0xDF, 0xF3][..],
            X86X87CompareSource::Register,
            false,
            1,
            true,
            3,
            0x07F3,
            false,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86X87Data {
                    kind: X86X87DataKind::Compare {
                        source: got_source,
                        unordered: got_unordered,
                        pop: got_pop,
                        eflags: got_eflags,
                    },
                    addr,
                    st: got_st,
                    fop: got_fop,
                },
                ..
            }] if *got_source == source
                && *got_unordered == unordered
                && *got_pop == pop
                && *got_eflags == eflags
                && *got_st == st
                && *got_fop == fop
                && addr.is_some() == has_memory
        ));
    }

    for (bytes, width, pop, truncate, fop) in [
        (&[0xDF, 0x10][..], X86X87IntWidth::I16, false, false, 0x0710),
        (&[0xDB, 0x10][..], X86X87IntWidth::I32, false, false, 0x0310),
        (&[0xDF, 0x18][..], X86X87IntWidth::I16, true, false, 0x0718),
        (&[0xDB, 0x18][..], X86X87IntWidth::I32, true, false, 0x0318),
        (&[0xDF, 0x38][..], X86X87IntWidth::I64, true, false, 0x0738),
        (&[0xDF, 0x08][..], X86X87IntWidth::I16, true, true, 0x0708),
        (&[0xDB, 0x08][..], X86X87IntWidth::I32, true, true, 0x0308),
        (&[0xDD, 0x08][..], X86X87IntWidth::I64, true, true, 0x0508),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86X87Data {
                    kind: X86X87DataKind::StoreInteger {
                        width: got_width,
                        pop: got_pop,
                        truncate: got_truncate,
                    },
                    addr: Some(_),
                    fop: got_fop,
                    ..
                },
                ..
            }] if *got_width == width
                && *got_pop == pop
                && *got_truncate == truncate
                && *got_fop == fop
        ));
    }

    // The exact ten-byte load retains architectural addr32 components without
    // allocator-owned W32 materialization temporaries.
    let addr32 = lift_single(&[0x67, 0xDB, 0x6C, 0x8B, 0x20]).unwrap();
    assert_eq!(addr32.bytes_consumed, 5);
    let [
        SmirOp {
            kind:
                OpKind::X86X87Data {
                    kind: X86X87DataKind::LoadExtended,
                    addr: Some(addr),
                    ..
                },
            ..
        },
    ] = addr32.ops.as_slice()
    else {
        panic!("expected one addr32 FLD m80 operation")
    };
    super::addr32_assertions::sib(addr, Some(X86Reg::Rbx), X86Reg::Rcx, 4, 0x20);

    for bytes in [
        &[0xF0, 0xD9, 0xC0][..],
        &[0xF0, 0xDB, 0x28][..],
        &[0xF0, 0xDB, 0x38][..],
        &[0xF0, 0xDD, 0xC0][..],
    ] {
        let result = lift_single(bytes).expect("LOCK x87 data form must strictly lift to #UD");
        assert_invalid_opcode_trap(&result, bytes.len());
    }
}
#[test]
fn lift_fxsave_fxrstor_width_addressing_and_legality() {
    for (bytes, save, rex_w) in [
        (&[0x0F, 0xAE, 0x00][..], true, false),
        (&[0x48, 0x0F, 0xAE, 0x00][..], true, true),
        (&[0x0F, 0xAE, 0x08][..], false, false),
        (&[0x48, 0x0F, 0xAE, 0x08][..], false, true),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(
            matches!(
                result.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::X86FxSave { rex_w: got, .. },
                    ..
                }] if save && *got == rex_w
            ) || matches!(
                result.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::X86FxRstor { rex_w: got, .. },
                    ..
                }] if !save && *got == rex_w
            )
        );
    }

    let addr32 = lift_single(&[0x67, 0x0F, 0xAE, 0x44, 0x4B, 0x20]).unwrap();
    assert_eq!(addr32.bytes_consumed, 6);
    let [
        SmirOp {
            kind: OpKind::X86FxSave { addr, rex_w: false },
            ..
        },
    ] = addr32.ops.as_slice()
    else {
        panic!("expected one addr32 FXSAVE operation")
    };
    super::addr32_assertions::sib(addr, Some(X86Reg::Rbx), X86Reg::Rcx, 2, 0x20);

    // Legacy optional prefixes are tolerated; LOCK and register operands
    // are architecturally invalid.
    for bytes in [
        &[0x66, 0x0F, 0xAE, 0x00][..],
        &[0xF2, 0x0F, 0xAE, 0x08][..],
        &[0xF3, 0x0F, 0xAE, 0x00][..],
    ] {
        assert!(lift_single(bytes).is_ok(), "{bytes:02X?}");
    }
    for bytes in [
        &[0xF0, 0x0F, 0xAE, 0x00][..],
        &[0x0F, 0xAE, 0xC0][..],
        &[0x0F, 0xAE, 0xC8][..],
    ] {
        let result =
            lift_single(bytes).expect("invalid FXSAVE/FXRSTOR form must strictly lift to #UD");
        assert_invalid_opcode_trap(&result, bytes.len());
    }
}
#[test]
fn lift_xsave_xsaveopt_xrstor_width_addressing_and_legality() {
    for (bytes, save, expected_kind, rex_w) in [
        (&[0x0F, 0xAE, 0x23][..], true, X86XSaveKind::XSave, false),
        (
            &[0x48, 0x0F, 0xAE, 0x23][..],
            true,
            X86XSaveKind::XSave,
            true,
        ),
        (&[0x0F, 0xAE, 0x33][..], true, X86XSaveKind::XSaveOpt, false),
        (
            &[0x48, 0x0F, 0xAE, 0x33][..],
            true,
            X86XSaveKind::XSaveOpt,
            true,
        ),
        (&[0x0F, 0xAE, 0x2B][..], false, X86XSaveKind::XSave, false),
        (
            &[0x48, 0x0F, 0xAE, 0x2B][..],
            false,
            X86XSaveKind::XSave,
            true,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(
            matches!(
                result.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::X86XSave {
                        rex_w: got_rex,
                        kind: got_kind,
                        ..
                    },
                    ..
                }] if save && *got_rex == rex_w && *got_kind == expected_kind
            ) || matches!(
                result.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::X86XRstor { rex_w: got_rex, .. },
                    ..
                }] if !save && *got_rex == rex_w
            ),
            "{bytes:02X?}"
        );
    }

    let addr32 = lift_single(&[0x67, 0x0F, 0xAE, 0x64, 0x4B, 0x20]).unwrap();
    assert_eq!(addr32.bytes_consumed, 6);
    let [
        SmirOp {
            kind:
                OpKind::X86XSave {
                    addr,
                    kind: X86XSaveKind::XSave,
                    ..
                },
            ..
        },
    ] = addr32.ops.as_slice()
    else {
        panic!("expected one addr32 XSAVE operation")
    };
    super::addr32_assertions::sib(addr, Some(X86Reg::Rbx), X86Reg::Rcx, 2, 0x20);

    for bytes in [
        &[0x66, 0x0F, 0xAE, 0x23][..],
        &[0x66, 0x0F, 0xAE, 0x2B][..],
        &[0xF2, 0x0F, 0xAE, 0x23][..],
        &[0xF3, 0x0F, 0xAE, 0x2B][..],
        &[0xF3, 0x0F, 0xAE, 0x33][..],
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| {
            panic!("invalid XSAVE-family form entered fallback: {bytes:02X?}: {error:?}")
        });
        assert_invalid_opcode_trap(&result, bytes.len());
    }

    assert!(matches!(
        lift_single(&[0x66, 0x0F, 0xAE, 0x33])
            .unwrap()
            .ops
            .last()
            .map(|op| &op.kind),
        Some(OpKind::X86CacheControl {
            kind: X86CacheControlKind::Clwb,
            ..
        })
    ));
    assert!(matches!(
        lift_single(&[0x0F, 0xAE, 0xE8]).unwrap().ops.as_slice(),
        [SmirOp {
            kind: OpKind::Fence {
                kind: FenceKind::LoadLoad
            },
            ..
        }]
    ));
    assert!(matches!(
        lift_single(&[0x0F, 0xAE, 0xF0]).unwrap().ops.as_slice(),
        [SmirOp {
            kind: OpKind::Fence {
                kind: FenceKind::Full
            },
            ..
        }]
    ));
}
#[test]
fn lift_compacted_xsave_family_group9_encodings_and_legality() {
    for (bytes, expected_kind, rex_w) in [
        (&[0x0F, 0xC7, 0x23][..], X86XSaveKind::XSaveC, false),
        (&[0x48, 0x0F, 0xC7, 0x23][..], X86XSaveKind::XSaveC, true),
        (&[0x0F, 0xC7, 0x2B][..], X86XSaveKind::XSaveS, false),
        (&[0x48, 0x0F, 0xC7, 0x2B][..], X86XSaveKind::XSaveS, true),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86XSave {
                    rex_w: got_rex,
                    kind,
                    ..
                },
                ..
            }] if *got_rex == rex_w && *kind == expected_kind
        ));
    }
    for (bytes, rex_w) in [
        (&[0x0F, 0xC7, 0x1B][..], false),
        (&[0x48, 0x0F, 0xC7, 0x1B][..], true),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86XRstor {
                    rex_w: got_rex,
                    supervisor: true,
                    ..
                },
                ..
            }] if *got_rex == rex_w
        ));
    }

    let addr32 = lift_single(&[0x67, 0x0F, 0xC7, 0x64, 0x4B, 0x20]).unwrap();
    assert_eq!(addr32.bytes_consumed, 6);
    let [
        SmirOp {
            kind:
                OpKind::X86XSave {
                    addr,
                    kind: X86XSaveKind::XSaveC,
                    ..
                },
            ..
        },
    ] = addr32.ops.as_slice()
    else {
        panic!("expected one addr32 XSAVEC operation")
    };
    super::addr32_assertions::sib(addr, Some(X86Reg::Rbx), X86Reg::Rcx, 2, 0x20);

    for bytes in [
        &[0xF0, 0x0F, 0xC7, 0x23][..],
        &[0x66, 0x0F, 0xC7, 0x23][..],
        &[0xF2, 0x0F, 0xC7, 0x1B][..],
        &[0xF3, 0x0F, 0xC7, 0x2B][..],
        &[0x0F, 0xC7, 0xD8][..],
        &[0x0F, 0xC7, 0xE0][..],
        &[0x0F, 0xC7, 0xE8][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "{bytes:02X?}"
        );
    }
}
#[test]
fn lift_group9_cmpxchg_random_seed_and_rdpid_encodings() {
    for (bytes, wide, locked) in [
        (&[0x0F, 0xC7, 0x0B][..], false, false),
        (&[0x66, 0x0F, 0xC7, 0x0B][..], false, false),
        (&[0x48, 0x0F, 0xC7, 0x0B][..], true, false),
        (&[0xF0, 0x48, 0x0F, 0xC7, 0x0B][..], true, true),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86Cmpxchg8b16b {
                    wide: got_wide,
                    locked: got_locked,
                    ..
                },
                ..
            }] if *got_wide == wide && *got_locked == locked
        ));
    }

    for (bytes, width, seed, register) in [
        (&[0x0F, 0xC7, 0xF0][..], OpWidth::W32, false, X86Reg::Rax),
        (
            &[0x66, 0x0F, 0xC7, 0xF0][..],
            OpWidth::W16,
            false,
            X86Reg::Rax,
        ),
        (
            &[0x48, 0x0F, 0xC7, 0xF0][..],
            OpWidth::W64,
            false,
            X86Reg::Rax,
        ),
        (
            &[0x41, 0x0F, 0xC7, 0xF0][..],
            OpWidth::W32,
            false,
            X86Reg::R8,
        ),
        (&[0x0F, 0xC7, 0xF8][..], OpWidth::W32, true, X86Reg::Rax),
        (
            &[0x66, 0x0F, 0xC7, 0xF8][..],
            OpWidth::W16,
            true,
            X86Reg::Rax,
        ),
        (
            &[0x48, 0x0F, 0xC7, 0xF8][..],
            OpWidth::W64,
            true,
            X86Reg::Rax,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86Random {
                    dst: VReg::Arch(ArchReg::X86(got_register)),
                    width: got_width,
                    seed: got_seed,
                },
                ..
            }] if *got_width == width && *got_seed == seed && *got_register == register
        ));
    }

    for bytes in [
        &[0xF3, 0x0F, 0xC7, 0xF8][..],
        &[0x66, 0xF3, 0x0F, 0xC7, 0xF8][..],
        &[0xF3, 0x48, 0x0F, 0xC7, 0xF8][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86ReadPid {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Rax))
                },
                ..
            }]
        ));
    }

    for bytes in [
        &[0x0F, 0xC7, 0xC8][..],
        &[0x0F, 0xC7, 0x30][..],
        &[0x0F, 0xC7, 0x38][..],
        &[0xF0, 0x0F, 0xC7, 0xF0][..],
        &[0xF0, 0x0F, 0xC7, 0xF8][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "{bytes:02X?}"
        );
    }
    let senduipi = lift_single(&[0xF3, 0x0F, 0xC7, 0xF0]).unwrap();
    assert_eq!(senduipi.bytes_consumed, 4);
    assert!(senduipi.ops.is_empty());
    assert!(matches!(
        senduipi.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));
}
#[test]
fn lift_pushf_popf_owns_each_fault_precise_stack_transaction() {
    use crate::smir::ir::ops::{X86StackFlagsKind, X86StackFlagsOp};

    for (bytes, kind, width) in [
        (&[0x9C][..], X86StackFlagsKind::Push, OpWidth::W64),
        (&[0x66, 0x9C][..], X86StackFlagsKind::Push, OpWidth::W16),
        (&[0x9D][..], X86StackFlagsKind::Pop, OpWidth::W64),
        (&[0x66, 0x9D][..], X86StackFlagsKind::Pop, OpWidth::W16),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86StackFlags(X86StackFlagsOp {
                    kind: got_kind,
                    width: got_width,
                    requires_apx: false,
                    next_pc,
                }),
                ..
            }] if *got_kind == kind
                && *got_width == width
                && *next_pc == 0x1000 + bytes.len() as u64
        ));
    }
}
#[test]
fn lift_lahf_sahf_use_exact_status_mask() {
    let lahf = lift_single(&[0x9F]).unwrap();
    assert_eq!(lahf.bytes_consumed, 1);
    assert!(matches!(lahf.ops[0].kind, OpKind::ReadFlags { .. }));
    assert!(lahf.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::And {
            src2: SrcOperand::Imm(0xD5),
            ..
        }
    )));
    assert!(matches!(
        lahf.ops.last().unwrap().kind,
        OpKind::Or {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            flags: FlagUpdate::None,
            ..
        }
    ));

    let sahf = lift_single(&[0x9E]).unwrap();
    assert_eq!(sahf.bytes_consumed, 1);
    assert!(matches!(sahf.ops[0].kind, OpKind::ReadFlags { .. }));
    assert!(sahf.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Shr {
            src: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            amount: SrcOperand::Imm(8),
            flags: FlagUpdate::None,
            ..
        }
    )));
    assert!(matches!(
        sahf.ops.last().unwrap().kind,
        OpKind::WriteFlags { .. }
    ));
}
#[test]
fn lift_mov_moffs_address_width_segment_and_direction() {
    let load = lift_single(&[0x48, 0xA1, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11]).unwrap();
    assert_eq!(load.bytes_consumed, 10);
    assert!(matches!(
        load.ops[0].kind,
        OpKind::Load {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            addr: Address::Absolute(0x1122_3344_5566_7788),
            width: MemWidth::B8,
            ..
        }
    ));

    let store = lift_single(&[0x67, 0x64, 0xA2, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert_eq!(store.bytes_consumed, 7);
    assert!(matches!(
        store.ops[0].kind,
        OpKind::Store {
            src: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            addr: Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: None,
                index: None,
                scale: 1,
                disp: 0x1234_5678,
            },
            width: MemWidth::B1,
        }
    ));

    let load_byte = lift_single(&[0x67, 0xA0, 0x00, 0x20, 0x00, 0x00]).unwrap();
    assert!(matches!(
        load_byte.ops[0].kind,
        OpKind::Load {
            addr: Address::Absolute(0x2000),
            width: MemWidth::B1,
            ..
        }
    ));

    let store_dword = lift_single(&[0xA3, 0x00, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]).unwrap();
    assert!(matches!(
        store_dword.ops[0].kind,
        OpKind::Store {
            addr: Address::Absolute(0x3000),
            width: MemWidth::B4,
            ..
        }
    ));

    assert!(matches!(
        lift_single(&[0xA1, 0, 0, 0]),
        Err(LiftError::Incomplete { need: 8, .. })
    ));
}
#[test]
fn lift_pop_rm_orders_rsp_update_before_memory_destination() {
    let result = lift_single(&[0x8F, 0x44, 0x24, 0x08]).unwrap(); // pop qword ptr [rsp+8]
    assert_eq!(result.bytes_consumed, 4);
    assert!(matches!(
        result.ops[0].kind,
        OpKind::Load {
            addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rsp))),
            width: MemWidth::B8,
            ..
        }
    ));
    assert!(matches!(
        result.ops[1].kind,
        OpKind::Add {
            dst: VReg::Virtual(_),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Rsp)),
            src2: SrcOperand::Imm(8),
            flags: FlagUpdate::None,
            ..
        }
    ));
    assert!(matches!(
        result.ops[2].kind,
        OpKind::Store {
            addr: Address::BaseOffset {
                base: VReg::Virtual(_),
                offset: 8,
                ..
            },
            width: MemWidth::B8,
            ..
        }
    ));
    assert!(matches!(
        result.ops[3].kind,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rsp)),
            width: OpWidth::W64,
            ..
        }
    ));

    let addr32 = lift_single(&[0x67, 0x8F, 0x44, 0x24, 0x08]).unwrap();
    let incremented_rsp = match addr32.ops[1].kind {
        OpKind::Add { dst, .. } => dst,
        ref other => panic!("expected stack increment, got {other:?}"),
    };
    let materialized_offset = match addr32.ops[2].kind {
        OpKind::Add {
            dst,
            src1,
            src2: SrcOperand::Imm(8),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        } => {
            assert_eq!(src1, incremented_rsp);
            dst
        }
        ref other => panic!("expected post-increment ESP address, got {other:?}"),
    };
    assert!(matches!(
        addr32.ops[3].kind,
        OpKind::Store {
            addr: Address::Direct(reg),
            width: MemWidth::B8,
            ..
        } if reg == materialized_offset
    ));

    let pop_rsp = lift_single(&[0x8F, 0xC4]).unwrap();
    assert!(matches!(
        pop_rsp.ops.last().unwrap().kind,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rsp)),
            width: OpWidth::W64,
            ..
        }
    ));

    let pop16 = lift_single(&[0x66, 0x8F, 0xC0]).unwrap();
    assert_eq!(pop16.ops.len(), 2);
    assert!(matches!(
        pop16.ops[0].kind,
        OpKind::Load {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            width: MemWidth::B2,
            ..
        }
    ));
    assert!(matches!(
        pop16.ops[1].kind,
        OpKind::Add {
            src2: SrcOperand::Imm(2),
            ..
        }
    ));
    // 8F C8 is not a legacy POP /1 after AMD XOP disambiguation: C8H carries
    // XOP map selector 8. The decoder therefore needs the remaining XOP prefix
    // and opcode bytes before it can classify the instruction.
    assert!(matches!(
        lift_single(&[0x8F, 0xC8]),
        Err(LiftError::Incomplete {
            have: 2,
            need: 4,
            ..
        })
    ));

    let pop_sp = lift_single(&[0x66, 0x8F, 0xC4]).unwrap();
    assert!(matches!(
        pop_sp.ops[2].kind,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rsp)),
            width: OpWidth::W64,
            ..
        }
    ));
    assert!(matches!(
        pop_sp.ops[3].kind,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rsp)),
            width: OpWidth::W16,
            ..
        }
    ));
}
#[test]
fn lift_short_push_pop_preserves_stack_width_and_rsp_alias_ordering() {
    let push_rsp = lift_single(&[0x54]).unwrap();
    let old_rsp = match push_rsp.ops[0].kind {
        OpKind::Mov {
            dst: temporary @ VReg::Virtual(_),
            src: SrcOperand::Reg(VReg::Arch(ArchReg::X86(X86Reg::Rsp))),
            width: OpWidth::W64,
        } => temporary,
        ref other => panic!("expected pre-decrement RSP snapshot, got {other:?}"),
    };
    assert!(matches!(
        push_rsp.ops[1].kind,
        OpKind::Sub {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rsp)),
            src2: SrcOperand::Imm(8),
            ..
        }
    ));
    assert!(matches!(
        push_rsp.ops[2].kind,
        OpKind::Store {
            src,
            addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rsp))),
            width: MemWidth::B8,
        } if src == old_rsp
    ));

    let push_ax = lift_single(&[0x66, 0x50]).unwrap();
    assert!(matches!(
        push_ax.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::Sub {
                    src2: SrcOperand::Imm(2),
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::Store {
                    src: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                    width: MemWidth::B2,
                    ..
                },
                ..
            }
        ]
    ));

    let push_imm16 = lift_single(&[0x66, 0x68, 0x34, 0xF2]).unwrap();
    assert_eq!(push_imm16.bytes_consumed, 4);
    assert!(matches!(
        push_imm16.ops[0].kind,
        OpKind::Sub {
            src2: SrcOperand::Imm(2),
            ..
        }
    ));
    assert!(matches!(
        push_imm16.ops[1],
        SmirOp {
            kind: OpKind::Store {
                src: VReg::Imm(-3532),
                width: MemWidth::B2,
                ..
            },
            x86_hint: Some(X86OpHint::PushImm16),
            ..
        }
    ));

    let pop_ax = lift_single(&[0x66, 0x58]).unwrap();
    assert!(matches!(
        pop_ax.ops[0].kind,
        OpKind::Load {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            width: MemWidth::B2,
            ..
        }
    ));
    assert!(matches!(
        pop_ax.ops[1].kind,
        OpKind::Add {
            src2: SrcOperand::Imm(2),
            ..
        }
    ));

    let pop_rsp = lift_single(&[0x5C]).unwrap();
    let popped = match pop_rsp.ops[0].kind {
        OpKind::Load {
            dst: temporary @ VReg::Virtual(_),
            addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rsp))),
            width: MemWidth::B8,
            ..
        } => temporary,
        ref other => panic!("expected POP RSP temporary load, got {other:?}"),
    };
    assert!(matches!(
        pop_rsp.ops[1].kind,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rsp)),
            src: SrcOperand::Reg(source),
            width: OpWidth::W64,
        } if source == popped
    ));
}
#[test]
fn lift_group4_register_memory_lock_and_invalid_forms() {
    let inc = lift_single(&[0xFE, 0xC0]).unwrap(); // inc al
    assert!(matches!(
        inc.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Inc {
                width: OpWidth::W8,
                ..
            },
            ..
        }]
    ));

    let dec_mem = lift_single(&[0xFE, 0x08]).unwrap(); // dec byte ptr [rax]
    assert!(matches!(
        dec_mem.ops[0].kind,
        OpKind::Load {
            width: MemWidth::B1,
            ..
        }
    ));
    assert!(matches!(
        dec_mem.ops[1].kind,
        OpKind::Dec {
            width: OpWidth::W8,
            flags: FlagUpdate::None,
            ..
        }
    ));
    assert!(matches!(
        dec_mem.ops[2].kind,
        OpKind::Store {
            width: MemWidth::B1,
            ..
        }
    ));
    assert!(matches!(
        dec_mem.ops[3].kind,
        OpKind::Dec {
            width: OpWidth::W8,
            flags: FlagUpdate::All,
            ..
        }
    ));

    let lock_inc = lift_single(&[0xF0, 0xFE, 0x00]).unwrap();
    assert!(lock_inc.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::AtomicRmw {
            op: AtomicOp::Add,
            width: MemWidth::B1,
            order: MemoryOrder::SeqCst,
            ..
        }
    )));

    assert!(matches!(
        lift_single(&[0xFE, 0xD0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
    let inc_ah = lift_single(&[0xFE, 0xC4]).unwrap();
    assert!(matches!(
        inc_ah.ops.first().unwrap().kind,
        OpKind::Shr {
            src: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            amount: SrcOperand::Imm(8),
            flags: FlagUpdate::None,
            ..
        }
    ));
    assert!(inc_ah.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Inc {
            width: OpWidth::W8,
            flags: FlagUpdate::All,
            ..
        }
    )));
    assert!(matches!(
        inc_ah.ops.last().unwrap().kind,
        OpKind::Or {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            flags: FlagUpdate::None,
            ..
        }
    ));
    assert!(matches!(
        lift_single(&[0xF0, 0xFE, 0xC0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}
#[test]
fn lock_memory_alu_group3_and_group5_use_atomic_rmw() {
    for (name, bytes, expected_atomic, expects_flags) in [
        ("add rm,reg", &[0xF0, 0x01, 0x08][..], AtomicOp::Add, true),
        (
            "adc rm,imm",
            &[0xF0, 0x83, 0x10, 0x01][..],
            AtomicOp::Add,
            true,
        ),
        (
            "sbb rm,imm",
            &[0xF0, 0x83, 0x18, 0x01][..],
            AtomicOp::Sub,
            true,
        ),
        (
            "and rm,imm",
            &[0xF0, 0x83, 0x20, 0x01][..],
            AtomicOp::And,
            true,
        ),
        (
            "xor rm,imm",
            &[0xF0, 0x83, 0x30, 0x01][..],
            AtomicOp::Xor,
            true,
        ),
        ("inc rm", &[0xF0, 0x48, 0xFF, 0x00][..], AtomicOp::Add, true),
        ("dec rm", &[0xF0, 0x48, 0xFF, 0x08][..], AtomicOp::Sub, true),
        ("not rm", &[0xF0, 0xF7, 0x10][..], AtomicOp::Nand, false),
        ("neg rm", &[0xF0, 0xF7, 0x18][..], AtomicOp::Neg, true),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|err| panic!("{name}: {err:?}"));
        let atomic = result
            .ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::AtomicRmw {
                        op,
                        order: MemoryOrder::SeqCst,
                        ..
                    } if op == expected_atomic
                )
            })
            .unwrap_or_else(|| panic!("{name}: missing {expected_atomic:?}: {:?}", result.ops));
        assert!(
            result.ops[..atomic]
                .iter()
                .all(|op| op.kind.flags_written().is_empty()),
            "{name}: pre-atomic flags",
        );
        assert_eq!(
            result.ops[atomic + 1..]
                .iter()
                .any(|op| !op.kind.flags_written().is_empty()),
            expects_flags,
            "{name}: post-atomic flag commit",
        );
    }

    let adc = lift_single(&[0xF0, 0x83, 0x10, 0x01]).unwrap();
    assert!(
        adc.ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::ReadFlags { .. }))
    );
    assert!(matches!(
        adc.ops.last().unwrap().kind,
        OpKind::Adc {
            flags: FlagUpdate::All,
            ..
        }
    ));

    for bytes in [
        &[0xF0, 0x01, 0xC8][..],       // ADD register destination
        &[0xF0, 0x03, 0x08][..],       // ADD reg,[mem]
        &[0xF0, 0x39, 0x08][..],       // CMP [mem],reg
        &[0xF0, 0x05, 0, 0, 0, 0][..], // ADD accumulator,imm
        &[0xF0, 0x83, 0xC0, 1][..],    // ADD register,imm
        &[0xF0, 0x83, 0x38, 1][..],    // CMP [mem],imm
        &[0xF0, 0xC1, 0x20, 1][..],    // shift immediate
        &[0xF0, 0xD1, 0x20][..],       // shift by one
        &[0xF0, 0xD3, 0x20][..],       // shift by CL
        &[0xF0, 0xF7, 0xD8][..],       // NEG register
        &[0xF0, 0xF7, 0x20][..],       // MUL [mem]
        &[0xF0, 0xFF, 0x10][..],       // CALL [mem]
        &[0xF0, 0xFF, 0x30][..],       // PUSH [mem]
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "illegal LOCK form accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn memory_rmw_flag_updates_follow_the_potentially_faulting_store() {
    for (name, bytes) in [
        ("add", &[0x01, 0x08][..]),
        ("adc immediate", &[0x83, 0x10, 0x01][..]),
        ("shift immediate", &[0xC1, 0x20, 0x01][..]),
        ("rotate one", &[0xD0, 0x08][..]),
        ("rotate through carry CL", &[0x48, 0xD3, 0x18][..]),
        ("neg", &[0xF7, 0x18][..]),
        ("inc", &[0x48, 0xFF, 0x00][..]),
        ("dec", &[0x66, 0xFF, 0x08][..]),
    ] {
        let result = lift_single(bytes).unwrap();
        let store = result
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::Store { .. }))
            .unwrap_or_else(|| panic!("{name}: missing store"));
        assert!(
            result.ops[..store]
                .iter()
                .all(|op| op.kind.flags_written().is_empty()),
            "{name}: flags written before store: {:?}",
            result.ops,
        );
        assert!(
            result.ops[store + 1..]
                .iter()
                .any(|op| !op.kind.flags_written().is_empty()),
            "{name}: missing post-store flag commit: {:?}",
            result.ops,
        );
    }
}
#[test]
fn lift_xlat_models_index_width_segment_and_invalid_lock() {
    let plain = lift_single(&[0xD7]).unwrap();
    assert_eq!(plain.bytes_consumed, 1);
    assert!(matches!(
        plain.ops[0].kind,
        OpKind::And {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            src2: SrcOperand::Imm(0xFF),
            flags: FlagUpdate::None,
            ..
        }
    ));
    assert!(matches!(
        plain.ops[1].kind,
        OpKind::Add {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
            ..
        }
    ));
    assert!(matches!(
        plain.ops[2].kind,
        OpKind::Load {
            width: MemWidth::B1,
            ..
        }
    ));
    assert!(matches!(
        plain.ops[3].kind,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            width: OpWidth::W8,
            ..
        }
    ));

    let addr32_fs = lift_single(&[0x67, 0x64, 0xD7]).unwrap();
    assert!(matches!(
        addr32_fs.ops[1].kind,
        OpKind::Add {
            width: OpWidth::W32,
            ..
        }
    ));
    assert!(matches!(
        addr32_fs.ops[2].kind,
        OpKind::Load {
            addr: Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        lift_single(&[0xF0, 0xD7]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}
#[test]
fn lift_nop_cache_and_prefetch_hints_consume_complete_encodings() {
    for bytes in [
        &[0x0F, 0x08][..],
        &[0x0F, 0x09][..],
        &[0x0F, 0x1A, 0xC0][..],
        &[0x0F, 0x1B, 0xC0][..],
        &[0xF3, 0x0F, 0x1E, 0xFA][..], // ENDBR64
        &[0x0F, 0x1F, 0x84, 0x88, 0x78, 0x56, 0x34, 0x12][..],
        &[0x0F, 0x18, 0x4C, 0x88, 0x20][..],
        &[0x0F, 0x0D, 0x4C, 0x88, 0x20][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(result.ops.is_empty(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    }

    let cldemote = lift_single(&[0x0F, 0x1C, 0x40, 0x20]).unwrap();
    assert_eq!(cldemote.bytes_consumed, 4);
    assert!(matches!(
        cldemote.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86CacheControl {
                addr: Address::BaseOffset {
                    base: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                    offset: 0x20,
                    ..
                },
                kind: X86CacheControlKind::Cldemote,
            },
            ..
        }]
    ));
    assert!(cldemote.ops[0].kind.has_side_effects());
    assert!(!cldemote.ops[0].kind.reads_memory());

    let cldemote_addr32 = lift_single(&[0x67, 0x0F, 0x1C, 0x43, 0x20]).unwrap();
    assert_eq!(cldemote_addr32.bytes_consumed, 5);
    assert!(matches!(
        cldemote_addr32.ops.last().map(|op| &op.kind),
        Some(OpKind::X86CacheControl {
            kind: X86CacheControlKind::Cldemote,
            ..
        })
    ));
    assert!(
        cldemote_addr32.ops[..cldemote_addr32.ops.len() - 1]
            .iter()
            .all(|op| matches!(
                op.kind,
                OpKind::Mov {
                    width: OpWidth::W32,
                    ..
                } | OpKind::Add {
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                    ..
                }
            ))
    );

    for bytes in [
        &[0x0F, 0x1C, 0xC0][..],
        &[0x0F, 0x1C, 0x08][..],
        &[0x66, 0x0F, 0x1C, 0xD8][..],
        &[0xF2, 0x0F, 0x1C, 0xF8][..],
        &[0xF3, 0x0F, 0x1C, 0x08][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(result.ops.is_empty(), "reserved hint {bytes:02X?}");
    }
    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x1C, 0x00]),
        Err(LiftError::InvalidEncoding { .. })
    ));

    for bytes in [
        &[0xF3, 0x0F, 0x1E, 0xC8][..],
        &[0xF3, 0x48, 0x0F, 0x1E, 0xC9][..],
        &[0xF3, 0x49, 0x0F, 0x1E, 0xC8][..],
    ] {
        let rdssp = lift_single(bytes).unwrap();
        assert_eq!(rdssp.bytes_consumed, bytes.len());
        assert!(rdssp.ops.is_empty());
        assert!(matches!(rdssp.control_flow, ControlFlow::Fallthrough));
    }
}
#[test]
fn lift_rdtsc_has_exact_destinations_and_length() {
    let result = lift_single(&[0x0F, 0x31]).unwrap();
    assert_eq!(result.bytes_consumed, 2);
    assert!(matches!(
        result.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86ReadTsc(X86ReadTscOp {
                dst_lo: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                dst_hi: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
                dst_aux: None,
            }),
            ..
        }]
    ));
    assert!(result.ops[0].kind.has_side_effects());
}
#[test]
fn test_lift_jmp() {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // JMP rel8
    let result = lifter.lift_insn(0x1000, &[0xEB, 0x10], &mut ctx).unwrap();
    assert_eq!(result.bytes_consumed, 2);
    assert!(matches!(
        result.control_flow,
        ControlFlow::Branch { target: 0x1012 }
    ));

    // JMP rel32
    let result = lifter
        .lift_insn(0x1000, &[0xE9, 0x00, 0x10, 0x00, 0x00], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 5);
    assert!(matches!(
        result.control_flow,
        ControlFlow::Branch { target: 0x2005 }
    ));
}
#[test]
fn test_lift_jcc() {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // JE rel8
    let result = lifter.lift_insn(0x1000, &[0x74, 0x10], &mut ctx).unwrap();
    assert_eq!(result.bytes_consumed, 2);
    match result.control_flow {
        ControlFlow::CondBranch {
            cond,
            target,
            fallthrough,
        } => {
            assert_eq!(cond, Condition::Eq);
            assert_eq!(target, 0x1012);
            assert_eq!(fallthrough, 0x1002);
        }
        _ => panic!("Expected CondBranch"),
    }
}
#[test]
fn lift_loop_family_counter_width_targets_and_flag_restoration() {
    for opcode in 0xE0..=0xE3 {
        let result = lift_single(&[opcode, 0xFE]).unwrap();
        assert_eq!(result.bytes_consumed, 2);
        assert!(matches!(
            result.control_flow,
            ControlFlow::CondBranchReg {
                taken: 0x1000,
                not_taken: 0x1002,
                ..
            }
        ));
        assert!(matches!(result.ops[0].kind, OpKind::ReadFlags { .. }));
        assert!(
            result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::WriteFlags { .. }))
        );
        assert_eq!(
            result
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::Dec { .. }))
                .count(),
            usize::from(opcode != 0xE3),
        );
    }

    let loop32 = lift_single(&[0x67, 0xE2, 0x00]).unwrap();
    assert!(loop32.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Dec {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
            ..
        }
    )));

    assert!(matches!(
        lift_single(&[0xE2]),
        Err(LiftError::Incomplete { need: 1, .. })
    ));
    assert!(matches!(
        lift_single(&[0xF0, 0xE2, 0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}
#[test]
fn lift_crc32c_covers_widths_high_bytes_rex_aliases_memory_and_invalids() {
    for (bytes, width, dst, data) in [
        (
            &[0xF2, 0x45, 0x0F, 0x38, 0xF0, 0xC1][..],
            OpWidth::W8,
            X86Reg::R8,
            X86Reg::R9,
        ),
        (
            &[0xF2, 0x4D, 0x0F, 0x38, 0xF0, 0xC1][..],
            OpWidth::W8,
            X86Reg::R8,
            X86Reg::R9,
        ),
        (
            &[0x66, 0xF2, 0x45, 0x0F, 0x38, 0xF1, 0xC1][..],
            OpWidth::W16,
            X86Reg::R8,
            X86Reg::R9,
        ),
        (
            &[0xF2, 0x45, 0x0F, 0x38, 0xF1, 0xC1][..],
            OpWidth::W32,
            X86Reg::R8,
            X86Reg::R9,
        ),
        (
            &[0xF2, 0x4D, 0x0F, 0x38, 0xF1, 0xC1][..],
            OpWidth::W64,
            X86Reg::R8,
            X86Reg::R9,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Crc32C {
                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                crc: VReg::Arch(ArchReg::X86(actual_crc)),
                data: VReg::Arch(ArchReg::X86(actual_data)),
                data_width,
            } if actual_dst == dst
                && actual_crc == dst
                && actual_data == data
                && data_width == width
        )));
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );
    }

    let high_byte = lift_single(&[0xF2, 0x0F, 0x38, 0xF0, 0xD5]).unwrap();
    let extraction = high_byte
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Shr {
                    src: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
                    amount: SrcOperand::Imm(8),
                    flags: FlagUpdate::None,
                    ..
                }
            )
        })
        .expect("legacy CH source must be extracted from RCX[15:8]");
    let crc = high_byte
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Crc32C {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
                    data_width: OpWidth::W8,
                    ..
                }
            )
        })
        .unwrap();
    assert!(extraction < crc);

    // Any REX prefix changes byte code 5 from CH to BPL.
    let rex_byte = lift_single(&[0xF2, 0x40, 0x0F, 0x38, 0xF0, 0xD5]).unwrap();
    assert!(!rex_byte.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Shr {
            amount: SrcOperand::Imm(8),
            ..
        }
    )));
    assert!(rex_byte.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Crc32C {
            data: VReg::Arch(ArchReg::X86(X86Reg::Rbp)),
            ..
        }
    )));

    let alias = lift_single(&[0xF2, 0x45, 0x0F, 0x38, 0xF0, 0xC0]).unwrap();
    assert!(alias.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Crc32C {
            dst: VReg::Arch(ArchReg::X86(X86Reg::R8)),
            crc: VReg::Arch(ArchReg::X86(X86Reg::R8)),
            data: VReg::Arch(ArchReg::X86(X86Reg::R8)),
            ..
        }
    )));

    let memory = lift_single(&[0xF2, 0x4C, 0x0F, 0x38, 0xF1, 0x40, 0x09]).unwrap();
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            width: MemWidth::B8,
            ..
        }
    )));
    assert!(
        !memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    // 66 is ignored for F0 byte forms, and REX.W dominates 66 for F1.
    assert!(lift_single(&[0x66, 0xF2, 0x0F, 0x38, 0xF0, 0xC1]).is_ok());
    assert!(lift_single(&[0x66, 0xF2, 0x48, 0x0F, 0x38, 0xF1, 0xC1]).is_ok());
    for bytes in [
        &[0xF0, 0xF2, 0x0F, 0x38, 0xF0, 0xC1][..],
        &[0xF3, 0x0F, 0x38, 0xF0, 0xC1][..],
        &[0xF2, 0x0F, 0x38, 0xF0][..],
        &[0xC4, 0xE2, 0x7B, 0xF0, 0xC1][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid CRC32 encoding accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_dot_products_covers_masks_widths_alignment_wig_and_invalids() {
    for (bytes, elem, width, dst, src1, src2, imm) in [
        (
            &[0x66, 0x45, 0x0F, 0x3A, 0x40, 0xCA, 0x5A][..],
            VecElementType::F32,
            VecWidth::V128,
            X86Reg::Xmm(9),
            X86Reg::Xmm(9),
            X86Reg::Xmm(10),
            0x5A,
        ),
        (
            &[0x66, 0x45, 0x0F, 0x3A, 0x41, 0xCA, 0x33][..],
            VecElementType::F64,
            VecWidth::V128,
            X86Reg::Xmm(9),
            X86Reg::Xmm(9),
            X86Reg::Xmm(10),
            0x33,
        ),
        (
            &[0xC4, 0x43, 0x21, 0x40, 0xCA, 0x5A][..],
            VecElementType::F32,
            VecWidth::V128,
            X86Reg::Xmm(9),
            X86Reg::Xmm(11),
            X86Reg::Xmm(10),
            0x5A,
        ),
        (
            &[0xC4, 0x43, 0x25, 0x40, 0xCA, 0xA5][..],
            VecElementType::F32,
            VecWidth::V256,
            X86Reg::Ymm(9),
            X86Reg::Ymm(11),
            X86Reg::Ymm(10),
            0xA5,
        ),
        (
            &[0xC4, 0x43, 0x21, 0x41, 0xCA, 0x31][..],
            VecElementType::F64,
            VecWidth::V128,
            X86Reg::Xmm(9),
            X86Reg::Xmm(11),
            X86Reg::Xmm(10),
            0x31,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        let legacy = bytes[0] == 0x66;
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86DotProduct {
                dst: actual_dst,
                src1: VReg::Arch(ArchReg::X86(actual_src1)),
                src2: VReg::Arch(ArchReg::X86(actual_src2)),
                elem: actual_elem,
                width: actual_width,
                imm: actual_imm,
            } if (legacy || actual_dst == VReg::Arch(ArchReg::X86(dst)))
                && actual_src1 == src1
                && actual_src2 == src2
                && actual_elem == elem
                && actual_width == width
                && actual_imm == imm
        )));
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );
    }

    let legacy_mem = lift_single(&[0x66, 0x44, 0x0F, 0x3A, 0x40, 0x48, 0x10, 0xF1]).unwrap();
    let alignment = legacy_mem
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .unwrap();
    let load = legacy_mem
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .unwrap();
    assert!(alignment < load);

    let vex_mem = lift_single(&[0xC4, 0x63, 0x25, 0x40, 0x48, 0x11, 0xFF]).unwrap();
    assert!(vex_mem.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V256,
            ..
        }
    )));
    assert!(
        !vex_mem
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    // W is ignored for every legacy/VEX form.
    assert!(lift_single(&[0x66, 0x4D, 0x0F, 0x3A, 0x40, 0xCA, 0x5A]).is_ok());
    assert!(lift_single(&[0xC4, 0x43, 0xA1, 0x40, 0xCA, 0x5A]).is_ok());
    assert!(lift_single(&[0xC4, 0x43, 0xA1, 0x41, 0xCA, 0x31]).is_ok());

    for bytes in [
        &[0x0F, 0x3A, 0x40, 0xCA, 0x5A][..],
        &[0xF0, 0x66, 0x0F, 0x3A, 0x40, 0xCA, 0x5A][..],
        &[0xF3, 0x66, 0x0F, 0x3A, 0x41, 0xCA, 0x33][..],
        &[0x66, 0x0F, 0x3A, 0x40, 0xCA][..],
        &[0xC4, 0x43, 0x20, 0x40, 0xCA, 0x5A][..],
        &[0xC4, 0x43, 0x25, 0x41, 0xCA, 0x33][..],
        &[0x62, 0xF3, 0x65, 0x08, 0x40, 0xCA, 0x5A][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid dot-product encoding accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn test_lift_push_pop() {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // PUSH RAX
    let result = lifter.lift_insn(0x1000, &[0x50], &mut ctx).unwrap();
    assert_eq!(result.bytes_consumed, 1);
    assert_eq!(result.ops.len(), 2); // SUB RSP + STORE

    // POP RAX
    let result = lifter.lift_insn(0x1000, &[0x58], &mut ctx).unwrap();
    assert_eq!(result.bytes_consumed, 1);
    assert_eq!(result.ops.len(), 2); // LOAD + ADD RSP
}
#[test]
fn test_lift_block() {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // Simple block: MOV EAX, 1; RET
    let mem = TestMemory::new(0x1000, vec![0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3]);
    let block = lifter.lift_block(0x1000, &mem, &mut ctx).unwrap();

    assert_eq!(block.guest_pc, 0x1000);
    assert!(matches!(block.terminator, Terminator::Return { .. }));
}
