//! Intel APX Group 3 and INC/DEC lifting tests.

use super::*;
use crate::smir::lift::x86_64::*;

fn apx_nf_prefix(nd: bool, w: bool, pp: u8) -> [u8; 4] {
    let p1 = (if nd { 0x3C } else { 0x7C }) | (if w { 0x80 } else { 0 }) | pp;
    let p2 = 0x0C | if nd { 0x10 } else { 0 };
    [0x62, 0xF4, p1, p2]
}

fn assert_apx_group3_ud(bytes: &[u8], expected_len: usize) {
    let result = lift_single(bytes).unwrap_or_else(|error| {
        panic!("reserved APX Group 3 encoding must strictly lift to #UD: {bytes:02X?}: {error:?}")
    });
    assert_invalid_opcode_trap(&result, expected_len);
}

#[test]
fn every_apx_nf_not_addressing_class_traps_at_modrm() {
    // Intel APX Architecture Specification revision 7.0 specifies {NF=0}
    // for NOT. F6/F7 /2 identifies the reserved form at ModR/M, before any
    // SIB, displacement, or memory operand. Cover both ND and W states, every
    // applicable pp class, and every Mod/RM addressing cell.
    for nd in [false, true] {
        for w in [false, true] {
            for opcode in [0xF6, 0xF7] {
                let valid_pp = if opcode == 0xF6 {
                    &[0][..]
                } else {
                    &[0, 1][..]
                };
                for &pp in valid_pp {
                    let mut opcode_only = apx_nf_prefix(nd, w, pp).to_vec();
                    opcode_only.push(opcode);
                    let error = lift_single(&opcode_only).unwrap_err();
                    assert!(
                        matches!(
                            error,
                            LiftError::Incomplete {
                                have: 5,
                                need: 6,
                                ..
                            }
                        ),
                        "opcode={opcode:02X} ND={nd} W={w} pp={pp}: {error:?}"
                    );

                    for mode in 0..=3 {
                        for rm in 0..=7 {
                            let mut bytes = opcode_only.clone();
                            bytes.push((mode << 6) | (2 << 3) | rm);
                            assert_apx_group3_ud(&bytes, 6);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn apx_nf_not_frontier_includes_legal_address_and_segment_overrides() {
    for bytes in [
        &[0x67, 0x62, 0xF4, 0xBC, 0x1C, 0xF7, 0x14][..],
        &[0x65, 0x62, 0xF4, 0xBC, 0x1C, 0xF7, 0x15],
    ] {
        assert_apx_group3_ud(bytes, 7);
    }
}

#[test]
fn neighboring_apx_nf_neg_and_non_nf_not_remain_liftable() {
    let neg = lift_single(&[0x62, 0xF4, 0xFC, 0x0C, 0xF7, 0xD8]).unwrap();
    assert!(matches!(
        neg.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Neg {
                flags: FlagUpdate::None,
                ..
            },
            ..
        }]
    ));

    let not = lift_single(&[0x62, 0xF4, 0xFC, 0x08, 0xF7, 0xD0]).unwrap();
    assert!(matches!(
        not.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Not { .. },
            ..
        }]
    ));
}

#[test]
fn lift_apx_ndd_group3_not_neg_use_vvvv_destination_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `not r8, rax` => 62 f4 bc 18 f7 d0.
    let not = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xBC, 0x18, 0xF7, 0xD0], &mut ctx)
        .unwrap();
    assert_eq!(not.bytes_consumed, 6);
    assert_eq!(not.ops.len(), 1);
    match &not.ops[0].kind {
        OpKind::Not {
            dst,
            src,
            width: OpWidth::W64,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*src, x86_gpr(0));
        }
        other => panic!("expected APX NDD NOT, got {other:?}"),
    }

    // LLVM 23: `neg r8, rax` => 62 f4 bc 18 f7 d8.
    let neg = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xBC, 0x18, 0xF7, 0xD8], &mut ctx)
        .unwrap();
    assert_eq!(neg.bytes_consumed, 6);
    assert_eq!(neg.ops.len(), 1);
    match &neg.ops[0].kind {
        OpKind::Neg {
            dst,
            src,
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*src, x86_gpr(0));
        }
        other => panic!("expected APX NDD NEG, got {other:?}"),
    }
}
#[test]
fn lift_apx_nf_group3_neg_suppresses_flags_and_ndd_memory_source() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    let neg_nf = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xFC, 0x0C, 0xF7, 0xD8], &mut ctx)
        .unwrap();
    assert_eq!(neg_nf.bytes_consumed, 6);
    match &neg_nf.ops[0].kind {
        OpKind::Neg {
            dst,
            src,
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } => {
            assert_eq!(*dst, x86_gpr(0));
            assert_eq!(*src, x86_gpr(0));
        }
        other => panic!("expected APX NF NEG, got {other:?}"),
    }

    let not_mem = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xBC, 0x18, 0xF7, 0x10], &mut ctx)
        .unwrap();
    assert_eq!(not_mem.bytes_consumed, 6);
    assert_eq!(not_mem.ops.len(), 2);
    let loaded = match &not_mem.ops[0].kind {
        OpKind::Load {
            dst,
            width: MemWidth::B8,
            ..
        } => *dst,
        other => panic!("expected APX NDD NOT memory load, got {other:?}"),
    };
    match &not_mem.ops[1].kind {
        OpKind::Not {
            dst,
            src,
            width: OpWidth::W64,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*src, loaded);
        }
        other => panic!("expected APX NDD NOT memory source, got {other:?}"),
    }
}
#[test]
fn lift_apx_nf_group3_implicit_mul_div_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (bytes, name, group) in [
        ([0x62, 0xF4, 0xFC, 0x0C, 0xF7, 0xE3], "mul", 4),
        ([0x62, 0xF4, 0xFC, 0x0C, 0xF7, 0xEB], "imul", 5),
        ([0x62, 0xF4, 0xFC, 0x0C, 0xF7, 0xF3], "div", 6),
        ([0x62, 0xF4, 0xFC, 0x0C, 0xF7, 0xFB], "idiv", 7),
    ] {
        let lifted = lifter.lift_insn(0x1000, &bytes, &mut ctx).unwrap();
        assert_eq!(lifted.bytes_consumed, 6, "{name}");
        assert_eq!(lifted.ops.len(), 1, "{name}");

        match (&lifted.ops[0].kind, group) {
            (
                OpKind::MulU {
                    dst_lo,
                    dst_hi: Some(dst_hi),
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                4,
            )
            | (
                OpKind::MulS {
                    dst_lo,
                    dst_hi: Some(dst_hi),
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                5,
            ) => {
                assert_eq!(*dst_lo, x86_gpr(0), "{name} low destination");
                assert_eq!(*dst_hi, x86_gpr(2), "{name} high destination");
                assert_eq!(*src1, x86_gpr(0), "{name} accumulator source");
                assert_eq!(*src2, x86_gpr(3), "{name} r/m source");
            }
            (
                OpKind::DivU {
                    quot,
                    rem: Some(rem),
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                6,
            )
            | (
                OpKind::DivS {
                    quot,
                    rem: Some(rem),
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                7,
            ) => {
                assert_eq!(*quot, x86_gpr(0), "{name} quotient");
                assert_eq!(*rem, x86_gpr(2), "{name} remainder");
                assert_eq!(*src1, x86_gpr(0), "{name} accumulator source");
                assert_eq!(*src2, x86_gpr(3), "{name} r/m source");
            }
            (other, _) => panic!("expected APX NF implicit {name}, got {other:?}"),
        }
    }
}
#[test]
fn lift_apx_group3_implicit_rejects_ndd_and_non_nf_forms() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (bytes, name) in [
        ([0x62, 0xF4, 0xFC, 0x08, 0xF7, 0xE3], "non-nf mul"),
        ([0x62, 0xF4, 0xFC, 0x1C, 0xF7, 0xE3], "ndd nf mul"),
    ] {
        let err = lifter.lift_insn(0x1000, &bytes, &mut ctx).unwrap_err();
        assert!(
            matches!(err, LiftError::Unsupported { .. }),
            "{name}: {err:?}"
        );
    }
}
#[test]
fn lift_apx_nf_group3_implicit_memory_source_does_not_store() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    let mul_mem = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xFC, 0x0C, 0xF7, 0x20], &mut ctx)
        .unwrap();
    assert_eq!(mul_mem.bytes_consumed, 6);
    assert_eq!(mul_mem.ops.len(), 2);
    let loaded = match &mul_mem.ops[0].kind {
        OpKind::Load {
            dst,
            width: MemWidth::B8,
            ..
        } => *dst,
        other => panic!("expected APX NF MUL memory load, got {other:?}"),
    };
    match &mul_mem.ops[1].kind {
        OpKind::MulU {
            dst_lo,
            dst_hi: Some(dst_hi),
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } => {
            assert_eq!(*dst_lo, x86_gpr(0));
            assert_eq!(*dst_hi, x86_gpr(2));
            assert_eq!(*src1, x86_gpr(0));
            assert_eq!(*src2, loaded);
        }
        other => panic!("expected APX NF MUL memory source, got {other:?}"),
    }
}
#[test]
fn lift_apx_ndd_nf_inc_dec_use_vvvv_destination_and_flags_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    let inc = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xBC, 0x18, 0xFF, 0xC0], &mut ctx)
        .unwrap();
    assert_eq!(inc.bytes_consumed, 6);
    match &inc.ops[0].kind {
        OpKind::Inc {
            dst,
            src,
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*src, x86_gpr(0));
        }
        other => panic!("expected APX NDD INC, got {other:?}"),
    }

    let dec = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xBC, 0x18, 0xFF, 0xC8], &mut ctx)
        .unwrap();
    match &dec.ops[0].kind {
        OpKind::Dec {
            dst,
            src,
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*src, x86_gpr(0));
        }
        other => panic!("expected APX NDD DEC, got {other:?}"),
    }

    let inc_nf = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xFC, 0x0C, 0xFF, 0xC0], &mut ctx)
        .unwrap();
    match &inc_nf.ops[0].kind {
        OpKind::Inc {
            dst,
            src,
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } => {
            assert_eq!(*dst, x86_gpr(0));
            assert_eq!(*src, x86_gpr(0));
        }
        other => panic!("expected APX NF INC, got {other:?}"),
    }

    let inc_mem = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xBC, 0x18, 0xFF, 0x00], &mut ctx)
        .unwrap();
    assert_eq!(inc_mem.ops.len(), 2);
    let loaded = match &inc_mem.ops[0].kind {
        OpKind::Load {
            dst,
            width: MemWidth::B8,
            ..
        } => *dst,
        other => panic!("expected APX NDD INC memory load, got {other:?}"),
    };
    match &inc_mem.ops[1].kind {
        OpKind::Inc {
            dst,
            src,
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*src, loaded);
        }
        other => panic!("expected APX NDD INC memory source, got {other:?}"),
    }
}
