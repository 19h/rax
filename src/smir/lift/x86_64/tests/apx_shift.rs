//! Intel APX Group 2 shift and rotate lifting tests.

use super::*;
use crate::smir::lift::x86_64::*;

fn apx_nf_prefix(nd: bool, w: bool, pp: u8) -> [u8; 4] {
    let p1 = (if nd { 0x3C } else { 0x7C }) | (if w { 0x80 } else { 0 }) | pp;
    let p2 = 0x0C | if nd { 0x10 } else { 0 };
    [0x62, 0xF4, p1, p2]
}

fn assert_apx_shift_ud(bytes: &[u8], expected_len: usize) {
    let result = lift_single(bytes).unwrap_or_else(|error| {
        panic!("reserved APX Group 2 encoding must strictly lift to #UD: {bytes:02X?}: {error:?}")
    });
    assert_invalid_opcode_trap(&result, expected_len);
}

#[test]
fn lift_apx_ndd_shift_rotate_use_group2_ops_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (bytes, name, amount) in [
        (
            [0x62, 0xF4, 0xBC, 0x18, 0xC1, 0xE0, 0x04],
            "shl",
            SrcOperand::Imm(4),
        ),
        (
            [0x62, 0xF4, 0xBC, 0x18, 0xD3, 0xE8, 0x00],
            "shr",
            SrcOperand::Reg(x86_gpr(1)),
        ),
        (
            [0x62, 0xF4, 0xBC, 0x18, 0xD1, 0xF8, 0x00],
            "sar",
            SrcOperand::Imm(1),
        ),
        (
            [0x62, 0xF4, 0xBC, 0x18, 0xC1, 0xC0, 0x07],
            "rol",
            SrcOperand::Imm(7),
        ),
        (
            [0x62, 0xF4, 0xBC, 0x18, 0xD3, 0xC8, 0x00],
            "ror",
            SrcOperand::Reg(x86_gpr(1)),
        ),
    ] {
        // LLVM 20 APX MAP4 NDD forms:
        //   shlq $4,  %rax, %r8 => 62 f4 bc 18 c1 e0 04
        //   shrq %cl, %rax, %r8 => 62 f4 bc 18 d3 e8
        //   sarq      %rax, %r8 => 62 f4 bc 18 d1 f8
        //   rolq $7,  %rax, %r8 => 62 f4 bc 18 c1 c0 07
        //   rorq %cl, %rax, %r8 => 62 f4 bc 18 d3 c8
        let len = if bytes[4] == 0xC1 { 7 } else { 6 };
        let result = lifter.lift_insn(0x1000, &bytes[..len], &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, len, "{name}");
        assert_eq!(result.ops.len(), 1, "{name}");

        match (name, &result.ops[0].kind) {
            (
                "shl",
                OpKind::Shl {
                    dst,
                    src,
                    amount: got_amount,
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "shr",
                OpKind::Shr {
                    dst,
                    src,
                    amount: got_amount,
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "sar",
                OpKind::Sar {
                    dst,
                    src,
                    amount: got_amount,
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ) => {
                assert_eq!(*dst, x86_gpr(8), "{name}");
                assert_eq!(*src, x86_gpr(0), "{name}");
                assert_eq!(*got_amount, amount, "{name}");
            }
            (
                "rol",
                OpKind::Rol {
                    dst,
                    src,
                    amount: got_amount,
                    width: OpWidth::W64,
                    flags,
                },
            )
            | (
                "ror",
                OpKind::Ror {
                    dst,
                    src,
                    amount: got_amount,
                    width: OpWidth::W64,
                    flags,
                },
            ) => {
                assert_eq!(*dst, x86_gpr(8), "{name}");
                assert_eq!(*src, x86_gpr(0), "{name}");
                assert_eq!(*got_amount, amount, "{name}");
                assert_eq!(*flags, x86_rotate_flags(), "{name}");
            }
            other => panic!("expected APX NDD {name}, got {other:?}"),
        }
    }
}
#[test]
fn lift_apx_shift_widths_nf_memory_and_cl_alias_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `shl r8d, eax, 4` => 62 f4 3c 18 c1 e0 04.
    let shl32 = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0xF4, 0x3C, 0x18, 0xC1, 0xE0, 0x04],
            &mut ctx,
        )
        .unwrap();
    match &shl32.ops[0].kind {
        OpKind::Shl {
            dst,
            src,
            amount: SrcOperand::Imm(4),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*src, x86_gpr(0));
        }
        other => panic!("expected APX NDD shl r32, got {other:?}"),
    }

    // LLVM 20: `shl r8b, al, 4` => 62 f4 3c 18 c0 e0 04.
    let shl8 = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0xF4, 0x3C, 0x18, 0xC0, 0xE0, 0x04],
            &mut ctx,
        )
        .unwrap();
    match &shl8.ops[0].kind {
        OpKind::Shl {
            dst,
            src,
            amount: SrcOperand::Imm(4),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*src, x86_gpr(0));
        }
        other => panic!("expected APX NDD shl r8, got {other:?}"),
    }

    // LLVM 20: `{nf} shr r8, rax, cl` => 62 f4 bc 1c d3 e8.
    let nf = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xBC, 0x1C, 0xD3, 0xE8], &mut ctx)
        .unwrap();
    match &nf.ops[0].kind {
        OpKind::Shr {
            dst,
            src,
            amount: SrcOperand::Reg(amount),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*src, x86_gpr(0));
            assert_eq!(*amount, x86_gpr(1));
        }
        other => panic!("expected APX NF NDD shr, got {other:?}"),
    }

    // LLVM 20: `shl r8, qword ptr [rax], 4` => 62 f4 bc 18 c1 20 04.
    let mem = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0xF4, 0xBC, 0x18, 0xC1, 0x20, 0x04],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(mem.ops.len(), 2);
    let tmp = match &mem.ops[0].kind {
        OpKind::Load {
            dst,
            addr: Address::Direct(base),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*base, x86_gpr(0));
            *dst
        }
        other => panic!("expected APX shift memory source load, got {other:?}"),
    };
    match &mem.ops[1].kind {
        OpKind::Shl {
            dst,
            src,
            amount: SrcOperand::Imm(4),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(8));
            assert_eq!(*src, tmp);
        }
        other => panic!("expected APX memory-source shift, got {other:?}"),
    }

    // LLVM 20: `shl rcx, rax, cl` => 62 f4 f4 18 d3 e0. The lowerer keeps
    // CL live while using a stack-resident destination, so no virtual count
    // capture is needed.
    let alias = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xF4, 0x18, 0xD3, 0xE0], &mut ctx)
        .unwrap();
    assert_eq!(alias.ops.len(), 1);
    match &alias.ops[0].kind {
        OpKind::Shl {
            dst,
            src,
            amount: SrcOperand::Reg(amount),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(1));
            assert_eq!(*src, x86_gpr(0));
            assert_eq!(*amount, x86_gpr(1));
        }
        other => panic!("expected direct APX NDD shift/CL alias, got {other:?}"),
    }
}
#[test]
fn lift_apx_ndd_carry_rotates_use_rcl_rcr_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (bytes, name, amount) in [
        (
            [0x62, 0xF4, 0xBC, 0x18, 0xD1, 0xD0],
            "rcl",
            SrcOperand::Imm(1),
        ),
        (
            [0x62, 0xF4, 0xBC, 0x18, 0xD3, 0xD8],
            "rcr",
            SrcOperand::Reg(x86_gpr(1)),
        ),
    ] {
        let result = lifter.lift_insn(0x1000, &bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 6, "{name}");
        assert_eq!(result.ops.len(), 1, "{name}");

        match (name, &result.ops[0].kind) {
            (
                "rcl",
                OpKind::Rcl {
                    dst,
                    src,
                    amount: got_amount,
                    width: OpWidth::W64,
                    flags,
                },
            )
            | (
                "rcr",
                OpKind::Rcr {
                    dst,
                    src,
                    amount: got_amount,
                    width: OpWidth::W64,
                    flags,
                },
            ) => {
                assert_eq!(*dst, x86_gpr(8), "{name}");
                assert_eq!(*src, x86_gpr(0), "{name}");
                assert_eq!(*got_amount, amount, "{name}");
                assert_eq!(*flags, x86_rotate_flags(), "{name}");
            }
            other => panic!("expected APX NDD {name}, got {other:?}"),
        }
    }

    // Intel APX revision 7.0 specifies {NF=0} for RCL and RCR.
    assert_apx_shift_ud(&[0x62, 0xF4, 0xBC, 0x1C, 0xD1, 0xD0], 6);
}

#[test]
fn every_apx_nf_rcl_rcr_addressing_class_traps_at_modrm() {
    // All six Group 2 count encodings select RCL/RCR in ModR/M.reg. Exercise
    // both ND and W states and every ModR/M addressing class. C0/C1 omit their
    // immediate deliberately; memory forms that name SIB/displacement cells
    // omit those bytes deliberately. Neither is required to establish #UD.
    for nd in [false, true] {
        for w in [false, true] {
            for opcode in [0xC0, 0xC1, 0xD0, 0xD1, 0xD2, 0xD3] {
                let valid_pp = if opcode & 1 == 0 {
                    &[0][..]
                } else {
                    &[0, 1][..]
                };
                for &pp in valid_pp {
                    let mut opcode_only = apx_nf_prefix(nd, w, pp).to_vec();
                    opcode_only.push(opcode);
                    assert!(matches!(
                        lift_single(&opcode_only),
                        Err(LiftError::Incomplete {
                            addr: 0x1000,
                            have: 5,
                            need: 6
                        })
                    ));

                    for group in [2u8, 3] {
                        for mod_bits in 0..=3u8 {
                            for rm in 0..=7u8 {
                                let mut bytes = opcode_only.clone();
                                bytes.push((mod_bits << 6) | (group << 3) | rm);
                                assert_apx_shift_ud(&bytes, 6);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn apx_nf_rcl_rcr_frontiers_include_legal_address_and_segment_overrides() {
    for legacy_prefix in [0x67, 0x65] {
        assert_apx_shift_ud(&[legacy_prefix, 0x62, 0xF4, 0xBC, 0x1C, 0xD1, 0xD0], 7);
        assert_apx_shift_ud(&[legacy_prefix, 0x62, 0xF4, 0xBC, 0x1C, 0xC1, 0x18], 7);
    }
}

#[test]
fn neighboring_apx_nf_group2_operations_remain_liftable() {
    for group in [0u8, 1, 4, 5, 6, 7] {
        let bytes = [0x62, 0xF4, 0xBC, 0x1C, 0xD1, 0xC0 | (group << 3)];
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("valid APX NF Group 2 /{group} must lift: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert_eq!(result.ops.len(), 1);
        assert!(match &result.ops[0].kind {
            OpKind::Rol { flags, .. } | OpKind::Ror { flags, .. } => {
                *flags == FlagUpdate::None
            }
            OpKind::Shl { flags, .. } | OpKind::Shr { flags, .. } | OpKind::Sar { flags, .. } =>
                *flags == FlagUpdate::None,
            _ => false,
        });
    }
}
