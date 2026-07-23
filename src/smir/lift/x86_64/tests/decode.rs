//! tests::decode tests

use super::*;
use crate::smir::lift::x86_64::*;

#[test]
fn test_prefix_decode() {
    // No prefix
    let prefix = decode_prefixes(&[0x90]).unwrap();
    assert_eq!(prefix.cursor, 0);
    assert!(!prefix.has_rex());

    // REX.W prefix
    let prefix = decode_prefixes(&[0x48, 0xB8]).unwrap();
    assert_eq!(prefix.cursor, 1);
    assert!(prefix.rex_w());
    assert_eq!(prefix.op_size(), 8);

    // Operand size override
    let prefix = decode_prefixes(&[0x66, 0xB8]).unwrap();
    assert_eq!(prefix.cursor, 1);
    assert!(prefix.operand_size_override);
    assert_eq!(prefix.op_size(), 2);

    // REX2.W with B high bit: LLVM encodes `mov r16, imm64` as d5 18 b8...
    let prefix = decode_prefixes(&[0xD5, 0x18, 0xB8]).unwrap();
    assert_eq!(prefix.cursor, 2);
    assert!(prefix.has_rex());
    assert!(prefix.rex_w());
    assert_eq!(prefix.rex_b(), 16);
    assert!(!prefix.rex2_m());

    // REX2.M compressed 0F map: LLVM encodes `imul r16, rax` as d5 c8 af c0.
    let prefix = decode_prefixes(&[0xD5, 0xC8, 0xAF]).unwrap();
    assert_eq!(prefix.cursor, 2);
    assert!(prefix.rex2_m());
    assert!(prefix.rex_w());
    assert_eq!(prefix.rex_r(), 16);
}

#[test]
fn legacy_prefix_after_rex_invalidates_rex_state() {
    for legacy in [
        0x66, 0x67, 0xF0, 0xF2, 0xF3, 0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65,
    ] {
        let rex_then_legacy = decode_prefixes(&[0x48, legacy, 0x90]).unwrap();
        assert_eq!(rex_then_legacy.cursor, 2, "legacy prefix {legacy:02X}");
        assert!(
            !rex_then_legacy.has_rex(),
            "legacy prefix {legacy:02X} after REX.W must invalidate REX"
        );

        let legacy_then_rex = decode_prefixes(&[legacy, 0x48, 0x90]).unwrap();
        assert_eq!(legacy_then_rex.cursor, 2, "legacy prefix {legacy:02X}");
        assert!(
            legacy_then_rex.rex_w(),
            "REX.W after legacy prefix {legacy:02X} must remain effective"
        );
    }
}

#[test]
fn rex_immediately_before_rex2_is_invalid_but_an_intervening_legacy_prefix_clears_rex() {
    for bytes in [
        &[0x48, 0xD5, 0x00, 0x89, 0xC0][..],
        &[0x4F, 0xD5, 0x80, 0xAF, 0xC0],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { addr: 0x1000, .. })
        ));
    }

    let result = lift_single(&[0x48, 0x66, 0xD5, 0x00, 0x89, 0xC0])
        .expect("legacy prefix after REX must invalidate REX before REX2");
    assert_eq!(result.bytes_consumed, 6);
}

#[test]
fn legacy_prefix_order_controls_effective_rex_width() {
    let rex_then_66 = lift_single(&[0x48, 0x66, 0xB8, 0x34, 0x12]).unwrap();
    assert_eq!(rex_then_66.bytes_consumed, 5);
    assert!(matches!(
        rex_then_66.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src: SrcOperand::Imm(0x1234),
                width: OpWidth::W16,
            },
            ..
        }]
    ));

    let prefix_then_rex =
        lift_single(&[0x66, 0x48, 0xB8, 0x78, 0x56, 0x34, 0x12, 0, 0, 0, 0]).unwrap();
    assert_eq!(prefix_then_rex.bytes_consumed, 11);
    assert!(matches!(
        prefix_then_rex.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src: SrcOperand::Imm64(0x1234_5678),
                width: OpWidth::W64,
            },
            ..
        }]
    ));
}
#[test]
fn lift_0f38_movbe_load_widths_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    let cases: &[(&[u8], &str, usize, MemWidth, OpWidth, VReg, VReg)] = &[
        (
            &[0x66, 0x0F, 0x38, 0xF0, 0x00],
            "movbe_load16",
            5,
            MemWidth::B2,
            OpWidth::W16,
            x86_gpr(0),
            x86_gpr(0),
        ),
        (
            &[0x0F, 0x38, 0xF0, 0x00],
            "movbe_load32",
            4,
            MemWidth::B4,
            OpWidth::W32,
            x86_gpr(0),
            x86_gpr(0),
        ),
        (
            &[0x48, 0x0F, 0x38, 0xF0, 0x07],
            "movbe_load64",
            5,
            MemWidth::B8,
            OpWidth::W64,
            x86_gpr(0),
            x86_gpr(7),
        ),
    ];

    for (bytes, name, bytes_consumed, mem_width, op_width, dst_reg, base_reg) in cases {
        // LLVM 23 examples:
        //   `movbe ax, word ptr [rax]`    => 66 0f 38 f0 00
        //   `movbe eax, dword ptr [rax]` => 0f 38 f0 00
        //   `movbe rax, qword ptr [rdi]` => 48 0f 38 f0 07
        let result = lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, *bytes_consumed, "{name}");
        assert_eq!(result.ops.len(), 2, "{name}");

        let loaded = match &result.ops[0].kind {
            OpKind::Load {
                dst,
                addr: Address::Direct(base),
                width,
                sign: SignExtend::Zero,
            } => {
                assert_eq!(*base, *base_reg, "{name}");
                assert_eq!(*width, *mem_width, "{name}");
                *dst
            }
            other => panic!("expected {name} memory load, got {other:?}"),
        };
        match &result.ops[1].kind {
            OpKind::Bswap { dst, src, width } => {
                assert_eq!(*dst, *dst_reg, "{name}");
                assert_eq!(*src, loaded, "{name}");
                assert_eq!(*width, *op_width, "{name}");
            }
            other => panic!("expected {name} loaded Bswap, got {other:?}"),
        }
    }
}
#[test]
fn lift_0f38_movbe_store_widths_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    let cases: &[(&[u8], &str, usize, MemWidth, OpWidth, VReg, VReg)] = &[
        (
            &[0x66, 0x0F, 0x38, 0xF1, 0x08],
            "movbe_store16",
            5,
            MemWidth::B2,
            OpWidth::W16,
            x86_gpr(1),
            x86_gpr(0),
        ),
        (
            &[0x0F, 0x38, 0xF1, 0x08],
            "movbe_store32",
            4,
            MemWidth::B4,
            OpWidth::W32,
            x86_gpr(1),
            x86_gpr(0),
        ),
        (
            &[0x48, 0x0F, 0x38, 0xF1, 0x17],
            "movbe_store64",
            5,
            MemWidth::B8,
            OpWidth::W64,
            x86_gpr(2),
            x86_gpr(7),
        ),
    ];

    for (bytes, name, bytes_consumed, mem_width, op_width, src_reg, base_reg) in cases {
        // LLVM 23 examples:
        //   `movbe word ptr [rax], cx`    => 66 0f 38 f1 08
        //   `movbe dword ptr [rax], ecx` => 0f 38 f1 08
        //   `movbe qword ptr [rdi], rdx` => 48 0f 38 f1 17
        let result = lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, *bytes_consumed, "{name}");
        assert_eq!(result.ops.len(), 2, "{name}");

        let swapped = match &result.ops[0].kind {
            OpKind::Bswap { dst, src, width } => {
                assert!(matches!(dst, VReg::Virtual(_)), "{name}");
                assert_eq!(*src, *src_reg, "{name}");
                assert_eq!(*width, *op_width, "{name}");
                *dst
            }
            other => panic!("expected {name} store Bswap, got {other:?}"),
        };
        match &result.ops[1].kind {
            OpKind::Store { src, addr, width } => {
                assert_eq!(*src, swapped, "{name}");
                assert_eq!(*width, *mem_width, "{name}");
                match addr {
                    Address::Direct(base) => assert_eq!(*base, *base_reg, "{name}"),
                    other => panic!("expected {name} direct address, got {other:?}"),
                }
            }
            other => panic!("expected {name} memory store, got {other:?}"),
        }
    }
}
#[test]
fn lift_0f38_movbe_rex_extended_memory_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `movbe r10, qword ptr [r8 + 4*r9 + 32]`
    //          => 4f 0f 38 f0 54 88 20.
    let load = lifter
        .lift_insn(
            0x1000,
            &[0x4F, 0x0F, 0x38, 0xF0, 0x54, 0x88, 0x20],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(load.bytes_consumed, 7);
    assert_eq!(load.ops.len(), 2);
    let loaded = match &load.ops[0].kind {
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } => {
            assert_0f38_movbe_rex_sib_addr(addr, "movbe_load_rex");
            *dst
        }
        other => panic!("expected MOVBE REX memory load, got {other:?}"),
    };
    match &load.ops[1].kind {
        OpKind::Bswap {
            dst,
            src,
            width: OpWidth::W64,
        } => {
            assert_eq!(*dst, x86_gpr(10));
            assert_eq!(*src, loaded);
        }
        other => panic!("expected MOVBE REX loaded Bswap, got {other:?}"),
    }

    // LLVM 23: `movbe qword ptr [r8 + 4*r9 + 32], r10`
    //          => 4f 0f 38 f1 54 88 20.
    let store = lifter
        .lift_insn(
            0x2000,
            &[0x4F, 0x0F, 0x38, 0xF1, 0x54, 0x88, 0x20],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(store.bytes_consumed, 7);
    assert_eq!(store.ops.len(), 2);
    let swapped = match &store.ops[0].kind {
        OpKind::Bswap {
            dst,
            src,
            width: OpWidth::W64,
        } => {
            assert!(matches!(dst, VReg::Virtual(_)));
            assert_eq!(*src, x86_gpr(10));
            *dst
        }
        other => panic!("expected MOVBE REX store Bswap, got {other:?}"),
    };
    match &store.ops[1].kind {
        OpKind::Store {
            src,
            addr,
            width: MemWidth::B8,
        } => {
            assert_eq!(*src, swapped);
            assert_0f38_movbe_rex_sib_addr(addr, "movbe_store_rex");
        }
        other => panic!("expected MOVBE REX memory store, got {other:?}"),
    }
}
#[test]
fn lift_0f38_movbe_rejects_invalid_forms_and_routes_f2_to_crc32() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (bytes, name) in [
        (&[0x0F, 0x38, 0xF0, 0xC0][..], "load register operand"),
        (&[0x0F, 0x38, 0xF1, 0xC0][..], "store register operand"),
        (&[0xF0, 0x0F, 0x38, 0xF0, 0x00][..], "lock prefix"),
        (&[0xF3, 0x0F, 0x38, 0xF0, 0x00][..], "rep prefix"),
    ] {
        let err = lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap_err();
        assert!(
            matches!(err, LiftError::InvalidEncoding { .. }),
            "{name}: {err:?}"
        );
    }

    let reserved_map1_row = lifter
        .lift_insn(
            0x1000,
            &[0xD5, 0xF8, 0x38, 0xF0, 0x54, 0x88, 0x20],
            &mut ctx,
        )
        .expect("REX2 compressed map 1 row 3 is an explicit #UD");
    assert_invalid_opcode_trap(&reserved_map1_row, 3);

    let crc = lifter
        .lift_insn(0x1000, &[0xF2, 0x0F, 0x38, 0xF0, 0xC3], &mut ctx)
        .unwrap();
    assert!(crc.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Crc32C {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            crc: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            data: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
            data_width: OpWidth::W8,
        }
    )));
}
#[test]
fn test_modrm_decode() {
    // MOD=3 (register)
    let prefix = X86Prefix::default();
    let modrm = decode_modrm(&[0xC0], &prefix, 0).unwrap();
    assert!(!modrm.is_memory);
    assert_eq!(modrm.reg, 0);
    assert_eq!(modrm.rm, 0);

    // MOD=0, RM=5 (RIP-relative)
    let modrm = decode_modrm(&[0x05, 0x10, 0x00, 0x00, 0x00], &prefix, 0).unwrap();
    assert!(modrm.is_memory);
    assert!(modrm.addr.as_ref().unwrap().rip_relative);
    assert_eq!(modrm.bytes_consumed, 5);
}
#[test]
fn lift_complete_x86_string_opcode_and_width_inventory() {
    for (opcode, expected_kind, expected_width) in [
        (0xA4, X86StringKind::Movs, MemWidth::B1),
        (0xA5, X86StringKind::Movs, MemWidth::B4),
        (0xA6, X86StringKind::Cmps, MemWidth::B1),
        (0xA7, X86StringKind::Cmps, MemWidth::B4),
        (0xAA, X86StringKind::Stos, MemWidth::B1),
        (0xAB, X86StringKind::Stos, MemWidth::B4),
        (0xAC, X86StringKind::Lods, MemWidth::B1),
        (0xAD, X86StringKind::Lods, MemWidth::B4),
        (0xAE, X86StringKind::Scas, MemWidth::B1),
        (0xAF, X86StringKind::Scas, MemWidth::B4),
    ] {
        let result = lift_single(&[opcode]).unwrap();
        assert_eq!(result.bytes_consumed, 1);
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86String {
                    kind,
                    rep: X86RepMode::None,
                    width,
                    address_width: OpWidth::W64,
                    ..
                },
                ..
            }] if *kind == expected_kind && *width == expected_width
        ));
    }

    let word = lift_single(&[0x66, 0xAD]).unwrap();
    assert!(matches!(
        word.ops[0].kind,
        OpKind::X86String {
            width: MemWidth::B2,
            ..
        }
    ));
    let qword = lift_single(&[0x48, 0xAD]).unwrap();
    assert!(matches!(
        qword.ops[0].kind,
        OpKind::X86String {
            width: MemWidth::B8,
            ..
        }
    ));
}
#[test]
fn lift_x86_string_rep_address_and_segment_prefixes() {
    let repe = lift_single(&[0xF3, 0xA6]).unwrap();
    assert!(matches!(
        repe.ops[0].kind,
        OpKind::X86String {
            kind: X86StringKind::Cmps,
            rep: X86RepMode::Repe,
            ..
        }
    ));
    let repne = lift_single(&[0xF2, 0xAF]).unwrap();
    assert!(matches!(
        repne.ops[0].kind,
        OpKind::X86String {
            kind: X86StringKind::Scas,
            rep: X86RepMode::Repne,
            ..
        }
    ));
    for bytes in [&[0xF2, 0xA4][..], &[0xF3, 0xAA], &[0xF2, 0xAC]] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops[0].kind,
            OpKind::X86String {
                rep: X86RepMode::Rep,
                ..
            }
        ));
    }

    let addr32_fs = lift_single(&[0x67, 0x64, 0xA4]).unwrap();
    assert!(matches!(
        addr32_fs.ops[0].kind,
        OpKind::X86String {
            kind: X86StringKind::Movs,
            src_segment: Some(VReg::Arch(ArchReg::X86(X86Reg::FsBase))),
            address_width: OpWidth::W32,
            ..
        }
    ));
    let ignored_scas_segment = lift_single(&[0x64, 0xAE]).unwrap();
    assert!(matches!(
        ignored_scas_segment.ops[0].kind,
        OpKind::X86String {
            kind: X86StringKind::Scas,
            src_segment: None,
            ..
        }
    ));
    assert!(matches!(
        lift_single(&[0xF0, 0xA4]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}
#[test]
fn lift_popcnt_tzcnt_lzcnt_forms_flags_and_invalid_prefixes() {
    let popcnt = lift_single(&[0xF3, 0x0F, 0xB8, 0xC3]).unwrap();
    assert_eq!(popcnt.bytes_consumed, 4);
    assert!(matches!(
        popcnt.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Count {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
                width: OpWidth::W32,
                kind: X86CountKind::Popcnt,
                flags: FlagUpdate::All,
            },
            ..
        }]
    ));

    let tzcnt_alias = lift_single(&[0xF3, 0x48, 0x0F, 0xBC, 0xC0]).unwrap();
    assert!(matches!(
        tzcnt_alias.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Count {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            src: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            width: OpWidth::W64,
            kind: X86CountKind::Tzcnt,
            flags: FlagUpdate::Specific(set),
        },
            ..
        }] if *set == FlagSet::CF.union(FlagSet::ZF)
    ));

    let lzcnt_mem16 = lift_single(&[0xF3, 0x66, 0x0F, 0xBD, 0x03]).unwrap();
    assert!(matches!(
        lzcnt_mem16.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::Load {
                    dst: loaded,
                    width: MemWidth::B2,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::X86Count {
                    src,
            width: OpWidth::W16,
                    kind: X86CountKind::Lzcnt,
                    flags: FlagUpdate::Specific(set),
                    ..
                },
                ..
            }
        ] if loaded == src && *set == FlagSet::CF.union(FlagSet::ZF)
    ));
    assert!(
        !popcnt.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::ReadFlags { .. } | OpKind::WriteFlags { .. }
        )),
        "legacy count flags must remain intrinsic to one x86 count op"
    );

    let bsf = lift_single(&[0x0F, 0xBC, 0xC3]).unwrap();
    assert!(bsf.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Bsf {
            flags: FlagUpdate::Specific(FlagSet::ZF),
            ..
        }
    )));
    assert!(
        !bsf.ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::Ctz { .. }))
    );

    let bsr = lift_single(&[0x48, 0x0F, 0xBD, 0xCB]).unwrap();
    assert!(bsr.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Bsr {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
            src: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(FlagSet::ZF),
        }
    )));

    let missing_mandatory_prefix =
        lift_single(&[0x0F, 0xB8, 0xC3]).expect("unprefixed 0F B8 must terminate as #UD");
    assert_invalid_opcode_trap(&missing_mandatory_prefix, 2);
    assert!(matches!(
        lift_single(&[0xF0, 0xF3, 0x0F, 0xB8, 0xC3]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}
#[test]
fn lift_emms_and_femms_exact_state_transition_prefixes_and_legality() {
    for bytes in [
        &[0x0F, 0x77][..],
        &[0x66, 0x0F, 0x77][..],
        &[0x67, 0x0F, 0x77][..],
        &[0xF2, 0x0F, 0x77][..],
        &[0xF3, 0x0F, 0x77][..],
        &[0x48, 0x0F, 0x77][..],
        &[0x64, 0x0F, 0x77][..],
        &[0x0F, 0x0E][..],
        &[0x66, 0x0F, 0x0E][..],
        &[0x67, 0x0F, 0x0E][..],
        &[0xF2, 0x0F, 0x0E][..],
        &[0xF3, 0x0F, 0x0E][..],
        &[0x48, 0x0F, 0x0E][..],
        &[0x64, 0x0F, 0x0E][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EmptyMmx,
                    addr: None,
                },
                ..
            }]
        ));
    }

    for bytes in [&[0xF0, 0x0F, 0x77][..], &[0xF0, 0x0F, 0x0E][..]] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "accepted invalid EMMS/FEMMS encoding {bytes:02X?}"
        );
    }

    let rex2_emms = lift_single(&[0xD5, 0x80, 0x77]).expect("REX2 compressed map 1 EMMS");
    assert_eq!(rex2_emms.bytes_consumed, 3);
    let ops = assert_rex2_guarded_ops(&rex2_emms, 1);
    assert!(matches!(
        ops,
        [SmirOp {
            kind: OpKind::X86X87Control {
                kind: X86X87ControlKind::EmptyMmx,
                addr: None,
            },
            ..
        }]
    ));

    assert!(matches!(
        lift_single(&[0xD5, 0x80, 0x0E]),
        Err(LiftError::InvalidEncoding { .. })
    ));

    for bytes in [&[0xD5, 0x00, 0x0F, 0x77][..], &[0xD5, 0x00, 0x0F, 0x0E][..]] {
        let result = lift_single(bytes).expect("REX2 followed by 0F is an explicit #UD");
        assert_invalid_opcode_trap(&result, 3);
    }
}
#[test]
fn newly_lifted_legacy_paths_reject_illegal_lock_prefixes() {
    for bytes in [
        &[0xF0, 0x98][..],
        &[0xF0, 0x9C][..],
        &[0xF0, 0x9D][..],
        &[0xF0, 0x9E][..],
        &[0xF0, 0x9F][..],
        &[0xF0, 0x8F, 0xC0][..],
        &[0xF0, 0x67, 0xA0, 0, 0, 0, 0][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "LOCK-prefixed bytes should be invalid: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_ud2_is_an_explicit_invalid_opcode_trap() {
    for bytes in [&[0x0F, 0x0B][..], &[0x66, 0x0F, 0x0B][..]] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.is_empty());
        assert!(matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }

    for bytes in [
        &[0x0F, 0xAA][..],
        &[0x0F, 0xB9, 0xC0][..],
        &[0x0F, 0xB9, 0x84, 0x88, 0x78, 0x56, 0x34, 0x12][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
}

#[test]
fn lift_reserved_two_byte_opcodes_are_explicit_invalid_opcode_traps() {
    const RESERVED: &[u8] = &[0x04, 0x0A, 0x0C];
    const LEGACY_PREFIXES: &[&[u8]] = &[
        &[],
        &[0x66],
        &[0x67],
        &[0xF2],
        &[0xF3],
        &[0x2E],
        &[0x48],
        &[0xF0],
        &[0x66, 0x67, 0xF3, 0x2E, 0x48],
    ];

    let assert_trap = |bytes: &[u8], expected_len: usize| {
        let result = lift_single(bytes)
            .unwrap_or_else(|error| panic!("reserved opcode {bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, expected_len, "{bytes:02X?}");
        assert!(result.ops.is_empty(), "{bytes:02X?}");
        assert!(result.branch_targets.is_empty(), "{bytes:02X?}");
        assert!(
            matches!(
                result.control_flow,
                ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode
                }
            ),
            "{bytes:02X?}"
        );
    };

    for &opcode in RESERVED {
        for &prefixes in LEGACY_PREFIXES {
            let mut bytes = prefixes.to_vec();
            bytes.extend([0x0F, opcode]);
            assert_trap(&bytes, bytes.len());
        }

        // REX2.M0=1 selects legacy map 1 without an encoded 0F escape.
        for bytes in [
            vec![0xD5, 0x80, opcode],
            vec![0xD5, 0xFF, opcode],
            vec![0x66, 0x67, 0xF3, 0x2E, 0xD5, 0x80, opcode],
        ] {
            assert_trap(&bytes, bytes.len());
        }

        // No ModR/M, SIB, displacement, or immediate byte belongs to a blank
        // opcode-map cell. Deterministically stop before operand-like bytes.
        let trailing = [0x0F, opcode, 0x04, 0x25, 0x78, 0x56, 0x34, 0x12];
        assert_trap(&trailing, 2);
    }
}

#[test]
fn reserved_two_byte_opcodes_are_exact_interpreter_frontiers_without_operand_fetch() {
    for opcode in [0x04, 0x0A, 0x0C] {
        // The memory image ends at the reserved main opcode. Successful
        // lifting therefore proves that no undefined operand byte was read.
        let code = vec![0x48, 0x83, 0xC0, 0x01, 0x0F, opcode];
        let mut lifter = X86_64Lifter::strict();
        lifter.set_interpreter_frontiers(true);
        let mut context = LiftContext::new(SourceArch::X86_64);
        let function = lifter
            .lift_function(0x1800, &TestMemory::new(0x1800, code), &mut context)
            .unwrap_or_else(|error| panic!("reserved opcode 0F {opcode:02X}: {error:?}"));

        let prefix = function
            .blocks
            .iter()
            .find(|block| block.guest_pc == 0x1800)
            .unwrap_or_else(|| panic!("0F {opcode:02X}: missing supported prefix"));
        let frontier = function
            .blocks
            .iter()
            .find(|block| block.guest_pc == 0x1804)
            .unwrap_or_else(|| panic!("0F {opcode:02X}: missing exact #UD frontier"));
        assert!(!prefix.ops.is_empty(), "0F {opcode:02X}");
        assert!(
            matches!(prefix.terminator, Terminator::Branch { target } if target == frontier.id),
            "0F {opcode:02X}"
        );
        assert!(frontier.ops.is_empty(), "0F {opcode:02X}");
        assert!(
            matches!(frontier.terminator, Terminator::Return { .. }),
            "0F {opcode:02X}"
        );
    }
}

#[test]
fn lift_ud0_is_an_explicit_two_byte_trap_without_operand_fetch() {
    let cases: &[(&[u8], usize)] = &[
        (&[0x0F, 0xFF], 2),
        (&[0x0F, 0xFF, 0xC0], 2),
        (&[0x0F, 0xFF, 0x04, 0x25, 0x78, 0x56, 0x34, 0x12], 2),
        (&[0x66, 0x0F, 0xFF, 0xC0], 3),
        (&[0xF0, 0x0F, 0xFF, 0xC0], 3),
        (&[0x48, 0x0F, 0xFF, 0xC0], 3),
        // REX2.M selects the 0F map without an encoded 0F byte.
        (&[0xD5, 0x80, 0xFF, 0xC0], 3),
        // REX2.M=0 makes a following 0F byte a reserved map-0 opcode.
        (&[0xD5, 0x00, 0x0F, 0xFF, 0xC0], 3),
    ];

    for &(bytes, expected_len) in cases {
        let result = lift_single(bytes).expect("UD0 must strictly lift to an explicit trap");
        assert_eq!(result.bytes_consumed, expected_len, "{bytes:02X?}");
        assert!(result.ops.is_empty(), "{bytes:02X?}");
        assert!(result.branch_targets.is_empty(), "{bytes:02X?}");
        assert!(matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }

    // A two-byte buffer is sufficient: this implementation follows the
    // architecturally permitted UD0 profile that does not decode ModR/M.
    let mem = TestMemory::new(0x1000, vec![0xB8, 0x78, 0x56, 0x34, 0x12, 0x0F, 0xFF]);
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let block = lifter.lift_block(0x1000, &mem, &mut ctx).unwrap();
    assert!(matches!(
        block.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src: SrcOperand::Imm(0x1234_5678),
                width: OpWidth::W32,
            },
            ..
        }]
    ));
    assert!(matches!(
        block.terminator,
        Terminator::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));
}
#[test]
fn lift_enter_decodes_width_nesting_mask_and_invalid_forms() {
    let enter64 = lift_single(&[0xC8, 0x20, 0, 0]).unwrap();
    assert_eq!(enter64.bytes_consumed, 4);
    assert!(matches!(
        enter64.ops[1].kind,
        OpKind::Sub {
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
            ..
        }
    ));
    assert!(matches!(
        enter64.ops[2].kind,
        OpKind::Store {
            width: MemWidth::B8,
            ..
        }
    ));

    let enter16_nested = lift_single(&[0x66, 0xC8, 0x10, 0, 0x22]).unwrap();
    assert_eq!(enter16_nested.bytes_consumed, 5);
    assert_eq!(
        enter16_nested
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        1,
        "nesting immediate must be masked to five bits (0x22 -> 2)",
    );
    assert!(matches!(
        lift_single(&[0xF0, 0xC8, 0, 0, 0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
    assert!(matches!(
        lift_single(&[0xC8, 0, 0]),
        Err(LiftError::Incomplete { .. })
    ));
}
#[test]
fn lift_long_mode_invalid_legacy_opcodes_are_explicit_ud_traps() {
    const INVALID: &[u8] = &[
        0x06, 0x07, 0x0E, 0x16, 0x17, 0x1E, 0x1F, // legacy segment PUSH/POP
        0x27, 0x2F, 0x37, 0x3F, // DAA/DAS/AAA/AAS
        0x60, 0x61, // PUSHA/POPA
        0x82, // legacy Group-1 alias
        0x9A, 0xEA, // far CALL/JMP immediate
        0xCE, // INTO
        0xD4, 0xD6, // AAM/SALC; D5 is the REX2 prefix in 64-bit mode
    ];
    const PREFIXES: &[&[u8]] = &[
        &[],
        &[0x66],
        &[0x67],
        &[0xF2],
        &[0xF3],
        &[0x2E],
        &[0x48],
        &[0xF0],
        &[0xD5, 0x00],
        &[0x66, 0x67, 0xF3, 0x2E, 0xD5, 0x00],
    ];

    for &opcode in INVALID {
        for &prefixes in PREFIXES {
            let mut bytes = prefixes.to_vec();
            bytes.push(opcode);
            let result = lift_single(&bytes).unwrap_or_else(|error| {
                panic!("opcode {opcode:02X}, prefixes {prefixes:02X?}: {error:?}")
            });
            assert_eq!(
                result.bytes_consumed,
                bytes.len(),
                "opcode {opcode:02X}, prefixes {prefixes:02X?}"
            );
            assert!(
                result.ops.is_empty(),
                "opcode {opcode:02X}, prefixes {prefixes:02X?}"
            );
            assert!(
                result.branch_targets.is_empty(),
                "opcode {opcode:02X}, prefixes {prefixes:02X?}"
            );
            assert!(
                matches!(
                    result.control_flow,
                    ControlFlow::Trap {
                        kind: TrapKind::InvalidOpcode
                    }
                ),
                "opcode {opcode:02X}, prefixes {prefixes:02X?}",
            );
        }
    }
}

#[test]
fn long_mode_invalid_primary_opcode_is_an_exact_interpreter_frontier() {
    for opcode in [0x06, 0x07, 0x0E, 0x16, 0x17, 0x1E, 0x1F, 0xD6] {
        let code = vec![0x48, 0x83, 0xC0, 0x01, opcode]; // ADD RAX,1; #UD
        let mut lifter = X86_64Lifter::strict();
        lifter.set_interpreter_frontiers(true);
        let mut context = LiftContext::new(SourceArch::X86_64);
        let function = lifter
            .lift_function(0x1800, &TestMemory::new(0x1800, code), &mut context)
            .unwrap_or_else(|error| panic!("opcode {opcode:02X}: {error:?}"));

        let prefix = function
            .blocks
            .iter()
            .find(|block| block.guest_pc == 0x1800)
            .unwrap_or_else(|| panic!("opcode {opcode:02X}: missing supported prefix"));
        let frontier = function
            .blocks
            .iter()
            .find(|block| block.guest_pc == 0x1804)
            .unwrap_or_else(|| panic!("opcode {opcode:02X}: missing exact #UD frontier"));
        assert!(!prefix.ops.is_empty(), "opcode {opcode:02X}");
        assert!(
            matches!(prefix.terminator, Terminator::Branch { target } if target == frontier.id),
            "opcode {opcode:02X}"
        );
        assert!(frontier.ops.is_empty(), "opcode {opcode:02X}");
        assert!(
            matches!(frontier.terminator, Terminator::Return { .. }),
            "opcode {opcode:02X}"
        );
    }
}
#[test]
fn lift_0f3a_extracts_cover_lanes_widths_scalar_tuples_high_regs_and_invalids() {
    for (bytes, elem, lane, mem_width, op_width, src, dst) in [
        (
            &[0x66, 0x45, 0x0F, 0x3A, 0x14, 0xC8, 0x1F][..],
            VecElementType::I8,
            15,
            None,
            Some(OpWidth::W32),
            X86Reg::Xmm(9),
            Some(X86Reg::R8),
        ),
        (
            &[0x66, 0x44, 0x0F, 0x3A, 0x15, 0x48, 0x22, 0x0F][..],
            VecElementType::I16,
            7,
            Some(MemWidth::B2),
            None,
            X86Reg::Xmm(9),
            None,
        ),
        (
            &[0xC4, 0x43, 0x79, 0x16, 0xC8, 0x07][..],
            VecElementType::I32,
            3,
            None,
            Some(OpWidth::W32),
            X86Reg::Xmm(9),
            Some(X86Reg::R8),
        ),
        (
            &[0xC4, 0x43, 0xF9, 0x16, 0xC8, 0x03][..],
            VecElementType::I64,
            1,
            None,
            Some(OpWidth::W64),
            X86Reg::Xmm(9),
            Some(X86Reg::R8),
        ),
        (
            &[0x62, 0xC3, 0x7D, 0x08, 0x17, 0xC8, 0x07][..],
            VecElementType::I32,
            3,
            None,
            Some(OpWidth::W32),
            X86Reg::Xmm(17),
            Some(X86Reg::R8),
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(actual_src)),
                lane: actual_lane,
                elem: actual_elem,
                sign: SignExtend::Zero,
                ..
            } if actual_src == src && actual_lane == lane && actual_elem == elem
        )));
        if let Some(width) = mem_width {
            assert!(result.ops.iter().any(
                |op| matches!(op.kind, OpKind::Store { width: actual, .. } if actual == width)
            ));
            assert!(
                !result
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
            );
        }
        if let (Some(width), Some(dst)) = (op_width, dst) {
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Mov {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    width: actual_width,
                    ..
                } if actual_dst == dst && actual_width == width
            )));
        }
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );
    }

    // EVEX Tuple1 Scalar disp8 scales by the scalar destination width.
    for (bytes, expected_offset, width) in [
        (
            &[0x62, 0xE3, 0x7D, 0x08, 0x15, 0x48, 0x11, 0x07][..],
            34,
            MemWidth::B2,
        ),
        (
            &[0x62, 0xE3, 0x7D, 0x08, 0x16, 0x48, 0x05, 0x03][..],
            20,
            MemWidth::B4,
        ),
        (
            &[0x62, 0xE3, 0xFD, 0x08, 0x16, 0x48, 0x03, 0x01][..],
            24,
            MemWidth::B8,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Store {
                addr: Address::BaseOffset { offset, .. },
                width: actual,
                ..
            } if offset == expected_offset && actual == width
        )));
    }

    // W is ignored for byte/word/EXTRACTPS forms in 64-bit mode.
    assert!(lift_single(&[0xC4, 0x43, 0xF9, 0x14, 0xC8, 0x0F]).is_ok());
    assert!(lift_single(&[0x62, 0xC3, 0xFD, 0x08, 0x15, 0xC8, 0x07]).is_ok());
    assert!(lift_single(&[0x62, 0xC3, 0xFD, 0x08, 0x17, 0xC8, 0x03]).is_ok());

    for bytes in [
        &[0x0F, 0x3A, 0x14, 0xC8, 0x0F][..],
        &[0xF0, 0x66, 0x0F, 0x3A, 0x15, 0xC8, 0x07][..],
        &[0xF3, 0x66, 0x0F, 0x3A, 0x17, 0xC8, 0x03][..],
        &[0x66, 0x0F, 0x3A, 0x16, 0xC8][..],
        &[0xC4, 0x43, 0x71, 0x14, 0xC8, 0x0F][..],
        &[0xC4, 0x43, 0x7D, 0x16, 0xC8, 0x03][..],
        &[0x62, 0xC3, 0x7D, 0x28, 0x17, 0xC8, 0x03][..],
        &[0x62, 0xC3, 0x7D, 0x09, 0x14, 0xC8, 0x0F][..],
        &[0x62, 0xC3, 0x7D, 0x18, 0x15, 0xC8, 0x07][..],
        // EVEX.X' cannot select a fifth-bit GPR destination.
        &[0x62, 0x83, 0x7D, 0x08, 0x16, 0xC8, 0x03][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid extract encoding accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_0f3a_inserts_cover_merges_sources_zero_masks_tuples_aliases_and_invalids() {
    for (bytes, elem, lane, merge, dst, load_width) in [
        (
            &[0x66, 0x45, 0x0F, 0x3A, 0x20, 0xC8, 0x1F][..],
            VecElementType::I8,
            15,
            X86Reg::Xmm(9),
            X86Reg::Xmm(9),
            None,
        ),
        (
            &[0xC4, 0x63, 0x29, 0x22, 0x48, 0x14, 0x07][..],
            VecElementType::I32,
            3,
            X86Reg::Xmm(10),
            X86Reg::Xmm(9),
            Some(MemWidth::B4),
        ),
        (
            &[0x62, 0xC3, 0xED, 0x00, 0x22, 0xC8, 0x03][..],
            VecElementType::I64,
            1,
            X86Reg::Xmm(18),
            X86Reg::Xmm(17),
            None,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(actual_merge)),
                elem: actual_elem,
                ..
            } if actual_merge == merge && actual_elem == elem
        )));
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                lane: actual_lane,
                elem: actual_elem,
                ..
            } if actual_lane == lane && actual_elem == elem
        )));
        assert!(
            result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    width: VecWidth::V128,
                    ..
                } if actual_dst == dst
            )) || merge == dst
        );
        if let Some(width) = load_width {
            assert!(result.ops.iter().any(
                |op| matches!(op.kind, OpKind::Load { width: actual, .. } if actual == width)
            ));
        }
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );
    }

    let insertps = lift_single(&[0xC4, 0x43, 0x29, 0x21, 0xCB, 0x2C]).unwrap();
    assert!(insertps.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(11))),
            lane: 0,
            elem: VecElementType::I32,
            ..
        }
    )));
    assert!(insertps.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
            lane: 1,
            elem: VecElementType::I32,
            ..
        }
    )));
    assert!(insertps.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VMov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
            width: VecWidth::V128,
            ..
        }
    )));

    // Memory Count_S is ignored, and EVEX Tuple1 Scalar disp8 scales by
    // the actual scalar width without any vector-alignment check.
    for (bytes, expected_offset, width) in [
        (
            &[0x62, 0xE3, 0x6D, 0x00, 0x20, 0x48, 0x11, 0xCF][..],
            17,
            MemWidth::B1,
        ),
        (
            &[0x62, 0xE3, 0x6D, 0x00, 0x22, 0x48, 0x05, 0xC3][..],
            20,
            MemWidth::B4,
        ),
        (
            &[0x62, 0xE3, 0xED, 0x00, 0x22, 0x48, 0x03, 0xC1][..],
            24,
            MemWidth::B8,
        ),
        (
            &[0x62, 0xE3, 0x6D, 0x00, 0x21, 0x48, 0x05, 0xEC][..],
            20,
            MemWidth::B4,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load {
                addr: Address::BaseOffset { offset, .. },
                width: actual,
                ..
            } if offset == expected_offset && actual == width
        )));
        assert!(
            !result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
    }

    // W is ignored for byte forms; EVEX INSERTPS requires W0.
    assert!(lift_single(&[0xC4, 0x43, 0xA9, 0x20, 0xC8, 0x0F]).is_ok());
    assert!(lift_single(&[0x62, 0xC3, 0xED, 0x00, 0x20, 0xC8, 0x0F]).is_ok());
    for bytes in [
        &[0x0F, 0x3A, 0x20, 0xC8, 0x0F][..],
        &[0xF0, 0x66, 0x0F, 0x3A, 0x21, 0xC8, 0x2C][..],
        &[0xF3, 0x66, 0x0F, 0x3A, 0x22, 0xC8, 0x03][..],
        &[0x66, 0x0F, 0x3A, 0x20, 0xC8][..],
        &[0xC4, 0x43, 0x2D, 0x20, 0xC8, 0x0F][..],
        &[0x62, 0xC3, 0x6D, 0x20, 0x21, 0xC8, 0x2C][..],
        &[0x62, 0xC3, 0xED, 0x00, 0x21, 0xC8, 0x2C][..],
        &[0x62, 0xC3, 0x6D, 0x01, 0x22, 0xC8, 0x03][..],
        &[0x62, 0xC3, 0x6D, 0x10, 0x20, 0xC8, 0x0F][..],
        // EVEX.X' cannot select a fifth-bit GPR source.
        &[0x62, 0x83, 0x6D, 0x00, 0x22, 0xC8, 0x03][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid insert encoding accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn senduipi_lifts_to_configured_invalid_opcode_and_remains_a_jit_frontier() {
    // MOV EAX,0x12345678; SENDUIPI. Strict lifting represents the configured
    // #UD explicitly, while JIT frontier mode preserves the native prefix
    // and returns to the interpreter at SENDUIPI.
    let bytes = vec![0xB8, 0x78, 0x56, 0x34, 0x12, 0xF3, 0x0F, 0xC7, 0xF0];
    let mem = TestMemory::new(0x1000, bytes);

    let mut strict = X86_64Lifter::strict();
    let mut strict_ctx = LiftContext::new(SourceArch::X86_64);
    let strict_function = strict.lift_function(0x1000, &mem, &mut strict_ctx).unwrap();
    let strict_entry = strict_function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1000)
        .unwrap();
    assert_eq!(strict_entry.ops.len(), 1);
    assert!(matches!(
        strict_entry.terminator,
        Terminator::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));

    let mut partial = X86_64Lifter::strict();
    partial.set_interpreter_frontiers(true);
    let mut partial_ctx = LiftContext::new(SourceArch::X86_64);
    let mut function = partial
        .lift_function(0x1000, &mem, &mut partial_ctx)
        .unwrap();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    let entry = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1000)
        .unwrap();
    let frontier = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1005)
        .unwrap();
    assert!(matches!(
        entry.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src: SrcOperand::Imm(0x1234_5678),
                width: OpWidth::W32,
            },
            ..
        }]
    ));
    assert!(matches!(
        entry.terminator,
        Terminator::Branch { target } if target == frontier.id
    ));
    assert!(frontier.ops.is_empty());
    assert!(matches!(frontier.terminator, Terminator::Return { .. }));

    // Exact instruction lifting retains the same explicit trap and full
    // instruction length outside region-frontier policy.
    let mut insn_ctx = LiftContext::new(SourceArch::X86_64);
    let insn = partial
        .lift_insn(0x1005, &[0xF3, 0x0F, 0xC7, 0xF0], &mut insn_ctx)
        .unwrap();
    assert_eq!(insn.bytes_consumed, 4);
    assert!(insn.ops.is_empty());
    assert!(matches!(
        insn.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));
}
#[test]
fn interpreter_frontiers_split_supported_prefix_before_terminal_control_flow() {
    let prefix = [0xB8, 0x78, 0x56, 0x34, 0x12]; // mov eax,0x12345678
    let terminals: &[(&str, &[u8])] = &[
        ("ret", &[0xC3]),
        ("hlt", &[0xF4]),
        ("syscall", &[0x0F, 0x05]),
        ("jmp-reg", &[0xFF, 0xE0]),
        ("jmp-mem", &[0xFF, 0x20]),
        ("call-rel", &[0xE8, 0, 0, 0, 0]),
        ("call-reg", &[0xFF, 0xD0]),
    ];

    for &(name, terminal) in terminals {
        let mut bytes = prefix.to_vec();
        bytes.extend_from_slice(terminal);
        let mem = TestMemory::new(0x1800, bytes);
        let mut lifter = X86_64Lifter::strict();
        lifter.set_interpreter_frontiers(true);
        let mut ctx = LiftContext::new(SourceArch::X86_64);
        let mut function = lifter.lift_function(0x1800, &mem, &mut ctx).unwrap();
        crate::smir::optimize::optimize_function(
            &mut function,
            crate::smir::optimize::OptLevel::O2,
        );

        let entry = function
            .blocks
            .iter()
            .find(|block| block.guest_pc == 0x1800)
            .unwrap_or_else(|| panic!("{name}: missing supported prefix block"));
        let frontier = function
            .blocks
            .iter()
            .find(|block| block.guest_pc == 0x1805)
            .unwrap_or_else(|| panic!("{name}: missing exact terminal frontier"));
        assert!(
            matches!(
                entry.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::Mov {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                        src: SrcOperand::Imm(0x1234_5678),
                        width: OpWidth::W32,
                    },
                    ..
                }]
            ),
            "{name}: supported prefix was not retained"
        );
        assert!(matches!(
            entry.terminator,
            Terminator::Branch { target } if target == frontier.id
        ));
        assert!(
            frontier.ops.is_empty(),
            "{name}: frontier executed terminal ops"
        );
        assert!(matches!(frontier.terminator, Terminator::Return { .. }));
    }
}
#[test]
fn interpreter_frontiers_retain_prefix_at_unreadable_boundary() {
    let mem = TestMemory::new(0x2000, vec![0xB8, 0xEF, 0xBE, 0xAD, 0xDE]);
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let function = lifter.lift_function(0x2000, &mem, &mut ctx).unwrap();

    let entry = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x2000)
        .unwrap();
    let frontier = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x2005)
        .unwrap();
    assert_eq!(entry.ops.len(), 1);
    assert!(matches!(
        entry.terminator,
        Terminator::Branch { target } if target == frontier.id
    ));
    assert!(frontier.ops.is_empty());
    assert!(matches!(frontier.terminator, Terminator::Return { .. }));
}
