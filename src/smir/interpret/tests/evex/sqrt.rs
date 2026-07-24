//! x86 square-root execution tests.

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn lifted_evex_packed_sqrt_broadcast_executes_masks_and_fault_suppression() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let sentinel = [0xCAFE_BABE_DEAD_BEEFu64; 16];
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    // EVEX disp8*N uses the 4-byte broadcast tuple, not the 64-byte ZMM
    // width: [RAX + 0x10 * 4] = 0x140. One scalar source feeds 16 lanes.
    ctx.write_vreg(rax, 0x100);
    memory
        .write(0x140, &81.0f32.to_bits().to_le_bytes())
        .unwrap();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF1, 0x7C, 0x58, 0x51, 0x50, 0x10],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                u64::from(9.0f32.to_bits()),
                "VSQRTPS broadcast lane {lane}"
            );
        }
    }

    // The 8-byte tuple scales the same disp8 to 0x80. Active lanes consume
    // the shared source; inactive merging lanes retain the old destination.
    memory
        .write(0x180, &144.0f64.to_bits().to_le_bytes())
        .unwrap();
    let mask = 0b0101_0101u64;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = sentinel;
        x86.k[2] = mask;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF1, 0xFD, 0x5A, 0x51, 0x58, 0x10],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..8u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[3], lane, 64),
                if mask & (1u64 << lane) != 0 {
                    12.0f64.to_bits()
                } else {
                    sentinel[usize::from(lane)]
                },
                "VSQRTPD broadcast lane {lane}"
            );
        }
    }

    // An all-zero mask suppresses an out-of-bounds scalar source completely;
    // zeroing masking still clears every destination lane.
    ctx.write_vreg(rax, 0x400);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = sentinel;
        x86.k[2] = 0;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0xDA, 0x51, 0x18], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert!(x86.xmm[3].iter().all(|word| *word == 0));
    }

    // Activating any lane makes the shared memory operand architecturally
    // live. The read faults before the destination is committed.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = sentinel;
        x86.k[2] = 1;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0x5A, 0x51, 0x18], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[3], sentinel);
    }
}

#[test]
fn lifted_dynamic_sqrt_uses_mxcsr_rounding_status_and_atomic_traps() {
    const BYTES: &[u8] = &[0x62, 0xF1, 0x7C, 0x08, 0x51, 0xCB];
    const SENTINEL: [u64; 16] = [0xA5A5_5A5A_DEAD_BEEF; 16];

    let initialize_invalid_source = |ctx: &mut SmirContext, mxcsr: u32| {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = SENTINEL;
            for (lane, bits) in [0xFF80_0000u32, 0x7FC0_0001, 0x7F80_0001, 0xBF80_0000]
                .into_iter()
                .enumerate()
            {
                SmirInterpreter::set_lane(&mut x86.xmm[3], lane as u8, 32, u64::from(bits));
            }
            x86.mxcsr = mxcsr;
        }
    };

    let mut masked = SmirContext::new_x86_64();
    initialize_invalid_source(&mut masked, 0x1F80);
    let exit = execute_lifted_x86(BYTES, &mut masked, &mut FlatMemory::new(0x100));
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &masked.arch_regs {
        assert_eq!(x86.mxcsr & 0x3F, 1, "invalid status must accrue");
        for (lane, expected) in [0xFFC0_0000u32, 0x7FC0_0001, 0x7FC0_0001, 0xFFC0_0000]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], lane as u8, 32),
                u64::from(expected),
                "masked invalid lane {lane}"
            );
        }
    }

    let mut unmasked = SmirContext::new_x86_64();
    initialize_invalid_source(&mut unmasked, 0x1F80 & !(1 << 7));
    let exit = execute_lifted_x86(BYTES, &mut unmasked, &mut FlatMemory::new(0x100));
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &unmasked.arch_regs {
        assert_eq!(x86.xmm[1], SENTINEL, "#XM must precede destination commit");
        assert_eq!(x86.mxcsr & 0x3F, 1, "#XM must still accrue MXCSR.IE");
    }

    for (rounding_control, expected) in [(1u32, 0x3FB5_04F3), (2, 0x3FB5_04F4)] {
        let mut rounded = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut rounded.arch_regs {
            for lane in 0..4u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[3], lane, 32, u64::from(2.0f32.to_bits()));
            }
            x86.mxcsr = (0x1F80 & !(3 << 13)) | (rounding_control << 13);
        }
        let exit = execute_lifted_x86(BYTES, &mut rounded, &mut FlatMemory::new(0x100));
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &rounded.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 32), expected);
            assert_eq!(x86.mxcsr & 0x3F, 1 << 5, "inexact status must accrue");
        }
    }
}

#[test]
fn lifted_dynamic_scalar_sqrt_mask_suppresses_invalid_before_compute() {
    const SENTINEL: [u64; 16] = [0xA5A5_5A5A_DEAD_BEEF; 16];
    for (p2, expected_low) in [(0x09, SENTINEL[0]), (0x89, 0)] {
        let mut context = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
            x86.xmm[0] = SENTINEL;
            x86.xmm[1][0] = (-1.0f64).to_bits();
            x86.k[1] = 0;
            x86.mxcsr = 0x1F80 & !(1 << 7);
        }
        let exit = execute_lifted_x86(
            &[0x62, 0xF1, 0xFF, p2, 0x51, 0xC1],
            &mut context,
            &mut FlatMemory::new(0x100),
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &context.arch_regs {
            assert_eq!(x86.xmm[0][0], expected_low);
            assert_eq!(x86.mxcsr & 0x3F, 0, "masked source reported invalid");
        }
    }
}

#[test]
fn x86_sqrt_integer_root_rounds_exactly_and_classifies_special_values() {
    for (mode, expected) in [
        (FpRoundMode::RoundNearest, 0x3FB5_04F3),
        (FpRoundMode::RoundDown, 0x3FB5_04F3),
        (FpRoundMode::RoundUp, 0x3FB5_04F4),
        (FpRoundMode::RoundTowardZero, 0x3FB5_04F3),
    ] {
        let result = SmirInterpreter::x86_simd_fp_sqrt(
            u64::from(2.0f32.to_bits()),
            X86_SIMD_F32,
            mode,
            0x1F80,
        );
        assert_eq!(result.bits, expected, "binary32 {mode:?}");
        assert_eq!(result.status, 1 << 5, "binary32 precision {mode:?}");
    }
    for (mode, expected) in [
        (FpRoundMode::RoundNearest, 0x3FF6_A09E_667F_3BCD),
        (FpRoundMode::RoundDown, 0x3FF6_A09E_667F_3BCC),
        (FpRoundMode::RoundUp, 0x3FF6_A09E_667F_3BCD),
        (FpRoundMode::RoundTowardZero, 0x3FF6_A09E_667F_3BCC),
    ] {
        let result =
            SmirInterpreter::x86_simd_fp_sqrt(2.0f64.to_bits(), X86_SIMD_F64, mode, 0x1F80);
        assert_eq!(result.bits, expected, "binary64 {mode:?}");
        assert_eq!(result.status, 1 << 5, "binary64 precision {mode:?}");
    }

    for (bits, format, expected) in [
        (
            u64::from(4.0f32.to_bits()),
            X86_SIMD_F32,
            u64::from(2.0f32.to_bits()),
        ),
        (4.0f64.to_bits(), X86_SIMD_F64, 2.0f64.to_bits()),
        (
            u64::from((-0.0f32).to_bits()),
            X86_SIMD_F32,
            u64::from((-0.0f32).to_bits()),
        ),
        (
            f64::INFINITY.to_bits(),
            X86_SIMD_F64,
            f64::INFINITY.to_bits(),
        ),
    ] {
        let result =
            SmirInterpreter::x86_simd_fp_sqrt(bits, format, FpRoundMode::RoundNearest, 0x1F80);
        assert_eq!(result.bits, expected);
        assert_eq!(result.status, 0);
    }

    for (bits, expected, status) in [
        (0xFFC1_2345u32, 0xFFC1_2345u32, 0),
        (0xFF81_2345u32, 0xFFC1_2345u32, 1),
        ((-1.0f32).to_bits(), 0xFFC0_0000, 1),
        (f32::NEG_INFINITY.to_bits(), 0xFFC0_0000, 1),
    ] {
        let result = SmirInterpreter::x86_simd_fp_sqrt(
            u64::from(bits),
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1F80,
        );
        assert_eq!(result.bits, u64::from(expected));
        assert_eq!(result.status, status);
    }

    let gradual =
        SmirInterpreter::x86_simd_fp_sqrt(1, X86_SIMD_F32, FpRoundMode::RoundNearest, 0x1F80);
    assert_eq!(gradual.bits, 0x1A35_04F3);
    assert_eq!(gradual.status, (1 << 1) | (1 << 5));
    let daz = SmirInterpreter::x86_simd_fp_sqrt(
        1,
        X86_SIMD_F32,
        FpRoundMode::RoundNearest,
        0x1F80 | (1 << 6),
    );
    assert_eq!(daz.bits, 0);
    assert_eq!(daz.status, 0);

    // The minimum binary64 subnormal is 2^-1074; its root is the exactly
    // representable normal value 2^-537.
    let minimum_f64 =
        SmirInterpreter::x86_simd_fp_sqrt(1, X86_SIMD_F64, FpRoundMode::RoundNearest, 0x1F80);
    assert_eq!(minimum_f64.bits, 0x1E60_0000_0000_0000);
    assert_eq!(minimum_f64.status, 1 << 1);
}

#[test]
fn x86_sqrt_rounding_matches_independent_ieee_reference_grid() {
    fn finite_parts(bits: u64, format: X86SimdFpFormat) -> (u128, i32) {
        let exponent_field =
            ((bits >> format.fraction_bits) & ((1u64 << format.exponent_bits) - 1)) as i32;
        let fraction = u128::from(bits & ((1u64 << format.fraction_bits) - 1));
        if exponent_field == 0 {
            (fraction, 1 - format.bias - format.fraction_bits as i32)
        } else {
            (
                fraction | (1u128 << format.fraction_bits),
                exponent_field - format.bias - format.fraction_bits as i32,
            )
        }
    }

    fn normalize(mut magnitude: u128, mut exponent: i32) -> (u128, i32) {
        let zeros = magnitude.trailing_zeros();
        magnitude >>= zeros;
        exponent += zeros as i32;
        (magnitude, exponent)
    }

    fn compare_square(candidate: u64, source: u64, format: X86SimdFpFormat) -> std::cmp::Ordering {
        let (candidate_significand, candidate_exponent) = finite_parts(candidate, format);
        let (source_significand, source_exponent) = finite_parts(source, format);
        let (square, square_exponent) = normalize(
            candidate_significand * candidate_significand,
            2 * candidate_exponent,
        );
        let (source, source_exponent) = normalize(source_significand, source_exponent);
        let square_top = 127 - square.leading_zeros() as i32 + square_exponent;
        let source_top = 127 - source.leading_zeros() as i32 + source_exponent;
        match square_top.cmp(&source_top) {
            std::cmp::Ordering::Equal => {
                let common_exponent = square_exponent.min(source_exponent);
                let square_shift = (square_exponent - common_exponent) as u32;
                let source_shift = (source_exponent - common_exponent) as u32;
                debug_assert!(square_shift < 128 && source_shift < 128);
                (square << square_shift).cmp(&(source << source_shift))
            }
            ordering => ordering,
        }
    }

    fn verify(bits: u64, format: X86SimdFpFormat) {
        let exponent_field = (bits >> format.fraction_bits) & ((1u64 << format.exponent_bits) - 1);
        let exponent_max = (1u64 << format.exponent_bits) - 1;
        if bits == 0 || exponent_field == exponent_max {
            return;
        }
        let nearest = if format.total_bits == 32 {
            u64::from(f32::from_bits(bits as u32).sqrt().to_bits())
        } else {
            f64::from_bits(bits).sqrt().to_bits()
        };
        let square_order = compare_square(nearest, bits, format);
        let exact = square_order == std::cmp::Ordering::Equal;
        let (down, up) = match square_order {
            std::cmp::Ordering::Less => (nearest, nearest + 1),
            std::cmp::Ordering::Equal => (nearest, nearest),
            std::cmp::Ordering::Greater => (nearest - 1, nearest),
        };
        let denormal = exponent_field == 0;
        let expected_status = u32::from(denormal) << 1 | u32::from(!exact) << 5;
        for (mode, expected) in [
            (FpRoundMode::RoundNearest, nearest),
            (FpRoundMode::RoundDown, down),
            (FpRoundMode::RoundUp, up),
            (FpRoundMode::RoundTowardZero, down),
        ] {
            let actual = SmirInterpreter::x86_simd_fp_sqrt(bits, format, mode, 0x1F80);
            assert_eq!(
                actual.bits, expected,
                "input={bits:016X} format={} mode={mode:?}",
                format.total_bits
            );
            assert_eq!(
                actual.status, expected_status,
                "status input={bits:016X} format={} mode={mode:?}",
                format.total_bits
            );
        }
    }

    for bits in [
        1u32,
        2,
        3,
        0x007F_FFFF,
        0x0080_0000,
        0x3F7F_FFFF,
        0x3F80_0000,
        0x4000_0000,
        0x7F7F_FFFF,
    ] {
        verify(u64::from(bits), X86_SIMD_F32);
    }
    for bits in [
        1u64,
        2,
        3,
        0x000F_FFFF_FFFF_FFFF,
        0x0010_0000_0000_0000,
        0x3FEF_FFFF_FFFF_FFFF,
        0x3FF0_0000_0000_0000,
        0x4000_0000_0000_0000,
        0x7FEF_FFFF_FFFF_FFFF,
    ] {
        verify(bits, X86_SIMD_F64);
    }

    let mut state = 0xD1B5_4A32_D192_ED03u64;
    for _ in 0..4_096 {
        state = state
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0xA076_1D64_78BD_642F);
        verify(u64::from(state as u32 & 0x7FFF_FFFF), X86_SIMD_F32);
        state = state
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0xE703_7ED1_A0B4_28DB);
        verify(state & 0x7FFF_FFFF_FFFF_FFFF, X86_SIMD_F64);
    }
}

#[test]
fn x86_sqrt_dynamic_exceptions_are_atomic_and_sae_is_state_silent() {
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let src = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    let sentinel = [0xA5A5_A5A5_A5A5_A5A5; 16];
    let execute = |suppress_exceptions: bool, mxcsr: u32| {
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 32, u64::from((-1.0f32).to_bits()));
            x86.mxcsr = mxcsr;
        }
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::X86Sqrt {
                dst,
                src,
                elem: VecElementType::F32,
                lanes: 1,
                round: if suppress_exceptions {
                    FpRoundMode::RoundNearest
                } else {
                    FpRoundMode::Dynamic
                },
                suppress_exceptions,
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let function = builder.finish();
        let exit = SmirInterpreter::new().execute_block(
            &mut ctx,
            &mut FlatMemory::new(0x100),
            &function.blocks[0],
        );
        (exit, ctx)
    };

    let (exit, ctx) = execute(false, 0x1F80 & !(1 << 7));
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel, "#XM must precede destination commit");
        assert_eq!(x86.mxcsr & 1, 1, "#XM still accrues MXCSR.IE");
    }

    let (exit, ctx) = execute(false, 0x1F80);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 0, 32), 0xFFC0_0000);
        assert_eq!(x86.mxcsr & 1, 1);
    }

    let sae_mxcsr = 0x1F80 & !(1 << 7);
    let (exit, ctx) = execute(true, sae_mxcsr);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 0, 32), 0xFFC0_0000);
        assert_eq!(x86.mxcsr, sae_mxcsr, "SAE must not update MXCSR");
    }
}

#[test]
fn optimizer_retains_dead_dynamic_x86_sqrt_mxcsr_effect() {
    use crate::smir::optimize::{OptLevel, optimize_function};

    let src = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86Sqrt {
            dst: VReg::Virtual(VirtualId(77)),
            src,
            elem: VecElementType::F32,
            lanes: 1,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
        },
    );
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);
    assert!(
        function.blocks[0]
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86Sqrt { .. }))
    );

    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 32, u64::from((-1.0f32).to_bits()));
        x86.mxcsr = 0x1F80;
    }
    let exit = SmirInterpreter::new().execute_block(
        &mut ctx,
        &mut FlatMemory::new(0x100),
        &function.blocks[0],
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mxcsr & 1, 1);
    }
}

#[test]
fn lifted_evex_sqrt_er_executes_all_families_rounding_modes_and_lanes() {
    let modes = [
        (0x18, 0x3FB5_04F3u64, 0x3FF6_A09E_667F_3BCDu64),
        (0x38, 0x3FB5_04F3, 0x3FF6_A09E_667F_3BCC),
        (0x58, 0x3FB5_04F4, 0x3FF6_A09E_667F_3BCD),
        (0x78, 0x3FB5_04F3, 0x3FF6_A09E_667F_3BCC),
    ];
    let mut memory = FlatMemory::new(0x100);

    for (p2, expected_f32, expected_f64) in modes {
        for (p1, elem_bits, source, expected, lanes) in [
            (0x7C, 32, u64::from(2.0f32.to_bits()), expected_f32, 16),
            (0xFD, 64, 2.0f64.to_bits(), expected_f64, 8),
        ] {
            let mut ctx = SmirContext::new_x86_64();
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                for lane in 0..lanes {
                    SmirInterpreter::set_lane(&mut x86.xmm[1], lane, elem_bits, source);
                }
                x86.mxcsr = (0x1F80 & !(0x3F | (3 << 13))) | (2 << 13);
            }
            let before = match &ctx.arch_regs {
                ArchRegState::X86_64(x86) => x86.mxcsr,
                _ => unreachable!(),
            };
            let exit = execute_lifted_x86(&[0x62, 0xF1, p1, p2, 0x51, 0xC1], &mut ctx, &mut memory);
            assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                for lane in 0..lanes {
                    assert_eq!(
                        SmirInterpreter::get_lane(&x86.xmm[0], lane, elem_bits),
                        expected,
                        "packed p1={p1:02X} p2={p2:02X} lane={lane}"
                    );
                }
                assert_eq!(x86.mxcsr, before, "packed ER must imply SAE");
            }
        }

        for (p1, elem_bits, source, expected) in [
            (0x7E, 32, u64::from(2.0f32.to_bits()), expected_f32),
            (0xFF, 64, 2.0f64.to_bits(), expected_f64),
        ] {
            let mut ctx = SmirContext::new_x86_64();
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [0xD00D_F00D_CAFE_BABE; 16];
                SmirInterpreter::set_lane(&mut x86.xmm[1], 0, elem_bits, source);
                x86.mxcsr = 0;
            }
            let exit = execute_lifted_x86(&[0x62, 0xF1, p1, p2, 0x51, 0xC1], &mut ctx, &mut memory);
            assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[0], 0, elem_bits),
                    expected
                );
                assert_eq!(x86.xmm[0][1], 0xD00D_F00D_CAFE_BABE);
                assert_eq!(x86.mxcsr, 0, "scalar ER must imply SAE");
            }
        }
    }
}

#[test]
fn lifted_evex_sqrt_er_preserves_mask_merge_flags_and_mxcsr() {
    let sentinel = [0x0123_4567_89AB_CDEF; 16];
    let mask = 0b0101_0101_0101_0101u64;
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[4] = sentinel;
        x86.k[1] = mask;
        for lane in 0..16u8 {
            let source = if mask & (1u64 << lane) != 0 {
                u64::from(4.0f32.to_bits())
            } else if lane & 2 == 0 {
                u64::from((-1.0f32).to_bits())
            } else {
                0x7F80_0001
            };
            SmirInterpreter::set_lane(&mut x86.xmm[0], lane, 32, source);
        }
        x86.mxcsr = 0;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF1, 0x7C, 0x19, 0x51, 0xE0],
        &mut ctx,
        &mut FlatMemory::new(0x100),
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[4], lane, 32),
                if mask & (1u64 << lane) != 0 {
                    u64::from(2.0f32.to_bits())
                } else {
                    SmirInterpreter::get_lane(&sentinel, lane, 32)
                },
                "masked lane {lane}"
            );
        }
        assert_eq!(x86.mxcsr, 0);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);

    // A disabled scalar lane keeps the old low element and cannot report the
    // negative source, even with every MXCSR exception architecturally unmasked.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.k[1] = 0;
        x86.xmm[0] = sentinel;
        SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 64, (-1.0f64).to_bits());
        x86.mxcsr = 0;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF1, 0xFF, 0x19, 0x51, 0xC1],
        &mut ctx,
        &mut FlatMemory::new(0x100),
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], sentinel[0]);
        assert_eq!(x86.mxcsr, 0);
    }
}

#[test]
fn optimized_evex_sqrt_er_matches_o0_o1_o2() {
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::optimize::{OptLevel, optimize_function};

    let mut observed = Vec::new();
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut lifter = X86_64Lifter::strict();
        let mut lift_ctx = LiftContext::new(SourceArch::X86_64);
        let lifted = lifter
            .lift_insn(0x1000, &[0x62, 0xF1, 0x7C, 0x58, 0x51, 0xC1], &mut lift_ctx)
            .unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let mut function = builder.finish();
        function.blocks[0].ops = lifted.ops;
        optimize_function(&mut function, level);

        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[1],
                    lane,
                    32,
                    u64::from((f32::from(lane) + 2.0).to_bits()),
                );
            }
            x86.mxcsr = 0x1F80;
        }
        let exit = SmirInterpreter::new().execute_block(
            &mut ctx,
            &mut FlatMemory::new(0x100),
            &function.blocks[0],
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            observed.push((x86.xmm[0], x86.mxcsr));
        }
    }
    assert_eq!(observed[0], observed[1]);
    assert_eq!(observed[0], observed[2]);
}
