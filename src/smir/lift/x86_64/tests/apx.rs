//! tests::apx tests

use super::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_adx_legacy_prefixes_like_llvm() {
    for (bytes, name, kind, width) in [
        (
            &[0x66, 0x0F, 0x38, 0xF6, 0xC3][..],
            "adcxl",
            X86AdxKind::Adcx,
            OpWidth::W32,
        ),
        (
            &[0x66, 0x48, 0x0F, 0x38, 0xF6, 0xC3][..],
            "adcxq",
            X86AdxKind::Adcx,
            OpWidth::W64,
        ),
        (
            &[0xF3, 0x0F, 0x38, 0xF6, 0xC3][..],
            "adoxl",
            X86AdxKind::Adox,
            OpWidth::W32,
        ),
        (
            &[0xF3, 0x48, 0x0F, 0x38, 0xF6, 0xC3][..],
            "adoxq",
            X86AdxKind::Adox,
            OpWidth::W64,
        ),
        (
            &[0x66, 0xF3, 0x0F, 0x38, 0xF6, 0xC3][..],
            "66+f3 adoxl",
            X86AdxKind::Adox,
            OpWidth::W32,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert_adx_sequence(&result, 0, kind, x86_gpr(0), x86_gpr(0), x86_gpr(3), width);
    }
}
#[test]
fn lift_adx_apx_nd_uses_vvvv_destination_like_llvm() {
    // LLVM 20: `adcxq %rbx, %rax, %r8` => 62 f4 bd 18 66 c3.
    let result = lift_single(&[0x62, 0xF4, 0xBD, 0x18, 0x66, 0xC3]).unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_adx_sequence(
        &result,
        0,
        X86AdxKind::Adcx,
        x86_gpr(8),
        x86_gpr(0),
        x86_gpr(3),
        OpWidth::W64,
    );

    // LLVM accepts nonzero EVEX aaa bits for this opcode and ignores them.
    let result = lift_single(&[0x62, 0xF4, 0xBD, 0x19, 0x66, 0xC3]).unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_adx_sequence(
        &result,
        0,
        X86AdxKind::Adcx,
        x86_gpr(8),
        x86_gpr(0),
        x86_gpr(3),
        OpWidth::W64,
    );

    // LLVM 20: `adoxq 32(%r17,%r18,4), %r19, %r20`
    // => 62 ec da 10 66 5c 91 20.
    let result = lift_single(&[0x62, 0xEC, 0xDA, 0x10, 0x66, 0x5C, 0x91, 0x20]).unwrap();
    assert_eq!(result.bytes_consumed, 8);
    let mem_src = match &result.ops[0].kind {
        OpKind::Load {
            dst,
            addr:
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x20,
                    ..
                },
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*base, x86_gpr(17));
            assert_eq!(*index, x86_gpr(18));
            *dst
        }
        other => panic!("expected APX ADOX memory load, got {other:?}"),
    };
    assert_adx_sequence(
        &result,
        1,
        X86AdxKind::Adox,
        x86_gpr(20),
        x86_gpr(19),
        mem_src,
        OpWidth::W64,
    );
}
#[test]
fn lift_adx_rejects_invalid_forms_like_llvm() {
    for (bytes, name) in [
        (&[0x0F, 0x38, 0xF6, 0xC3][..], "legacy missing prefix"),
        (&[0xF2, 0x0F, 0x38, 0xF6, 0xC3][..], "legacy f2 prefix"),
    ] {
        let err = lift_single(bytes).expect_err(name);
        assert!(
            matches!(err, LiftError::InvalidEncoding { .. }),
            "{name}: {err:?}"
        );
    }

    for (bytes, name) in [
        (&[0x62, 0xF4, 0xBD, 0x08, 0x66, 0xC3][..], "APX missing ND"),
        (&[0x62, 0xF4, 0xBD, 0x1C, 0x66, 0xC3][..], "APX NF reserved"),
        (&[0x62, 0xF4, 0xBC, 0x18, 0x66, 0xC3][..], "APX pp none"),
        (&[0x62, 0xF4, 0xBF, 0x18, 0x66, 0xC3][..], "APX pp 3"),
        (
            &[0x62, 0xF4, 0xBD, 0x98, 0x66, 0xC3][..],
            "APX z bit reserved",
        ),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| {
            panic!("{name}: reserved APX ADX form must strictly lift to #UD: {error:?}")
        });
        assert_eq!(result.bytes_consumed, 5, "{name}");
        assert!(result.ops.is_empty(), "{name}");
        assert!(
            matches!(
                result.control_flow,
                ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode
                }
            ),
            "{name}: {:?}",
            result.control_flow
        );
    }
}
#[test]
fn lift_rao_int_legacy_prefixes_like_llvm() {
    for (bytes, name, op, width) in [
        (
            &[0x0F, 0x38, 0xFC, 0x18][..],
            "aaddl",
            AtomicOp::Add,
            MemWidth::B4,
        ),
        (
            &[0x48, 0x0F, 0x38, 0xFC, 0x18][..],
            "aaddq",
            AtomicOp::Add,
            MemWidth::B8,
        ),
        (
            &[0x66, 0x0F, 0x38, 0xFC, 0x18][..],
            "aandl",
            AtomicOp::And,
            MemWidth::B4,
        ),
        (
            &[0xF2, 0x48, 0x0F, 0x38, 0xFC, 0x18][..],
            "aorq",
            AtomicOp::Or,
            MemWidth::B8,
        ),
        (
            &[0xF3, 0x0F, 0x38, 0xFC, 0x18][..],
            "axorl",
            AtomicOp::Xor,
            MemWidth::B4,
        ),
        (
            &[0xF3, 0x48, 0x0F, 0x38, 0xFC, 0x18][..],
            "axorq",
            AtomicOp::Xor,
            MemWidth::B8,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert_eq!(result.ops.len(), 1, "{name}");
        match &result.ops[0].kind {
            OpKind::AtomicRmw {
                addr: Address::Direct(base),
                src,
                op: got_op,
                width: got_width,
                order: MemoryOrder::SeqCst,
                ..
            } => {
                assert_eq!(*base, x86_gpr(0), "{name}");
                assert_eq!(*src, x86_gpr(3), "{name}");
                assert_eq!(*got_op, op, "{name}");
                assert_eq!(*got_width, width, "{name}");
            }
            other => panic!("expected {name} AtomicRmw, got {other:?}"),
        }
    }
}
#[test]
fn lift_rao_int_apx_egpr_memory_like_llvm() {
    // LLVM 20: `aandq %r17, (%r16)` => 62 ec fd 08 fc 08.
    let result = lift_single(&[0x62, 0xEC, 0xFD, 0x08, 0xFC, 0x08]).unwrap();
    assert_eq!(result.bytes_consumed, 6);
    match &result.ops[0].kind {
        OpKind::AtomicRmw {
            addr: Address::Direct(base),
            src,
            op: AtomicOp::And,
            width: MemWidth::B8,
            order: MemoryOrder::SeqCst,
            ..
        } => {
            assert_eq!(*base, x86_gpr(16));
            assert_eq!(*src, x86_gpr(17));
        }
        other => panic!("expected APX AAND AtomicRmw, got {other:?}"),
    }

    // LLVM 20: `aaddl %r19d, 32(%r17,%r18,4)`
    // => 62 ec 78 08 fc 5c 91 20.
    let result = lift_single(&[0x62, 0xEC, 0x78, 0x08, 0xFC, 0x5C, 0x91, 0x20]).unwrap();
    assert_eq!(result.bytes_consumed, 8);
    match &result.ops[0].kind {
        OpKind::AtomicRmw {
            addr:
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x20,
                    ..
                },
            src,
            op: AtomicOp::Add,
            width: MemWidth::B4,
            order: MemoryOrder::SeqCst,
            ..
        } => {
            assert_eq!(*base, x86_gpr(17));
            assert_eq!(*index, x86_gpr(18));
            assert_eq!(*src, x86_gpr(19));
        }
        other => panic!("expected APX AADD complex AtomicRmw, got {other:?}"),
    }
}
#[test]
fn lift_rao_int_rejects_invalid_forms_like_llvm() {
    for (bytes, name) in [
        (&[0x0F, 0x38, 0xFC, 0xD8][..], "legacy register operand"),
        (
            &[0x62, 0xF4, 0xFC, 0x0C, 0xFC, 0x18][..],
            "EVEX NF reserved",
        ),
        (
            &[0x62, 0xF4, 0xFC, 0x18, 0xFC, 0x18][..],
            "EVEX ND reserved",
        ),
        (
            &[0x62, 0xF4, 0xBC, 0x08, 0xFC, 0x18][..],
            "EVEX vvvv reserved",
        ),
        (
            &[0x62, 0xF4, 0xFC, 0x09, 0xFC, 0x18][..],
            "EVEX aaa reserved",
        ),
        (
            &[0x62, 0xF4, 0xFC, 0x08, 0xFC, 0xD8][..],
            "EVEX register operand",
        ),
    ] {
        let err = lift_single(bytes).expect_err(name);
        assert!(
            matches!(err, LiftError::InvalidEncoding { .. }),
            "{name}: {err:?}"
        );
    }
}
#[test]
fn rex2_modrm_decode_extends_to_apx_gprs() {
    let prefix = decode_prefixes(&[0xD5, 0x5D, 0x89, 0xF8]).unwrap();
    let modrm = decode_modrm(&[0xF8], &prefix, 0).unwrap();
    assert!(!modrm.is_memory);
    assert_eq!(modrm.reg, 31);
    assert_eq!(modrm.rm, 24);
}
#[test]
fn lift_rex2_mov_egpr_imm64_uses_llvm_encoding() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `mov r16, 0x1122334455667788`
    let result = lifter
        .lift_insn(
            0x1000,
            &[
                0xD5, 0x18, 0xB8, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11,
            ],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(result.bytes_consumed, 11);
    let ops = assert_rex2_guarded_ops(&result, 1);
    match &ops[0].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm64(0x1122_3344_5566_7788),
            width: OpWidth::W64,
        } => assert_eq!(*dst, x86_gpr(16)),
        other => panic!("expected R16 imm64 mov, got {other:?}"),
    }

    // LLVM 20: `mov r24, 0x1122334455667788`
    let result = lifter
        .lift_insn(
            0x1000,
            &[
                0xD5, 0x19, 0xB8, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11,
            ],
            &mut ctx,
        )
        .unwrap();
    let ops = assert_rex2_guarded_ops(&result, 1);
    match &ops[0].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm64(0x1122_3344_5566_7788),
            width: OpWidth::W64,
        } => assert_eq!(*dst, x86_gpr(24)),
        other => panic!("expected R24 imm64 mov, got {other:?}"),
    }
}
#[test]
fn lift_rex2_mov_egpr_reg_uses_llvm_encoding() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `mov r16, rax`
    let result = lifter
        .lift_insn(0x1000, &[0xD5, 0x18, 0x89, 0xC0], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 4);
    let ops = assert_rex2_guarded_ops(&result, 1);
    match &ops[0].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } => {
            assert_eq!(*dst, x86_gpr(16));
            assert_eq!(*src, x86_gpr(0));
        }
        other => panic!("expected mov r16, rax, got {other:?}"),
    }

    // LLVM 20: `mov r16, rax` has the APX register in r/m; `mov rax, r16`
    // uses ModR/M.reg extension instead.
    let result = lifter
        .lift_insn(0x1000, &[0xD5, 0x48, 0x89, 0xC0], &mut ctx)
        .unwrap();
    let ops = assert_rex2_guarded_ops(&result, 1);
    match &ops[0].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } => {
            assert_eq!(*dst, x86_gpr(0));
            assert_eq!(*src, x86_gpr(16));
        }
        other => panic!("expected mov rax, r16, got {other:?}"),
    }

    // LLVM 20: `mov r24, r31`
    let result = lifter
        .lift_insn(0x1000, &[0xD5, 0x5D, 0x89, 0xF8], &mut ctx)
        .unwrap();
    let ops = assert_rex2_guarded_ops(&result, 1);
    match &ops[0].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } => {
            assert_eq!(*dst, x86_gpr(24));
            assert_eq!(*src, x86_gpr(31));
        }
        other => panic!("expected mov r24, r31, got {other:?}"),
    }
}
#[test]
fn lift_rex2_push_pop_egpr_uses_llvm_encoding() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20 accepts this non-canonical payload and disassembles it as
    // `pushp %r16`; the canonical encoding below keeps the APX oracle exact.
    let result = lifter
        .lift_insn(0x1000, &[0xD5, 0x10, 0x50], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 3);
    let ops = assert_rex2_guarded_ops(&result, 2);
    match &ops[1].kind {
        OpKind::Store {
            src,
            addr: Address::Direct(_),
            width: MemWidth::B8,
        } => assert_eq!(*src, x86_gpr(16)),
        other => panic!("expected push r16 store, got {other:?}"),
    }

    // LLVM 20: `pushp %r16` => d5 18 50.
    let result = lifter
        .lift_insn(0x1000, &[0xD5, 0x18, 0x50], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 3);
    let ops = assert_rex2_guarded_ops(&result, 2);
    match &ops[1].kind {
        OpKind::Store {
            src,
            addr: Address::Direct(_),
            width: MemWidth::B8,
        } => assert_eq!(*src, x86_gpr(16)),
        other => panic!("expected pushp r16 store, got {other:?}"),
    }

    // LLVM 20 accepts this non-canonical payload and disassembles it as
    // `popp %r31`; the canonical encoding for a concrete register follows.
    let result = lifter
        .lift_insn(0x1000, &[0xD5, 0x11, 0x5F], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 3);
    let ops = assert_rex2_guarded_ops(&result, 2);
    match &ops[0].kind {
        OpKind::Load {
            dst,
            addr: Address::Direct(_),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } => assert_eq!(*dst, x86_gpr(31)),
        other => panic!("expected pop r31 load, got {other:?}"),
    }

    // LLVM 20: `popp %r16` => d5 18 58.
    let result = lifter
        .lift_insn(0x1000, &[0xD5, 0x18, 0x58], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 3);
    let ops = assert_rex2_guarded_ops(&result, 2);
    match &ops[0].kind {
        OpKind::Load {
            dst,
            addr: Address::Direct(_),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } => assert_eq!(*dst, x86_gpr(16)),
        other => panic!("expected popp r16 load, got {other:?}"),
    }
}
#[test]
fn lift_rex2_cmpxchg_registers_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `cmpxchgq %r17, %r16` => d5 d8 b1 c8.
    let result = lifter
        .lift_insn(0x1000, &[0xD5, 0xD8, 0xB1, 0xC8], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 4);
    let ops = assert_rex2_guarded_ops(&result, 7);

    let saved_src = match &ops[0].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } => {
            assert_eq!(*src, x86_gpr(17));
            *dst
        }
        other => panic!("expected CMPXCHG source snapshot, got {other:?}"),
    };
    let saved_acc = match &ops[1].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } => {
            assert_eq!(*src, x86_gpr(0));
            *dst
        }
        other => panic!("expected CMPXCHG accumulator snapshot, got {other:?}"),
    };
    let old_dst = match &ops[2].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } => {
            assert_eq!(*src, x86_gpr(16));
            *dst
        }
        other => panic!("expected CMPXCHG destination snapshot, got {other:?}"),
    };
    match &ops[3].kind {
        OpKind::Cmp {
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
        } => {
            assert_eq!(*src1, saved_acc);
            assert_eq!(*src2, old_dst);
        }
        other => panic!("expected CMPXCHG compare, got {other:?}"),
    }
    match &ops[4].kind {
        OpKind::SetCC {
            cond: Condition::Eq,
            width: OpWidth::W8,
            ..
        } => {}
        other => panic!("expected CMPXCHG equality condition, got {other:?}"),
    }
    // The destination/accumulator writes use CMove, which preserves the
    // register on the no-op path instead of an unconditional Select that would
    // zero-extend a sub-64-bit write and clear the upper bits. (#21)
    match &ops[5].kind {
        OpKind::CMove {
            dst,
            src,
            cond: Condition::Eq,
            width: OpWidth::W64,
        } => {
            assert_eq!(*dst, x86_gpr(16));
            assert_eq!(*src, saved_src);
        }
        other => panic!("expected CMPXCHG destination cmove, got {other:?}"),
    }
    match &ops[6].kind {
        OpKind::CMove {
            dst,
            src,
            cond: Condition::Ne,
            width: OpWidth::W64,
        } => {
            assert_eq!(*dst, x86_gpr(0));
            assert_eq!(*src, old_dst);
        }
        other => panic!("expected CMPXCHG accumulator cmove, got {other:?}"),
    }
}
#[test]
fn lift_rex2_cmpxchg_memory_egpr_sib_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `cmpxchgq %r18, 32(%r16,%r17,4)` => d5 f8 b1 54 88 20.
    let result = lifter
        .lift_insn(0x1000, &[0xD5, 0xF8, 0xB1, 0x54, 0x88, 0x20], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 6);
    let ops = assert_rex2_guarded_ops(&result, 8);

    let saved_src = match &ops[0].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } => {
            assert_eq!(*src, x86_gpr(18));
            *dst
        }
        other => panic!("expected CMPXCHG memory source snapshot, got {other:?}"),
    };
    let saved_acc = match &ops[1].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } => {
            assert_eq!(*src, x86_gpr(0));
            *dst
        }
        other => panic!("expected CMPXCHG memory accumulator snapshot, got {other:?}"),
    };
    let old_dst = match &ops[2].kind {
        OpKind::Load {
            dst,
            addr:
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x20,
                    ..
                },
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*base, x86_gpr(16));
            assert_eq!(*index, x86_gpr(17));
            *dst
        }
        other => panic!("expected CMPXCHG memory destination load, got {other:?}"),
    };
    match &ops[3].kind {
        OpKind::Cmp {
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
        } => {
            assert_eq!(*src1, saved_acc);
            assert_eq!(*src2, old_dst);
        }
        other => panic!("expected CMPXCHG memory compare, got {other:?}"),
    }
    let matched = match &ops[4].kind {
        OpKind::SetCC {
            dst,
            cond: Condition::Eq,
            width: OpWidth::W8,
        } => *dst,
        other => panic!("expected CMPXCHG memory equality condition, got {other:?}"),
    };
    let new_dst = match &ops[5].kind {
        OpKind::Select {
            dst,
            cond,
            src_true,
            src_false,
            width: OpWidth::W64,
        } => {
            assert_eq!(*cond, matched);
            assert_eq!(*src_true, saved_src);
            assert_eq!(*src_false, old_dst);
            *dst
        }
        other => panic!("expected CMPXCHG memory destination select, got {other:?}"),
    };
    match &ops[6].kind {
        OpKind::PredStore {
            src: SrcOperand::Reg(src),
            cond,
            addr:
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x20,
                    ..
                },
            width: MemWidth::B8,
        } => {
            assert_eq!(*src, new_dst);
            assert_eq!(*cond, matched);
            assert_eq!(*base, x86_gpr(16));
            assert_eq!(*index, x86_gpr(17));
        }
        other => panic!("expected CMPXCHG predicated memory store, got {other:?}"),
    }
    // The accumulator write uses CMove so a successful compare leaves RAX
    // unchanged rather than zero-extending a sub-64-bit Select write. (#21)
    match &ops[7].kind {
        OpKind::CMove {
            dst,
            src,
            cond: Condition::Ne,
            width: OpWidth::W64,
        } => {
            assert_eq!(*dst, x86_gpr(0));
            assert_eq!(*src, old_dst);
        }
        other => panic!("expected CMPXCHG memory accumulator cmove, got {other:?}"),
    }
}
#[test]
fn lift_rex2_xadd_registers_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    let cases: &[(&[u8], &str, usize, VReg, VReg, OpWidth)] = &[
        (
            &[0xD5, 0xD8, 0xC1, 0xC8],
            "xadd_r16_r17",
            4,
            x86_gpr(16),
            x86_gpr(17),
            OpWidth::W64,
        ),
        (
            &[0xD5, 0xD0, 0xC1, 0xC8],
            "xadd_r16d_r17d",
            4,
            x86_gpr(16),
            x86_gpr(17),
            OpWidth::W32,
        ),
        (
            &[0x66, 0xD5, 0xD0, 0xC1, 0xC8],
            "xadd_r16w_r17w",
            5,
            x86_gpr(16),
            x86_gpr(17),
            OpWidth::W16,
        ),
        (
            &[0xD5, 0xD0, 0xC0, 0xC8],
            "xadd_r16b_r17b",
            4,
            x86_gpr(16),
            x86_gpr(17),
            OpWidth::W8,
        ),
        (
            &[0xD5, 0xDD, 0xC1, 0xF8],
            "xadd_r24_r31",
            4,
            x86_gpr(24),
            x86_gpr(31),
            OpWidth::W64,
        ),
    ];

    for (bytes, name, bytes_consumed, dst_reg, src_reg, width) in cases {
        // LLVM 23 examples:
        //   `xadd r16, r17`   => d5 d8 c1 c8
        //   `xadd r16d, r17d` => d5 d0 c1 c8
        //   `xadd r16w, r17w` => 66 d5 d0 c1 c8
        //   `xadd r16b, r17b` => d5 d0 c0 c8
        //   `xadd r24, r31`   => d5 dd c1 f8
        let result = lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, *bytes_consumed, "{name}");
        assert_rex2_guarded_ops(&result, 1);
        assert_xadd_register_ops(&result, name, *dst_reg, *src_reg, *width);
    }
}
#[test]
fn lift_rex2_xadd_memory_egpr_sib_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    let cases: &[(&[u8], &str, usize, MemWidth, OpWidth)] = &[
        (
            &[0xD5, 0xF8, 0xC1, 0x54, 0x88, 0x20],
            "xaddq",
            6,
            MemWidth::B8,
            OpWidth::W64,
        ),
        (
            &[0xD5, 0xF0, 0xC1, 0x54, 0x88, 0x20],
            "xaddl",
            6,
            MemWidth::B4,
            OpWidth::W32,
        ),
        (
            &[0x66, 0xD5, 0xF0, 0xC1, 0x54, 0x88, 0x20],
            "xaddw",
            7,
            MemWidth::B2,
            OpWidth::W16,
        ),
        (
            &[0xD5, 0xF0, 0xC0, 0x54, 0x88, 0x20],
            "xaddb",
            6,
            MemWidth::B1,
            OpWidth::W8,
        ),
    ];

    for (bytes, name, bytes_consumed, mem_width, op_width) in cases {
        // LLVM 23 examples:
        //   `xaddq %r18, 32(%r16,%r17,4)` => d5 f8 c1 54 88 20
        //   `xaddl %r18d, 32(%r16,%r17,4)` => d5 f0 c1 54 88 20
        //   `xaddw %r18w, 32(%r16,%r17,4)` => 66 d5 f0 c1 54 88 20
        //   `xaddb %r18b, 32(%r16,%r17,4)` => d5 f0 c0 54 88 20
        let result = lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, *bytes_consumed, "{name}");
        // A non-LOCK memory XADD must be fault-precise: the separate Store can
        // fault (e.g. a read-only page), and a faulting XADD must leave flags
        // and the source register unchanged. So the flag-producing Add is
        // emitted AFTER the store (and writeback), not before. The store-feeding
        // Add therefore carries no flags; a trailing flag-only Add commits them
        // once the store has retired. (#23)
        let ops = assert_rex2_guarded_ops(&result, 6);

        let saved_src = match &ops[0].kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(src),
                width,
            } => {
                assert_eq!(*src, x86_gpr(18), "{name}");
                assert_eq!(*width, *op_width, "{name}");
                *dst
            }
            other => panic!("expected REX2 {name} source snapshot, got {other:?}"),
        };
        let old_dst = match &ops[1].kind {
            OpKind::Load {
                dst,
                addr,
                width,
                sign: SignExtend::Zero,
            } => {
                assert_rex2_xadd_sib_addr(addr, name);
                assert_eq!(*width, *mem_width, "{name}");
                *dst
            }
            other => panic!("expected REX2 {name} memory load, got {other:?}"),
        };
        // Store-feeding sum: NO flags, so a faulting store cannot have committed
        // flag state.
        let sum = match &ops[2].kind {
            OpKind::Add {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width,
                flags: FlagUpdate::None,
            } => {
                assert_eq!(*src1, old_dst, "{name}");
                assert_eq!(*src2, saved_src, "{name}");
                assert_eq!(*width, *op_width, "{name}");
                *dst
            }
            other => panic!("expected REX2 {name} flag-free store sum, got {other:?}"),
        };
        match &ops[3].kind {
            OpKind::Store { src, addr, width } => {
                assert_eq!(*src, sum, "{name}");
                assert_rex2_xadd_sib_addr(addr, name);
                assert_eq!(*width, *mem_width, "{name}");
            }
            other => panic!("expected REX2 {name} memory store, got {other:?}"),
        }
        match &ops[4].kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(src),
                width,
            } => {
                assert_eq!(*dst, x86_gpr(18), "{name}");
                assert_eq!(*src, old_dst, "{name}");
                assert_eq!(*width, *op_width, "{name}");
            }
            other => panic!("expected REX2 {name} source writeback, got {other:?}"),
        }
        // Flags are committed only after the store and writeback have retired,
        // recomputed from the same operands as the store-feeding sum.
        match &ops[5].kind {
            OpKind::Add {
                dst: _,
                src1,
                src2: SrcOperand::Reg(src2),
                width,
                flags: FlagUpdate::All,
            } => {
                assert_eq!(*src1, old_dst, "{name}");
                assert_eq!(*src2, saved_src, "{name}");
                assert_eq!(*width, *op_width, "{name}");
            }
            other => panic!("expected REX2 {name} post-store flag add, got {other:?}"),
        }
    }
}
#[test]
fn lift_rex2_lock_xadd_memory_uses_atomic_add_like_spec() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    let cases: &[(&[u8], &str, usize, MemWidth, OpWidth)] = &[
        (
            &[0xF0, 0xD5, 0xF8, 0xC1, 0x54, 0x88, 0x20],
            "lock_xaddq",
            7,
            MemWidth::B8,
            OpWidth::W64,
        ),
        (
            &[0xF0, 0xD5, 0xF0, 0xC1, 0x54, 0x88, 0x20],
            "lock_xaddl",
            7,
            MemWidth::B4,
            OpWidth::W32,
        ),
        (
            &[0x66, 0xF0, 0xD5, 0xF0, 0xC1, 0x54, 0x88, 0x20],
            "lock_xaddw",
            8,
            MemWidth::B2,
            OpWidth::W16,
        ),
        (
            &[0xF0, 0xD5, 0xF0, 0xC0, 0x54, 0x88, 0x20],
            "lock_xaddb",
            7,
            MemWidth::B1,
            OpWidth::W8,
        ),
    ];

    for (bytes, name, bytes_consumed, mem_width, op_width) in cases {
        // LLVM 23 examples:
        //   `lock xaddq %r18, 32(%r16,%r17,4)` => f0 d5 f8 c1 54 88 20
        //   `lock xaddl %r18d, 32(%r16,%r17,4)` => f0 d5 f0 c1 54 88 20
        //   `lock xaddw %r18w, 32(%r16,%r17,4)` => 66 f0 d5 f0 c1 54 88 20
        //   `lock xaddb %r18b, 32(%r16,%r17,4)` => f0 d5 f0 c0 54 88 20
        let result = lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, *bytes_consumed, "{name}");
        let ops = assert_rex2_guarded_ops(&result, 4);

        let saved_src = match &ops[0].kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(src),
                width,
            } => {
                assert_eq!(*src, x86_gpr(18), "{name}");
                assert_eq!(*width, *op_width, "{name}");
                *dst
            }
            other => panic!("expected REX2 {name} source snapshot, got {other:?}"),
        };
        let old_dst = match &ops[1].kind {
            OpKind::AtomicRmw {
                dst,
                addr,
                src,
                op: AtomicOp::Add,
                width,
                order: MemoryOrder::SeqCst,
            } => {
                assert_rex2_xadd_sib_addr(addr, name);
                assert_eq!(*src, saved_src, "{name}");
                assert_eq!(*width, *mem_width, "{name}");
                *dst
            }
            other => panic!("expected REX2 {name} AtomicRmw Add, got {other:?}"),
        };
        match &ops[2].kind {
            OpKind::Add {
                dst: _,
                src1,
                src2: SrcOperand::Reg(src2),
                width,
                flags: FlagUpdate::All,
            } => {
                assert_eq!(*src1, old_dst, "{name}");
                assert_eq!(*src2, saved_src, "{name}");
                assert_eq!(*width, *op_width, "{name}");
            }
            other => panic!("expected REX2 {name} flag-producing add, got {other:?}"),
        }
        match &ops[3].kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(src),
                width,
            } => {
                assert_eq!(*dst, x86_gpr(18), "{name}");
                assert_eq!(*src, old_dst, "{name}");
                assert_eq!(*width, *op_width, "{name}");
            }
            other => panic!("expected REX2 {name} source writeback, got {other:?}"),
        }
    }
}
#[test]
fn lift_rex2_xchg_registers_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (bytes, name, reg1, reg2) in [
        (
            [0xD5, 0x58, 0x87, 0xC1],
            "xchg_r16_r17",
            x86_gpr(17),
            x86_gpr(16),
        ),
        (
            [0xD5, 0x5D, 0x87, 0xC7],
            "xchg_r24_r31",
            x86_gpr(31),
            x86_gpr(24),
        ),
    ] {
        let result = lifter.lift_insn(0x1000, &bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 4, "{name}");
        let ops = assert_rex2_guarded_ops(&result, 1);
        match &ops[0].kind {
            OpKind::Xchg {
                reg1: got_reg1,
                reg2: got_reg2,
                width: OpWidth::W64,
            } => {
                assert_eq!(*got_reg1, reg1, "{name}");
                assert_eq!(*got_reg2, reg2, "{name}");
            }
            other => panic!("expected REX2 {name} Xchg, got {other:?}"),
        }
    }
}
#[test]
fn lift_rex2_xchg_memory_uses_atomic_swap_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (bytes, name, mem_width, op_width) in [
        (
            [0xD5, 0x78, 0x87, 0x54, 0x88, 0x20],
            "xchgq",
            MemWidth::B8,
            OpWidth::W64,
        ),
        (
            [0xD5, 0x70, 0x86, 0x54, 0x88, 0x20],
            "xchgb",
            MemWidth::B1,
            OpWidth::W8,
        ),
    ] {
        // LLVM 23:
        //   `xchgq %r18, 32(%r16,%r17,4)` => d5 78 87 54 88 20
        //   `xchgb %r18b, 32(%r16,%r17,4)` => d5 70 86 54 88 20
        let result = lifter.lift_insn(0x1000, &bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 6, "{name}");
        let ops = assert_rex2_guarded_ops(&result, 2);

        let old_mem = match &ops[0].kind {
            OpKind::AtomicRmw {
                dst,
                addr:
                    Address::BaseIndexScale {
                        base: Some(base),
                        index,
                        scale: 4,
                        disp: 0x20,
                        disp_size: DispSize::Disp8,
                    },
                src,
                op: AtomicOp::Swap,
                width,
                order: MemoryOrder::SeqCst,
            } => {
                assert_eq!(*base, x86_gpr(16), "{name}");
                assert_eq!(*index, x86_gpr(17), "{name}");
                assert_eq!(*src, x86_gpr(18), "{name}");
                assert_eq!(*width, mem_width, "{name}");
                *dst
            }
            other => panic!("expected REX2 {name} AtomicRmw Swap, got {other:?}"),
        };

        match &ops[1].kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(src),
                width,
            } => {
                assert_eq!(*dst, x86_gpr(18), "{name}");
                assert_eq!(*src, old_mem, "{name}");
                assert_eq!(*width, op_width, "{name}");
            }
            other => panic!("expected REX2 {name} register writeback, got {other:?}"),
        }
    }
}
#[test]
fn lift_rex2_jmpabs_uses_llvm_encoding() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `jmpabs 0x1122334455667788` as REX2 + A1 imm64.
    let result = lifter
        .lift_insn(
            0x1000,
            &[
                0xD5, 0x00, 0xA1, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11,
            ],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(result.bytes_consumed, 11);
    assert_rex2_guarded_ops(&result, 0);
    match result.control_flow {
        ControlFlow::Branch {
            target: 0x1122_3344_5566_7788,
        } => {}
        other => panic!("expected JMPABS direct branch, got {other:?}"),
    }
}
#[test]
fn lift_rex2_jmpabs_ignores_w_bit_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20 also decodes REX2.W + A1 as JMPABS, not MOV rAX,moffs.
    let result = lifter
        .lift_insn(
            0x1000,
            &[
                0xD5, 0x08, 0xA1, 0x21, 0x43, 0x65, 0x87, 0x78, 0x56, 0x34, 0x12,
            ],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(result.bytes_consumed, 11);
    assert_rex2_guarded_ops(&result, 0);
    match result.control_flow {
        ControlFlow::Branch {
            target: 0x1234_5678_8765_4321,
        } => {}
        other => panic!("expected JMPABS direct branch, got {other:?}"),
    }
}
#[test]
fn lift_rex2_jmpabs_requires_imm64() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    let err = lifter
        .lift_insn(0x1000, &[0xD5, 0x00, 0xA1, 0x88, 0x77], &mut ctx)
        .unwrap_err();
    assert!(matches!(
        err,
        LiftError::Incomplete {
            addr: 0x1000,
            have: 2,
            need: 8
        }
    ));
}
#[test]
fn lift_apx_ndd_double_shifts_use_shld_shrd_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `shldq $4, %rbx, %rax, %r8` => 62 f4 bc 18 24 d8 04.
    let shld = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0xF4, 0xBC, 0x18, 0x24, 0xD8, 0x04],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(shld.bytes_consumed, 7);
    assert_eq!(shld.ops.len(), 1);
    match &shld.ops[0].kind {
        OpKind::X86NddDoubleShift {
            dst,
            base,
            fill,
            amount: SrcOperand::Imm(4),
            width: OpWidth::W64,
            left: true,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*base, x86_gpr(0));
            assert_eq!(*fill, x86_gpr(3));
        }
        other => panic!("expected APX NDD shld, got {other:?}"),
    }

    // LLVM 20: `shrdq %cl, %rbx, %rax, %r8` => 62 f4 bc 18 ad d8.
    let shrd = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xBC, 0x18, 0xAD, 0xD8], &mut ctx)
        .unwrap();
    assert_eq!(shrd.bytes_consumed, 6);
    assert_eq!(shrd.ops.len(), 1);
    match &shrd.ops[0].kind {
        OpKind::X86NddDoubleShift {
            dst,
            base,
            fill,
            amount: SrcOperand::Reg(amount),
            width: OpWidth::W64,
            left: false,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*base, x86_gpr(0));
            assert_eq!(*fill, x86_gpr(3));
            assert_eq!(*amount, x86_gpr(1));
        }
        other => panic!("expected APX NDD shrd, got {other:?}"),
    }

    // LLVM 20: `{nf} shldq $4, %rbx, %rax, %r8` => 62 f4 bc 1c 24 d8 04.
    let nf = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0xF4, 0xBC, 0x1C, 0x24, 0xD8, 0x04],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(nf.bytes_consumed, 7);
    assert_eq!(nf.ops.len(), 1);
    match &nf.ops[0].kind {
        OpKind::X86NddDoubleShift {
            dst,
            base,
            fill,
            amount: SrcOperand::Imm(4),
            width: OpWidth::W64,
            left: true,
            flags: FlagUpdate::None,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*base, x86_gpr(0));
            assert_eq!(*fill, x86_gpr(3));
        }
        other => panic!("expected APX NF NDD shld, got {other:?}"),
    }
}
#[test]
fn lift_apx_ndd_double_shift_aliases_use_one_direct_smir_op() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `shldq $4, %rbx, %rax, %rbx` => 62 f4 e4 18 24 d8 04.
    let src_alias = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0xF4, 0xE4, 0x18, 0x24, 0xD8, 0x04],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(src_alias.bytes_consumed, 7);
    assert_eq!(src_alias.ops.len(), 1);
    match &src_alias.ops[0].kind {
        OpKind::X86NddDoubleShift {
            dst,
            base,
            fill,
            amount: SrcOperand::Imm(4),
            width: OpWidth::W64,
            left: true,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(3));
            assert_eq!(*base, x86_gpr(0));
            assert_eq!(*fill, x86_gpr(3));
        }
        other => panic!("expected direct APX NDD shld alias, got {other:?}"),
    }

    // LLVM 20: `shrdq %cl, %rbx, %rax, %rcx` => 62 f4 f4 18 ad d8.
    let cl_alias = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xF4, 0x18, 0xAD, 0xD8], &mut ctx)
        .unwrap();
    assert_eq!(cl_alias.bytes_consumed, 6);
    assert_eq!(cl_alias.ops.len(), 1);
    match &cl_alias.ops[0].kind {
        OpKind::X86NddDoubleShift {
            dst,
            base,
            fill,
            amount: SrcOperand::Reg(amount),
            width: OpWidth::W64,
            left: false,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(1));
            assert_eq!(*base, x86_gpr(0));
            assert_eq!(*fill, x86_gpr(3));
            assert_eq!(*amount, x86_gpr(1));
        }
        other => panic!("expected direct APX NDD shrd CL alias, got {other:?}"),
    }
}
#[test]
fn lift_apx_ndd_nf_imul_reg_uses_muls_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (bytes, name, flags) in [
        ([0x62, 0xF4, 0xBC, 0x18, 0xAF, 0xC3], "ndd", FlagUpdate::All),
        (
            [0x62, 0xF4, 0xBC, 0x1C, 0xAF, 0xC3],
            "ndd_nf",
            FlagUpdate::None,
        ),
    ] {
        let result = lifter.lift_insn(0x1000, &bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 6, "{name}");
        assert_eq!(result.ops.len(), 1, "{name}");
        match &result.ops[0].kind {
            OpKind::MulS {
                dst_lo,
                dst_hi: None,
                src1,
                src2: SrcOperand::Reg(src2),
                width: OpWidth::W64,
                flags: got_flags,
            } => {
                assert_eq!(*dst_lo, x86_gpr(8), "{name}");
                assert_eq!(*src1, x86_gpr(0), "{name}");
                assert_eq!(*src2, x86_gpr(3), "{name}");
                assert_eq!(*got_flags, flags, "{name}");
            }
            other => panic!("expected APX {name} IMUL MulS, got {other:?}"),
        }
    }

    // LLVM 20: `{nf} imulq %rbx, %rax` => 62 f4 fc 0c af c3.
    let nf = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xFC, 0x0C, 0xAF, 0xC3], &mut ctx)
        .unwrap();
    assert_eq!(nf.bytes_consumed, 6);
    assert_eq!(nf.ops.len(), 1);
    match &nf.ops[0].kind {
        OpKind::MulS {
            dst_lo,
            dst_hi: None,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } => {
            assert_eq!(*dst_lo, x86_gpr(0));
            assert_eq!(*src1, x86_gpr(0));
            assert_eq!(*src2, x86_gpr(3));
        }
        other => panic!("expected APX NF IMUL MulS, got {other:?}"),
    }
}
#[test]
fn lift_apx_ndd_imul_alias_uses_direct_architectural_sources() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `imulq %rbx, %rax, %rbx` => 62 f4 e4 18 af c3.
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xE4, 0x18, 0xAF, 0xC3], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::MulS {
            dst_lo,
            dst_hi: None,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst_lo, x86_gpr(3));
            assert_eq!(*src1, x86_gpr(0));
            assert_eq!(*src2, x86_gpr(3));
        }
        other => panic!("expected direct APX NDD IMUL alias, got {other:?}"),
    }

    let nf = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xE4, 0x1C, 0xAF, 0xC3], &mut ctx)
        .unwrap();
    assert_eq!(nf.ops.len(), 1);
    assert!(matches!(
        nf.ops[0].kind,
        OpKind::MulS {
            dst_lo,
            dst_hi: None,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if dst_lo == x86_gpr(3) && src1 == x86_gpr(0) && src2 == x86_gpr(3)
    ));
}
#[test]
fn lift_apx_setzucc_registers_zero_full_gpr_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (bytes, name, dst, cond) in [
        (
            [0x62, 0xF4, 0x7F, 0x18, 0x40, 0xC0],
            "setzuo_al",
            x86_gpr(0),
            Condition::Overflow,
        ),
        (
            [0x62, 0xF4, 0x7F, 0x18, 0x45, 0xC3],
            "setzune_bl",
            x86_gpr(3),
            Condition::Ne,
        ),
        (
            [0x62, 0xD4, 0x7F, 0x18, 0x40, 0xC0],
            "setzuo_r8b",
            x86_gpr(8),
            Condition::Overflow,
        ),
    ] {
        let result = lifter.lift_insn(0x1000, &bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 6, "{name}");
        assert_eq!(result.ops.len(), 1, "{name}");
        match &result.ops[0].kind {
            OpKind::SetCC {
                dst: got_dst,
                cond: got_cond,
                width: OpWidth::W64,
            } => {
                assert_eq!(*got_dst, dst, "{name}");
                assert_eq!(*got_cond, cond, "{name}");
            }
            other => panic!("expected APX {name} as full-width SetCC, got {other:?}"),
        }
    }
}
#[test]
fn lift_apx_setzucc_memory_stores_one_byte_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `setzuo (%rax)` => 62 f4 7f 18 40 00.
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0x7F, 0x18, 0x40, 0x00], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_eq!(result.ops.len(), 2);
    let tmp = match &result.ops[0].kind {
        OpKind::SetCC {
            dst,
            cond: Condition::Overflow,
            width: OpWidth::W8,
        } => {
            assert!(matches!(dst, VReg::Virtual(_)));
            *dst
        }
        other => panic!("expected APX SETZUcc temp byte SetCC, got {other:?}"),
    };
    match &result.ops[1].kind {
        OpKind::Store {
            src,
            addr: Address::Direct(base),
            width: MemWidth::B1,
        } => {
            assert_eq!(*src, tmp);
            assert_eq!(*base, x86_gpr(0));
        }
        other => panic!("expected APX SETZUcc byte store, got {other:?}"),
    }
}
#[test]
fn lift_apx_cmov_nd_uses_vvvv_destination_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `cmovbq %rbx, %rax, %r8` => 62 f4 bc 18 42 c3.
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xBC, 0x18, 0x42, 0xC3], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_eq!(result.ops.len(), 2);
    let cond = match &result.ops[0].kind {
        OpKind::SetCC {
            dst,
            cond: Condition::Ult,
            width: OpWidth::W8,
        } => *dst,
        other => panic!("expected CMOV_ND condition, got {other:?}"),
    };
    match &result.ops[1].kind {
        OpKind::Select {
            dst,
            cond: got_cond,
            src_true,
            src_false,
            width: OpWidth::W64,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*got_cond, cond);
            assert_eq!(*src_true, x86_gpr(3));
            assert_eq!(*src_false, x86_gpr(0));
        }
        other => panic!("expected CMOV_ND Select, got {other:?}"),
    }
}
#[test]
fn lift_apx_cfcmov_two_operand_directions_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `cfcmovbq %rbx, %rax` => 62 f4 fc 0c 42 d8.
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xFC, 0x0C, 0x42, 0xD8], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_eq!(result.ops.len(), 3);
    let cond = match &result.ops[0].kind {
        OpKind::SetCC {
            dst,
            cond: Condition::Ult,
            width: OpWidth::W8,
        } => *dst,
        other => panic!("expected CFCMOV condition, got {other:?}"),
    };
    let zero = match &result.ops[1].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => *dst,
        other => panic!("expected CFCMOV false zero temp, got {other:?}"),
    };
    match &result.ops[2].kind {
        OpKind::Select {
            dst,
            cond: got_cond,
            src_true,
            src_false,
            width: OpWidth::W64,
        } => {
            assert_eq!(*dst, x86_gpr(0));
            assert_eq!(*got_cond, cond);
            assert_eq!(*src_true, x86_gpr(3));
            assert_eq!(*src_false, zero);
        }
        other => panic!("expected CFCMOV reg-destination Select, got {other:?}"),
    }

    // LLVM also decodes clear NF with PP=0 as the opposite reg-reg direction:
    // `cfcmovbq %rax, %rbx` from 62 f4 fc 08 42 d8.
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xFC, 0x08, 0x42, 0xD8], &mut ctx)
        .unwrap();
    match &result.ops[2].kind {
        OpKind::Select {
            dst,
            src_true,
            width: OpWidth::W64,
            ..
        } => {
            assert_eq!(*dst, x86_gpr(3));
            assert_eq!(*src_true, x86_gpr(0));
        }
        other => panic!("expected opposite CFCMOV direction, got {other:?}"),
    }
}
#[test]
fn lift_apx_cfcmov_memory_uses_predicated_access_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `cfcmovbq (%rbx), %rax` => 62 f4 fc 08 42 03.
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xFC, 0x08, 0x42, 0x03], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_eq!(result.ops.len(), 4);
    let cond = match &result.ops[1].kind {
        OpKind::SetCC {
            dst,
            cond: Condition::Ult,
            width: OpWidth::W8,
        } => *dst,
        other => panic!("expected CFCMOVrm condition, got {other:?}"),
    };
    let loaded = match &result.ops[2].kind {
        OpKind::PredLoad {
            dst,
            cond: got_cond,
            addr: Address::Direct(base),
            width: MemWidth::B8,
            signed: SignExtend::Zero,
        } => {
            assert_eq!(*got_cond, cond);
            assert_eq!(*base, x86_gpr(3));
            *dst
        }
        other => panic!("expected CFCMOVrm PredLoad, got {other:?}"),
    };
    match &result.ops[3].kind {
        OpKind::Select {
            dst,
            src_true,
            width: OpWidth::W64,
            ..
        } => {
            assert_eq!(*dst, x86_gpr(0));
            assert_eq!(*src_true, loaded);
        }
        other => panic!("expected CFCMOVrm final Select, got {other:?}"),
    }

    // LLVM 20: `cfcmovbq %rbx, (%rax)` => 62 f4 fc 0c 42 18.
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xFC, 0x0C, 0x42, 0x18], &mut ctx)
        .unwrap();
    assert_eq!(result.ops.len(), 2);
    let cond = match &result.ops[0].kind {
        OpKind::SetCC {
            dst,
            cond: Condition::Ult,
            width: OpWidth::W8,
        } => *dst,
        other => panic!("expected CFCMOVmr condition, got {other:?}"),
    };
    match &result.ops[1].kind {
        OpKind::PredStore {
            src: SrcOperand::Reg(src),
            cond: got_cond,
            addr: Address::Direct(base),
            width: MemWidth::B8,
        } => {
            assert_eq!(*src, x86_gpr(3));
            assert_eq!(*got_cond, cond);
            assert_eq!(*base, x86_gpr(0));
        }
        other => panic!("expected CFCMOVmr PredStore, got {other:?}"),
    }
}
#[test]
fn lift_apx_cfcmov_nd_memory_source_suppresses_false_fault_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `cfcmovbq (%rbx), %rax, %r8` => 62 f4 bc 1c 42 03.
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xBC, 0x1C, 0x42, 0x03], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_eq!(result.ops.len(), 3);
    let cond = match &result.ops[0].kind {
        OpKind::SetCC {
            dst,
            cond: Condition::Ult,
            width: OpWidth::W8,
        } => *dst,
        other => panic!("expected CFCMOV_ND condition, got {other:?}"),
    };
    let loaded = match &result.ops[1].kind {
        OpKind::PredLoad {
            dst,
            cond: got_cond,
            addr: Address::Direct(base),
            width: MemWidth::B8,
            signed: SignExtend::Zero,
        } => {
            assert_eq!(*got_cond, cond);
            assert_eq!(*base, x86_gpr(3));
            *dst
        }
        other => panic!("expected CFCMOV_ND PredLoad, got {other:?}"),
    };
    match &result.ops[2].kind {
        OpKind::Select {
            dst,
            cond: got_cond,
            src_true,
            src_false,
            width: OpWidth::W64,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*got_cond, cond);
            assert_eq!(*src_true, loaded);
            assert_eq!(*src_false, x86_gpr(0));
        }
        other => panic!("expected CFCMOV_ND final Select, got {other:?}"),
    }
}
#[test]
fn lift_apx_conditional_map4_rejects_invalid_pp2_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    let err = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0x7E, 0x18, 0x42, 0xC0], &mut ctx)
        .unwrap_err();
    assert!(matches!(err, LiftError::InvalidEncoding { .. }), "{err:?}");
}
