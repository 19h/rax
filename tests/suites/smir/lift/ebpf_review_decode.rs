use super::*;
use rax::isa::ebpf::{EbpfDecodeError, EbpfOp, decode_insn};

#[test]
fn btf_helper_ids_do_not_alias_the_static_helper_registry() {
    // RFC 9669 section 4.3.1 identifies helpers by distinct static/BTF IDs.
    // The engine has no BTF platform resolver, so a static callback with the
    // same numeric ID must not satisfy a src_reg=2 call.
    for endian in [Endian::Little, Endian::Big] {
        let bytes = encode_insn(0x85, 0, 2, 0, 7, endian);
        let insn = decode_insn(&bytes, EbpfDialect::Rfc9669, endian).unwrap();
        let mut cpu = EbpfCpu::new(EbpfConfig {
            endian,
            ..EbpfConfig::default()
        });
        cpu.regs.r[0] = 42;
        cpu.register_syscall(7, |_, _| Ok(700));
        let mut memory = FlatMemory::new(16);
        let direct = cpu.execute_decoded(&insn, &mut memory);
        let mut lifter = EbpfLifter::new(EbpfDialect::Rfc9669, endian);
        let lifted = lifter.lift_insn(0, &bytes, &mut LiftContext::new(lifter.source_arch()));
        assert_eq!(direct, Err(EbpfTrap::UnsupportedBtfHelper { pc: 0, id: 7 }));
        assert!(matches!(
            lifted,
            Err(rax::smir::lift::LiftError::Unsupported { .. })
        ));
        assert_eq!(cpu.regs.r[0], 42);
        assert_eq!(cpu.regs.pc, 0);

        let options = rax::oracle::OracleOptions {
            isa: rax::oracle::OracleIsa::Ebpf,
            ebpf_endian: match endian {
                Endian::Little => rax::config::Endianness::Little,
                Endian::Big => rax::config::Endianness::Big,
            },
            ..rax::oracle::OracleOptions::default()
        };
        let decoded = rax::oracle::decode_to_json(&bytes, &options).unwrap();
        assert_eq!(decoded["decoded_ops"][0]["src"], 2);
        assert!(
            decoded["smir"]["error"]
                .as_str()
                .unwrap()
                .contains("BTF helper call requires a platform resolver")
        );

        let static_bytes = encode_insn(0x85, 0, 0, 0, 7, endian);
        let static_insn = decode_insn(&static_bytes, EbpfDialect::Rfc9669, endian).unwrap();
        assert_eq!(cpu.execute_decoded(&static_insn, &mut memory), Ok(None));
        assert_eq!(cpu.regs.r[0], 700);
        assert_eq!(cpu.regs.pc, 8);
        assert!(
            lifter
                .lift_insn(
                    0,
                    &static_bytes,
                    &mut LiftContext::new(lifter.source_arch())
                )
                .is_ok()
        );
    }
}

#[test]
fn rfc_unused_fields_are_rejected_in_both_byte_orders() {
    // RFC 9669 sections 3.1, 4.1-4.3, and 5.1. Each tuple is an otherwise
    // valid instruction with exactly one nonzero unused field.
    let invalid = [
        (0x95, 0, 0, 0, 1),
        (0x95, 1, 0, 0, 0),
        (0x95, 0, 1, 0, 0),
        (0x95, 0, 0, 1, 0),
        (0x07, 1, 2, 0, 1),
        (0x07, 1, 0, 1, 1),
        (0x0f, 1, 2, 0, 1),
        (0x0f, 1, 2, 1, 0),
        (0x87, 1, 0, 0, 1),
        (0x87, 1, 2, 0, 0),
        (0xd4, 1, 2, 0, 32),
        (0xd4, 1, 0, 1, 32),
        (0x61, 1, 2, 0, 1),
        (0x63, 1, 2, 0, 1),
        (0x62, 1, 2, 0, 1),
        (0x05, 1, 0, 0, 0),
        (0x05, 0, 1, 0, 0),
        (0x05, 0, 0, 0, 1),
        (0x06, 0, 0, 1, 0),
        (0x85, 1, 1, 0, 0),
        (0x85, 0, 1, 1, 0),
        (0x15, 1, 2, 0, 0),
        (0x1d, 1, 2, 0, 1),
    ];
    for endian in [Endian::Little, Endian::Big] {
        for (opc, dst, src, off, imm) in invalid {
            let bytes = encode_insn(opc, dst, src, off, imm, endian);
            assert!(
                decode_insn(&bytes, EbpfDialect::Rfc9669, endian).is_err(),
                "accepted reserved fields: {bytes:02x?}, {endian:?}"
            );
        }
        let mut wide = encode_lddw(1, 42, endian);
        wide[2] = 1;
        assert!(decode_insn(&wide, EbpfDialect::Rfc9669, endian).is_err());
        // RFC 9669 specifies CALL with K source only; 0x8d is an SBPF extension.
        assert!(
            decode_insn(
                &encode_insn(0x8d, 1, 0, 0, 0, endian),
                EbpfDialect::Rfc9669,
                endian
            )
            .is_err()
        );
    }
}

#[test]
fn frame_pointer_writes_follow_the_reference_version_matrix() {
    for dialect in [
        EbpfDialect::Rfc9669,
        EbpfDialect::Solana(SbpfVersion::V0),
        EbpfDialect::Solana(SbpfVersion::V1),
        EbpfDialect::Solana(SbpfVersion::V2),
        EbpfDialect::Solana(SbpfVersion::V3),
        EbpfDialect::Solana(SbpfVersion::V4),
    ] {
        let manual = matches!(
            dialect,
            EbpfDialect::Solana(SbpfVersion::V1 | SbpfVersion::V2)
        );
        for opc in [0xb7, 0x07, 0x0f, 0xbc] {
            let src = if opc & 8 != 0 { 1 } else { 0 };
            let imm = if opc & 8 != 0 { 0 } else { -16 };
            let bytes = encode_insn(opc, 10, src, 0, imm, Endian::Little);
            assert_eq!(
                decode_insn(&bytes, dialect, Endian::Little).is_ok(),
                manual && opc == 0x07,
                "{dialect:?}, opcode={opc:#x}"
            );
        }
        let store = if dialect == EbpfDialect::Solana(SbpfVersion::V2) {
            0x97
        } else {
            0x7a
        };
        assert!(
            decode_insn(
                &encode_insn(store, 10, 0, -8, 42, Endian::Little),
                dialect,
                Endian::Little
            )
            .is_ok()
        );
    }
    for imm in [0x01, 0x41, 0x51, 0xa1, 0xe1] {
        assert!(
            decode_insn(
                &encode_insn(0xdb, 1, 10, 0, imm, Endian::Little),
                EbpfDialect::Rfc9669,
                Endian::Little
            )
            .is_err()
        );
    }
    // Reading r10 as an atomic operand without writing it remains legal.
    assert!(
        decode_insn(
            &encode_insn(0xdb, 1, 10, 0, 0, Endian::Little),
            EbpfDialect::Rfc9669,
            Endian::Little
        )
        .is_ok()
    );
}

#[test]
fn pseudo_loads_fail_explicitly_without_a_platform_resolver() {
    for endian in [Endian::Little, Endian::Big] {
        for src_type in 1..=6 {
            let mut bytes = encode_lddw(1, 42, endian);
            bytes[1] = match endian {
                Endian::Little => src_type << 4 | 1,
                Endian::Big => 0x10 | src_type,
            };
            let insn = decode_insn(&bytes, EbpfDialect::Rfc9669, endian).unwrap();
            assert!(matches!(insn.op, EbpfOp::LoadImm64 { src_type: t, .. } if t == src_type));
            let mut cpu = EbpfCpu::new(EbpfConfig {
                endian,
                ..EbpfConfig::default()
            });
            cpu.regs.r[1] = 0x1234;
            let mut mem = FlatMemory::new(32);
            assert!(
                cpu.execute_decoded(&insn, &mut mem).is_err(),
                "subtype={src_type}"
            );
            assert_eq!(cpu.regs.r[1], 0x1234);
            assert_eq!(cpu.regs.pc, 0);
            let mut lifter = EbpfLifter::new(EbpfDialect::Rfc9669, endian);
            assert!(
                lifter
                    .lift_insn(0, &bytes, &mut LiftContext::new(lifter.source_arch()))
                    .is_err()
            );
        }
    }
}

#[test]
fn sbpf_fixed_and_manual_frames_match_reference_on_nested_calls() {
    for version in [
        SbpfVersion::V0,
        SbpfVersion::V1,
        SbpfVersion::V2,
        SbpfVersion::V3,
        SbpfVersion::V4,
    ] {
        let manual = matches!(version, SbpfVersion::V1 | SbpfVersion::V2);
        assert_eq!(version.manual_stack_frame_bump(), manual);
        for gaps in [false, true] {
            let mut cpu = EbpfCpu::new(EbpfConfig {
                dialect: EbpfDialect::Solana(version),
                enable_stack_frame_gaps: gaps,
                text_range: Some((0, 48)),
                max_call_depth: 4,
                ..EbpfConfig::default()
            });
            let initial_fp = cpu.regs.r[10];
            assert_eq!(
                initial_fp,
                rax::isa::ebpf::MM_STACK_START + if manual { 4 * 4096 } else { 4096 }
            );
            cpu.regs.r[6..10].copy_from_slice(&[6, 7, 8, 9]);
            let mut mem = FlatMemory::new(64);
            cpu.register_function(1, 16);
            cpu.register_function(2, 32);
            let call1 = encode_insn(0x85, 0, 1, 0, 1, Endian::Little);
            let call2 = encode_insn(
                0x85,
                0,
                1,
                0,
                if manual || version == SbpfVersion::V0 {
                    2
                } else {
                    1
                },
                Endian::Little,
            );
            let exit = encode_insn(0x95, 0, 0, 0, 0, Endian::Little);
            mem.load(0, &call1);
            mem.load(8, &exit);
            mem.load(16, &call2);
            mem.load(24, &exit);
            mem.load(32, &exit);
            let step = if manual {
                0
            } else if version == SbpfVersion::V0 && gaps {
                8192
            } else {
                4096
            };
            assert_eq!(cpu.step(&mut mem), Ok(None));
            assert_eq!(cpu.regs.pc, 16);
            assert_eq!(cpu.regs.r[10], initial_fp + step);
            cpu.regs.r[6] = 60;
            assert_eq!(cpu.step(&mut mem), Ok(None));
            assert_eq!(cpu.regs.pc, 32);
            assert_eq!(cpu.regs.r[10], initial_fp + 2 * step);
            cpu.regs.r[6] = 600;
            assert_eq!(cpu.step(&mut mem), Ok(None));
            assert_eq!(cpu.regs.r[6], 60);
            assert_eq!(cpu.regs.r[10], initial_fp + step);
            assert_eq!(cpu.step(&mut mem), Ok(None));
            assert_eq!(&cpu.regs.r[6..10], &[6, 7, 8, 9]);
            assert_eq!(cpu.regs.r[10], initial_fp);
            assert!(matches!(
                cpu.step(&mut mem),
                Ok(Some(EbpfExit::Exit { .. }))
            ));
        }
    }
}
