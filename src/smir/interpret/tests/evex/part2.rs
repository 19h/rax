//! evex part 2 tests

use super::*;
use crate::smir::interpret::*;
use crate::smir::interpret::tests::*;

    #[test]
    fn executes_evex_vpconflict_prefix_dependencies_and_fault_atomicity() {
        fn vec_u32(values: &[u32]) -> VecValue {
            vec_from_bytes(
                &values
                    .iter()
                    .copied()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
        }

        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1] = vec_u32(&[1, 2, 1, 1]);
            x86.k[1] = 0xF;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x89, 0xC4, 0xC1], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], vec_u32(&[0, 0, 1, 5]));
            assert_eq!(&x86.xmm[0][2..], &[0; 14]);
        }

        memory
            .write(
                0x3F4,
                &[1u32, 2, 1]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3F4);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.k[1] = 1 << 2;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x89, 0xC4, 0x00], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], vec_u32(&[0, 0, 1, 0]));
        }

        let sentinel = [0x6E6E_6E6E_6E6E_6E6E; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.k[1] = 1 << 3;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x89, 0xC4, 0x00], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }
    }
    #[test]
    fn executes_vpmadd52_low_high_accumulator_masks_broadcast_and_fault_atomicity() {
        fn vec_u64(values: &[u64]) -> VecValue {
            vec_from_bytes(
                &values
                    .iter()
                    .copied()
                    .flat_map(u64::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
        }
        const MASK52: u64 = (1u64 << 52) - 1;
        let reference = |acc: u64, a: u64, b: u64, high: bool| {
            let product = u128::from(a & MASK52) * u128::from(b & MASK52);
            let addend = if high {
                ((product >> 52) as u64) & MASK52
            } else {
                product as u64 & MASK52
            };
            acc.wrapping_add(addend)
        };
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = vec_u64(&[5, 7]);
            x86.xmm[2] = vec_u64(&[MASK52, 0x12345]);
            x86.xmm[3] = vec_u64(&[3, 0x6789A]);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0xE9, 0xB4, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.xmm[1],
                vec_u64(&[
                    reference(5, MASK52, 3, false),
                    reference(7, 0x12345, 0x6789A, false),
                ])
            );
            assert_eq!(&x86.xmm[1][2..], &[0; 14]);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = vec_u64(&[10, 20, 30, 40, 50, 60, 70, 80]);
            x86.xmm[18] = vec_u64(&[MASK52; 8]);
            x86.xmm[19] = vec_u64(&[MASK52; 8]);
            x86.k[2] = 0x55;
        }
        execute_lifted_x86(&[0x62, 0xA2, 0xED, 0xC2, 0xB4, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 64),
                    if lane % 2 == 0 {
                        reference(10 * (lane as u64 + 1), MASK52, MASK52, false)
                    } else {
                        0
                    }
                );
            }
        }

        memory.write(0x3F8, &MASK52.to_le_bytes()).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3F8);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[20] = vec_u64(&[1, 2, 3, 4]);
            x86.xmm[21] = vec_u64(&[MASK52; 4]);
            x86.k[3] = 1;
        }
        execute_lifted_x86(&[0x62, 0xE2, 0xD5, 0x33, 0xB5, 0x20], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[20], 0, 64),
                reference(1, MASK52, MASK52, true)
            );
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[20], 1, 64), 2);
        }

        let sentinel = [0x7171_7171_7171_7171; 16];
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x400);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[20] = sentinel;
            x86.k[3] = 1;
        }
        let exit = execute_lifted_x86(&[0x62, 0xE2, 0xD5, 0x33, 0xB5, 0x20], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[20], sentinel);
        }
    }
    #[test]
    fn executes_evex_map5_fp16_arithmetic_masks_and_zeroes_upper_state() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        let mask = 0xa55a_a55au64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [u64::MAX; 16];
            x86.xmm[2] = [0x3c00_3c00_3c00_3c00; 16];
            x86.xmm[3] = [0x4000_4000_4000_4000; 16];
            x86.k[4] = mask;
        }

        execute_lifted_x86(&[0x62, 0xF5, 0x6C, 0xCC, 0x58, 0xCB], &mut ctx, &mut memory);

        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..32u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 16),
                    if mask & (1u64 << lane) != 0 {
                        0x4200
                    } else {
                        0
                    }
                );
            }
            assert_eq!(&x86.xmm[1][8..], &[0; 8]);
        }

        // 1.0 + 2^-11 is exactly halfway between adjacent FP16 values at 1.0;
        // MXCSR round-toward-positive must select the upper value.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = [0x3c00_3c00_3c00_3c00; 16];
            x86.xmm[3] = [0x1000_1000_1000_1000; 16];
            x86.mxcsr = 0x5f80;
        }
        execute_lifted_x86(&[0x62, 0xF5, 0x6C, 0x48, 0x58, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16), 0x3c01);
        }

        memory
            .write(0x80, &[0x00, 0x40].repeat(32))
            .expect("FP16 full-width source");
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.gpr[0] = 0x80;
            x86.xmm[2] = [0x3c00_3c00_3c00_3c00; 16];
        }
        execute_lifted_x86(&[0x62, 0xF5, 0x6C, 0x48, 0x58, 0x08], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..32u8 {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 16), 0x4200);
            }
        }

        memory
            .write(0xC0, &0x4000u16.to_le_bytes())
            .expect("FP16 broadcast source");
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.gpr[0] = 0xC0;
            x86.xmm[2] = [0x3c00_3c00_3c00_3c00; 16];
        }
        execute_lifted_x86(&[0x62, 0xF5, 0x6C, 0x58, 0x59, 0x08], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..32u8 {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 16), 0x4000);
            }
        }

        let sentinel = [0xA55A_A55A_A55A_A55A; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.gpr[0] = 0x200;
            x86.xmm[1] = sentinel;
            x86.k[4] = 0;
        }
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF5, 0x6C, 0x0C, 0x5C, 0x08], &mut ctx, &mut memory);
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[1][..2], &sentinel[..2]);
            assert_eq!(&x86.xmm[1][2..], &[0; 14]);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[4] = 1;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF5, 0x6C, 0x0C, 0x5E, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel, "fault must not commit destination");
        }
    }
    #[test]
    fn executes_evex_fp16_minmax_selection_exceptions_masks_and_scalar_merge() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        let sentinel = [0xA55A_A55A_A55A_A55A; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[2] = [0; 16];
            x86.xmm[3] = [0; 16];
            for (lane, a, b) in [
                (0, 0x3C00, 0x4000), // 1 < 2: select source 1.
                (1, 0x0000, 0x8000), // Equal zero: preserve source-2 sign.
                (2, 0x7E11, 0x3C00), // Source-1 QNaN: select source 2.
                (3, 0x3C00, 0xFE22), // Source-2 QNaN: preserve its payload.
                (4, 0x3C00, 0x7C01), // Source-2 SNaN remains signaling.
                (5, 0x0001, 0x3C00), // FP16 denormal remains nonzero with DAZ.
            ] {
                SmirInterpreter::set_lane(&mut x86.xmm[2], lane, 16, a);
                SmirInterpreter::set_lane(&mut x86.xmm[3], lane, 16, b);
            }
            x86.mxcsr |= 1 << 6; // DAZ does not zero AVX512-FP16 inputs.
        }

        let exit = execute_lifted_x86(&[0x62, 0xF5, 0x6C, 0x08, 0x5D, 0xCB], &mut ctx, &mut memory);
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for (lane, expected) in [
                (0, 0x3C00),
                (1, 0x8000),
                (2, 0x3C00),
                (3, 0xFE22),
                (4, 0x7C01),
                (5, 0x0001),
            ] {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 16), expected);
            }
            assert_eq!(x86.mxcsr & 0x3, 0x3, "invalid and denormal status");
            assert_eq!(&x86.xmm[1][2..], &[0; 14]);
        }

        // Inactive lanes neither inspect exceptional inputs nor access memory.
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x200);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[2] = [0x3C00_3C00_3C00_3C00; 16];
            x86.k[2] = 0;
            x86.mxcsr = 0x1F80;
        }
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF5, 0x6C, 0x0A, 0x5F, 0x08], &mut ctx, &mut memory);
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr, 0x1F80);
            assert_eq!(&x86.xmm[1][..2], &sentinel[..2]);
            assert_eq!(&x86.xmm[1][2..], &[0; 14]);
        }

        // An unmasked invalid exception records MXCSR.IE and traps before the
        // architectural destination is committed.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[2] = [0x3C00_3C00_3C00_3C00; 16];
            x86.xmm[3] = [0x7C01_7C01_7C01_7C01; 16];
            x86.mxcsr = 0x1F80 & !(1 << 7);
        }
        let trap = execute_lifted_x86(&[0x62, 0xF5, 0x6C, 0x08, 0x5F, 0xCB], &mut ctx, &mut memory);
        assert!(matches!(
            trap,
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr & 1, 1);
            assert_eq!(x86.xmm[1], sentinel);
        }

        // Packed SAE suppresses status and traps while preserving source-2
        // SNaN bits in every selected lane.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.mxcsr = 0x1F80 & !(1 << 7);
        }
        let sae = execute_lifted_x86(&[0x62, 0xF5, 0x6C, 0x18, 0x5F, 0xCB], &mut ctx, &mut memory);
        assert!(matches!(sae, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr & 1, 0);
            for lane in 0..32u8 {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 16), 0x7C01);
            }
        }

        // Scalar VMINSH updates only lane 0, merges lanes 1..7 from source 1,
        // and clears all architectural state above bit 127.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[2] = [0; 16];
            x86.xmm[3] = [0; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x4000);
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 16, 0x3C00);
            for lane in 1..8u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[2], lane, 16, 0x4400 + u64::from(lane));
            }
            x86.mxcsr = 0x1F80;
        }
        let scalar =
            execute_lifted_x86(&[0x62, 0xF5, 0x6E, 0x08, 0x5D, 0xCB], &mut ctx, &mut memory);
        assert!(matches!(scalar, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16), 0x3C00);
            for lane in 1..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 16),
                    0x4400 + u64::from(lane)
                );
            }
            assert_eq!(&x86.xmm[1][2..], &[0; 14]);
        }
    }
    #[test]
    fn executes_evex_map5_fp16_embedded_rounding_independent_of_mxcsr() {
        for (p2, mxcsr_rounding, expected_positive, expected_negative) in [
            (0x18, 2u32, 0x3c00, 0xbc00), // RN-SAE; MXCSR requests RU.
            (0x38, 2u32, 0x3c00, 0xbc01), // RD-SAE; MXCSR requests RU.
            (0x58, 1u32, 0x3c01, 0xbc00), // RU-SAE; MXCSR requests RD.
            (0x78, 2u32, 0x3c00, 0xbc00), // RZ-SAE; MXCSR requests RU.
        ] {
            let mut ctx = SmirContext::new_x86_64();
            let mut memory = FlatMemory::new(0x10);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mxcsr = (x86.mxcsr & !(3 << 13)) | (mxcsr_rounding << 13);
                for lane in 0..32u8 {
                    let negative = lane & 1 != 0;
                    SmirInterpreter::set_lane(
                        &mut x86.xmm[2],
                        lane,
                        16,
                        if negative { 0xbc00 } else { 0x3c00 },
                    );
                    SmirInterpreter::set_lane(
                        &mut x86.xmm[3],
                        lane,
                        16,
                        if negative { 0x9000 } else { 0x1000 },
                    );
                }
            }

            let result =
                execute_lifted_x86(&[0x62, 0xF5, 0x6C, p2, 0x58, 0xCB], &mut ctx, &mut memory);
            assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                for lane in 0..32u8 {
                    assert_eq!(
                        SmirInterpreter::get_lane(&x86.xmm[1], lane, 16),
                        if lane & 1 != 0 {
                            expected_negative
                        } else {
                            expected_positive
                        },
                        "P2={p2:#04x}, lane={lane}"
                    );
                }
            }
        }
    }
    #[test]
    fn lifted_evex_fp16_sqrt_executes_masks_merges_aliases_and_suppresses_faults() {
        let expected = |raw: u16, rounding: u8| {
            SmirInterpreter::x86_f32_to_fp16(SmirInterpreter::x86_fp16_to_f32(raw).sqrt(), rounding)
        };
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        let sentinel = [0xA55A_A55A_A55A_A55A; 16];
        let mask = 0xA55A_C33Cu64;
        let flags_before = 0xCD7;
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            for lane in 0..32u8 {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[19],
                    lane,
                    16,
                    u64::from([0x0000u16, 0x3C00, 0x4400, 0x4880][usize::from(lane & 3)]),
                );
            }
            x86.k[3] = mask;
        }
        execute_lifted_x86(&[0x62, 0xA5, 0x7C, 0xCB, 0x51, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..32u8 {
                let input = [0x0000u16, 0x3C00, 0x4400, 0x4880][usize::from(lane & 3)];
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 16),
                    if mask & (1u64 << lane) != 0 {
                        u64::from(expected(input, 0))
                    } else {
                        0
                    },
                    "VSQRTPH lane {lane}",
                );
            }
            assert_eq!(&x86.xmm[17][8..], &[0; 8]);
        }

        // Scalar masking applies only to lane 0; lanes 1..7 always merge from
        // SRC1 and all state above bit 127 is cleared.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.xmm[18] = [0; 16];
            x86.xmm[19] = [0; 16];
            for lane in 0..8u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[18], lane, 16, 0x1100 + u64::from(lane));
            }
            SmirInterpreter::set_lane(&mut x86.xmm[19], 0, 16, 0x4400);
            x86.k[3] = 1;
        }
        execute_lifted_x86(&[0x62, 0xA5, 0x6E, 0x83, 0x51, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[17], 0, 16), 0x4000);
            for lane in 1..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 16),
                    0x1100 + u64::from(lane),
                );
            }
            assert_eq!(&x86.xmm[17][2..], &[0; 14]);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.k[3] = 0;
        }
        execute_lifted_x86(&[0x62, 0xA5, 0x6E, 0x03, 0x51, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[17], 0, 16), 0xA55A);
            for lane in 1..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 16),
                    0x1100 + u64::from(lane),
                );
            }
        }

        // All architectural operands may alias; source bits are snapshotted
        // before the scalar XMM reconstruction clears upper state.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = [0; 16];
            for lane in 0..8u8 {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[8],
                    lane,
                    16,
                    if lane == 0 {
                        0x4880
                    } else {
                        0x2200 + u64::from(lane)
                    },
                );
            }
        }
        execute_lifted_x86(&[0x62, 0x55, 0x3E, 0x08, 0x51, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[8], 0, 16), 0x4200);
            for lane in 1..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[8], lane, 16),
                    0x2200 + u64::from(lane),
                );
            }
            assert_eq!(&x86.xmm[8][2..], &[0; 14]);
        }

        // Type E2/E3 mask fault suppression applies to both packed broadcast
        // and scalar memory operands, and faults precede destination writes.
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x200);
        for (bytes, dst, mask_reg) in [
            (&[0x62, 0xF5, 0x7C, 0x5A, 0x51, 0x08][..], 1usize, 2usize),
            (&[0x62, 0xF5, 0x6E, 0x0A, 0x51, 0x08][..], 1usize, 2usize),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[dst] = sentinel;
                x86.k[mask_reg] = 0;
            }
            let suppressed = execute_lifted_x86(bytes, &mut ctx, &mut memory);
            assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[dst] = sentinel;
                x86.k[mask_reg] = 1;
            }
            let fault = execute_lifted_x86(bytes, &mut ctx, &mut memory);
            assert!(matches!(
                fault,
                BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
            ));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[dst], sentinel);
            }
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_evex_vmovsh_executes_aliases_masks_load_store_and_fault_suppression() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        let sentinel = [0xA55A_A55A_A55A_A55A; 16];
        let flags_before = 0xCD7;
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[2] = [0; 16];
            x86.xmm[3] = [0; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 16, 0x4567);
            for lane in 1..8u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[2], lane, 16, 0x1200 + u64::from(lane));
            }
        }
        execute_lifted_x86(&[0x62, 0xF5, 0x6E, 0x08, 0x10, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16), 0x4567);
            for lane in 1..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 16),
                    0x1200 + u64::from(lane),
                );
            }
            assert_eq!(&x86.xmm[1][2..], &[0; 14]);
        }

        // The opcode 11h register alias accepts complete operand aliasing.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = [0; 16];
            for lane in 0..8u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[8], lane, 16, 0x2200 + u64::from(lane));
            }
        }
        execute_lifted_x86(&[0x62, 0x55, 0x3E, 0x08, 0x11, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[8], lane, 16),
                    0x2200 + u64::from(lane),
                );
            }
            assert_eq!(&x86.xmm[8][2..], &[0; 14]);
        }

        // A masked-off register move merges or zeroes only the low lane;
        // lanes 1..=7 still come from SRC1.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[2] = 0;
        }
        execute_lifted_x86(&[0x62, 0xF5, 0x6E, 0x0A, 0x10, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16), 0xA55A);
            for lane in 1..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 16),
                    0x1200 + u64::from(lane),
                );
            }
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
        }
        execute_lifted_x86(&[0x62, 0xF5, 0x6E, 0x8A, 0x10, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16), 0);
            for lane in 1..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 16),
                    0x1200 + u64::from(lane),
                );
            }
        }

        memory.write(0x100, &0xBEEFu16.to_le_bytes()).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
        }
        execute_lifted_x86(&[0x62, 0xF5, 0x7E, 0x08, 0x10, 0x08], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16), 0xBEEF);
            assert_eq!(&x86.xmm[1][1..], &[0; 15]);
        }

        // Type E5 suppresses masked-off memory accesses. A two-operand load
        // still clears everything above the selected low lane when it retires.
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x200);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[2] = 0;
        }
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF5, 0x7E, 0x0A, 0x10, 0x08], &mut ctx, &mut memory);
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16), 0xA55A);
            assert_eq!(&x86.xmm[1][1..], &[0; 15]);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[2] = 1;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF5, 0x7E, 0x0A, 0x10, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }

        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x100);
        memory.write(0x100, &[0xAA, 0xBB]).unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = [0; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x1234);
            x86.k[2] = 0;
        }
        execute_lifted_x86(&[0x62, 0xF5, 0x7E, 0x0A, 0x11, 0x10], &mut ctx, &mut memory);
        let mut stored = [0u8; 2];
        memory.read(0x100, &mut stored).unwrap();
        assert_eq!(stored, [0xAA, 0xBB]);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[2] = 1;
        }
        execute_lifted_x86(&[0x62, 0xF5, 0x7E, 0x0A, 0x11, 0x10], &mut ctx, &mut memory);
        memory.read(0x100, &mut stored).unwrap();
        assert_eq!(u16::from_le_bytes(stored), 0x1234);

        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x200);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[2] = 0;
        }
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF5, 0x7E, 0x0A, 0x11, 0x10], &mut ctx, &mut memory);
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[2] = 1;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF5, 0x7E, 0x0A, 0x11, 0x10], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_evex_fp16_scalar_arithmetic_executes_ops_masks_aliases_and_faults() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        let sentinel = [0xA55A_A55A_A55A_A55A; 16];
        let flags_before = 0xCD7;
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for (opcode, first, second, expected) in [
            (0x58, 0x3C00u16, 0x4000u16, 0x4200u16), // 1 + 2 = 3
            (0x59, 0x4200, 0x4000, 0x4600),          // 3 * 2 = 6
            (0x5C, 0x4500, 0x4000, 0x4200),          // 5 - 2 = 3
            (0x5E, 0x4600, 0x4000, 0x4200),          // 6 / 2 = 3
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1] = sentinel;
                x86.xmm[2] = [0; 16];
                x86.xmm[3] = [0; 16];
                for lane in 0..8u8 {
                    SmirInterpreter::set_lane(
                        &mut x86.xmm[2],
                        lane,
                        16,
                        if lane == 0 {
                            u64::from(first)
                        } else {
                            0x1100 + u64::from(lane)
                        },
                    );
                }
                SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 16, u64::from(second));
            }
            execute_lifted_x86(
                &[0x62, 0xF5, 0x6E, 0x08, opcode, 0xCB],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], 0, 16),
                    u64::from(expected),
                    "scalar FP16 opcode {opcode:#04x}",
                );
                for lane in 1..8u8 {
                    assert_eq!(
                        SmirInterpreter::get_lane(&x86.xmm[1], lane, 16),
                        0x1100 + u64::from(lane),
                    );
                }
                assert_eq!(&x86.xmm[1][2..], &[0; 14]);
            }
        }

        // The low destination lane observes merge/zero masking independently
        // of the seven upper lanes, which always come from SRC1.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[2] = [0; 16];
            x86.xmm[3] = [0; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x3C00);
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 16, 0x4000);
            for lane in 1..8u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[2], lane, 16, 0x3300 + u64::from(lane));
            }
            x86.k[2] = 0;
        }
        execute_lifted_x86(&[0x62, 0xF5, 0x6E, 0x0A, 0x58, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16), 0xA55A);
            for lane in 1..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 16),
                    0x3300 + u64::from(lane),
                );
            }
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[2] = 0;
        }
        execute_lifted_x86(&[0x62, 0xF5, 0x6E, 0x8A, 0x59, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16), 0);
            for lane in 1..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 16),
                    0x3300 + u64::from(lane),
                );
            }
        }

        // All operands may alias without consuming reconstructed destination
        // lanes as inputs to the scalar operation.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = [0; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[8], 0, 16, 0x4000);
            for lane in 1..8u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[8], lane, 16, 0x4400 + u64::from(lane));
            }
        }
        execute_lifted_x86(&[0x62, 0x55, 0x3E, 0x08, 0x58, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[8], 0, 16), 0x4400);
            for lane in 1..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[8], lane, 16),
                    0x4400 + u64::from(lane),
                );
            }
            assert_eq!(&x86.xmm[8][2..], &[0; 14]);
        }

        // Type E3 all-zero masking suppresses the scalar source read; an
        // active mask faults before any destination lane is committed.
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x200);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[2] = 0;
        }
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF5, 0x6E, 0x0A, 0x5E, 0x08], &mut ctx, &mut memory);
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[2] = 1;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF5, 0x6E, 0x0A, 0x5E, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn executes_evex_compress_expand_all_elements_aliases_dense_memory_and_faults() {
        let cases = [
            (
                &[0x62, 0xF2, 0x7D, 0x09, 0x63, 0xD1][..],
                true,
                8u32,
                16u8,
                1usize,
                2usize,
                1usize,
                false,
            ),
            (
                &[0x62, 0xF2, 0xFD, 0xAB, 0x63, 0xEC][..],
                true,
                16,
                16,
                4,
                5,
                3,
                true,
            ),
            (
                &[0x62, 0xA2, 0x7D, 0x4A, 0x8B, 0xD1][..],
                true,
                32,
                16,
                17,
                18,
                2,
                false,
            ),
            (
                &[0x62, 0xF2, 0xFD, 0x89, 0x8B, 0xD9][..],
                true,
                64,
                2,
                1,
                3,
                1,
                true,
            ),
            (
                &[0x62, 0xF2, 0x7D, 0x2B, 0x8A, 0xF4][..],
                true,
                32,
                8,
                4,
                6,
                3,
                false,
            ),
            (
                &[0x62, 0xA2, 0xFD, 0xCA, 0x8A, 0xD9][..],
                true,
                64,
                8,
                17,
                19,
                2,
                true,
            ),
            (
                &[0x62, 0xF2, 0x7D, 0x09, 0x62, 0xCA][..],
                false,
                8,
                16,
                1,
                2,
                1,
                false,
            ),
            (
                &[0x62, 0xF2, 0xFD, 0xAB, 0x62, 0xE5][..],
                false,
                16,
                16,
                4,
                5,
                3,
                true,
            ),
            (
                &[0x62, 0xA2, 0x7D, 0x4A, 0x89, 0xCA][..],
                false,
                32,
                16,
                17,
                18,
                2,
                false,
            ),
            (
                &[0x62, 0xF2, 0xFD, 0x89, 0x89, 0xCB][..],
                false,
                64,
                2,
                1,
                3,
                1,
                true,
            ),
            (
                &[0x62, 0xF2, 0x7D, 0x2B, 0x88, 0xE6][..],
                false,
                32,
                8,
                4,
                6,
                3,
                false,
            ),
            (
                &[0x62, 0xA2, 0xFD, 0xCA, 0x88, 0xCB][..],
                false,
                64,
                8,
                17,
                19,
                2,
                true,
            ),
        ];
        let mut memory = FlatMemory::new(0x400);
        for (bytes, compress, bits, lanes, dst, src, mask_reg, zeroing) in cases {
            let mask = match lanes {
                2 => 0b10,
                8 => 0b1010_0101,
                16 => 0b1010_0101_1100_0011,
                _ => unreachable!(),
            };
            let mut source = [0x1111_1111_1111_1111; 16];
            let mut old = [0x2222_2222_2222_2222; 16];
            for lane in 0..lanes {
                SmirInterpreter::set_lane(&mut source, lane, bits, u64::from(lane) + 1);
                SmirInterpreter::set_lane(&mut old, lane, bits, u64::from(lane) + 0x80);
            }
            let mut expected = [0u64; 16];
            if compress {
                let mut output = 0;
                for lane in 0..lanes {
                    if mask & (1u64 << lane) != 0 {
                        SmirInterpreter::set_lane(
                            &mut expected,
                            output,
                            bits,
                            SmirInterpreter::get_lane(&source, lane, bits),
                        );
                        output += 1;
                    }
                }
                if !zeroing {
                    for lane in output..lanes {
                        SmirInterpreter::set_lane(
                            &mut expected,
                            lane,
                            bits,
                            SmirInterpreter::get_lane(&old, lane, bits),
                        );
                    }
                }
            } else {
                let mut input = 0;
                for lane in 0..lanes {
                    if mask & (1u64 << lane) != 0 {
                        SmirInterpreter::set_lane(
                            &mut expected,
                            lane,
                            bits,
                            SmirInterpreter::get_lane(&source, input, bits),
                        );
                        input += 1;
                    } else if !zeroing {
                        SmirInterpreter::set_lane(
                            &mut expected,
                            lane,
                            bits,
                            SmirInterpreter::get_lane(&old, lane, bits),
                        );
                    }
                }
            }

            let mut ctx = SmirContext::new_x86_64();
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[dst] = old;
                x86.xmm[src] = source;
                x86.k[mask_reg] = mask;
            }
            execute_lifted_x86(bytes, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[dst], expected, "case {bytes:02X?}");
            }
        }

        // Source/destination aliasing uses the complete original register.
        let mut ctx = SmirContext::new_x86_64();
        let mut alias = [0u64; 16];
        for lane in 0..16u8 {
            SmirInterpreter::set_lane(&mut alias, lane, 8, u64::from(lane) + 1);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = alias;
            x86.k[1] = 0b1010_0101_1100_0011;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x09, 0x63, 0xC9], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let selected = [1u8, 2, 7, 8, 9, 11, 14, 16];
            for (lane, value) in selected.into_iter().enumerate() {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane as u8, 8),
                    u64::from(value)
                );
            }
        }

        // Dense memory stores contain only selected elements, in source order.
        memory.write(0x100, &[0xCC; 64]).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            let mut source = [0u64; 16];
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut source, lane, 32, u64::from(lane) + 100);
            }
            x86.xmm[18] = source;
            x86.k[2] = (1 << 0) | (1 << 3) | (1 << 5);
        }
        execute_lifted_x86(&[0x62, 0xE2, 0x7D, 0x4A, 0x8B, 0x10], &mut ctx, &mut memory);
        let mut stored = [0u8; 16];
        memory.read(0x100, &mut stored).unwrap();
        assert_eq!(
            &stored[..12],
            &[100u32, 103, 105].map(u32::to_le_bytes).concat()
        );
        assert_eq!(&stored[12..], &[0xCC; 4]);

        // Dense memory loads are distributed to sparse destination lanes.
        memory
            .write(0x180, &[11u32, 22, 33].map(u32::to_le_bytes).concat())
            .unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x180);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = [0xDEAD_BEEF_DEAD_BEEF; 16];
            x86.k[2] = (1 << 0) | (1 << 3) | (1 << 5);
        }
        execute_lifted_x86(&[0x62, 0xE2, 0x7D, 0xCA, 0x89, 0x08], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 32),
                    match lane {
                        0 => 11,
                        3 => 22,
                        5 => 33,
                        _ => 0,
                    }
                );
            }
            assert_eq!(&x86.xmm[17][8..], &[0; 8]);
        }

        // A faulting expand leaves the architectural destination unchanged;
        // a faulting compress may have completed earlier dense stores.
        memory.write(0x3FF, &[0x5A]).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3FF);
        let sentinel = [0x4242_4242_4242_4242; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 0b11;
        }
        let load_fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x09, 0x62, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            load_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = alias;
            x86.k[1] = 0b11;
        }
        let store_fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x09, 0x63, 0x10], &mut ctx, &mut memory);
        assert!(matches!(
            store_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        let mut first = [0u8; 1];
        memory.read(0x3FF, &mut first).unwrap();
        assert_eq!(first[0], 1);
    }
    #[test]
    fn executes_evex_packed_rotates_counts_masks_aliases_broadcasts_and_faults() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        let flags_before = 0xCD7;
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Immediate D-word rotate covers count reduction and upper-state zeroing.
        let dwords = [0x0123_4567u32, 0x8000_0001, 0xFFFF_FFFF, 0x1357_9BDF];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0xCCCC_CCCC_CCCC_CCCC; 16];
            for (lane, value) in dwords.into_iter().enumerate() {
                SmirInterpreter::set_lane(&mut x86.xmm[2], lane as u8, 32, u64::from(value));
            }
        }
        execute_lifted_x86(
            &[0x62, 0xF1, 0x75, 0x08, 0x72, 0xCA, 39],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for (lane, value) in dwords.into_iter().enumerate() {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane as u8, 32),
                    u64::from(value.rotate_left(39 % 32))
                );
            }
            assert_eq!(&x86.xmm[1][2..], &[0; 14]);
        }

        // Per-lane counts use the complete element and reduce modulo its width.
        let counts = [0u32, 1, 32, 255];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..4u8 {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[2],
                    lane,
                    32,
                    u64::from(dwords[usize::from(lane)]),
                );
                SmirInterpreter::set_lane(
                    &mut x86.xmm[3],
                    lane,
                    32,
                    u64::from(counts[usize::from(lane)]),
                );
            }
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0x15, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..4usize {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane as u8, 32),
                    u64::from(dwords[lane].rotate_left(counts[lane] % 32))
                );
            }
        }

        // All three operands may alias; reads must be snapshotted before write.
        let mut alias = [0u64; 16];
        for (lane, value) in [1u32, 7, 31, 0x8000_0001].into_iter().enumerate() {
            SmirInterpreter::set_lane(&mut alias, lane as u8, 32, u64::from(value));
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = alias;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x08, 0x15, 0xC9], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..4u8 {
                let value = SmirInterpreter::get_lane(&alias, lane, 32) as u32;
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                    u64::from(value.rotate_left(value % 32))
                );
            }
        }

        // Merge masking and right rotation preserve inactive destination lanes.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[5] = [0xA5A5_A5A5_A5A5_A5A5; 16];
            for lane in 0..4u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[6], lane, 32, 1u64 << lane);
            }
            x86.k[1] = 0b0101;
        }
        execute_lifted_x86(
            &[0x62, 0xF1, 0x55, 0x09, 0x72, 0xC6, 5],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..4u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[5], lane, 32),
                    if lane & 1 == 0 {
                        u64::from((1u32 << lane).rotate_right(5))
                    } else {
                        0xA5A5_A5A5
                    }
                );
            }
        }

        // Compressed disp8 broadcast plus zero masking.
        memory.write(508, &0x8000_0001u32.to_le_bytes()).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3] = [u64::MAX; 16];
            x86.k[2] = 0b1010_0101;
        }
        execute_lifted_x86(
            &[0x62, 0xF1, 0x65, 0xBA, 0x72, 0x48, 0x7F, 31],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[3], lane, 32),
                    if 0b1010_0101 & (1 << lane) != 0 {
                        u64::from(0x8000_0001u32.rotate_left(31))
                    } else {
                        0
                    }
                );
            }
            assert_eq!(&x86.xmm[3][4..], &[0; 12]);
        }

        // E4 suppresses all memory access for an all-zero mask. If one lane is
        // active, a fault occurs before the architectural destination commits.
        let sentinel = [0x4242_4242_4242_4242; 16];
        let mut architectural_sentinel = [0u64; 16];
        architectural_sentinel[..8].copy_from_slice(&sentinel[..8]);
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[5] = sentinel;
            x86.xmm[6] = alias;
            x86.k[2] = 0;
        }
        let suppressed = execute_lifted_x86(
            &[0x62, 0xF2, 0x4D, 0x5A, 0x14, 0x68, 0x01],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[5], architectural_sentinel);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[2] = 1;
        }
        let fault = execute_lifted_x86(
            &[0x62, 0xF2, 0x4D, 0x5A, 0x14, 0x68, 0x01],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[5], architectural_sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn executes_evex_ternary_logic_all_tables_masks_aliases_memory_and_faults() {
        fn ternary(a: u64, b: u64, c: u64, imm: u8) -> u64 {
            let mut out = 0u64;
            for bit in 0..64 {
                let index = (((a >> bit) & 1) << 2) | (((b >> bit) & 1) << 1) | ((c >> bit) & 1);
                out |= u64::from((imm >> index) & 1) << bit;
            }
            out
        }

        let a = [0x0123_4567_89AB_CDEFu64, 0xFFFF_0000_AAAA_5555];
        let b = [0x1357_9BDF_2468_ACE0u64, 0x0F0F_F0F0_3333_CCCC];
        let c = [0xCAFE_BABE_DEAD_BEEFu64, 0x55AA_55AA_FF00_00FF];
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        let flags_before = 0xCD7;
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Exhaust the complete imm8 truth-table space. Destination input A is
        // read before the same architectural register is overwritten.
        for imm in 0u8..=u8::MAX {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1] = [0xCCCC_CCCC_CCCC_CCCC; 16];
                x86.xmm[1][..2].copy_from_slice(&a);
                x86.xmm[2][..2].copy_from_slice(&b);
                x86.xmm[3][..2].copy_from_slice(&c);
            }
            execute_lifted_x86(
                &[0x62, 0xF3, 0x6D, 0x08, 0x25, 0xCB, imm],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[1][0], ternary(a[0], b[0], c[0], imm), "imm={imm}");
                assert_eq!(x86.xmm[1][1], ternary(a[1], b[1], c[1], imm), "imm={imm}");
                assert_eq!(&x86.xmm[1][2..], &[0; 14], "imm={imm}");
            }
        }

        // High YMM registers, Q-word granularity, and zero masking.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[20] = [0xAAAA_AAAA_AAAA_AAAA; 16];
            x86.xmm[21][..4].copy_from_slice(&[
                0x1111_1111_1111_1111,
                0x2222_2222_2222_2222,
                0x3333_3333_3333_3333,
                0x4444_4444_4444_4444,
            ]);
            x86.xmm[22][..4].copy_from_slice(&[
                0xFFFF_0000_FFFF_0000,
                0x0000_FFFF_0000_FFFF,
                0x55AA_55AA_55AA_55AA,
                0xAA55_AA55_AA55_AA55,
            ]);
            x86.k[3] = 0b0101;
        }
        let old = [0xAAAA_AAAA_AAAA_AAAAu64; 4];
        let src2 = [
            0x1111_1111_1111_1111u64,
            0x2222_2222_2222_2222,
            0x3333_3333_3333_3333,
            0x4444_4444_4444_4444,
        ];
        let src3 = [
            0xFFFF_0000_FFFF_0000u64,
            0x0000_FFFF_0000_FFFF,
            0x55AA_55AA_55AA_55AA,
            0xAA55_AA55_AA55_AA55,
        ];
        execute_lifted_x86(
            &[0x62, 0xA3, 0xD5, 0xA3, 0x25, 0xE6, 0xE2],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..4usize {
                assert_eq!(
                    x86.xmm[20][lane],
                    if lane & 1 == 0 {
                        ternary(old[lane], src2[lane], src3[lane], 0xE2)
                    } else {
                        0
                    }
                );
            }
            assert_eq!(&x86.xmm[20][4..], &[0; 12]);
        }

        // E4 broadcast source uses compressed disp8 and preserves inactive
        // destination lanes under merge masking.
        memory.write(508, &0x8000_0001u32.to_le_bytes()).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::R13)), 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = [0xA5A5_A5A5_A5A5_A5A5; 16];
            x86.xmm[18] = [0x5A5A_5A5A_5A5A_5A5A; 16];
            x86.k[7] = 0x5555;
        }
        execute_lifted_x86(
            &[0x62, 0xC3, 0x6D, 0x57, 0x25, 0x4D, 0x7F, 0xE4],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 32),
                    if lane & 1 == 0 {
                        ternary(0xA5A5_A5A5, 0x5A5A_5A5A, 0x8000_0001, 0xE4) & 0xFFFF_FFFF
                    } else {
                        0xA5A5_A5A5
                    }
                );
            }
            assert_eq!(&x86.xmm[17][8..], &[0; 8]);
        }

        // An all-zero mask suppresses the out-of-range broadcast access; an
        // active lane faults before any architectural destination write.
        let mut sentinel = [0u64; 16];
        sentinel[..8].fill(0x4242_4242_4242_4242);
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::R13)), 0x204);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.k[7] = 0;
        }
        let suppressed = execute_lifted_x86(
            &[0x62, 0xC3, 0x6D, 0x57, 0x25, 0x4D, 0x7F, 0xE4],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[17], sentinel);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[7] = 1;
        }
        let fault = execute_lifted_x86(
            &[0x62, 0xC3, 0x6D, 0x57, 0x25, 0x4D, 0x7F, 0xE4],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[17], sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn executes_evex_packed_funnel_shifts_all_forms_masks_memory_and_faults() {
        fn reference(src: u64, fill: u64, count: u64, bits: u32, left: bool) -> u64 {
            let mask = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            let count = (count % u64::from(bits)) as u32;
            if count == 0 {
                src & mask
            } else if left {
                ((src << count) | (fill >> (bits - count))) & mask
            } else {
                (src >> count) | ((fill << (bits - count)) & mask)
            }
        }

        let cases = [
            (
                &[0x62, 0xF3, 0xED, 0x08, 0x70, 0xCB, 23][..],
                16,
                true,
                false,
                23,
            ),
            (
                &[0x62, 0xF3, 0x6D, 0x08, 0x71, 0xCB, 47][..],
                32,
                true,
                false,
                47,
            ),
            (
                &[0x62, 0xF3, 0xED, 0x08, 0x71, 0xCB, 79][..],
                64,
                true,
                false,
                79,
            ),
            (
                &[0x62, 0xF3, 0xED, 0x08, 0x72, 0xCB, 17][..],
                16,
                false,
                false,
                17,
            ),
            (
                &[0x62, 0xF3, 0x6D, 0x08, 0x73, 0xCB, 39][..],
                32,
                false,
                false,
                39,
            ),
            (
                &[0x62, 0xF3, 0xED, 0x08, 0x73, 0xCB, 65][..],
                64,
                false,
                false,
                65,
            ),
            (&[0x62, 0xF2, 0xED, 0x08, 0x70, 0xCB][..], 16, true, true, 0),
            (&[0x62, 0xF2, 0x6D, 0x08, 0x71, 0xCB][..], 32, true, true, 0),
            (&[0x62, 0xF2, 0xED, 0x08, 0x71, 0xCB][..], 64, true, true, 0),
            (
                &[0x62, 0xF2, 0xED, 0x08, 0x72, 0xCB][..],
                16,
                false,
                true,
                0,
            ),
            (
                &[0x62, 0xF2, 0x6D, 0x08, 0x73, 0xCB][..],
                32,
                false,
                true,
                0,
            ),
            (
                &[0x62, 0xF2, 0xED, 0x08, 0x73, 0xCB][..],
                64,
                false,
                true,
                0,
            ),
        ];
        let flags_before = 0xCD7;
        let mut memory = FlatMemory::new(0x400);
        for (bytes, bits, left, variable, immediate) in cases {
            let mut ctx = SmirContext::new_x86_64();
            ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
            ctx.flags.lazy = None;
            let lanes = 128 / bits;
            let mut old = [0u64; 16];
            let mut second = [0u64; 16];
            let mut third = [0u64; 16];
            for lane in 0..lanes as u8 {
                SmirInterpreter::set_lane(
                    &mut old,
                    lane,
                    bits,
                    0xA5A5_A5A5_A5A5_A5A5u64.rotate_left(u32::from(lane)),
                );
                SmirInterpreter::set_lane(
                    &mut second,
                    lane,
                    bits,
                    0x1357_9BDF_2468_ACE0u64.rotate_right(u32::from(lane)),
                );
                SmirInterpreter::set_lane(
                    &mut third,
                    lane,
                    bits,
                    if variable {
                        [0, 1, u64::from(bits), u64::MAX][usize::from(lane) & 3]
                    } else {
                        0xCAFE_BABE_DEAD_BEEFu64.rotate_left(u32::from(lane))
                    },
                );
            }
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1] = old;
                x86.xmm[2] = second;
                x86.xmm[3] = third;
            }
            execute_lifted_x86(bytes, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                for lane in 0..lanes as u8 {
                    let primary = SmirInterpreter::get_lane(
                        if variable { &old } else { &second },
                        lane,
                        bits,
                    );
                    let fill = SmirInterpreter::get_lane(
                        if variable { &second } else { &third },
                        lane,
                        bits,
                    );
                    let count = if variable {
                        SmirInterpreter::get_lane(&third, lane, bits)
                    } else {
                        immediate
                    };
                    assert_eq!(
                        SmirInterpreter::get_lane(&x86.xmm[1], lane, bits),
                        reference(primary, fill, count, bits, left),
                        "bits={bits} left={left} variable={variable} lane={lane}"
                    );
                }
                assert_eq!(&x86.xmm[1][2..], &[0; 14]);
            }
            ctx.flags.materialize_all();
            assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
        }

        // Broadcast source, compressed disp8, zero mask, and E4 fault behavior.
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        memory.write(508, &0x8000_0001u32.to_le_bytes()).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[4] = [u64::MAX; 16];
            x86.xmm[5] = [0x1234_5678_9ABC_DEF0; 16];
            x86.k[2] = 0b0101_1010;
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x55, 0xBA, 0x71, 0x60, 0x7F, 31],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8u8 {
                let primary = SmirInterpreter::get_lane(&x86.xmm[5], lane, 32);
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[4], lane, 32),
                    if 0b0101_1010 & (1 << lane) != 0 {
                        reference(primary, 0x8000_0001, 31, 32, true)
                    } else {
                        0
                    }
                );
            }
            assert_eq!(&x86.xmm[4][4..], &[0; 12]);
        }

        let mut sentinel = [0u64; 16];
        sentinel[..4].fill(0x4242_4242_4242_4242);
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x204);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[4] = sentinel;
            x86.k[2] = 0;
        }
        let suppressed = execute_lifted_x86(
            &[0x62, 0xF3, 0x55, 0xBA, 0x71, 0x60, 0x7F, 31],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[4], [0; 16]);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[4] = sentinel;
            x86.k[2] = 1;
        }
        let fault = execute_lifted_x86(
            &[0x62, 0xF3, 0x55, 0xBA, 0x71, 0x60, 0x7F, 31],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[4], sentinel);
        }
    }
    #[test]
    fn executes_evex_multishift_qb_controls_masks_broadcast_and_e4nf_faults() {
        let controls = [
            0u8, 1, 7, 8, 15, 31, 56, 63, 64, 65, 71, 72, 79, 95, 120, 127,
        ];
        let sources = [0x0123_4567_89AB_CDEFu64, 0xFEDC_BA98_7654_3210];
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for (lane, control) in controls.into_iter().enumerate() {
                SmirInterpreter::set_lane(&mut x86.xmm[2], lane as u8, 8, u64::from(control));
            }
            x86.xmm[3][..2].copy_from_slice(&sources);
            x86.xmm[1] = [u64::MAX; 16];
        }
        execute_lifted_x86(&[0x62, 0xF2, 0xED, 0x08, 0x83, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 8),
                    sources[usize::from(lane / 8)]
                        .rotate_right(u32::from(controls[usize::from(lane)] & 63))
                        & 0xFF
                );
            }
            assert_eq!(&x86.xmm[1][2..], &[0; 14]);
        }

        // Broadcast supplies the same qword to every block; byte-granular
        // merge masking preserves inactive destination bytes.
        memory
            .write(1016, &0x8040_2010_0804_0201u64.to_le_bytes())
            .unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::R13)), 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = [0xA5A5_A5A5_A5A5_A5A5; 16];
            for lane in 0..64u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[18], lane, 8, u64::from(lane));
            }
            x86.k[7] = 0x5555_5555_5555_5555;
        }
        execute_lifted_x86(
            &[0x62, 0xC2, 0xED, 0x57, 0x83, 0x4D, 0x7F],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..64u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 8),
                    if lane & 1 == 0 {
                        0x8040_2010_0804_0201u64.rotate_right(u32::from(lane & 63)) & 0xFF
                    } else {
                        0xA5
                    }
                );
            }
            assert_eq!(&x86.xmm[17][8..], &[0; 8]);
        }

        // E4NF performs the complete full-vector access even when k1 is zero.
        let sentinel = [0x4242_4242_4242_4242; 16];
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3C0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[31] = sentinel;
            x86.k[1] = 0;
        }
        let fault = execute_lifted_x86(
            &[0x62, 0x62, 0x8D, 0xC1, 0x83, 0x78, 0x01],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[31], sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn executes_evex_scatter_vsib_masks_signed_indices_and_partial_faults() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.write_vreg(rax, 0x100);
        let indices = [0i32, 2, -1, 4];
        let values = [0x1111_1111u32, 0x2222_2222, 0x3333_3333, 0x4444_4444];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..4u8 {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[2],
                    lane,
                    32,
                    indices[usize::from(lane)] as u32 as u64,
                );
                SmirInterpreter::set_lane(
                    &mut x86.xmm[1],
                    lane,
                    32,
                    u64::from(values[usize::from(lane)]),
                );
            }
            x86.k[1] = 0b1111;
        }
        execute_lifted_x86(
            &[0x62, 0xF2, 0x7D, 0x09, 0xA0, 0x0C, 0x90],
            &mut ctx,
            &mut memory,
        );
        for lane in 0..4usize {
            let address = (0x100i64 + i64::from(indices[lane]) * 4) as u64;
            let mut actual = [0u8; 4];
            memory.read(address, &mut actual).unwrap();
            assert_eq!(u32::from_le_bytes(actual), values[lane]);
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[1], 0);
            for lane in 0..4u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                    u64::from(values[usize::from(lane)])
                );
            }
        }

        // Inactive lanes suppress invalid addresses completely.
        ctx.write_vreg(rax, 0x400);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[1] = 0;
        }
        let suppressed = execute_lifted_x86(
            &[0x62, 0xF2, 0x7D, 0x09, 0xA0, 0x0C, 0x90],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[1], 0);
        }

        // Completed stores clear their mask bits. A faulting lane retains its
        // bit and stops later lanes, while earlier memory writes remain visible.
        ctx.write_vreg(rax, 0x1FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 32, 0);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 1, 32, 1);
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 32, 0xAABB_CCDD);
            SmirInterpreter::set_lane(&mut x86.xmm[1], 1, 32, 0xEEFF_0011);
            x86.k[1] = 0b11;
        }
        let fault = execute_lifted_x86(
            &[0x62, 0xF2, 0x7D, 0x09, 0xA0, 0x0C, 0x90],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        let mut stored = [0u8; 4];
        memory.read(0x1FC, &mut stored).unwrap();
        assert_eq!(u32::from_le_bytes(stored), 0xAABB_CCDD);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[1], 0b10);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn executes_evex_vector_align_wrap_masks_aliases_broadcast_and_e4nf_faults() {
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..4u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[2], lane, 32, 10 + u64::from(lane));
                SmirInterpreter::set_lane(&mut x86.xmm[3], lane, 32, u64::from(lane));
            }
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x6D, 0x08, 0x03, 0xCB, 5],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for (lane, expected) in [1u64, 2, 3, 10].into_iter().enumerate() {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane as u8, 32),
                    expected
                );
            }
            assert_eq!(&x86.xmm[1][2..], &[0; 14]);
        }

        // Source 3 may alias the destination; all extracts precede masked writes.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..4u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 32, 20 + u64::from(lane));
                SmirInterpreter::set_lane(&mut x86.xmm[2], lane, 32, 30 + u64::from(lane));
            }
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x6D, 0x08, 0x03, 0xC9, 3],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                (0..4u8)
                    .map(|lane| SmirInterpreter::get_lane(&x86.xmm[1], lane, 32))
                    .collect::<Vec<_>>(),
                [23, 30, 31, 32]
            );
        }

        // Scalar memory broadcast and compressed disp8 with byte-mask merge.
        memory.write(508, &99u32.to_le_bytes()).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::R13)), 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = [0xA5A5_A5A5_A5A5_A5A5; 16];
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[18], lane, 32, 100 + u64::from(lane));
            }
            x86.k[7] = 0x5555;
        }
        execute_lifted_x86(
            &[0x62, 0xC3, 0x6D, 0x57, 0x03, 0x4D, 0x7F, 15],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 32),
                    if lane & 1 == 0 {
                        if lane == 0 {
                            99
                        } else {
                            100 + u64::from(lane - 1)
                        }
                    } else {
                        0xA5A5_A5A5
                    }
                );
            }
        }

        // E4NF full-vector memory access is not suppressed by an empty mask.
        let sentinel = [0x4242_4242_4242_4242; 16];
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::R13)), 0x3C0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.k[7] = 0;
        }
        let fault = execute_lifted_x86(
            &[0x62, 0xC3, 0x6D, 0x47, 0x03, 0x4D, 0x01, 31],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[17], sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn executes_evex_integer_test_masks_all_elements_polarities_masks_and_faults() {
        let cases = [
            (&[0x62, 0xF2, 0x65, 0x08, 0x26, 0xD4][..], 8u32, false),
            (&[0x62, 0xF2, 0xE5, 0x08, 0x26, 0xD4][..], 16, false),
            (&[0x62, 0xF2, 0x65, 0x08, 0x27, 0xD4][..], 32, false),
            (&[0x62, 0xF2, 0xE5, 0x08, 0x27, 0xD4][..], 64, false),
            (&[0x62, 0xF2, 0x66, 0x08, 0x26, 0xD4][..], 8, true),
            (&[0x62, 0xF2, 0xE6, 0x08, 0x26, 0xD4][..], 16, true),
            (&[0x62, 0xF2, 0x66, 0x08, 0x27, 0xD4][..], 32, true),
            (&[0x62, 0xF2, 0xE6, 0x08, 0x27, 0xD4][..], 64, true),
        ];
        let mut memory = FlatMemory::new(0x400);
        for (bytes, bits, inverted) in cases {
            let lanes = 128 / bits;
            let mut ctx = SmirContext::new_x86_64();
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                for lane in 0..lanes as u8 {
                    let (a, b) = match lane % 3 {
                        0 => (u64::MAX, u64::MAX),
                        1 => (0, u64::MAX),
                        _ => (u64::MAX, 0),
                    };
                    SmirInterpreter::set_lane(&mut x86.xmm[3], lane, bits, a);
                    SmirInterpreter::set_lane(&mut x86.xmm[4], lane, bits, b);
                }
                x86.k[2] = u64::MAX;
            }
            execute_lifted_x86(bytes, &mut ctx, &mut memory);
            let valid = (1u64 << lanes) - 1;
            let nonzero =
                (0..lanes).fold(0u64, |mask, lane| mask | u64::from(lane % 3 == 0) << lane);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.k[2], if inverted { valid ^ nonzero } else { nonzero });
            }
        }

        // Destination writemask is zeroing and may alias the destination K reg.
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3] = [u64::MAX; 16];
            x86.xmm[4] = [u64::MAX; 16];
            x86.k[2] = 0x5;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x65, 0x0A, 0x27, 0xD4], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[2], 0x5);
        }

        // Type E4 suppresses a broadcast memory fault when every lane is
        // inactive, but an active lane faults before the K destination write.
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x400);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[2] = 0;
            x86.k[4] = 0xA5;
        }
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x52, 0x27, 0x20], &mut ctx, &mut memory);
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[4], 0);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[2] = 1;
            x86.k[4] = 0xA5;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x52, 0x27, 0x20], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[4], 0xA5);
        }
    }
    #[test]
    fn executes_evex_pair_intersect_duplicates_pairing_and_e4nf_faults() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        let flags_before = 0xCD7;
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for (lane, value) in [1u32, 2, 2, 5].into_iter().enumerate() {
                SmirInterpreter::set_lane(&mut x86.xmm[3], lane as u8, 32, u64::from(value));
            }
            for (lane, value) in [3u32, 2, 4, 1].into_iter().enumerate() {
                SmirInterpreter::set_lane(&mut x86.xmm[4], lane as u8, 32, u64::from(value));
            }
            x86.k[2] = u64::MAX;
            x86.k[3] = u64::MAX;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x67, 0x08, 0x68, 0xD4], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[2], 0b0111);
            assert_eq!(x86.k[3], 0b1010);
        }

        // E4NF broadcast source faults before either architectural K write.
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x400);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[6] = 0xA5;
            x86.k[7] = 0x5A;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x77, 0x50, 0x68, 0x30], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!((x86.k[6], x86.k[7]), (0xA5, 0x5A));
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn executes_evex_packed_fp_arithmetic_masks_broadcasts_aliases_and_e4_faults() {
        let mut memory = FlatMemory::new(0x400);
        let mut ctx = SmirContext::new_x86_64();
        let flags_before = 0xCD7;
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        let control = 0xA55Au64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[16],
                    lane,
                    32,
                    (f32::from(lane) + 1.0).to_bits().into(),
                );
                SmirInterpreter::set_lane(
                    &mut x86.xmm[18],
                    lane,
                    32,
                    (f32::from(lane) + 2.0).to_bits().into(),
                );
                SmirInterpreter::set_lane(&mut x86.xmm[17], lane, 32, 0x7FC0_0000);
            }
            x86.k[3] = control;
        }
        execute_lifted_x86(&[0x62, 0xA1, 0x7C, 0xC3, 0x58, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                let expected = if control & (1u64 << lane) != 0 {
                    (f32::from(lane) * 2.0 + 3.0).to_bits()
                } else {
                    0
                };
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 32),
                    u64::from(expected)
                );
            }
        }

        // A broadcast memory source is read under each destination mask bit;
        // simple exact operands make division results reproducible.
        memory
            .write(0x100, &2.0f64.to_bits().to_le_bytes())
            .unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0xC0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..8u8 {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[2],
                    lane,
                    64,
                    (f64::from(lane) * 2.0 + 4.0).to_bits(),
                );
            }
            x86.k[1] = u64::MAX;
        }
        execute_lifted_x86(
            &[0x62, 0xF1, 0xED, 0x59, 0x5E, 0x48, 0x08],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 64),
                    (f64::from(lane) + 2.0).to_bits()
                );
            }
        }

        // Type E4 suppresses every masked-off full-vector load and commits no
        // destination state when an active lane faults.
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x400);
        let mut sentinel = [0u64; 16];
        sentinel[..8].fill(0xA5A5_A5A5_A5A5_A5A5);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 0;
        }
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF1, 0x6C, 0x49, 0x5C, 0x08], &mut ctx, &mut memory);
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[1] = 1;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF1, 0x6C, 0x49, 0x5C, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn executes_evex_packed_sqrt_masks_zeroes_and_suppresses_e4_faults() {
        let mut memory = FlatMemory::new(0x400);
        let mut ctx = SmirContext::new_x86_64();
        let flags_before = 0xCD7;
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        let mask32 = 0xA55Au64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..16u8 {
                let root = f32::from(lane) + 1.0;
                SmirInterpreter::set_lane(
                    &mut x86.xmm[0],
                    lane,
                    32,
                    (root * root).to_bits().into(),
                );
                SmirInterpreter::set_lane(&mut x86.xmm[4], lane, 32, 0xDEAD_0000 | u64::from(lane));
            }
            x86.k[1] = mask32;
        }
        execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x49, 0x51, 0xE0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                let expected = if mask32 & (1u64 << lane) != 0 {
                    (f32::from(lane) + 1.0).to_bits()
                } else {
                    0xDEAD_0000 | u32::from(lane)
                };
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[4], lane, 32),
                    u64::from(expected)
                );
            }
        }

        let mask64 = 0x5Au64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..8u8 {
                let root = f64::from(lane) + 1.0;
                SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 64, (root * root).to_bits());
                SmirInterpreter::set_lane(&mut x86.xmm[7], lane, 64, u64::MAX);
            }
            x86.k[2] = mask64;
        }
        execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0xCA, 0x51, 0xF9], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[7], lane, 64),
                    if mask64 & (1u64 << lane) != 0 {
                        (f64::from(lane) + 1.0).to_bits()
                    } else {
                        0
                    }
                );
            }
        }

        // An all-zero E4 mask suppresses every memory element access; the
        // first active lane exposes the fault without modifying the result.
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x400);
        let sentinel = [0xA5A5_A5A5_A5A5_A5A5; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 0;
        }
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x49, 0x51, 0x08], &mut ctx, &mut memory);
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][..8], sentinel[..8]);
            assert!(x86.xmm[1][8..].iter().all(|word| *word == 0));
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 1;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x49, 0x51, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn executes_evex_mask_blends_select_sources_zero_and_suppress_e4_faults() {
        let mut memory = FlatMemory::new(0x200);
        let mut ctx = SmirContext::new_x86_64();
        let control = 0xA55Au64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[16], lane, 32, 0x1000 + u64::from(lane));
                SmirInterpreter::set_lane(&mut x86.xmm[18], lane, 32, 0x2000 + u64::from(lane));
                SmirInterpreter::set_lane(&mut x86.xmm[17], lane, 32, 0xDEAD_BEEF);
            }
            x86.k[3] = control;
        }
        execute_lifted_x86(&[0x62, 0xA2, 0x7D, 0xC3, 0x65, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 32),
                    if control & (1u64 << lane) != 0 {
                        0x2000 + u64::from(lane)
                    } else {
                        0
                    }
                );
            }
        }

        let byte_control = 0xA55A_F00F_9669_3CC3u64;
        let (byte_src1, byte_src2) = match &mut ctx.arch_regs {
            ArchRegState::X86_64(x86) => {
                x86.k[3] = byte_control;
                (x86.xmm[16], x86.xmm[18])
            }
            _ => unreachable!(),
        };
        execute_lifted_x86(&[0x62, 0xA2, 0x7D, 0xC3, 0x66, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..64u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 8),
                    if byte_control & (1u64 << lane) != 0 {
                        SmirInterpreter::get_lane(&byte_src2, lane, 8)
                    } else {
                        0
                    },
                    "byte lane {lane}; inactive source would be {}",
                    SmirInterpreter::get_lane(&byte_src1, lane, 8)
                );
            }
        }

        // Merging mask selects SRC1, not the previous destination value.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..4u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[2], lane, 64, 0x3000 + u64::from(lane));
                SmirInterpreter::set_lane(&mut x86.xmm[3], lane, 64, 0x4000 + u64::from(lane));
                SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 64, u64::MAX);
            }
            x86.k[2] = 0b0101;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0xED, 0x2A, 0x64, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..4u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 64),
                    if 0b0101 & (1u64 << lane) != 0 {
                        0x4000 + u64::from(lane)
                    } else {
                        0x3000 + u64::from(lane)
                    }
                );
            }
        }

        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x200);
        let mut sentinel = [0u64; 16];
        sentinel[..8].fill(0xA5A5_A5A5_A5A5_A5A5);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 0;
        }
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0xC9, 0x64, 0x08], &mut ctx, &mut memory);
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], [0; 16]);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 1;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0xC9, 0x64, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }
    }
    #[test]
    fn executes_evex_integer_narrow_all_modes_ratios_masks_aliases_memory_and_faults() {
        let mut memory = FlatMemory::new(0x400);
        for high in [0x10u8, 0x20, 0x30] {
            for (low, src_bits, dst_bits, lanes) in [
                (0u8, 16u32, 8u32, 8u8),
                (1, 32, 8, 4),
                (2, 64, 8, 2),
                (3, 32, 16, 4),
                (4, 64, 16, 2),
                (5, 64, 32, 2),
            ] {
                let opcode = high | low;
                let bytes = [0x62, 0xF2, 0x7E, 0x09, opcode, 0xD1];
                let src_mask = if src_bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << src_bits) - 1
                };
                let dst_mask = (1u64 << dst_bits) - 1;
                let values = [
                    1u64 << (src_bits - 1),
                    src_mask,
                    0,
                    (1u64 << (src_bits - 1)) - 1,
                    1u64 << dst_bits,
                    src_mask - 7,
                    (1u64 << (dst_bits - 1)) - 1,
                    1u64 << (dst_bits - 1),
                ];
                let control = 0b1010_0101u64 & ((1u64 << lanes) - 1);
                let mut source = [0u64; 16];
                let mut old = [0xA5A5_A5A5_A5A5_A5A5; 16];
                let mut expected = [0u64; 16];
                for lane in 0..lanes {
                    let raw = values[lane as usize];
                    SmirInterpreter::set_lane(&mut source, lane, src_bits, raw);
                    SmirInterpreter::set_lane(&mut old, lane, dst_bits, 0x5A & dst_mask);
                    if control & (1u64 << lane) != 0 {
                        let shift = 128 - src_bits;
                        let signed = (i128::from(raw) << shift) >> shift;
                        let narrowed = match high {
                            0x10 => raw.min(dst_mask),
                            0x20 => {
                                let low = -(1i128 << (dst_bits - 1));
                                let high = (1i128 << (dst_bits - 1)) - 1;
                                signed.clamp(low, high) as u64 & dst_mask
                            }
                            0x30 => raw & dst_mask,
                            _ => unreachable!(),
                        };
                        SmirInterpreter::set_lane(&mut expected, lane, dst_bits, narrowed);
                    } else {
                        SmirInterpreter::set_lane(&mut expected, lane, dst_bits, 0x5A & dst_mask);
                    }
                }
                let mut ctx = SmirContext::new_x86_64();
                if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                    x86.xmm[1] = old;
                    x86.xmm[2] = source;
                    x86.k[1] = control;
                }
                execute_lifted_x86(&bytes, &mut ctx, &mut memory);
                if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                    assert_eq!(x86.xmm[1], expected, "opcode {opcode:02X}");
                }
            }
        }

        // The source and narrowed destination may alias the shared ZMM state.
        let mut ctx = SmirContext::new_x86_64();
        let mut alias = [0u64; 16];
        for lane in 0..32u8 {
            SmirInterpreter::set_lane(&mut alias, lane, 16, u64::from(lane) + 0x100);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = alias;
            x86.k[1] = u32::MAX as u64;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x7E, 0x49, 0x30, 0xC9], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..32u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 8),
                    u64::from(lane)
                );
            }
            assert_eq!(&x86.xmm[1][4..], &[0; 12]);
        }

        // Memory destinations retain inactive bytes and suppress their faults.
        memory.write(0x100, &[0xCC; 16]).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            let mut source = [0u64; 16];
            for (lane, value) in [i32::MIN, -1, 0, 300].into_iter().enumerate() {
                SmirInterpreter::set_lane(&mut source, lane as u8, 32, value as u32 as u64);
            }
            x86.xmm[2] = source;
            x86.k[1] = 0b1011;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x7E, 0x09, 0x21, 0x10], &mut ctx, &mut memory);
        let mut narrowed = [0u8; 4];
        memory.read(0x100, &mut narrowed).unwrap();
        assert_eq!(narrowed, [0x80, 0xFF, 0xCC, 0x7F]);

        memory.write(0x3FF, &[0xCC]).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3FF);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[1] = 0;
        }
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF2, 0x7E, 0x09, 0x31, 0x10], &mut ctx, &mut memory);
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[1] = 0b11;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x7E, 0x09, 0x31, 0x10], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        let mut first = [0u8; 1];
        memory.read(0x3FF, &mut first).unwrap();
        assert_eq!(first[0], 0);
    }
