//! Native x86-64 lowering coverage for VEX-encoded AVX-512 opmask operations.

use super::*;
use crate::smir::ir::ops::{
    X86OpmaskBinaryKind, X86OpmaskMoveDestination, X86OpmaskMoveSource, X86OpmaskOp,
    X86OpmaskShiftKind, X86OpmaskTestKind,
};
use crate::smir::ir::types::VirtualId;

fn k(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::K(index)))
}

fn gpr(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn kind(op: X86OpmaskOp) -> OpKind {
    OpKind::X86Opmask(op)
}

fn logic_encoding(width: OpWidth) -> (u8, bool) {
    match width {
        OpWidth::W8 => (1, false),
        OpWidth::W16 => (0, false),
        OpWidth::W32 => (1, true),
        OpWidth::W64 => (0, true),
        OpWidth::W128 => unreachable!(),
    }
}

fn gpr_move_encoding(width: OpWidth) -> (u8, bool) {
    match width {
        OpWidth::W8 => (1, false),
        OpWidth::W16 => (0, false),
        OpWidth::W32 => (3, false),
        OpWidth::W64 => (3, true),
        OpWidth::W128 => unreachable!(),
    }
}

#[allow(clippy::too_many_arguments)]
fn expected_vex(
    map: u8,
    w: bool,
    vvvv: u8,
    l: bool,
    pp: u8,
    r: bool,
    b: bool,
    opcode: u8,
    modrm: u8,
    immediate: Option<u8>,
) -> Vec<u8> {
    let mut bytes = vec![
        0xC4,
        (u8::from(!r) << 7) | 0x40 | (u8::from(!b) << 5) | map,
        (u8::from(w) << 7) | (((!vvvv) & 0x0F) << 3) | (u8::from(l) << 2) | pp,
        opcode,
        modrm,
    ];
    bytes.extend(immediate);
    bytes
}

fn assert_contains(code: &[u8], expected: &[u8], label: &str) {
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "{label}: missing {expected:02X?} in {code:02X?}"
    );
}

fn lower_with_options(
    ops: Vec<(u64, OpKind)>,
    mem_helpers: bool,
    preserve_vectors: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (pc, op) in ops {
        builder.push_op(pc, op);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_preserve_vector_mem_helpers(preserve_vectors);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lowerer_emits_every_binary_not_and_test_width_with_exact_vex_fields() {
    for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
        let (pp, w) = logic_encoding(width);
        for (opcode, operation) in [
            (0x41, X86OpmaskBinaryKind::And),
            (0x42, X86OpmaskBinaryKind::AndNot),
            (0x45, X86OpmaskBinaryKind::Or),
            (0x46, X86OpmaskBinaryKind::Xnor),
            (0x47, X86OpmaskBinaryKind::Xor),
            (0x4A, X86OpmaskBinaryKind::Add),
        ] {
            let code = lower_single_op(kind(X86OpmaskOp::Binary {
                kind: operation,
                dst: k(3),
                src1: k(1),
                src2: k(2),
                width,
            }));
            let expected = expected_vex(1, w, 1, true, pp, false, false, opcode, 0xDA, None);
            assert_contains(&code, &expected, &format!("{operation:?} {width:?}"));
        }

        let code = lower_single_op(kind(X86OpmaskOp::Not {
            dst: k(3),
            src: k(2),
            width,
        }));
        let expected = expected_vex(1, w, 0, false, pp, false, false, 0x44, 0xDA, None);
        assert_contains(&code, &expected, &format!("KNOT {width:?}"));

        for (opcode, operation) in [
            (0x99, X86OpmaskTestKind::And),
            (0x98, X86OpmaskTestKind::Or),
        ] {
            let code = lower_single_op(kind(X86OpmaskOp::Test {
                kind: operation,
                src1: k(1),
                src2: k(2),
                width,
            }));
            let expected = expected_vex(1, w, 0, false, pp, false, false, opcode, 0xCA, None);
            assert_contains(&code, &expected, &format!("{operation:?} {width:?}"));
        }
    }
}

#[test]
fn lowerer_emits_every_unpack_and_shift_width_with_exact_vex_fields() {
    for (width, pp, w) in [
        (OpWidth::W16, 1, false),
        (OpWidth::W32, 0, false),
        (OpWidth::W64, 0, true),
    ] {
        let code = lower_single_op(kind(X86OpmaskOp::Unpack {
            dst: k(3),
            src1: k(1),
            src2: k(2),
            width,
        }));
        let expected = expected_vex(1, w, 1, true, pp, false, false, 0x4B, 0xDA, None);
        assert_contains(&code, &expected, &format!("KUNPCK {width:?}"));
    }

    for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
        let w = matches!(width, OpWidth::W16 | OpWidth::W64);
        let wide_opcode = u8::from(matches!(width, OpWidth::W32 | OpWidth::W64));
        for (base_opcode, operation) in [
            (0x30, X86OpmaskShiftKind::Right),
            (0x32, X86OpmaskShiftKind::Left),
        ] {
            let opcode = base_opcode + wide_opcode;
            let code = lower_single_op(kind(X86OpmaskOp::Shift {
                kind: operation,
                dst: k(3),
                src: k(2),
                width,
                count: 0xA5,
            }));
            let expected = expected_vex(3, w, 0, false, 1, false, false, opcode, 0xDA, Some(0xA5));
            assert_contains(&code, &expected, &format!("{operation:?} {width:?}"));
        }
    }
}

#[test]
fn lowerer_emits_every_kmov_width_direction_and_register_extension_field() {
    for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
        let (pp, w) = logic_encoding(width);
        let code = lower_single_op(kind(X86OpmaskOp::MoveToMask {
            dst: k(3),
            src: X86OpmaskMoveSource::Mask(k(2)),
            width,
        }));
        let expected = expected_vex(1, w, 0, false, pp, false, false, 0x90, 0xDA, None);
        assert_contains(&code, &expected, &format!("KMOV K-to-K {width:?}"));

        let (gpr_pp, gpr_w) = gpr_move_encoding(width);
        let code = lower_single_op(kind(X86OpmaskOp::MoveToMask {
            dst: k(5),
            src: X86OpmaskMoveSource::Gpr(gpr(X86Reg::R9)),
            width,
        }));
        let expected = expected_vex(1, gpr_w, 0, false, gpr_pp, false, true, 0x92, 0xE9, None);
        assert_contains(&code, &expected, &format!("KMOV r9-to-k5 {width:?}"));

        let code = lower_single_op(kind(X86OpmaskOp::MoveFromMask {
            dst: X86OpmaskMoveDestination::Gpr(gpr(X86Reg::R9)),
            src: k(5),
            width,
        }));
        let expected = expected_vex(1, gpr_w, 0, false, gpr_pp, true, false, 0x93, 0xCD, None);
        assert_contains(&code, &expected, &format!("KMOV k5-to-r9 {width:?}"));
    }
}

#[test]
fn lowerer_matches_known_llvm_kmovq_extended_gpr_encodings() {
    let to_mask = lower_single_op(kind(X86OpmaskOp::MoveToMask {
        dst: k(5),
        src: X86OpmaskMoveSource::Gpr(gpr(X86Reg::R9)),
        width: OpWidth::W64,
    }));
    assert_contains(
        &to_mask,
        &[0xC4, 0xC1, 0xFB, 0x92, 0xE9],
        "LLVM kmovq r9-to-k5 anchor",
    );

    let from_mask = lower_single_op(kind(X86OpmaskOp::MoveFromMask {
        dst: X86OpmaskMoveDestination::Gpr(gpr(X86Reg::R9)),
        src: k(5),
        width: OpWidth::W64,
    }));
    assert_contains(
        &from_mask,
        &[0xC4, 0x61, 0xFB, 0x93, 0xCD],
        "LLVM kmovq k5-to-r9 anchor",
    );
}

#[test]
fn lowerer_rejects_every_noncanonical_opmask_shape() {
    for (label, opmask) in [
        (
            "K8 destination",
            X86OpmaskOp::Not {
                dst: k(8),
                src: k(1),
                width: OpWidth::W16,
            },
        ),
        (
            "virtual source",
            X86OpmaskOp::Not {
                dst: k(1),
                src: VReg::Virtual(VirtualId(0)),
                width: OpWidth::W16,
            },
        ),
        (
            "APX GPR",
            X86OpmaskOp::MoveToMask {
                dst: k(1),
                src: X86OpmaskMoveSource::Gpr(gpr(X86Reg::R16)),
                width: OpWidth::W64,
            },
        ),
        (
            "128-bit width",
            X86OpmaskOp::Binary {
                kind: X86OpmaskBinaryKind::And,
                dst: k(1),
                src1: k(2),
                src2: k(3),
                width: OpWidth::W128,
            },
        ),
        (
            "8-bit unpack",
            X86OpmaskOp::Unpack {
                dst: k(1),
                src1: k(2),
                src2: k(3),
                width: OpWidth::W8,
            },
        ),
    ] {
        assert!(
            matches!(
                lower_single_op_err(kind(opmask)),
                LowerError::InvalidOperand { .. }
            ),
            "{label}"
        );
    }
}

#[test]
fn kmov_memory_lowering_requires_both_helper_modes_and_stages_exact_width() {
    for (width, pp, w, byte_size) in [
        (OpWidth::W8, 1, false, 1_u32),
        (OpWidth::W16, 0, false, 2),
        (OpWidth::W32, 1, true, 4),
        (OpWidth::W64, 0, true, 8),
    ] {
        for (is_load, opmask, opcode, modrm) in [
            (
                true,
                X86OpmaskOp::MoveToMask {
                    dst: k(3),
                    src: X86OpmaskMoveSource::Memory(Address::Direct(gpr(X86Reg::Rax))),
                    width,
                },
                0x90,
                0x1C,
            ),
            (
                false,
                X86OpmaskOp::MoveFromMask {
                    dst: X86OpmaskMoveDestination::Memory(Address::Direct(gpr(X86Reg::Rax))),
                    src: k(3),
                    width,
                },
                0x91,
                0x1C,
            ),
        ] {
            let op = kind(opmask);
            assert!(matches!(
                lower_with_options(vec![(0x2345, op.clone())], false, false),
                Err(LowerError::UnsupportedOp { .. })
            ));
            assert!(matches!(
                lower_with_options(vec![(0x2345, op.clone())], true, false),
                Err(LowerError::UnsupportedOp { .. })
            ));
            let (code, _) = lower_with_options(vec![(0x2345, op)], true, true)
                .unwrap_or_else(|error| panic!("{is_load} {width:?}: {error:?}"));
            let mut expected = expected_vex(1, w, 0, false, pp, false, false, opcode, modrm, None);
            expected.push(0x24);
            assert_contains(
                &code,
                &expected,
                &format!("KMOV memory {is_load} {width:?}"),
            );
            assert!(
                code.windows(4)
                    .any(|window| window == byte_size.to_le_bytes()),
                "missing helper byte size {byte_size}: {code:02X?}"
            );
            assert!(
                code.windows(4)
                    .any(|window| window == 0x2345_u32.to_le_bytes()),
                "missing fault PC: {code:02X?}"
            );
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_opmask_round_trips_full_k_state_and_state_backs_rsp_rbp() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    let source_rsp = 0xFEDC_BA98_7654_3210_u64;
    let (code, entry) = lower_with_options(
        vec![
            (
                0x1000,
                kind(X86OpmaskOp::MoveToMask {
                    dst: k(1),
                    src: X86OpmaskMoveSource::Gpr(gpr(X86Reg::Rsp)),
                    width: OpWidth::W64,
                }),
            ),
            (
                0x1005,
                kind(X86OpmaskOp::Binary {
                    kind: X86OpmaskBinaryKind::Xor,
                    dst: k(3),
                    src1: k(1),
                    src2: k(2),
                    width: OpWidth::W64,
                }),
            ),
            (
                0x100A,
                kind(X86OpmaskOp::MoveFromMask {
                    dst: X86OpmaskMoveDestination::Gpr(gpr(X86Reg::Rbp)),
                    src: k(3),
                    width: OpWidth::W64,
                }),
            ),
        ],
        false,
        false,
    )
    .expect("lower register opmask sequence");
    let exec = ExecMem::new(&code).expect("map register opmask sequence");
    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    for index in 0..8 {
        regs.k[index] = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.gpr[4] = source_rsp;
    regs.gpr[5] = 0x1111_2222_3333_4444;
    let source_k2 = regs.k[2];
    let preserved = regs.k;

    exec.run(entry, &mut regs);

    assert_eq!(regs.k[1], source_rsp);
    assert_eq!(regs.k[2], source_k2);
    assert_eq!(regs.k[3], source_rsp ^ source_k2);
    assert_eq!(regs.gpr[4], source_rsp);
    assert_eq!(regs.gpr[5], source_rsp ^ source_k2);
    for index in [0, 4, 5, 6, 7] {
        assert_eq!(regs.k[index], preserved[index], "K{index}");
    }
}
