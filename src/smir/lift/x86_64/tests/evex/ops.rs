//! evex::ops tests

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

#[test]
fn evex_decorations_are_validated_by_opcode_lifters() {
    // A valid decorated AVX512ER form reaches the opcode-specific lifter
    // instead of a decoder-global decoration gate.
    assert!(matches!(
        lift_single(&[0x62, 0xF2, 0x7D, 0x49, 0xCA, 0xC8]),
        Ok(LiftResult { ops, .. }) if ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86Recip28 {
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                ..
            }
        ))
    ));

    // Shared VEX/EVEX dispatch must remain fail-closed when the EVEX form
    // reserves masking, and a VEX-only family must reject EVEX outright.
    assert!(matches!(
        lift_single(&[0x62, 0xF3, 0x7D, 0x09, 0x44, 0xC0, 0x00]),
        Err(LiftError::InvalidEncoding { .. })
    ));
    assert!(matches!(
        lift_single(&[0x62, 0xF2, 0x7F, 0x29, 0xCB, 0xC0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}
#[test]
fn lift_cmpccxadd_evex_egpr_memory_like_llvm() {
    let result = lift_single(&[0x62, 0xEA, 0x61, 0x00, 0xE2, 0x44, 0x91, 0x20]).unwrap();
    assert_eq!(result.bytes_consumed, 8);
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::AtomicCmpXadd {
            dst_old,
            addr:
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x20,
                    ..
                },
            cmp,
            add,
            cond: Condition::Ult,
            width: MemWidth::B4,
            order: MemoryOrder::SeqCst,
        } => {
            assert_eq!(*dst_old, x86_gpr(16));
            assert_eq!(*cmp, x86_gpr(16));
            assert_eq!(*add, x86_gpr(19));
            assert_eq!(*base, x86_gpr(17));
            assert_eq!(*index, x86_gpr(18));
        }
        other => panic!("expected EVEX CMPccXADD AtomicCmpXadd, got {other:?}"),
    }

    let result = lift_single(&[0x62, 0xEA, 0x65, 0x08, 0xE2, 0x08]).unwrap();
    match &result.ops[0].kind {
        OpKind::AtomicCmpXadd {
            dst_old,
            addr: Address::Direct(base),
            cmp,
            add,
            width: MemWidth::B4,
            ..
        } => {
            assert_eq!(*dst_old, x86_gpr(17));
            assert_eq!(*cmp, x86_gpr(17));
            assert_eq!(*base, x86_gpr(16));
            assert_eq!(*add, x86_gpr(3));
        }
        other => panic!("expected EVEX CMPccXADD with legacy addend, got {other:?}"),
    }
}
#[test]
fn lift_apx_imul_immediates_use_evex_destination_and_flags() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `{nf} imulq $7, %rax, %r8` => 62 74 fc 0c 6b c0 07.
    let nf_imm8 = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0x74, 0xFC, 0x0C, 0x6B, 0xC0, 0x07],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(nf_imm8.bytes_consumed, 7);
    assert_eq!(nf_imm8.ops.len(), 1);
    match &nf_imm8.ops[0].kind {
        OpKind::MulS {
            dst_lo,
            dst_hi: None,
            src1,
            src2: SrcOperand::Imm(7),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } => {
            assert_eq!(*dst_lo, x86_gpr(8));
            assert_eq!(*src1, x86_gpr(0));
        }
        other => panic!("expected APX NF IMUL imm8 MulS, got {other:?}"),
    }

    // LLVM 20: `{nf} imulq $0x12345678, %rax, %r8`.
    let nf_imm32 = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0x74, 0xFC, 0x0C, 0x69, 0xC0, 0x78, 0x56, 0x34, 0x12],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(nf_imm32.bytes_consumed, 10);
    match &nf_imm32.ops[0].kind {
        OpKind::MulS {
            dst_lo,
            dst_hi: None,
            src1,
            src2: SrcOperand::Imm(0x1234_5678),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } => {
            assert_eq!(*dst_lo, x86_gpr(8));
            assert_eq!(*src1, x86_gpr(0));
        }
        other => panic!("expected APX NF IMUL imm32 MulS, got {other:?}"),
    }

    // LLVM 23: `{nf} imulw $0x1234, %r14w, %r13w`. Opcode 69 carries an
    // imm16 at word width; consuming four bytes would swallow the next
    // instruction and misdecode the signed multiplier.
    let nf_imm16 = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0x54, 0x7D, 0x0C, 0x69, 0xEE, 0x34, 0x12],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(nf_imm16.bytes_consumed, 8);
    assert_eq!(nf_imm16.ops.len(), 1);
    assert_eq!(nf_imm16.ops[0].x86_hint, Some(X86OpHint::ImulImm32));
    match &nf_imm16.ops[0].kind {
        OpKind::MulS {
            dst_lo,
            dst_hi: None,
            src1,
            src2: SrcOperand::Imm(0x1234),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        } => {
            assert_eq!(*dst_lo, x86_gpr(13));
            assert_eq!(*src1, x86_gpr(14));
        }
        other => panic!("expected APX NF IMUL imm16 MulS, got {other:?}"),
    }

    let nf_imm16_negative = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0x54, 0x7D, 0x0C, 0x69, 0xEE, 0xFE, 0xFF],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(nf_imm16_negative.bytes_consumed, 8);
    assert!(matches!(
        nf_imm16_negative.ops[0].kind,
        OpKind::MulS {
            src2: SrcOperand::Imm(-2),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
            ..
        }
    ));

    // APX NDD immediate form uses vvvv as the destination. LLVM prefers the
    // non-NDD EVEX encoding for this syntax because legacy IMUL already has
    // an independent immediate destination.
    let ndd_imm8 = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0xF4, 0xBC, 0x18, 0x6B, 0xC0, 0xF9],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(ndd_imm8.bytes_consumed, 7);
    match &ndd_imm8.ops[0].kind {
        OpKind::MulS {
            dst_lo,
            dst_hi: None,
            src1,
            src2: SrcOperand::Imm(-7),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst_lo, x86_gpr(8));
            assert_eq!(*src1, x86_gpr(0));
        }
        other => panic!("expected APX NDD IMUL imm8 MulS, got {other:?}"),
    }
}
#[test]
fn lift_apx_movrs_evex_memory_egpr_widths_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (bytes, name, width) in [
        (
            &[0x62, 0xEC, 0xF8, 0x08, 0x8B, 0x44, 0x91, 0x20][..],
            "movrs64",
            MemWidth::B8,
        ),
        (
            &[0x62, 0xEC, 0x78, 0x08, 0x8B, 0x44, 0x91, 0x20][..],
            "movrs32",
            MemWidth::B4,
        ),
        (
            &[0x62, 0xEC, 0x79, 0x08, 0x8B, 0x44, 0x91, 0x20][..],
            "movrs16",
            MemWidth::B2,
        ),
        (
            &[0x62, 0xEC, 0x78, 0x08, 0x8A, 0x44, 0x91, 0x20][..],
            "movrs8",
            MemWidth::B1,
        ),
        (
            &[0x62, 0xEC, 0xF8, 0x09, 0x8B, 0x44, 0x91, 0x20][..],
            "movrs64_aaa1",
            MemWidth::B8,
        ),
    ] {
        let result = lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 8, "{name}");
        assert_eq!(result.ops.len(), 1, "{name}");
        match &result.ops[0].kind {
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
                width: got_width,
                sign: SignExtend::Zero,
            } => {
                assert_eq!(*dst, x86_gpr(16), "{name}");
                assert_eq!(*base, x86_gpr(17), "{name}");
                assert_eq!(*index, x86_gpr(18), "{name}");
                assert_eq!(*got_width, width, "{name}");
            }
            other => panic!("expected APX EVEX {name} Load, got {other:?}"),
        }
    }
}
#[test]
fn lift_apx_evex_setcc_without_zu_keeps_byte_width_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `{evex} setb %al` => 62 f4 7f 08 42 c0.
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0x7F, 0x08, 0x42, 0xC0], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::SetCC {
            dst,
            cond: Condition::Ult,
            width: OpWidth::W8,
        } => assert_eq!(*dst, x86_gpr(0)),
        other => panic!("expected EVEX SETcc byte register write, got {other:?}"),
    }

    // LLVM 20: `{evex} setb (%rax)` => 62 f4 7f 08 42 00.
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0x7F, 0x08, 0x42, 0x00], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_eq!(result.ops.len(), 2);
    let tmp = match &result.ops[0].kind {
        OpKind::SetCC {
            dst,
            cond: Condition::Ult,
            width: OpWidth::W8,
        } => *dst,
        other => panic!("expected EVEX SETcc byte temp, got {other:?}"),
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
        other => panic!("expected EVEX SETcc byte store, got {other:?}"),
    }
}
#[test]
fn lift_evex_vmovw_covers_directions_wig_extensions_memory_and_invalids() {
    for bytes in [
        &[0x62, 0xC5, 0x7D, 0x08, 0x6E, 0xC8][..],
        &[0x62, 0xC5, 0xFD, 0x08, 0x6E, 0xC8][..],
    ] {
        let load = lift_single(bytes).unwrap();
        assert_eq!(load.bytes_consumed, bytes.len());
        assert!(load.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                scalar: VReg::Arch(ArchReg::X86(X86Reg::R8)),
                lane: 0,
                elem: VecElementType::I16,
                ..
            }
        )));
    }

    let store = lift_single(&[0x62, 0xC5, 0x7D, 0x08, 0x7E, 0xC8]).unwrap();
    assert!(store.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            lane: 0,
            elem: VecElementType::I16,
            sign: SignExtend::Zero,
            ..
        }
    )));
    assert!(matches!(
        store.ops.last().unwrap().kind,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::R8)),
            src: SrcOperand::Reg(_),
            width: OpWidth::W32,
        }
    ));

    let load_memory = lift_single(&[0x62, 0xF5, 0x7D, 0x08, 0x6E, 0x48, 0x7F]).unwrap();
    assert!(load_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 254, .. },
            width: MemWidth::B2,
            ..
        }
    )));
    assert!(load_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            elem: VecElementType::I16,
            ..
        }
    )));

    let store_memory = lift_single(&[0x62, 0xF5, 0x7D, 0x08, 0x7E, 0x48, 0x7F]).unwrap();
    assert!(matches!(
        store_memory.ops.last().unwrap().kind,
        OpKind::Store {
            addr: Address::BaseOffset { offset: 254, .. },
            width: MemWidth::B2,
            ..
        }
    ));

    for invalid in [
        &[0x62, 0xF5, 0x7C, 0x08, 0x6E, 0xC1][..], // pp != 66
        &[0x62, 0xF5, 0x7D, 0x28, 0x6E, 0xC1][..], // L'L != 00b
        &[0x62, 0xF5, 0x75, 0x08, 0x6E, 0xC1][..], // reserved vvvv
        &[0x62, 0xF5, 0x7D, 0x00, 0x6E, 0xC1][..], // reserved V'
        &[0x62, 0xF5, 0x7D, 0x09, 0x6E, 0xC1][..], // reserved opmask
        &[0x62, 0xF5, 0x7D, 0x88, 0x6E, 0xC1][..], // reserved zeroing
        &[0x62, 0xF5, 0x7D, 0x18, 0x6E, 0xC1][..], // reserved EVEX.b
        &[0x62, 0xB5, 0x7D, 0x08, 0x6E, 0xC1][..], // no GPR bit 4
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_ternary_logic_covers_widths_high_regs_e4_memory_and_invalids() {
    for (bytes, elem, width, imm) in [
        (
            &[0x62, 0xF3, 0x6D, 0x08, 0x25, 0xCB, 0x96][..],
            VecElementType::I32,
            VecWidth::V128,
            0x96,
        ),
        (
            &[0x62, 0xA3, 0xD5, 0xA3, 0x25, 0xE6, 0xE2][..],
            VecElementType::I64,
            VecWidth::V256,
            0xE2,
        ),
        (
            &[0x62, 0xC3, 0x6D, 0x57, 0x25, 0x4D, 0x7F, 0xE4][..],
            VecElementType::I32,
            VecWidth::V512,
            0xE4,
        ),
        (
            &[0x62, 0x63, 0x8D, 0xC1, 0x25, 0x78, 0x01, 0xCA][..],
            VecElementType::I64,
            VecWidth::V512,
            0xCA,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86TernaryLogic {
                width: actual_width,
                imm: actual_imm,
                elem: actual_elem,
                mask,
                zeroing,
                ..
            } if actual_width == width
                && actual_imm == imm
                && actual_elem == elem
                && mask.is_some() == (bytes[3] & 7 != 0)
                && zeroing == (bytes[3] & 0x80 != 0)
        )));
    }

    let register = lift_single(&[0x62, 0xF3, 0x6D, 0x08, 0x25, 0xCB, 0x96]).unwrap();
    assert!(register.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86TernaryLogic {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            src3: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
            ..
        }
    )));

    let broadcast = lift_single(&[0x62, 0xC3, 0x6D, 0x57, 0x25, 0x4D, 0x7F, 0xE4]).unwrap();
    assert_eq!(
        broadcast
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
        16
    );
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 508, .. },
            ..
        }
    )));

    let full = lift_single(&[0x62, 0x63, 0x8D, 0xC1, 0x25, 0x78, 0x01, 0xCA]).unwrap();
    assert_eq!(
        full.ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        8
    );
    assert!(full.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 64, .. },
            ..
        }
    )));

    for bytes in [
        &[0xC4, 0xE3, 0x6D, 0x25, 0xCB, 0x96][..], // EVEX-only
        &[0x62, 0xF3, 0x6C, 0x08, 0x25, 0xCB, 0x96][..], // mandatory 66 absent
        &[0x62, 0xF3, 0x6D, 0x68, 0x25, 0xCB, 0x96][..], // L'L=3
        &[0x62, 0xF3, 0x6D, 0x88, 0x25, 0xCB, 0x96][..], // {z} with k0
        &[0x62, 0xF3, 0x6D, 0x18, 0x25, 0xCB, 0x96][..], // EVEX.b on register
        &[0x62, 0xF3, 0x6D, 0x08, 0x25, 0xCB][..], // missing imm8
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "accepted reserved ternary-logic encoding {bytes:02X?}"
        );
    }
}
#[test]
fn lift_evex_sparse_prefetch_covers_all_groups_types_and_reserved_encodings() {
    for opcode in [0xC6u8, 0xC7] {
        for group in [1u8, 2, 5, 6] {
            for w in [false, true] {
                let bytes = [
                    0x62,
                    0xF2,
                    if w { 0xFD } else { 0x7D },
                    0x49,
                    opcode,
                    group << 3 | 4,
                    0x80,
                ];
                let lifted = lift_single(&bytes).unwrap_or_else(|error| {
                    panic!("rejected sparse-prefetch encoding {bytes:02X?}: {error:?}")
                });
                assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
                assert!(lifted.ops.is_empty(), "{bytes:02X?}");
                assert!(matches!(lifted.control_flow, ControlFlow::Fallthrough));
            }
        }
    }

    let address32 = lift_single(&[0x67, 0x62, 0xF2, 0x7D, 0x49, 0xC6, 0x4C, 0x80, 0x7F]).unwrap();
    assert_eq!(address32.bytes_consumed, 9);
    assert!(address32.ops.is_empty());

    for bytes in [
        &[0xC4, 0xE2, 0x7D, 0xC6, 0x0C, 0x80][..], // EVEX-only
        &[0x62, 0xFA, 0x7D, 0x49, 0xC6, 0x0C, 0x80][..], // EVEX fixed-zero absent
        &[0x62, 0xF2, 0x7C, 0x49, 0xC6, 0x0C, 0x80][..], // mandatory 66 absent
        &[0x62, 0xF2, 0x79, 0x49, 0xC6, 0x0C, 0x80][..], // EVEX fixed-one absent
        &[0x62, 0xF2, 0x7D, 0x09, 0xC6, 0x0C, 0x80][..], // L'L != 512
        &[0x62, 0xF2, 0x7D, 0x69, 0xC6, 0x0C, 0x80][..], // L'L=3
        &[0x62, 0xF2, 0x75, 0x49, 0xC6, 0x0C, 0x80][..], // EVEX.vvvv reserved
        &[0x62, 0xF2, 0x7D, 0x41, 0xC6, 0x0C, 0x80][..], // EVEX.V' reserved
        &[0x62, 0xF2, 0x7D, 0x48, 0xC6, 0x0C, 0x80][..], // k0 reserved
        &[0x62, 0xF2, 0x7D, 0xC9, 0xC6, 0x0C, 0x80][..], // EVEX.z reserved
        &[0x62, 0xF2, 0x7D, 0x59, 0xC6, 0x0C, 0x80][..], // EVEX.b reserved
        &[0x62, 0xF2, 0x7D, 0x49, 0xC6, 0xCC][..], // register operand
        &[0x62, 0xF2, 0x7D, 0x49, 0xC6, 0x08][..], // memory without SIB
        &[0x62, 0xF2, 0x7D, 0x49, 0xC6, 0x04, 0x80][..], // invalid /0 group
        &[0x62, 0xF2, 0x7D, 0x49, 0xC6, 0x1C, 0x80][..], // invalid /3 group
        &[0x62, 0xF2, 0x7D, 0x49, 0xC6, 0x24, 0x80][..], // invalid /4 group
        &[0x62, 0xF2, 0x7D, 0x49, 0xC6, 0x3C, 0x80][..], // invalid /7 group
        &[0x62, 0xF2, 0x7D, 0x49, 0xC6, 0x0C][..], // missing SIB
        &[0x62, 0xF2, 0x7D, 0x49, 0xC6, 0x4C, 0x80][..], // missing disp8
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "accepted reserved sparse-prefetch encoding {bytes:02X?}"
        );
    }
}
#[test]
fn lift_evex_pair_intersect_covers_elements_pairing_e4nf_memory_and_invalids() {
    for (bytes, low, high) in [
        (&[0x62, 0xF2, 0x67, 0x08, 0x68, 0xD4][..], 2, 3),
        (&[0x62, 0xB2, 0xDF, 0x20, 0x68, 0xE5][..], 4, 5),
        (&[0x62, 0xB2, 0x77, 0x40, 0x68, 0xF2][..], 6, 7),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(matches!(
            &lifted.ops[lifted.ops.len() - 2..],
            [
                SmirOp { kind: OpKind::Mov { dst: VReg::Arch(ArchReg::X86(X86Reg::K(a))), .. }, .. },
                SmirOp { kind: OpKind::Mov { dst: VReg::Arch(ArchReg::X86(X86Reg::K(b))), .. }, .. }
            ] if *a == low && *b == high
        ));
    }
    let memory = lift_single(&[0x62, 0xF2, 0x77, 0x50, 0x68, 0x70, 0x7F]).unwrap();
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            width: MemWidth::B4,
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
        &[0xC4, 0xE2, 0x67, 0x68, 0xD4][..],       // EVEX-only
        &[0x62, 0xF2, 0x65, 0x08, 0x68, 0xD4][..], // mandatory F2 absent
        &[0x62, 0xF2, 0x67, 0x09, 0x68, 0xD4][..], // aaa reserved
        &[0x62, 0xF2, 0x67, 0x88, 0x68, 0xD4][..], // z reserved
        &[0x62, 0xF2, 0x67, 0x18, 0x68, 0xD4][..], // broadcast on register
        &[0x62, 0xE2, 0x67, 0x08, 0x68, 0xD4][..], // extended K destination
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_function_retains_exact_evex_replay_provenance_through_optimization() {
    const VADDPS: [u8; 6] = [0x62, 0xF1, 0x6C, 0xC9, 0x58, 0xCB];
    let mut bytes = VADDPS.to_vec(); // vaddps zmm1{k1}{z}, zmm2, zmm3
    bytes.push(0xC3); // interpreter frontier
    let mem = TestMemory::new(0x1800, bytes);
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let mut function = lifter.lift_function(0x1800, &mem, &mut ctx).unwrap();

    let entry_id = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1800)
        .unwrap()
        .id;
    assert_eq!(
        function
            .x86_instruction_bytes
            .get(&(entry_id, 0x1800))
            .map(X86InstructionBytes::as_slice),
        Some(VADDPS.as_slice())
    );
    assert!(
        !function
            .x86_instruction_bytes
            .contains_key(&(entry_id, 0x1806)),
        "frontier instruction must not be attached to the executable block"
    );

    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
    let entry = function.get_block(entry_id).unwrap();
    let spans = crate::smir::ir::x86_evex_fp_replay_spans(entry, &function.x86_instruction_bytes);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans.get(&0).unwrap().instruction.as_slice(), VADDPS);
}
