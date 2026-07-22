//! Intel APX integer ALU lifting tests.

use super::*;
use crate::smir::lift::x86_64::*;

fn apx_nf_prefix(nd: bool, w: bool, pp: u8) -> [u8; 4] {
    let p1 = (if nd { 0x3C } else { 0x7C }) | (if w { 0x80 } else { 0 }) | pp;
    let p2 = 0x0C | if nd { 0x10 } else { 0 };
    [0x62, 0xF4, p1, p2]
}

fn assert_apx_alu_ud(bytes: &[u8], expected_len: usize) {
    let result = lift_single(bytes).unwrap_or_else(|error| {
        panic!("reserved APX ALU encoding must strictly lift to #UD: {bytes:02X?}: {error:?}")
    });
    assert_invalid_opcode_trap(&result, expected_len);
}

#[test]
fn lift_apx_ndd_group1_immediates_use_vvvv_destination() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (group, name) in [
        (0u8, "add"),
        (1u8, "or"),
        (2u8, "adc"),
        (3u8, "sbb"),
        (4u8, "and"),
        (5u8, "sub"),
        (6u8, "xor"),
    ] {
        // LLVM 23 APX NDD-style prefix: W64, ND, destination in vvvv = r8.
        // ModR/M r/m is rax, so the lifted shape is `r8 = rax <op> -16`.
        let result = lifter
            .lift_insn(
                0x1000,
                &[0x62, 0xF4, 0xBC, 0x18, 0x83, 0xC0 | (group << 3), 0xF0],
                &mut ctx,
            )
            .unwrap();
        assert_eq!(result.bytes_consumed, 7, "{name}");
        assert_eq!(result.ops.len(), 1, "{name}");

        match (name, &result.ops[0].kind) {
            (
                "add",
                OpKind::Add {
                    dst,
                    src1,
                    src2: SrcOperand::Imm(-16),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "or",
                OpKind::Or {
                    dst,
                    src1,
                    src2: SrcOperand::Imm(-16),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "adc",
                OpKind::Adc {
                    dst,
                    src1,
                    src2: SrcOperand::Imm(-16),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "sbb",
                OpKind::Sbb {
                    dst,
                    src1,
                    src2: SrcOperand::Imm(-16),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "and",
                OpKind::And {
                    dst,
                    src1,
                    src2: SrcOperand::Imm(-16),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "sub",
                OpKind::Sub {
                    dst,
                    src1,
                    src2: SrcOperand::Imm(-16),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "xor",
                OpKind::Xor {
                    dst,
                    src1,
                    src2: SrcOperand::Imm(-16),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ) => {
                assert_eq!(*dst, x86_gpr(8), "{name}");
                assert_eq!(*src1, x86_gpr(0), "{name}");
            }
            other => panic!("expected APX NDD {name} imm8, got {other:?}"),
        }
    }
}
#[test]
fn lift_apx_ndd_adc_sbb_use_carry_ops_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (bytes, name) in [
        ([0x62, 0xF4, 0xBC, 0x18, 0x11, 0xD8], "adc"),
        ([0x62, 0xF4, 0xBC, 0x18, 0x19, 0xD8], "sbb"),
    ] {
        // LLVM 20:
        //   adcq %rbx, %rax, %r8 => 62 f4 bc 18 11 d8
        //   sbbq %rbx, %rax, %r8 => 62 f4 bc 18 19 d8
        let result = lifter.lift_insn(0x1000, &bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 6, "{name}");
        assert_eq!(result.ops.len(), 1, "{name}");

        match (name, &result.ops[0].kind) {
            (
                "adc",
                OpKind::Adc {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "sbb",
                OpKind::Sbb {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ) => {
                assert_eq!(*dst, x86_gpr(8), "{name}");
                assert_eq!(*src1, x86_gpr(0), "{name}");
                assert_eq!(*src2, x86_gpr(3), "{name}");
            }
            other => panic!("expected APX NDD {name}, got {other:?}"),
        }
    }
}
#[test]
fn lift_apx_ndd_adc_sbb_alias_second_source_without_virtual_preservation() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (bytes, name) in [
        ([0x62, 0x74, 0xBC, 0x18, 0x11, 0xC0], "adc"),
        ([0x62, 0x74, 0xBC, 0x18, 0x19, 0xC0], "sbb"),
    ] {
        // LLVM 20:
        //   adcq %r8, %rax, %r8 => 62 74 bc 18 11 c0
        //   sbbq %r8, %rax, %r8 => 62 74 bc 18 19 c0
        // The native lowerer handles this alias directly, so retaining a
        // virtual preservation move here would unnecessarily block JIT.
        let result = lifter.lift_insn(0x1000, &bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, 6, "{name}");
        assert_eq!(result.ops.len(), 1, "{name}");
        match (name, &result.ops[0].kind) {
            (
                "adc",
                OpKind::Adc {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "sbb",
                OpKind::Sbb {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ) => {
                assert_eq!(*dst, x86_gpr(8), "{name}");
                assert_eq!(*src1, x86_gpr(0), "{name}");
                assert_eq!(*src2, x86_gpr(8), "{name}");
            }
            other => panic!("expected direct APX NDD {name} alias, got {other:?}"),
        }
    }
}
#[test]
fn lift_apx_nf_adc_sbb_are_precise_invalid_opcode_traps() {
    for bytes in [
        [0x62, 0xF4, 0xBC, 0x1C, 0x11, 0xD8],
        [0x62, 0xF4, 0xBC, 0x1C, 0x19, 0xD8],
    ] {
        // Intel APX revision 7.0 specifies {NF=0} for ADC and SBB.
        assert_apx_alu_ud(&bytes, 5);
    }

    for bytes in [
        [0x62, 0xF4, 0xBC, 0x1C, 0x83, 0xD0, 0x01],
        [0x62, 0xF4, 0xBC, 0x1C, 0x83, 0xD8, 0x01],
    ] {
        assert_apx_alu_ud(&bytes, 6);
    }
}

#[test]
fn every_apx_nf_adc_sbb_register_opcode_traps_at_the_opcode_frontier() {
    // The opcode itself distinguishes ADC/SBB for every /r direction and
    // operand-size class. Exercise both ND states, both W states, the missing
    // ModR/M frontier, and all 256 apparent ModR/M bytes.
    for nd in [false, true] {
        for w in [false, true] {
            for opcode in [0x10, 0x11, 0x12, 0x13, 0x18, 0x19, 0x1A, 0x1B] {
                let valid_pp = if opcode & 1 == 0 {
                    &[0][..]
                } else {
                    &[0, 1][..]
                };
                for &pp in valid_pp {
                    let mut opcode_only = apx_nf_prefix(nd, w, pp).to_vec();
                    opcode_only.push(opcode);
                    assert_apx_alu_ud(&opcode_only, 5);

                    for modrm in 0..=u8::MAX {
                        let mut bytes = opcode_only.clone();
                        bytes.push(modrm);
                        assert_apx_alu_ud(&bytes, 5);
                    }
                }
            }
        }
    }
}

#[test]
fn every_apx_nf_adc_sbb_immediate_addressing_class_traps_at_modrm() {
    // Group 1 selects ADC/SBB in ModR/M.reg. The fault is therefore known
    // after exactly that byte, even when its apparent memory form would need
    // a SIB, displacement, or immediate.
    for nd in [false, true] {
        for w in [false, true] {
            for opcode in [0x80, 0x81, 0x83] {
                let valid_pp = if opcode == 0x80 {
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
                                assert_apx_alu_ud(&bytes, 6);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn apx_nf_adc_sbb_frontiers_include_legal_address_and_segment_overrides() {
    for legacy_prefix in [0x67, 0x65] {
        assert_apx_alu_ud(&[legacy_prefix, 0x62, 0xF4, 0xBC, 0x1C, 0x11], 6);
        assert_apx_alu_ud(&[legacy_prefix, 0x62, 0xF4, 0xBC, 0x1C, 0x83, 0xD0], 7);
    }
}

#[test]
fn neighboring_apx_nf_group1_add_and_sub_remain_liftable() {
    for (group, name) in [(0u8, "add"), (5, "sub")] {
        let bytes = [0x62, 0xF4, 0xBC, 0x1C, 0x83, 0xC0 | (group << 3), 0x01];
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("valid APX NF {name} must lift: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(match (group, result.ops.as_slice()) {
            (
                0,
                [
                    SmirOp {
                        kind:
                            OpKind::Add {
                                flags: FlagUpdate::None,
                                ..
                            },
                        ..
                    },
                ],
            )
            | (
                5,
                [
                    SmirOp {
                        kind:
                            OpKind::Sub {
                                flags: FlagUpdate::None,
                                ..
                            },
                        ..
                    },
                ],
            ) => true,
            _ => false,
        });
    }
}
#[test]
fn lift_apx_ndd_nf_add_suppresses_flag_updates() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `{nf} add rax, rbx` as EVEX MAP4 01 /r.
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xFC, 0x0C, 0x01, 0xD8], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::Add {
            dst,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } => {
            assert_eq!(*dst, x86_gpr(0));
            assert_eq!(*src1, x86_gpr(0));
            assert_eq!(*src2, x86_gpr(3));
        }
        other => panic!("expected APX NF add rax, rbx, got {other:?}"),
    }
}
#[test]
fn lift_apx_ndd_memory_source_decodes_x4_sib_index_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `add eax, ebx, dword ptr [rax + 2*r16]`.
    let result = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0xF4, 0x78, 0x18, 0x03, 0x1C, 0x40],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(result.bytes_consumed, 7);
    assert_eq!(result.ops.len(), 2);

    let tmp = match &result.ops[0].kind {
        OpKind::Load {
            dst,
            addr:
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 2,
                    disp: 0,
                    ..
                },
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*base, x86_gpr(0));
            assert_eq!(*index, x86_gpr(16));
            *dst
        }
        other => panic!("expected APX memory source load with r16 index, got {other:?}"),
    };
    match &result.ops[1].kind {
        OpKind::Add {
            dst,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(0));
            assert_eq!(*src1, x86_gpr(3));
            assert_eq!(*src2, tmp);
        }
        other => panic!("expected APX NDD memory-source add, got {other:?}"),
    }
}
#[test]
fn lift_apx_ndd_memory_source_decodes_b4_sib_base_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // Same operation shape, but B4 extends the SIB base to r16.
    let result = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0xFC, 0x7C, 0x18, 0x03, 0x1C, 0x40],
            &mut ctx,
        )
        .unwrap();
    match &result.ops[0].kind {
        OpKind::Load {
            addr:
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 2,
                    disp: 0,
                    ..
                },
            ..
        } => {
            assert_eq!(*base, x86_gpr(16));
            assert_eq!(*index, x86_gpr(0));
        }
        other => panic!("expected APX memory source load with r16 base, got {other:?}"),
    }
}
#[test]
fn lift_apx_ndd_memory_destination_becomes_register_result() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // Legacy 01 /r would write memory. APX ND redirects the result to vvvv.
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0x7C, 0x18, 0x01, 0x18], &mut ctx)
        .unwrap();
    assert_eq!(result.ops.len(), 2);
    assert!(
        !result
            .ops
            .iter()
            .any(|op| matches!(&op.kind, OpKind::Store { .. })),
        "NDD memory-destination ALU must not write the legacy memory destination"
    );
    let tmp = match &result.ops[0].kind {
        OpKind::Load { dst, .. } => *dst,
        other => panic!("expected memory destination load, got {other:?}"),
    };
    match &result.ops[1].kind {
        OpKind::Add {
            dst,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        } => {
            assert_eq!(*dst, x86_gpr(0));
            assert_eq!(*src1, tmp);
            assert_eq!(*src2, x86_gpr(3));
        }
        other => panic!("expected APX NDD register result, got {other:?}"),
    }
}
#[test]
fn lift_apx_ndd_binary_alu_aliases_second_source_without_virtual_preservation() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for (opcode, name) in [
        (0x03, "add"),
        (0x0B, "or"),
        (0x23, "and"),
        (0x2B, "sub"),
        (0x33, "xor"),
    ] {
        // These APX NDD encodings select EAX as both destination and source
        // 2, with EBX as source 1. Alias-safe lowering means the lifter can
        // retain that architectural identity instead of introducing a
        // virtual source-capture move that would disqualify the JIT block.
        let result = lifter
            .lift_insn(0x1000, &[0x62, 0xF4, 0x7C, 0x18, opcode, 0xD8], &mut ctx)
            .unwrap();
        assert_eq!(result.ops.len(), 1, "{name}");
        let exact_shape = match (name, &result.ops[0].kind) {
            (
                "add",
                OpKind::Add {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "or",
                OpKind::Or {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "and",
                OpKind::And {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "sub",
                OpKind::Sub {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
            )
            | (
                "xor",
                OpKind::Xor {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
            ) => *dst == x86_gpr(0) && *src1 == x86_gpr(3) && *src2 == x86_gpr(0),
            _ => false,
        };
        assert!(exact_shape, "unexpected direct APX NDD {name}: {result:?}");
    }
}
