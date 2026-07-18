//! evex part 3 tests

use super::*;
use crate::smir::interpret::*;
use crate::smir::interpret::tests::*;

    #[test]
    fn executes_evex_mask_vector_conversions_all_elements_widths_and_high_registers() {
        let cases = [
            (
                &[0x62, 0xF2, 0x7E, 0x08, 0x28, 0xD1][..],
                true,
                8u32,
                16u8,
                2usize,
                1usize,
            ),
            (
                &[0x62, 0xF2, 0xFE, 0x28, 0x28, 0xE3][..],
                true,
                16,
                16,
                4,
                3,
            ),
            (
                &[0x62, 0xE2, 0x7E, 0x48, 0x38, 0xD2][..],
                true,
                32,
                16,
                18,
                2,
            ),
            (&[0x62, 0xF2, 0xFE, 0x08, 0x38, 0xD9][..], true, 64, 2, 3, 1),
            (
                &[0x62, 0xF2, 0x7E, 0x08, 0x29, 0xCA][..],
                false,
                8,
                16,
                2,
                1,
            ),
            (
                &[0x62, 0xF2, 0xFE, 0x28, 0x29, 0xDC][..],
                false,
                16,
                16,
                4,
                3,
            ),
            (
                &[0x62, 0xB2, 0x7E, 0x48, 0x39, 0xD2][..],
                false,
                32,
                16,
                18,
                2,
            ),
            (
                &[0x62, 0xF2, 0xFE, 0x08, 0x39, 0xCB][..],
                false,
                64,
                2,
                3,
                1,
            ),
        ];
        for (bytes, mask_to_vector, bits, lanes, vector_reg, mask_reg) in cases {
            let pattern = 0xA5A5_5AA5_F00F_9669u64;
            let mut ctx = SmirContext::new_x86_64();
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                if mask_to_vector {
                    x86.k[mask_reg] = pattern;
                    x86.xmm[vector_reg] = [u64::MAX; 16];
                } else {
                    let mut source = [0u64; 16];
                    for lane in 0..lanes {
                        let value = if pattern & (1u64 << lane) != 0 {
                            1u64 << (bits - 1)
                        } else {
                            (1u64 << (bits - 1)) - 1
                        };
                        SmirInterpreter::set_lane(&mut source, lane, bits, value);
                    }
                    x86.xmm[vector_reg] = source;
                    x86.k[mask_reg] = u64::MAX;
                }
            }
            let mut memory = FlatMemory::new(0x100);
            execute_lifted_x86(bytes, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                let lane_mask = if lanes == 64 {
                    u64::MAX
                } else {
                    (1u64 << lanes) - 1
                };
                if mask_to_vector {
                    let elem_mask = if bits == 64 {
                        u64::MAX
                    } else {
                        (1u64 << bits) - 1
                    };
                    let mut expected = [0u64; 16];
                    for lane in 0..lanes {
                        SmirInterpreter::set_lane(
                            &mut expected,
                            lane,
                            bits,
                            if pattern & (1u64 << lane) != 0 {
                                elem_mask
                            } else {
                                0
                            },
                        );
                    }
                    assert_eq!(x86.xmm[vector_reg], expected);
                } else {
                    assert_eq!(x86.k[mask_reg], pattern & lane_mask);
                }
            }
        }
    }
    #[test]
    fn lifted_legacy_vex_evex_movd_movq_execute_zeroing_widths_and_faults_exactly() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let flags_before = 0xCD7;
        let legacy_old = [0xA5A5_A5A5_A5A5_A5A5u64; 16];
        let mut memory = FlatMemory::new(0x400);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Prefix-free MOVD/MOVQ address MMX state. MOVD zero-extends its
        // 32-bit source, enters MMX state, and preserves x87 TOP.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::MAX;
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 5 << 11;
        }
        ctx.write_vreg(rcx, 0x1122_3344_5566_7788);
        execute_lifted_x86(&[0x0F, 0x6E, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0x0000_0000_5566_7788);
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 5 << 11);
        }

        // REX.W MOVQ transfers all 64 bits from memory.
        memory
            .write(0x40, &0xFEDC_BA98_7654_3210u64.to_le_bytes())
            .unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0;
            x86.x87.tag_word = 0xFFFF;
        }
        ctx.write_vreg(rax, 0x40);
        execute_lifted_x86(&[0x48, 0x0F, 0x6E, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xFEDC_BA98_7654_3210);
            assert_eq!(x86.x87.tag_word, 0);
        }

        // MMX MOVD to a GPR performs the architectural 32-bit zero-extension.
        ctx.write_vreg(rcx, u64::MAX);
        execute_lifted_x86(&[0x0F, 0x7E, 0xC1], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rcx), 0x7654_3210);

        // Faulting MMX loads and stores do not enter MMX state or mutate MMX
        // data, because the explicit state transition follows memory access.
        let mm_fault_sentinel = 0xA5A5_5A5A_C3C3_3C3C;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = mm_fault_sentinel;
            x86.x87.tag_word = 0xFFFF;
        }
        ctx.write_vreg(rax, 0x1000);
        let load_exit = execute_lifted_x86(&[0x48, 0x0F, 0x6E, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            load_exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], mm_fault_sentinel);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }
        let store_exit = execute_lifted_x86(&[0x48, 0x0F, 0x7E, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            store_exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], mm_fault_sentinel);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        // Legacy MOVD clears bits 127:32 but preserves the shared backing state
        // above bit 127.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = legacy_old;
        }
        ctx.write_vreg(rcx, 0x1122_3344_5566_7788);
        execute_lifted_x86(&[0x66, 0x0F, 0x6E, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0x0000_0000_5566_7788);
            assert_eq!(x86.xmm[0][1], 0);
            assert_eq!(&x86.xmm[0][2..], &legacy_old[2..]);
        }

        // Legacy MOVQ has the same upper-state rule and reads exactly 8 bytes.
        memory
            .write(0x80, &0x0123_4567_89AB_CDEFu64.to_le_bytes())
            .unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = legacy_old;
        }
        ctx.write_vreg(rax, 0x80);
        execute_lifted_x86(&[0x66, 0x48, 0x0F, 0x6E, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0x0123_4567_89AB_CDEF);
            assert_eq!(x86.xmm[0][1], 0);
            assert_eq!(&x86.xmm[0][2..], &legacy_old[2..]);
        }

        // VEX MOVD and EVEX MOVQ zero all state above the scalar result.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
        }
        ctx.write_vreg(rcx, 0xFFFF_FFFF_CAFE_BABE);
        execute_lifted_x86(&[0xC5, 0xF9, 0x6E, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0xCAFE_BABE);
            assert!(x86.xmm[0][1..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = [u64::MAX; 16];
        }
        ctx.write_vreg(r8, 0xDEAD_BEEF_0123_4567);
        execute_lifted_x86(&[0x62, 0xC1, 0xFD, 0x08, 0x6E, 0xC8], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[17][0], 0xDEAD_BEEF_0123_4567);
            assert!(x86.xmm[17][1..].iter().all(|word| *word == 0));
        }

        // MOVD to a GPR is a 32-bit write and therefore zero-extends RCX.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0][0] = 0x1122_3344_89AB_CDEF;
        }
        ctx.write_vreg(rcx, u64::MAX);
        execute_lifted_x86(&[0x66, 0x0F, 0x7E, 0xC1], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rcx), 0x89AB_CDEF);

        // MOVQ to memory writes exactly the low qword.
        ctx.write_vreg(rax, 0x100);
        execute_lifted_x86(&[0x66, 0x48, 0x0F, 0x7E, 0x00], &mut ctx, &mut memory);
        let mut qword = [0u8; 8];
        memory.read(0x100, &mut qword).unwrap();
        assert_eq!(u64::from_le_bytes(qword), 0x1122_3344_89AB_CDEF);

        // EVEX disp8*N uses N=8 for VMOVQ.
        memory
            .write(0x180, &0x8877_6655_4433_2211u64.to_le_bytes())
            .unwrap();
        ctx.write_vreg(rax, 0x100);
        execute_lifted_x86(
            &[0x62, 0xF1, 0xFD, 0x08, 0x6E, 0x40, 0x10],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0x8877_6655_4433_2211);
            assert!(x86.xmm[0][1..].iter().all(|word| *word == 0));
        }

        // VMOVW reads only the low word of a GPR and clears all remaining
        // architectural vector state, including state above bit 127.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = [u64::MAX; 16];
        }
        ctx.write_vreg(r8, 0xDEAD_BEEF_CAFE_A1B2);
        execute_lifted_x86(&[0x62, 0xC5, 0x7D, 0x08, 0x6E, 0xC8], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[17][0], 0xA1B2);
            assert!(x86.xmm[17][1..].iter().all(|word| *word == 0));
        }

        // The reverse register form writes a zero-extended 32-bit GPR result.
        ctx.write_vreg(r8, u64::MAX);
        execute_lifted_x86(&[0x62, 0xC5, 0xFD, 0x08, 0x7E, 0xC8], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(r8), 0xA1B2);

        // Type E9NF scalar memory tuples scale disp8 by 2 and transfer exactly
        // one word in either direction.
        memory.write(0x1FE, &0x5AA5u16.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x100);
        execute_lifted_x86(
            &[0x62, 0xF5, 0x7D, 0x08, 0x6E, 0x48, 0x7F],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x5AA5);
            assert!(x86.xmm[1][1..].iter().all(|word| *word == 0));
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 0x0123_4567_89AB_CDEF;
        }
        execute_lifted_x86(
            &[0x62, 0xF5, 0x7D, 0x08, 0x7E, 0x48, 0x7F],
            &mut ctx,
            &mut memory,
        );
        let mut word = [0u8; 2];
        memory.read(0x1FE, &mut word).unwrap();
        assert_eq!(u16::from_le_bytes(word), 0xCDEF);

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

        // A load fault occurs before any architectural vector write.
        let fault_sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = fault_sentinel;
        }
        ctx.write_vreg(rax, 0x1000);
        let exit = execute_lifted_x86(&[0xC5, 0xF9, 0x6E, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], fault_sentinel);
        }

        // A faulting VMOVW load likewise precedes every architectural vector
        // write despite the expanded zero-and-insert representation.
        let word_fault_sentinel = [0x5A5A_A5A5_5A5A_A5A5u64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = word_fault_sentinel;
        }
        let exit = execute_lifted_x86(&[0x62, 0xF5, 0x7D, 0x08, 0x6E, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], word_fault_sentinel);
        }
    }
    #[test]
    fn lifted_evex_immediate_integer_compares_execute_predicates_masks_and_fault_suppression() {
        fn packed(values: &[u64], elem_bytes: usize) -> VecValue {
            let mut bytes = Vec::with_capacity(values.len() * elem_bytes);
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes()[..elem_bytes]);
            }
            vec_from_bytes(&bytes)
        }

        fn signed_value(value: u64, bits: u32) -> i64 {
            let shift = 64 - bits;
            ((value << shift) as i64) >> shift
        }

        fn predicate(predicate: u8, lhs: u64, rhs: u64, bits: u32, signed: bool) -> bool {
            let relation = if signed {
                signed_value(lhs, bits).cmp(&signed_value(rhs, bits))
            } else {
                lhs.cmp(&rhs)
            };
            match predicate & 7 {
                0 => relation.is_eq(),
                1 => relation.is_lt(),
                2 => !relation.is_gt(),
                3 => false,
                4 => !relation.is_eq(),
                5 => !relation.is_lt(),
                6 => relation.is_gt(),
                7 => true,
                _ => unreachable!(),
            }
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k3 = VReg::Arch(ArchReg::X86(X86Reg::K(3)));
        let k4 = VReg::Arch(ArchReg::X86(X86Reg::K(4)));
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Signed and unsigned dword forms execute all predicates. High imm8
        // bits are ignored, so the second pass also probes imm8[7:3].
        let lhs = (0..16)
            .map(|lane| [0, 0x8000_0000, 7, 5][lane % 4])
            .collect::<Vec<_>>();
        let rhs = (0..16)
            .map(|lane| [0, 1, 3, 5][lane % 4])
            .collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed(&lhs, 4);
            x86.xmm[2] = packed(&rhs, 4);
        }
        for (opcode, signed) in [(0x1F, true), (0x1E, false)] {
            for low_predicate in 0u8..8 {
                for immediate in [low_predicate, low_predicate | 0xF8] {
                    ctx.write_vreg(k3, u64::MAX);
                    execute_lifted_x86(
                        &[0x62, 0xF3, 0x75, 0x48, opcode, 0xDA, immediate],
                        &mut ctx,
                        &mut memory,
                    );
                    let expected =
                        lhs.iter()
                            .zip(&rhs)
                            .enumerate()
                            .fold(0u64, |mask, (lane, (&a, &b))| {
                                mask | (u64::from(predicate(immediate, a, b, 32, signed)) << lane)
                            });
                    assert_eq!(
                        ctx.read_vreg(k3),
                        expected,
                        "opcode {opcode:02X}, predicate {immediate:02X}",
                    );
                }
            }
        }

        // Every B/W/D/Q signed form orders the element sign bit below zero;
        // every unsigned counterpart orders the identical bits above zero.
        for (opcode, w, elem_bytes, signed) in [
            (0x3F, false, 1usize, true),
            (0x3E, false, 1, false),
            (0x3F, true, 2, true),
            (0x3E, true, 2, false),
            (0x1F, false, 4, true),
            (0x1E, false, 4, false),
            (0x1F, true, 8, true),
            (0x1E, true, 8, false),
        ] {
            let lanes = 16 / elem_bytes;
            let sign_bit = 1u64 << (elem_bytes * 8 - 1);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1] = packed(&vec![sign_bit; lanes], elem_bytes);
                x86.xmm[2] = packed(&vec![0; lanes], elem_bytes);
            }
            let p1 = if w { 0xF5 } else { 0x75 };
            execute_lifted_x86(
                &[0x62, 0xF3, p1, 0x08, opcode, 0xDA, 0x01],
                &mut ctx,
                &mut memory,
            );
            let lane_mask = (1u64 << lanes) - 1;
            assert_eq!(
                ctx.read_vreg(k3),
                if signed { lane_mask } else { 0 },
                "opcode {opcode:02X}, W={w}",
            );
        }

        // High ZMM sources and an input writemask produce a zeroing-only K3
        // result. The destination is committed only after both sources exist.
        let high_lhs = (0..16)
            .map(|lane| if lane % 3 == 0 { 20 } else { lane as u64 })
            .collect::<Vec<_>>();
        let high_rhs = vec![10; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = packed(&high_lhs, 4);
            x86.xmm[18] = packed(&high_rhs, 4);
        }
        ctx.write_vreg(k4, 0xA55A);
        execute_lifted_x86(
            &[0x62, 0xB3, 0x75, 0x44, 0x1E, 0xDA, 0x06],
            &mut ctx,
            &mut memory,
        );
        let raw = high_lhs
            .iter()
            .zip(&high_rhs)
            .enumerate()
            .fold(0u64, |mask, (lane, (&a, &b))| {
                mask | (u64::from(a > b) << lane)
            });
        assert_eq!(ctx.read_vreg(k3), raw & 0xA55A);

        // Broadcast disp8*N uses N=4. Only active lanes access the scalar and
        // inactive destination bits are zero, including bits above KL.
        memory.write(0x104, &0u32.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x100);
        let broadcast_lhs = (0..16)
            .map(|lane| if lane % 2 == 0 { u32::MAX as u64 } else { 1 })
            .collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed(&broadcast_lhs, 4);
        }
        ctx.write_vreg(k4, 0x5555);
        execute_lifted_x86(
            &[0x62, 0xF3, 0x75, 0x5C, 0x1F, 0x58, 0x01, 0x01],
            &mut ctx,
            &mut memory,
        );
        assert_eq!(ctx.read_vreg(k3), 0x5555);

        // Full-vector disp8*N uses N=64. TRUE still performs each active load
        // and writes exactly the input writemask.
        memory.write(0x140, &[0xA5; 64]).unwrap();
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k4, 0x8421);
        execute_lifted_x86(
            &[0x62, 0xF3, 0x75, 0x4C, 0x1F, 0x58, 0x01, 0x07],
            &mut ctx,
            &mut memory,
        );
        assert_eq!(ctx.read_vreg(k3), 0x8421);

        // An all-zero writemask suppresses every E4 memory access, including
        // a TRUE predicate. Activating one lane exposes the fault and leaves
        // the complete destination unchanged.
        ctx.write_vreg(rax, 0x1000);
        ctx.write_vreg(k4, 0);
        ctx.write_vreg(k3, u64::MAX);
        let suppressed = execute_lifted_x86(
            &[0x62, 0xF3, 0x75, 0x4C, 0x1F, 0x18, 0x07],
            &mut ctx,
            &mut memory,
        );
        assert!(!matches!(
            suppressed,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        assert_eq!(ctx.read_vreg(k3), 0);

        ctx.write_vreg(k4, 1);
        ctx.write_vreg(k3, 0xDEAD_BEEF);
        let exposed = execute_lifted_x86(
            &[0x62, 0xF3, 0x75, 0x4C, 0x1F, 0x18, 0x07],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            exposed,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        assert_eq!(ctx.read_vreg(k3), 0xDEAD_BEEF);

        // FALSE is not permission to elide an unmasked memory operand.
        ctx.write_vreg(k3, 0xC0DE_CAFE);
        let false_fault = execute_lifted_x86(
            &[0x62, 0xF3, 0x75, 0x48, 0x1F, 0x18, 0x03],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            false_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        assert_eq!(ctx.read_vreg(k3), 0xC0DE_CAFE);

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_evex_chunk_extract_insert_execute_masks_aliases_tuples_and_e6nf_faults() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // A high-register 128-bit extract selects imm8[1:0], applies zeroing
        // masking at dword granularity, and clears all backing state above XMM.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[18], lane, 32, 0x1000 + u64::from(lane));
            }
            x86.xmm[17] = [u64::MAX; 16];
            x86.k[3] = 0b1010;
        }
        execute_lifted_x86(
            &[0x62, 0xA3, 0x7D, 0xCB, 0x19, 0xD1, 0x03],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..4u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 32),
                    if 0b1010 & (1u64 << lane) != 0 {
                        0x1000 + u64::from(12 + lane)
                    } else {
                        0
                    }
                );
            }
            assert!(x86.xmm[17][2..].iter().all(|word| *word == 0));
        }

        // Insert captures both high-register inputs before committing an
        // aliased destination and applies the writemask after chunk assembly.
        let insert_mask = 0xA55Au64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[18], lane, 32, 0x2000 + u64::from(lane));
            }
            for lane in 0..4u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[19], lane, 32, 0x3000 + u64::from(lane));
            }
            x86.xmm[17] = [u64::MAX; 16];
            x86.k[3] = insert_mask;
        }
        execute_lifted_x86(
            &[0x62, 0xA3, 0x6D, 0xC3, 0x18, 0xCB, 0x02],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                let raw = if (8..12).contains(&lane) {
                    0x3000 + u64::from(lane - 8)
                } else {
                    0x2000 + u64::from(lane)
                };
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 32),
                    if insert_mask & (1u64 << lane) != 0 {
                        raw
                    } else {
                        0
                    }
                );
            }
        }

        // Masked extract-to-memory uses Tuple4 scaling (disp8*16), preserves
        // inactive dwords, and selects the upper half of the YMM source.
        let old_dwords = [0xAAAA_0000u32, 0xBBBB_0001, 0xCCCC_0002, 0xDDDD_0003];
        for (lane, value) in old_dwords.into_iter().enumerate() {
            memory
                .write(0x120 + lane as u64 * 4, &value.to_le_bytes())
                .unwrap();
        }
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k2, 0b0101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..8u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[3], lane, 32, 0x4000 + u64::from(lane));
            }
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x2A, 0x39, 0x58, 0x02, 0x01],
            &mut ctx,
            &mut memory,
        );
        let mut extracted = [0u8; 16];
        memory.read(0x120, &mut extracted).unwrap();
        for lane in 0..4usize {
            let actual = u32::from_le_bytes(extracted[lane * 4..lane * 4 + 4].try_into().unwrap());
            assert_eq!(
                actual,
                if 0b0101 & (1 << lane) != 0 {
                    0x4000 + 4 + lane as u32
                } else {
                    old_dwords[lane]
                }
            );
        }

        // Insert-from-memory also scales disp8 by 16 bytes. E6NF performs the
        // complete read before merging or zeroing any destination element.
        let inserted_qwords = [0x1111_2222_3333_4444u64, 0x5555_6666_7777_8888];
        for (lane, value) in inserted_qwords.into_iter().enumerate() {
            memory
                .write(0x120 + lane as u64 * 8, &value.to_le_bytes())
                .unwrap();
        }
        ctx.write_vreg(k2, 0b1010);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..4u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[4], lane, 64, 0x5000 + u64::from(lane));
                SmirInterpreter::set_lane(&mut x86.xmm[3], lane, 64, 0x6000 + u64::from(lane));
            }
            x86.xmm[3][4..].fill(u64::MAX);
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0xDD, 0x2A, 0x18, 0x58, 0x02, 0x01],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[3][0], 0x6000);
            assert_eq!(x86.xmm[3][1], 0x5001);
            assert_eq!(x86.xmm[3][2], 0x6002);
            assert_eq!(x86.xmm[3][3], inserted_qwords[1]);
            assert!(x86.xmm[3][4..].iter().all(|word| *word == 0));
        }

        // Unlike E1/E4, an all-zero E6NF writemask does not suppress either
        // an insert source fault or an extract destination access fault.
        let sentinel = [0xA5A5_A5A5_A5A5_A5A5; 16];
        ctx.write_vreg(rax, 0x400);
        ctx.write_vreg(k2, 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3] = sentinel;
        }
        let insert_fault = execute_lifted_x86(
            &[0x62, 0xF3, 0xDD, 0x2A, 0x18, 0x18, 0x01],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            insert_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[3], sentinel);
        }

        let extract_fault = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x2A, 0x39, 0x18, 0x01],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            extract_fault,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_evex_shuffle_128_chunks_executes_selectors_masks_broadcasts_and_e4nf_faults() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // imm8=0x4e selects SRC1 chunks 2,3 and SRC2 chunks 0,1. Masking is
        // applied afterward at dword granularity to high ZMM registers.
        let mask = 0xA55Au64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[18], lane, 32, 0x1000 + u64::from(lane));
                SmirInterpreter::set_lane(&mut x86.xmm[19], lane, 32, 0x2000 + u64::from(lane));
            }
            x86.xmm[17] = [u64::MAX; 16];
            x86.k[3] = mask;
        }
        execute_lifted_x86(
            &[0x62, 0xA3, 0x6D, 0xC3, 0x23, 0xCB, 0x4E],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                let raw = match lane / 4 {
                    0 | 1 => 0x1000 + u64::from(lane + 8),
                    2 | 3 => 0x2000 + u64::from(lane - 8),
                    _ => unreachable!(),
                };
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 32),
                    if mask & (1u64 << lane) != 0 { raw } else { 0 }
                );
            }
        }

        // The 256-bit qword form uses imm8 bits 0 and 1 only: SRC1 chunk 1
        // supplies the low half and SRC2 chunk 0 supplies the high half.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..4u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[21], lane, 64, 0x3000 + u64::from(lane));
                SmirInterpreter::set_lane(&mut x86.xmm[22], lane, 64, 0x4000 + u64::from(lane));
                SmirInterpreter::set_lane(&mut x86.xmm[20], lane, 64, 0x5000 + u64::from(lane));
            }
            x86.xmm[20][4..].fill(u64::MAX);
            x86.k[4] = 0b1010;
        }
        execute_lifted_x86(
            &[0x62, 0xA3, 0xD5, 0x24, 0x43, 0xE6, 0xB1],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[20][0], 0x5000);
            assert_eq!(x86.xmm[20][1], 0x3003);
            assert_eq!(x86.xmm[20][2], 0x5002);
            assert_eq!(x86.xmm[20][3], 0x4001);
            assert!(x86.xmm[20][4..].iter().all(|word| *word == 0));
        }

        // A dword broadcast uses disp8*4. It fills both SRC2-selected chunks,
        // while the low chunks retain their independent SRC1 selectors.
        memory.write(0x104, &0xDEAD_BEEFu32.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k2, u64::MAX);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[2], lane, 32, 0x6000 + u64::from(lane));
            }
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x6D, 0x5A, 0x23, 0x48, 0x01, 0x1B],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                    if lane < 4 {
                        0x6000 + u64::from(lane + 12)
                    } else if lane < 8 {
                        0x6000 + u64::from(lane + 4)
                    } else {
                        0xDEAD_BEEF
                    }
                );
            }
        }

        // E4NF does not suppress the scalar broadcast read when every
        // destination mask bit is clear; a fault precedes all destination state.
        ctx.write_vreg(rax, 0x400);
        ctx.write_vreg(k2, 0);
        let sentinel = [0xA5A5_A5A5_A5A5_A5A5; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
        }
        let fault = execute_lifted_x86(
            &[0x62, 0xF3, 0x6D, 0x5A, 0x23, 0x08, 0x1B],
            &mut ctx,
            &mut memory,
        );
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
    fn lifted_evex_gfni_executes_field_algebra_masks_and_memory_fault_classes() {
        fn packed(bytes: &[u8], fill: u64) -> VecValue {
            let mut value = [fill; 16];
            for (index, chunk) in bytes.chunks_exact(8).enumerate() {
                value[index] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            value
        }

        fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(len)
                .collect()
        }

        fn gf_mul(mut a: u8, mut b: u8) -> u8 {
            let mut result = 0;
            for _ in 0..8 {
                if b & 1 != 0 {
                    result ^= a;
                }
                let carry = a & 0x80 != 0;
                a <<= 1;
                if carry {
                    a ^= 0x1B;
                }
                b >>= 1;
            }
            result
        }

        fn gf_inverse(value: u8) -> u8 {
            if value == 0 {
                return 0;
            }
            let mut result = 1;
            for _ in 0..254 {
                result = gf_mul(result, value);
            }
            result
        }

        fn affine(matrix: &[u8], input: u8, imm: u8) -> u8 {
            let mut result = 0;
            for bit in 0..8 {
                let parity = (matrix[7 - bit] & input).count_ones() as u8 & 1;
                result |= (parity ^ ((imm >> bit) & 1)) << bit;
            }
            result
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let flags_before = 0xCD7;
        let sentinel = [0xA5A5_A5A5_A5A5_A5A5; 16];
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Legacy SSE is destructive and preserves shared state above XMM,
        // while VEX.256 is three-operand and clears state above YMM.
        let legacy_left = (0..16u8)
            .map(|lane| lane.wrapping_mul(0x57).wrapping_add(0x13))
            .collect::<Vec<_>>();
        let legacy_right = (0..16u8)
            .map(|lane| lane.wrapping_mul(0x83).wrapping_add(0x29))
            .collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = packed(&legacy_left, 0xA5A5_A5A5_A5A5_A5A5);
            x86.xmm[9] = packed(&legacy_right, 0);
        }
        execute_lifted_x86(&[0x66, 0x45, 0x0F, 0x38, 0xCF, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[8], 16),
                legacy_left
                    .iter()
                    .copied()
                    .zip(legacy_right.iter().copied())
                    .map(|(left, right)| gf_mul(left, right))
                    .collect::<Vec<_>>()
            );
            assert!(
                x86.xmm[8][2..]
                    .iter()
                    .all(|word| *word == 0xA5A5_A5A5_A5A5_A5A5)
            );
        }

        let vex_left = (0..32u8)
            .map(|lane| lane.wrapping_mul(0x31).wrapping_add(0xC7))
            .collect::<Vec<_>>();
        let vex_right = (0..32u8)
            .map(|lane| lane.wrapping_mul(0xA7).wrapping_add(0x02))
            .collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = sentinel;
            x86.xmm[9] = packed(&vex_left, 0);
            x86.xmm[10] = packed(&vex_right, 0);
        }
        execute_lifted_x86(&[0xC4, 0x42, 0x35, 0xCF, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[8], 32),
                vex_left
                    .iter()
                    .copied()
                    .zip(vex_right.iter().copied())
                    .map(|(left, right)| gf_mul(left, right))
                    .collect::<Vec<_>>()
            );
            assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
        }

        // The immediate affine forms additionally validate VEX source
        // ordering and legacy destructive inverse composition.
        let identity_matrix = [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01];
        let vex_affine_input = (0..32u8)
            .map(|lane| lane.wrapping_mul(0x5D).wrapping_add(0x21))
            .collect::<Vec<_>>();
        let vex_affine_matrices = identity_matrix.repeat(4);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = sentinel;
            x86.xmm[9] = packed(&vex_affine_input, 0);
            x86.xmm[10] = packed(&vex_affine_matrices, 0);
        }
        execute_lifted_x86(&[0xC4, 0x43, 0xB5, 0xCE, 0xC2, 0x63], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[8], 32),
                vex_affine_input
                    .iter()
                    .copied()
                    .map(|value| affine(&identity_matrix, value, 0x63))
                    .collect::<Vec<_>>()
            );
            assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
        }

        let legacy_inverse_input = (0..16u8)
            .map(|lane| lane.wrapping_mul(0x97).wrapping_add(0x53))
            .collect::<Vec<_>>();
        let legacy_inverse_matrices = identity_matrix.repeat(2);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = packed(&legacy_inverse_input, 0x5A5A_5A5A_5A5A_5A5A);
            x86.xmm[9] = packed(&legacy_inverse_matrices, 0);
        }
        execute_lifted_x86(
            &[0x66, 0x45, 0x0F, 0x3A, 0xCF, 0xC1, 0x63],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[8], 16),
                legacy_inverse_input
                    .iter()
                    .copied()
                    .map(|value| affine(&identity_matrix, gf_inverse(value), 0x63))
                    .collect::<Vec<_>>()
            );
            assert!(
                x86.xmm[8][2..]
                    .iter()
                    .all(|word| *word == 0x5A5A_5A5A_5A5A_5A5A)
            );
        }

        // Only the legacy m128 form imposes 16-byte alignment. The same
        // unaligned address is valid for VEX.128.
        memory.write(0x101, &legacy_right).unwrap();
        ctx.write_vreg(rax, 0x101);
        let legacy_before = packed(&legacy_left, 0xA5A5_A5A5_A5A5_A5A5);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = legacy_before;
            x86.xmm[1] = packed(&legacy_left, 0);
        }
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0xCF, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], legacy_before);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x71, 0xCF, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                legacy_left
                    .iter()
                    .copied()
                    .zip(legacy_right.iter().copied())
                    .map(|(left, right)| gf_mul(left, right))
                    .collect::<Vec<_>>()
            );
            assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        }

        // High ZMM sources and destination exercise every EVEX extension bit;
        // k3 zeroing applies independently to each of the 64 byte products.
        let multiplicands = (0..64u8)
            .map(|lane| lane.wrapping_mul(0x53).wrapping_add(0xCA))
            .collect::<Vec<_>>();
        let multipliers = (0..64u8)
            .map(|lane| lane.wrapping_mul(0x87).wrapping_add(0x11))
            .collect::<Vec<_>>();
        let multiply_mask = 0xA55A_C33C_F00F_8111u64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.xmm[18] = packed(&multiplicands, 0);
            x86.xmm[19] = packed(&multipliers, 0);
            x86.k[3] = multiply_mask;
        }
        execute_lifted_x86(&[0x62, 0xA2, 0x6D, 0xC3, 0xCF, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let actual = bytes(&x86.xmm[17], 64);
            for lane in 0..64 {
                assert_eq!(
                    actual[lane],
                    if multiply_mask >> lane & 1 != 0 {
                        gf_mul(multiplicands[lane], multipliers[lane])
                    } else {
                        0
                    },
                    "multiply lane {lane}",
                );
            }
        }

        // Two independent 64-bit matrices validate row selection at qword
        // boundaries. Merge-masked bytes retain the old XMM1 value.
        let affine_input = (0..16u8)
            .map(|lane| lane.wrapping_mul(0x31).wrapping_add(7))
            .collect::<Vec<_>>();
        let identity = [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01];
        let reverse = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80];
        let matrices = [identity, reverse].concat();
        let affine_mask = 0xA55Au64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[3] = packed(&affine_input, 0);
            x86.xmm[4] = packed(&matrices, 0);
            x86.k[2] = affine_mask;
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0xE5, 0x0A, 0xCE, 0xCC, 0x00],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let actual = bytes(&x86.xmm[1], 16);
            for lane in 0..16 {
                assert_eq!(
                    actual[lane],
                    if affine_mask >> lane & 1 != 0 {
                        affine(
                            &matrices[lane / 8 * 8..lane / 8 * 8 + 8],
                            affine_input[lane],
                            0,
                        )
                    } else {
                        0xA5
                    },
                    "affine lane {lane}",
                );
            }
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        }

        // An identity affine matrix exposes the multiplicative inverse
        // directly. Four batches cover all 256 possible field elements.
        let identity_matrices = identity.repeat(8);
        for batch in 0..4u16 {
            let inputs = (0..64u16)
                .map(|lane| (batch * 64 + lane) as u8)
                .collect::<Vec<_>>();
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1] = packed(&inputs, 0);
                x86.xmm[2] = packed(&identity_matrices, 0);
            }
            execute_lifted_x86(
                &[0x62, 0xF3, 0xF5, 0x48, 0xCF, 0xC2, 0x00],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[0], 64),
                    inputs.iter().copied().map(gf_inverse).collect::<Vec<_>>(),
                    "inverse batch {batch}",
                );
            }
        }

        // The SDM's AES S-box matrix provides a fixed affine-inverse
        // composition: S(0x53) = 0xED with imm8=0x63.
        let sbox_matrix = 0xF1E3_C78F_1F3E_7CF8u64.to_le_bytes().repeat(8);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed(&[0x53; 64], 0);
            x86.xmm[2] = packed(&sbox_matrix, 0);
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0xF5, 0x48, 0xCF, 0xC2, 0x63],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 64), vec![0xED; 64]);
        }

        // A valid 64-bit broadcast is applied to every qword. Inactive bytes
        // merge from ZMM4 after the unconditional Type E4NF memory read.
        memory.write(0x108, &identity).unwrap();
        ctx.write_vreg(rax, 0x100);
        let inverse_mask = 0x0F0F_F0F0_55AA_AA55u64;
        let inverse_input = (0..64u8)
            .map(|lane| lane.wrapping_mul(0x29).wrapping_add(3))
            .collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[4] = sentinel;
            x86.xmm[6] = packed(&inverse_input, 0);
            x86.k[5] = inverse_mask;
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0xCD, 0x5D, 0xCF, 0x60, 0x01, 0x00],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let actual = bytes(&x86.xmm[4], 64);
            for lane in 0..64 {
                assert_eq!(
                    actual[lane],
                    if inverse_mask >> lane & 1 != 0 {
                        gf_inverse(inverse_input[lane])
                    } else {
                        0xA5
                    },
                    "broadcast inverse lane {lane}",
                );
            }
        }

        // MULB is Type E4: an all-zero mask suppresses every invalid byte
        // access. Affine is Type E4NF: the same mask cannot suppress its m64
        // broadcast access, and the destination remains unchanged on fault.
        ctx.write_vreg(rax, 0x400);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[4] = sentinel;
            x86.k[5] = 0;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x4D, 0x4D, 0xCF, 0x20], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[4][..8], &sentinel[..8]);
            assert!(x86.xmm[4][8..].iter().all(|word| *word == 0));
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[4] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xF3, 0xCD, 0x5D, 0xCF, 0x20, 0x63],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[4], sentinel);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_evex_fp_class_executes_all_classes_daz_masks_and_fault_suppression() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Each lane is one of the eight SDM classes. imm8=0xff accepts every
        // class, leaving the input writemask as the exact destination value.
        let classes = [
            0x7FC0_0001u64, // qNaN
            0x0000_0000,    // +0
            0x8000_0000,    // -0
            0x7F80_0000,    // +infinity
            0xFF80_0000,    // -infinity
            0x0000_0001,    // denormal
            0xBF80_0000,    // negative finite
            0x7F80_0001,    // sNaN
        ];
        let mask = 0xA55Au64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[17],
                    lane,
                    32,
                    classes[usize::from(lane % 8)],
                );
            }
            x86.k[2] = u64::MAX;
            x86.k[3] = mask;
            x86.mxcsr = 0x1F80;
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xB3, 0x7D, 0x4B, 0x66, 0xD1, 0xFF],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[2], mask);
            assert_eq!(x86.mxcsr, 0x1F80);
        }

        // FP16 uses the same eight classes but does not apply MXCSR.DAZ. A
        // denormal lane therefore remains classified as denormal with DAZ set.
        let fp16_classes = [
            0x7E01u64, // qNaN
            0x0000,    // +0
            0x8000,    // -0
            0x7C00,    // +infinity
            0xFC00,    // -infinity
            0x0001,    // denormal
            0xBC00,    // negative finite
            0x7C01,    // sNaN
        ];
        let fp16_mask = 0xA55A_5AA5u64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..32u8 {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[17],
                    lane,
                    16,
                    fp16_classes[usize::from(lane % 8)],
                );
            }
            x86.k[2] = u64::MAX;
            x86.k[3] = fp16_mask;
            x86.mxcsr = 0x1F80 | (1 << 6);
        }
        execute_lifted_x86(
            &[0x62, 0xB3, 0x7C, 0x4B, 0x66, 0xD1, 0xFF],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[2], fp16_mask);
            assert_eq!(x86.mxcsr, 0x1F80 | (1 << 6));
        }

        // Scalar classification accepts high XMM sources and distinguishes an
        // sNaN from a qNaN using the raw quiet bit.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[18], 0, 64, 0x7FF0_0000_0000_0001);
            x86.k[1] = u64::MAX;
        }
        execute_lifted_x86(
            &[0x62, 0xB3, 0xFD, 0x08, 0x67, 0xCA, 0x80],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[1], 1);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[18], 0, 16, 0x7C01);
            x86.k[1] = u64::MAX;
        }
        execute_lifted_x86(
            &[0x62, 0xB3, 0x7C, 0x08, 0x67, 0xCA, 0x80],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[1], 1);
        }

        // A broadcasted binary64 subnormal is classified as denormal with DAZ
        // clear and as zero with DAZ set. FPCLASS never changes MXCSR status.
        memory.write(0x108, &1u64.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[5] = 0xAD;
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0xFD, 0x5D, 0x66, 0x60, 0x01, 0x20],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[4], 0xAD);
            assert_eq!(x86.mxcsr, 0x1F80);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[5] = 0xAD;
            x86.mxcsr = 0x1F80 | (1 << 6);
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0xFD, 0x5D, 0x66, 0x60, 0x01, 0x20],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[4], 0);
            assert_eq!(x86.mxcsr, 0x1F80 | (1 << 6));
        }

        // E4/E6 fault suppression skips an invalid packed broadcast or scalar
        // memory source when every relevant input-mask bit is clear.
        ctx.write_vreg(rax, 0x400);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[5] = 0;
            x86.k[7] = 0;
            x86.k[4] = u64::MAX;
            x86.k[6] = u64::MAX;
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xF3, 0xFD, 0x5D, 0x66, 0x60, 0x01, 0x20],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xF3, 0x7D, 0x0F, 0x67, 0x70, 0x01, 0x7F],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[4], 0);
            assert_eq!(x86.k[6], 0);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[7] = 1;
            x86.k[6] = 0x55;
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xF3, 0x7D, 0x0F, 0x67, 0x70, 0x01, 0x7F],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[6], 0x55);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn x86_fp16_fma_is_single_round_mask_aware_and_exception_precise() {
        for (mode, expected) in [
            (FpRoundMode::RoundNearest, 0x3C00),
            (FpRoundMode::RoundDown, 0x3C00),
            (FpRoundMode::RoundUp, 0x3C01),
            (FpRoundMode::RoundTowardZero, 0x3C00),
        ] {
            // 1 * 1 + 2^-11 is exactly halfway between 1 and the next FP16
            // value. The result must be rounded once in the selected mode.
            let result = SmirInterpreter::x86_simd_fp_fma_non_nan(
                0x3C00,
                0x3C00,
                0x1000,
                X86_SIMD_F16,
                mode,
                0x1F80,
            );
            assert_eq!(result.bits, expected, "{mode:?}");
            assert_eq!(result.status, 1 << 5, "{mode:?}");
        }
        let gradual = SmirInterpreter::x86_simd_fp_fma_non_nan(
            0x0001,
            0x3C00,
            0,
            X86_SIMD_F16,
            FpRoundMode::RoundNearest,
            0x1F80 | (1 << 15),
        );
        assert_eq!(gradual.bits, 0x0001, "AVX512-FP16 ignores MXCSR.FTZ");

        let regs = [
            X86Reg::Xmm(0),
            X86Reg::Xmm(1),
            X86Reg::Xmm(2),
            X86Reg::Xmm(3),
        ]
        .map(|reg| VReg::Arch(ArchReg::X86(reg)));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let make_function = |round| {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::X86FP16Fma {
                    dst: regs[3],
                    src1: regs[0],
                    src2: regs[1],
                    src3: regs[2],
                    mask: Some(k1),
                    kind: X86FmaKind::Add,
                    order: X86FmaOrder::Order132,
                    round,
                    lanes: 2,
                },
            );
            builder.set_terminator(Terminator::Trap {
                kind: TrapKind::Halt,
            });
            builder.finish()
        };
        let initialize = |ctx: &mut SmirContext| {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                // Lane zero would raise invalid and denormal-operand, but K1
                // suppresses it. Lane one is the exact halfway case above.
                SmirInterpreter::set_lane(&mut x86.xmm[0], 0, 16, 0x7D01);
                SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 16, 0x0001);
                SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x7E55);
                SmirInterpreter::set_lane(&mut x86.xmm[0], 1, 16, 0x3C00);
                SmirInterpreter::set_lane(&mut x86.xmm[1], 1, 16, 0x1000);
                SmirInterpreter::set_lane(&mut x86.xmm[2], 1, 16, 0x3C00);
            }
            ctx.write_vreg(k1, 1 << 1);
        };

        let mut memory = FlatMemory::new(0x100);
        let mut dynamic = SmirContext::new_x86_64();
        initialize(&mut dynamic);
        let function = make_function(FpRoundMode::Dynamic);
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut dynamic, &mut memory, &function.blocks[0]),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &dynamic.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[3], 0, 16), 0);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[3], 1, 16), 0x3C00);
            assert_eq!(x86.mxcsr & 0x3F, 1 << 5);
        }

        let mut embedded = SmirContext::new_x86_64();
        initialize(&mut embedded);
        let function = make_function(FpRoundMode::RoundUp);
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut embedded, &mut memory, &function.blocks[0]),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &embedded.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[3], 1, 16), 0x3C01);
            assert_eq!(x86.mxcsr & 0x3F, 0, "embedded rounding implies SAE");
        }

        let mut nan = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut nan.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[0], 0, 16, 0x7E11);
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 16, 0x7E22);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x7E33);
        }
        nan.write_vreg(k1, 1);
        let function = make_function(FpRoundMode::Dynamic);
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut nan, &mut memory, &function.blocks[0]),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &nan.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[3], 0, 16), 0x7E11);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }

        let mut invalid = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut invalid.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[0], 0, 16, 0);
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 16, 0x3C00);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x7C00);
        }
        invalid.write_vreg(k1, 1);
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut invalid, &mut memory, &function.blocks[0]),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &invalid.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[3], 0, 16), 0xFE00);
            assert_eq!(x86.mxcsr & 0x3F, 1);
        }

        let mut fault = SmirContext::new_x86_64();
        initialize(&mut fault);
        fault.write_vreg(k1, 1);
        let sentinel = [0xA5A5_A5A5_A5A5_A5A5; 16];
        if let ArchRegState::X86_64(x86) = &mut fault.arch_regs {
            x86.mxcsr &= !(1 << 7);
            x86.xmm[3] = sentinel;
        }
        let function = make_function(FpRoundMode::Dynamic);
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut fault, &mut memory, &function.blocks[0]),
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        if let ArchRegState::X86_64(x86) = &fault.arch_regs {
            assert_eq!(
                x86.xmm[3], sentinel,
                "unmasked exception committed a result"
            );
            assert_ne!(x86.mxcsr & 1, 0);
        }
    }
    #[test]
    fn lifted_evex_fp16_fma_executes_and_zero_mask_suppresses_memory_faults() {
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..8 {
                SmirInterpreter::set_lane(&mut x86.xmm[0], lane, 16, 0x3C00);
                SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 16, 0x3C00);
            }
        }
        let mut memory = FlatMemory::new(0x100);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF6, 0x7D, 0x08, 0x98, 0xC8], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8 {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 16), 0x4000);
            }
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        ctx.write_vreg(rax, 0x1_0000);
        ctx.write_vreg(k1, 0);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF6, 0x7D, 0x99, 0x98, 0x10], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert!(x86.xmm[2].iter().all(|word| *word == 0));
        }
    }
    #[test]
    fn x86_fp16_complex_preserves_boundary_rounding_pair_masks_scalar_copy_and_masked_exceptions() {
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let src1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
        let src2 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let make_function = |mask, pairs, scalar, mask_zeroing, accumulate, conjugate, round| {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::X86FP16Complex {
                    dst,
                    src1,
                    src2,
                    mask,
                    width: VecWidth::V128,
                    pairs,
                    scalar,
                    mask_zeroing,
                    accumulate,
                    conjugate,
                    round,
                },
            );
            builder.set_terminator(Terminator::Trap {
                kind: TrapKind::Halt,
            });
            builder.finish()
        };
        let mut memory = FlatMemory::new(0x100);

        for (accumulate, conjugate, expected_real, expected_imag) in [
            (false, false, 0x4000, 0x4200),
            (false, true, 0x4000, 0xC200),
            (true, false, 0x4200, 0x4500),
            (true, true, 0x4200, 0xBC00),
        ] {
            let mut ctx = SmirContext::new_x86_64();
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                SmirInterpreter::set_lane(&mut x86.xmm[0], 0, 16, 0x3C00); // 1 + 2i accumulator
                SmirInterpreter::set_lane(&mut x86.xmm[0], 1, 16, 0x4000);
                SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 16, 0x3C00); // 1 + 0i
                SmirInterpreter::set_lane(&mut x86.xmm[1], 1, 16, 0);
                SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x4000); // 2 + 3i
                SmirInterpreter::set_lane(&mut x86.xmm[2], 1, 16, 0x4200);
            }
            let function = make_function(
                None,
                1,
                false,
                false,
                accumulate,
                conjugate,
                FpRoundMode::Dynamic,
            );
            assert!(matches!(
                SmirInterpreter::new().execute_block(&mut ctx, &mut memory, &function.blocks[0]),
                BlockResult::Exit(ExitReason::Halt)
            ));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 0, 16), expected_real);
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 1, 16), expected_imag);
            }
        }

        // The first product is rounded to 106.125 before the second FMA.
        // Collapsing the two architectural boundaries would produce 0x56A1.
        let mut boundary = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut boundary.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 16, 0x44E5);
            SmirInterpreter::set_lane(&mut x86.xmm[1], 1, 16, 0x2A72);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x4D6C);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 1, 16, 0x3FFB);
        }
        let function = make_function(None, 1, false, false, false, false, FpRoundMode::Dynamic);
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut boundary, &mut memory, &function.blocks[0]),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &boundary.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 0, 16), 0x56A0);
        }

        for (round, expected) in [
            (FpRoundMode::RoundNearest, 0x3C00),
            (FpRoundMode::RoundDown, 0x3BFF),
            (FpRoundMode::RoundUp, 0x3C00),
            (FpRoundMode::RoundTowardZero, 0x3BFF),
        ] {
            let mut directed = SmirContext::new_x86_64();
            if let ArchRegState::X86_64(x86) = &mut directed.arch_regs {
                SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 16, 0x3C00);
                SmirInterpreter::set_lane(&mut x86.xmm[1], 1, 16, 0x3C00);
                SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x3C00);
                SmirInterpreter::set_lane(&mut x86.xmm[2], 1, 16, 0x0C00);
            }
            let function = make_function(None, 1, false, false, false, false, round);
            assert!(matches!(
                SmirInterpreter::new().execute_block(
                    &mut directed,
                    &mut memory,
                    &function.blocks[0]
                ),
                BlockResult::Exit(ExitReason::Halt)
            ));
            if let ArchRegState::X86_64(x86) = &directed.arch_regs {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[0], 0, 16),
                    expected,
                    "{round:?}"
                );
                assert_eq!(x86.mxcsr & 0x3F, 0, "embedded rounding leaked status");
            }
        }

        let mut gradual = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut gradual.arch_regs {
            x86.mxcsr |= (1 << 6) | (1 << 15); // DAZ and FTZ are ignored for FP16
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 16, 0x0001);
            SmirInterpreter::set_lane(&mut x86.xmm[1], 1, 16, 0);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x3C00);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 1, 16, 0);
        }
        let function = make_function(None, 1, false, false, false, false, FpRoundMode::Dynamic);
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut gradual, &mut memory, &function.blocks[0]),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &gradual.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 0, 16), 0x0001);
            assert_ne!(x86.mxcsr & (1 << 1), 0);
        }

        let initialize_masked = |ctx: &mut SmirContext| {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                SmirInterpreter::set_lane(&mut x86.xmm[0], 0, 16, 0x3555);
                SmirInterpreter::set_lane(&mut x86.xmm[0], 1, 16, 0x3666);
                SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 16, 0x7D01);
                SmirInterpreter::set_lane(&mut x86.xmm[1], 1, 16, 0);
                SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x3C00);
                SmirInterpreter::set_lane(&mut x86.xmm[2], 1, 16, 0);
                SmirInterpreter::set_lane(&mut x86.xmm[1], 2, 16, 0x3C00);
                SmirInterpreter::set_lane(&mut x86.xmm[1], 3, 16, 0);
                SmirInterpreter::set_lane(&mut x86.xmm[2], 2, 16, 0x4000);
                SmirInterpreter::set_lane(&mut x86.xmm[2], 3, 16, 0x4200);
            }
            ctx.write_vreg(k1, 1 << 1);
        };
        let mut masked_merge = SmirContext::new_x86_64();
        initialize_masked(&mut masked_merge);
        let function = make_function(
            Some(k1),
            4,
            false,
            false,
            false,
            false,
            FpRoundMode::Dynamic,
        );
        assert!(matches!(
            SmirInterpreter::new().execute_block(
                &mut masked_merge,
                &mut memory,
                &function.blocks[0]
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &masked_merge.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 0, 16), 0x3555);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 1, 16), 0x3666);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 2, 16), 0x4000);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 3, 16), 0x4200);
            assert_eq!(x86.mxcsr & 0x3F, 0, "masked pair raised an exception");
        }

        let mut scalar = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut scalar.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 16, 0x3C00);
            SmirInterpreter::set_lane(&mut x86.xmm[1], 1, 16, 0);
            for lane in 2..8 {
                SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 16, 0x3000 + u64::from(lane));
            }
            x86.xmm[1][2..].fill(u64::MAX);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x4000);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 1, 16, 0x4200);
        }
        let function = make_function(None, 1, true, false, false, false, FpRoundMode::Dynamic);
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut scalar, &mut memory, &function.blocks[0]),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &scalar.arch_regs {
            for lane in 2..8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[0], lane, 16),
                    0x3000 + u64::from(lane)
                );
            }
            assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        }

        let mut mandatory_masking = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut mandatory_masking.arch_regs {
            x86.mxcsr &= !(1 << 7); // invalid is architecturally unmasked in MXCSR
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 16, 0x7D01);
            SmirInterpreter::set_lane(&mut x86.xmm[1], 1, 16, 0);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x3C00);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 1, 16, 0);
        }
        let function = make_function(None, 1, false, false, false, false, FpRoundMode::Dynamic);
        assert!(matches!(
            SmirInterpreter::new().execute_block(
                &mut mandatory_masking,
                &mut memory,
                &function.blocks[0]
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &mandatory_masking.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 0, 16), 0x7F01);
            assert_ne!(x86.mxcsr & 1, 0);
        }

        let mut embedded = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut embedded.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 16, 0x7D01);
            SmirInterpreter::set_lane(&mut x86.xmm[1], 1, 16, 0);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0x3C00);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 1, 16, 0);
        }
        let function = make_function(
            None,
            1,
            false,
            false,
            false,
            false,
            FpRoundMode::RoundNearest,
        );
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut embedded, &mut memory, &function.blocks[0]),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &embedded.arch_regs {
            assert_eq!(
                x86.mxcsr & 0x3F,
                0,
                "embedded rounding did not suppress status"
            );
        }
    }
    #[test]
    fn lifted_evex_fp16_complex_zero_mask_suppresses_memory_fault() {
        let mut ctx = SmirContext::new_x86_64();
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        ctx.write_vreg(rax, 0x1_0000);
        ctx.write_vreg(k1, 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2][0] = 0xA5A5_5A5A_1234_5678;
        }
        let mut memory = FlatMemory::new(0x100);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF6, 0x7E, 0x09, 0xD7, 0x10], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2][0] & 0xFFFF_FFFF, 0x1234_5678);
            assert!(x86.xmm[2][2..].iter().all(|word| *word == 0));
        }
    }
    #[test]
    fn lifted_vex_evex_gather_executes_vsib_masks_and_restartable_faults() {
        fn packed_lanes(values: &[u64], bits: u32, fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (lane, value) in values.iter().copied().enumerate() {
                SmirInterpreter::set_lane(&mut out, lane as u8, bits, value);
            }
            out
        }

        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k3 = VReg::Arch(ArchReg::X86(X86Reg::K(3)));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // VEX.256 VPGATHERDD uses signed dword indices, the sign bit of each
        // dword mask element, and scale four. Inactive lanes retain DEST.
        let base = 0x100u64;
        let indices = [0i32, 1, -1, 3, 4, 5, 6, 7];
        let gathered = [
            0x1000_0000u32,
            0x1000_0001,
            0x1000_00FF,
            0x1000_0003,
            0x1000_0004,
            0x1000_0005,
            0x1000_0006,
            0x1000_0007,
        ];
        for (index, value) in indices.into_iter().zip(gathered) {
            memory
                .write(
                    base.wrapping_add_signed(i64::from(index) * 4),
                    &value.to_le_bytes(),
                )
                .unwrap();
        }
        let old = (0..8).map(|lane| 0xA000_0000u64 + lane).collect::<Vec<_>>();
        let active = [true, false, true, false, true, true, false, true];
        let masks = active
            .into_iter()
            .map(|set| {
                if set {
                    u64::from(u32::MAX)
                } else {
                    0x7FFF_FFFF
                }
            })
            .collect::<Vec<_>>();
        ctx.write_vreg(rax, base);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed_lanes(&masks, 32, u64::MAX);
            x86.xmm[2] = packed_lanes(
                &indices
                    .into_iter()
                    .map(|value| u64::from(value as u32))
                    .collect::<Vec<_>>(),
                32,
                0,
            );
            x86.xmm[3] = packed_lanes(&old, 32, u64::MAX);
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x90, 0x1C, 0x90], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[3], lane, 32),
                    if active[lane as usize] {
                        u64::from(gathered[lane as usize])
                    } else {
                        old[lane as usize]
                    }
                );
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 32), 0);
            }
            assert!(x86.xmm[3][4..].iter().all(|word| *word == 0));
            assert!(x86.xmm[1][4..].iter().all(|word| *word == 0));
        }

        // A 67h override truncates the GPR base and performs VSIB arithmetic
        // modulo 2^32 before memory access. FS is added after ordinary 64-bit
        // base/index/displacement arithmetic in the segment-prefixed form.
        memory.write(0x44, &0x4455_6677u32.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0xFFFF_FFFF_0000_0040);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed_lanes(&[u64::from(u32::MAX)], 32, 0);
            x86.xmm[2] = [0; 16];
            x86.xmm[3] = [0; 16];
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x67, 0xC4, 0xE2, 0x75, 0x90, 0x5C, 0x90, 0x04],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[3], 0, 32), 0x4455_6677);
        }
        let fs_base = VReg::Arch(ArchReg::X86(X86Reg::FsBase));
        memory.write(0x144, &0x8899_AABBu32.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x40);
        ctx.write_vreg(fs_base, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed_lanes(&[u64::from(u32::MAX)], 32, 0);
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x64, 0xC4, 0xE2, 0x75, 0x90, 0x5C, 0x90, 0x04],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[3], 0, 32), 0x8899_AABB);
        }
        ctx.write_vreg(fs_base, 0);

        // A lane-two fault occurs only after lanes zero and one have committed.
        // Their mask elements are cleared; the faulting and later active mask
        // elements remain normalized to all ones for restart.
        let fault_base = 0x3F8u64;
        memory
            .write(fault_base, &0x1111_1111u32.to_le_bytes())
            .unwrap();
        memory
            .write(fault_base + 4, &0x2222_2222u32.to_le_bytes())
            .unwrap();
        ctx.write_vreg(rax, fault_base);
        let fault_indices = (0..8).map(|lane| lane as u64).collect::<Vec<_>>();
        let fault_masks = (0..8)
            .map(|lane| if lane < 4 { u64::from(u32::MAX) } else { 1 })
            .collect::<Vec<_>>();
        let sentinel = (0..8).map(|lane| 0xB000_0000u64 + lane).collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed_lanes(&fault_masks, 32, u64::MAX);
            x86.xmm[2] = packed_lanes(&fault_indices, 32, 0);
            x86.xmm[3] = packed_lanes(&sentinel, 32, u64::MAX);
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x90, 0x1C, 0x90], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[3], 0, 32), 0x1111_1111);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[3], 1, 32), 0x2222_2222);
            for lane in 2..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[3], lane, 32),
                    sentinel[lane as usize]
                );
            }
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 32), 0);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 1, 32), 0);
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], 2, 32),
                u64::from(u32::MAX)
            );
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], 3, 32),
                u64::from(u32::MAX)
            );
            for lane in 4..8u8 {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 32), 0);
            }
        }

        // EVEX.512 VGATHERDPS uses an opmask and high ZMM index/destination
        // registers. A successful instruction clears the complete opmask.
        let evex_base = 0x180u64;
        let evex_values = (0..16)
            .map(|lane| 0xC000_0000u32 + lane)
            .collect::<Vec<_>>();
        for (lane, value) in evex_values.iter().copied().enumerate() {
            memory
                .write(evex_base + (lane * 4) as u64, &value.to_le_bytes())
                .unwrap();
        }
        let evex_mask = 0xA55Au64;
        let evex_old = (0..16)
            .map(|lane| 0xD000_0000u64 + lane)
            .collect::<Vec<_>>();
        ctx.write_vreg(rax, evex_base);
        ctx.write_vreg(k3, evex_mask | (u64::MAX << 16));
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = packed_lanes(&(0..16).collect::<Vec<_>>(), 32, 0);
            x86.xmm[18] = packed_lanes(&evex_old, 32, 0);
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xE2, 0x7D, 0x43, 0x92, 0x14, 0x88],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[18], lane, 32),
                    if evex_mask >> lane & 1 != 0 {
                        u64::from(evex_values[lane as usize])
                    } else {
                        evex_old[lane as usize]
                    }
                );
            }
        }
        assert_eq!(ctx.read_vreg(k3), 0);

        // EVEX disp8 is compressed by the scalar data tuple (N=8 here).
        // This also covers dword indices feeding qword results in high regs.
        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let k5 = VReg::Arch(ArchReg::X86(X86Reg::K(5)));
        let compressed_base = 0x240u64;
        let qwords = (0..8)
            .map(|lane| 0x1122_3344_5566_7700u64 + lane)
            .collect::<Vec<_>>();
        for (lane, value) in qwords.iter().copied().enumerate() {
            memory
                .write(
                    compressed_base + 16 + (lane * 8) as u64,
                    &value.to_le_bytes(),
                )
                .unwrap();
        }
        ctx.write_vreg(r8, compressed_base);
        ctx.write_vreg(k5, 0xFF);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[19] = packed_lanes(&(0..8).collect::<Vec<_>>(), 32, 0);
            x86.xmm[20] = [u64::MAX; 16];
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xC2, 0xFD, 0x45, 0x90, 0x64, 0xD8, 0x02],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[20], lane, 64),
                    qwords[lane as usize]
                );
            }
            assert!(x86.xmm[20][8..].iter().all(|word| *word == 0));
        }
        assert_eq!(ctx.read_vreg(k5), 0);

        // An all-zero EVEX mask suppresses every memory access, including an
        // otherwise faulting VSIB base, and leaves the full-width destination.
        ctx.write_vreg(rax, 0x1000);
        ctx.write_vreg(k3, 0);
        let zero_mask_sentinel = [0xE5E5_E5E5_E5E5_E5E5; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[18] = zero_mask_sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xE2, 0x7D, 0x43, 0x92, 0x14, 0x88],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[18][..8], &zero_mask_sentinel[..8]);
            assert!(x86.xmm[18][8..].iter().all(|word| *word == 0));
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn x86_fp16_to_fp32_is_exhaustive_over_binary16_encoding_space() {
        for bits in 0..=u16::MAX {
            let sign = u32::from(bits >> 15) << 31;
            let exponent = u32::from((bits >> 10) & 0x1F);
            let fraction = u32::from(bits & 0x03FF);
            let expected = if exponent == 0x1F {
                if fraction == 0 {
                    sign | 0x7F80_0000
                } else {
                    sign | 0x7FC0_0000 | (fraction << 13)
                }
            } else if exponent == 0 && fraction == 0 {
                sign
            } else {
                // Independent numerical oracle: every binary16 finite value is
                // exactly representable in binary32, including subnormals.
                let magnitude = if exponent == 0 {
                    (fraction as f32) * 2.0f32.powi(-24)
                } else {
                    ((1024 + fraction) as f32) * 2.0f32.powi(exponent as i32 - 25)
                };
                if sign == 0 {
                    magnitude.to_bits()
                } else {
                    (-magnitude).to_bits()
                }
            };
            assert_eq!(
                SmirInterpreter::x86_fp16_to_fp32_bits(bits),
                expected,
                "binary16 input {bits:#06x}",
            );
        }
    }
    #[test]
    fn lifted_legacy_evex_packed_immediate_shifts_execute_masks_and_fault_classes() {
        fn bytes(value: &VecValue, count: usize) -> Vec<u8> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count)
                .collect()
        }

        fn shifted_mmx(value: u64, bits: u32, amount: u8, shift: ShiftOp) -> u64 {
            let lane_mask = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            let mut result = 0;
            for lane in 0..64 / bits {
                let lane_shift = lane * bits;
                let input = value >> lane_shift & lane_mask;
                let output = if u32::from(amount) >= bits {
                    if shift == ShiftOp::Asr && input & (1u64 << (bits - 1)) != 0 {
                        lane_mask
                    } else {
                        0
                    }
                } else {
                    match shift {
                        ShiftOp::Lsl => input << amount & lane_mask,
                        ShiftOp::Lsr => input >> amount,
                        ShiftOp::Asr => {
                            let signed = if bits == 64 {
                                input as i64
                            } else {
                                ((input << (64 - bits)) as i64) >> (64 - bits)
                            };
                            (signed >> amount) as u64 & lane_mask
                        }
                        _ => unreachable!(),
                    }
                };
                result |= output << lane_shift;
            }
            result
        }

        let sentinel = [0x6B6B_6B6B_6B6B_6B6Bu64; 16];
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        let mmx_source = 0x8001_7FFF_F00F_00F0;
        for (opcode, group, bits, shift) in [
            (0x71, 2, 16, ShiftOp::Lsr),
            (0x71, 4, 16, ShiftOp::Asr),
            (0x71, 6, 16, ShiftOp::Lsl),
            (0x72, 2, 32, ShiftOp::Lsr),
            (0x72, 4, 32, ShiftOp::Asr),
            (0x72, 6, 32, ShiftOp::Lsl),
            (0x73, 2, 64, ShiftOp::Lsr),
            (0x73, 6, 64, ShiftOp::Lsl),
        ] {
            for amount in [0, bits as u8 - 1, bits as u8, bits as u8 + 1, u8::MAX] {
                if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                    x86.mm[0] = mmx_source;
                    x86.x87.tag_word = 0xFFFF;
                    x86.x87.status_word = 3 << 11;
                }
                execute_lifted_x86(
                    &[0x0F, opcode, 0xC0 | (group << 3), amount],
                    &mut ctx,
                    &mut memory,
                );
                if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                    assert_eq!(
                        x86.mm[0],
                        shifted_mmx(mmx_source, bits, amount, shift),
                        "MMX opcode {opcode:02X}, count {amount}",
                    );
                    assert_eq!(x86.x87.tag_word, 0);
                    assert_eq!(x86.x87.status_word & 0x3800, 3 << 11);
                }
            }
        }

        let words = (0..8)
            .flat_map(|lane| if lane % 2 == 0 { 0x8001u16 } else { 0x7FFF }.to_le_bytes())
            .collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vec_from_bytes(&words);
            x86.xmm[9][2..].fill(upper);
        }
        execute_lifted_x86(&[0x66, 0x41, 0x0F, 0x71, 0xE1, 17], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let expected = (0..8)
                .flat_map(|lane| if lane % 2 == 0 { u16::MAX } else { 0 }.to_le_bytes())
                .collect::<Vec<_>>();
            assert_eq!(bytes(&x86.xmm[9], 16), expected);
            assert!(x86.xmm[9][2..].iter().all(|word| *word == upper));
        }

        let lane_bytes = (0u8..16).collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vec_from_bytes(&lane_bytes);
            x86.xmm[9][2..].fill(upper);
        }
        execute_lifted_x86(&[0x66, 0x41, 0x0F, 0x73, 0xF9, 1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[9], 16),
                [0].into_iter().chain(0u8..15).collect::<Vec<_>>()
            );
            assert!(x86.xmm[9][2..].iter().all(|word| *word == upper));
        }

        // EVEX.W=1 selects arithmetic qword shifts and masks each qword.
        let qwords = [
            0x8000_0000_0000_0010,
            0x7FFF_FFFF_FFFF_FFF0,
            0xF000_0000_0000_0000,
            0x1000_0000_0000_0000,
            0xFFFF_FFFF_FFFF_FFFF,
            1,
            0x9000_0000_0000_0000,
            0x7000_0000_0000_0000,
        ];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = sentinel;
            x86.xmm[18][..8].copy_from_slice(&qwords);
        }
        ctx.write_vreg(k1, 0xA5);
        execute_lifted_x86(
            &[0x62, 0xB1, 0xFD, 0x41, 0x72, 0xE2, 0x04],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for (lane, value) in qwords.iter().copied().enumerate() {
                assert_eq!(
                    x86.xmm[16][lane],
                    if 0xA5u64 >> lane & 1 != 0 {
                        ((value as i64) >> 4) as u64
                    } else {
                        sentinel[lane]
                    },
                );
            }
        }

        // Type E4 full-tuple memory suppresses faults independently per active
        // dword. Only lane 0 at 0xFC is mapped.
        memory.write(0xFC, &0x8000_0018u32.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0xFC);
        ctx.write_vreg(k1, 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let active_lane = execute_lifted_x86(
            &[0x62, 0xF1, 0x7D, 0x49, 0x72, 0x10, 0x03],
            &mut ctx,
            &mut memory,
        );
        assert!(!matches!(
            active_lane,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 0, 32), 0x1000_0003);
            for lane in 1..16 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[0], lane, 32),
                    0x6B6B_6B6B
                );
            }
        }

        ctx.write_vreg(k1, 2);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let exposed = execute_lifted_x86(
            &[0x62, 0xF1, 0x7D, 0x49, 0x72, 0x10, 0x03],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            exposed,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        // Word immediate shifts are E4NF.nb: the complete vector load faults
        // despite a mask selecting only the mapped first word.
        ctx.write_vreg(k1, 1);
        let e4nf = execute_lifted_x86(
            &[0x62, 0xF1, 0x7D, 0x49, 0x71, 0x10, 0x03],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            e4nf,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));

        // A masked-off broadcast performs no memory access at all.
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k1, 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let broadcast = execute_lifted_x86(
            &[0x62, 0xF1, 0x7D, 0x59, 0x72, 0x10, 0x03],
            &mut ctx,
            &mut memory,
        );
        assert!(!matches!(
            broadcast,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert!(x86.xmm[0][..8].iter().all(|word| *word == sentinel[0]));
            assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn x86_fixup_imm_exact_semantics_cover_tokens_responses_daz_and_reports() {
        let dest32 = 0xDEAD_BEEFu64;
        let positive_two32 = u64::from(2.0f32.to_bits());
        let expected32 = [
            dest32,
            positive_two32,
            0x7FC0_0000,
            0xFFC0_0000,
            0xFF80_0000,
            0x7F80_0000,
            0x7F80_0000,
            0x8000_0000,
            0x0000_0000,
            0xBF80_0000,
            0x3F80_0000,
            0x3F00_0000,
            0x42B4_0000,
            0x3FC9_0FDB,
            0x7F7F_FFFF,
            0xFF7F_FFFF,
        ];
        for response in 0..16u8 {
            let result = SmirInterpreter::x86_simd_fixup_imm(
                dest32,
                positive_two32,
                u64::from(response) << 28,
                X86_SIMD_F32,
                0x1F80,
                0,
            );
            assert_eq!(
                result.bits, expected32[response as usize],
                "FP32 response {response:#x}"
            );
            assert_eq!(result.status, 0);
        }

        let dest64 = 0xDEAD_BEEF_CAFE_BABEu64;
        let positive_two64 = 2.0f64.to_bits();
        let expected64 = [
            dest64,
            positive_two64,
            0x7FF8_0000_0000_0000,
            0xFFF8_0000_0000_0000,
            0xFFF0_0000_0000_0000,
            0x7FF0_0000_0000_0000,
            0x7FF0_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x0000_0000_0000_0000,
            0xBFF0_0000_0000_0000,
            0x3FF0_0000_0000_0000,
            0x3FE0_0000_0000_0000,
            0x4056_8000_0000_0000,
            0x3FF9_21FB_5444_2D18,
            0x7FEF_FFFF_FFFF_FFFF,
            0xFFEF_FFFF_FFFF_FFFF,
        ];
        for response in 0..16u8 {
            let result = SmirInterpreter::x86_simd_fixup_imm(
                dest64,
                positive_two64,
                u64::from(response) << 28,
                X86_SIMD_F64,
                0x1F80,
                0,
            );
            assert_eq!(
                result.bits, expected64[response as usize],
                "FP64 response {response:#x}"
            );
            assert_eq!(result.status, 0);
        }

        let tokens = [
            (0x7FC1_2345u64, 0),
            (0x7F81_2345, 1),
            (0x8000_0000, 2),
            (0x3F80_0000, 3),
            (0xFF80_0000, 4),
            (0x7F80_0000, 5),
            (0xC000_0000, 6),
            (0x4000_0000, 7),
        ];
        for (src, token) in tokens {
            let result = SmirInterpreter::x86_simd_fixup_imm(
                dest32,
                src,
                0xAu64 << (token * 4),
                X86_SIMD_F32,
                0x1F80,
                0xFF,
            );
            assert_eq!(result.bits, 0x3F80_0000, "token {token}");
            assert_eq!(
                result.status,
                match token {
                    1 | 4 | 5 | 6 => 1 << 0,
                    2 | 3 => (1 << 0) | (1 << 2),
                    _ => 0,
                },
                "token {token} report"
            );
        }

        let qnan =
            SmirInterpreter::x86_simd_fixup_imm(dest32, 0xFFC1_2345, 2, X86_SIMD_F32, 0x1F80, 0);
        assert_eq!(qnan.bits, 0xFFC1_2345);
        let snan = SmirInterpreter::x86_simd_fixup_imm(
            dest32,
            0xFF81_2345,
            2u64 << 4,
            X86_SIMD_F32,
            0x1F80,
            0,
        );
        assert_eq!(snan.bits, 0xFFC1_2345);
        assert_eq!(snan.status, 0, "sNaN reporting depends only on imm[4]");

        let negative_subnormal = 0x8000_0001u64;
        let table = (0xAu64 << 8) | (0x9u64 << 24);
        assert_eq!(
            SmirInterpreter::x86_simd_fixup_imm(
                dest32,
                negative_subnormal,
                table,
                X86_SIMD_F32,
                0x1F80,
                0,
            )
            .bits,
            0xBF80_0000,
        );
        assert_eq!(
            SmirInterpreter::x86_simd_fixup_imm(
                dest32,
                negative_subnormal,
                table,
                X86_SIMD_F32,
                0x1FC0,
                0,
            )
            .bits,
            0x3F80_0000,
            "DAZ maps a negative subnormal to the +zero token",
        );
        assert_eq!(
            SmirInterpreter::x86_simd_fixup_imm(
                dest32,
                0x8000_0000,
                0x6u64 << 8,
                X86_SIMD_F32,
                0x1FC0,
                0,
            )
            .bits,
            0x7F80_0000,
            "DAZ replacement +0 also controls response-six sign",
        );
    }
    #[test]
    fn lifted_x86_fixup_imm_preserves_old_dst_masks_upper_lanes_and_masked_reporting() {
        let run = |encoding: &[u8], mask: u64, initial_mxcsr: u32| {
            let mut ctx = SmirContext::new_x86_64();
            let mut memory = FlatMemory::new(0x100);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1][0] = 0xAAAA_AAAA_DEAD_BEEF;
                x86.xmm[1][1] = 0xBBBB_BBBB_CCCC_CCCC;
                x86.xmm[2][0] = 0x1111_1111_0000_0000;
                x86.xmm[2][1] = 0x2222_2222_3333_3333;
                x86.xmm[3][0] = 0x0000_0000_0000_0A00;
                x86.k[1] = mask;
                x86.mxcsr = initial_mxcsr;
            }
            let exit = execute_lifted_x86(encoding, &mut ctx, &mut memory);
            assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
            ctx
        };

        // Active +zero chooses table response +1 and imm[1:0] reports IE+ZE.
        // Even with MXCSR exception masks cleared, VFIXUPIMM never raises #XM.
        let active = run(&[0x62, 0xF3, 0x6D, 0x09, 0x55, 0xCB, 0x03], 1, 0);
        if let ArchRegState::X86_64(x86) = &active.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x1111_1111_3F80_0000);
            assert_eq!(x86.xmm[1][1], 0x2222_2222_3333_3333);
            assert_eq!(x86.mxcsr & 0x3F, (1 << 0) | (1 << 2));
        }

        let sae = run(&[0x62, 0xF3, 0x6D, 0x19, 0x55, 0xCB, 0x03], 1, 0);
        if let ArchRegState::X86_64(x86) = &sae.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x1111_1111_3F80_0000);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }

        let merge = run(&[0x62, 0xF3, 0x6D, 0x09, 0x55, 0xCB, 0x03], 0, 0);
        if let ArchRegState::X86_64(x86) = &merge.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x1111_1111_DEAD_BEEF);
            assert_eq!(x86.mxcsr & 0x3F, 0, "inactive lane does not report");
        }

        let zero = run(&[0x62, 0xF3, 0x6D, 0x89, 0x55, 0xCB, 0x03], 0, 0);
        if let ArchRegState::X86_64(x86) = &zero.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x1111_1111_0000_0000);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }

        // An active response-zero lane consumes and preserves the old low dst.
        let response_zero = run(&[0x62, 0xF3, 0x6D, 0x09, 0x55, 0xCA, 0x00], 1, 0);
        if let ArchRegState::X86_64(x86) = &response_zero.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x1111_1111_DEAD_BEEF);
        }

        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k1, 0);
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xF3, 0x6D, 0x09, 0x55, 0x08, 0xFF],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.write_vreg(k1, 1);
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xF3, 0x6D, 0x09, 0x55, 0x08, 0xFF],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));

        // Scalar memory EVEX.b is SAE. A valid active load still occurs, but
        // its enabled zero reports do not update MXCSR.
        memory.write(0x80, &0xA00u32.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x80);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 0xAAAA_AAAA_DEAD_BEEF;
            x86.xmm[2][0] = 0x1111_1111_0000_0000;
            x86.mxcsr = 0;
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xF3, 0x6D, 0x19, 0x55, 0x08, 0x03],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x1111_1111_3F80_0000);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }
    }
    #[test]
    fn x86_fp16_approx_exhaustively_satisfies_special_cases_and_error_bound() {
        let limit = 2.0f64.powi(-11) + 2.0f64.powi(-14);
        for bits in 0u16..=u16::MAX {
            let magnitude = bits & 0x7FFF;
            let fraction = bits & 0x03FF;
            let input = f64::from(SmirInterpreter::x86_fp16_to_f32(bits));

            let reciprocal = SmirInterpreter::x86_fp16_approx(bits, false);
            if magnitude & 0x7C00 == 0x7C00 && fraction != 0 {
                assert_eq!(reciprocal, bits | 0x0200, "VRCP NaN {bits:04X}");
            } else if magnitude == 0 {
                assert_eq!(reciprocal, (bits & 0x8000) | 0x7C00);
            } else if magnitude == 0x7C00 {
                assert_eq!(reciprocal, bits & 0x8000);
            } else {
                let actual = f64::from(SmirInterpreter::x86_fp16_to_f32(reciprocal));
                let reference = input.recip();
                if actual.is_infinite() {
                    assert!(input.abs() <= 2.0f64.powi(-16));
                } else if reciprocal & 0x7C00 == 0 {
                    // The stated relative bound is unrepresentable for some
                    // binary16 subnormal reciprocals. Round-to-nearest instead
                    // supplies the tight binary16 absolute-error bound.
                    assert!(
                        (actual - reference).abs() <= 2.0f64.powi(-25),
                        "VRCPH {bits:04X}: subnormal absolute error {:e}",
                        (actual - reference).abs()
                    );
                } else {
                    let relative_error = ((actual - reference) / reference).abs();
                    assert!(
                        relative_error < limit,
                        "VRCPH {bits:04X}: relative error {relative_error:e}"
                    );
                }
            }

            let rsqrt = SmirInterpreter::x86_fp16_approx(bits, true);
            if magnitude & 0x7C00 == 0x7C00 && fraction != 0 {
                assert_eq!(rsqrt, bits | 0x0200, "VRSQRT NaN {bits:04X}");
            } else if bits & 0x8000 != 0 && magnitude != 0 {
                assert_eq!(rsqrt, 0xFE00, "VRSQRT negative {bits:04X}");
            } else if magnitude == 0 {
                assert_eq!(rsqrt, (bits & 0x8000) | 0x7C00);
            } else if magnitude == 0x7C00 {
                assert_eq!(rsqrt, 0);
            } else {
                let actual = f64::from(SmirInterpreter::x86_fp16_to_f32(rsqrt));
                let reference = input.sqrt().recip();
                let relative_error = ((actual - reference) / reference).abs();
                assert!(
                    relative_error < limit,
                    "VRSQRTH {bits:04X}: relative error {relative_error:e}"
                );
                if magnitude < 0x0400 {
                    assert_ne!(rsqrt & 0x7C00, 0, "denormal input returned denormal");
                }
            }
        }

        for exponent in -14..=15 {
            let input = 2.0f32.powi(exponent);
            let bits = SmirInterpreter::x86_f32_to_fp16(input, 0);
            let expected = SmirInterpreter::x86_f32_to_fp16(input.recip(), 0);
            assert_eq!(SmirInterpreter::x86_fp16_approx(bits, false), expected);
            if exponent % 2 == 0 {
                let expected_rsqrt =
                    SmirInterpreter::x86_f32_to_fp16(2.0f32.powi(-exponent / 2), 0);
                assert_eq!(SmirInterpreter::x86_fp16_approx(bits, true), expected_rsqrt);
            }
        }
    }
    #[test]
    fn lifted_x86_fp16_approx_preserves_masks_scalar_merge_mxcsr_and_fault_suppression() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0xAAAA_AAAA_DEAD_BEEF; 16];
            x86.xmm[2] = [
                0x0123_4567_89AB_CDEF,
                0x0FED_CBA9_8765_4321,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ];
            x86.xmm[3][0] = 0x4400;
            x86.mxcsr = 0xA5A5;
        }
        execute_lifted_x86(&[0x62, 0xF6, 0x6D, 0x08, 0x4F, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_89AB_3800);
            assert_eq!(x86.xmm[1][1], 0x0FED_CBA9_8765_4321);
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
            assert_eq!(x86.mxcsr, 0xA5A5);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0xDEAD_BEEF_CAFE_BABE; 16];
            for lane in 0..8 {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[3],
                    lane,
                    16,
                    u64::from(0x3C00u16 + u16::from(lane)),
                );
            }
            x86.k[2] = 0x55;
            x86.mxcsr = 0x5A5A;
        }
        execute_lifted_x86(&[0x62, 0xF6, 0x7D, 0x0A, 0x4C, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8 {
                let actual = SmirInterpreter::get_lane(&x86.xmm[1], lane, 16) as u16;
                if lane % 2 == 0 {
                    assert_eq!(
                        actual,
                        SmirInterpreter::x86_fp16_approx(0x3C00 + u16::from(lane), false)
                    );
                } else {
                    assert_eq!(
                        actual,
                        SmirInterpreter::get_lane(&[0xDEAD_BEEF_CAFE_BABE; 16], lane, 16) as u16
                    );
                }
            }
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
            assert_eq!(x86.mxcsr, 0x5A5A);
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k1, 0);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF6, 0x6D, 0x09, 0x4D, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.write_vreg(k1, 1);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF6, 0x6D, 0x09, 0x4D, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
    }
    #[test]
    fn lifted_avx512_four_fma_executes_sequential_rounding_masks_scalar_and_aliases() {
        let gradual = SmirInterpreter::x86_f32_fma_boundary(
            0x0080_0000,
            0x3F00_0000,
            0,
            false,
            FpRoundMode::RoundNearest,
            0x1F80,
        );
        assert_eq!(gradual.bits, 0x0040_0000);
        assert_eq!(gradual.status, 0);
        let flushed = SmirInterpreter::x86_f32_fma_boundary(
            0x0080_0000,
            0x3F00_0000,
            0,
            false,
            FpRoundMode::RoundNearest,
            0x9F80,
        );
        assert_eq!(flushed.bits, 0);
        assert_eq!(flushed.status, (1 << 4) | (1 << 5));

        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        let write_tuple = |memory: &mut FlatMemory, values: [f32; 4]| {
            let bytes = values
                .into_iter()
                .flat_map(|value| value.to_bits().to_le_bytes())
                .collect::<Vec<_>>();
            memory.write(0x80, &bytes).unwrap();
        };
        write_tuple(&mut memory, [10.0, 20.0, 30.0, 40.0]);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.gpr[0] = 0x80;
            x86.mxcsr = 0x1F80;
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 32, u64::from(1.0f32.to_bits()));
                for (reg, value) in [(4, 2.0f32), (5, 3.0), (6, 4.0), (7, 5.0)] {
                    SmirInterpreter::set_lane(
                        &mut x86.xmm[reg],
                        lane,
                        32,
                        u64::from(value.to_bits()),
                    );
                }
            }
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x48, 0x9A, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                    u64::from(401.0f32.to_bits())
                );
            }
            assert!(x86.xmm[1][8..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 32, u64::from(1.0f32.to_bits()));
            }
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x48, 0xAA, 0x08], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], 0, 32),
                u64::from((-399.0f32).to_bits())
            );
        }

        // Four half-ULP additions round independently. RN-even retains 1.0 at
        // every boundary; RU advances by one ULP at each boundary.
        write_tuple(&mut memory, [1.0; 4]);
        for (mxcsr, expected) in [(0x1F80, 0x3F80_0000u32), (0x5F80, 0x3F80_0004)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mxcsr = mxcsr;
                for lane in 0..16u8 {
                    SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 32, 0x3F80_0000);
                    for reg in 4..=7 {
                        SmirInterpreter::set_lane(&mut x86.xmm[reg], lane, 32, 0x3380_0000);
                    }
                }
            }
            execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x48, 0x9A, 0x08], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], 0, 32),
                    u64::from(expected)
                );
                assert_ne!(x86.mxcsr & (1 << 5), 0, "each half-ULP FMA is inexact");
            }
        }

        // Packed zeroing applies only to inactive destination lanes and scalar
        // operation preserves bits 127:32 while clearing bits above 127.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[1] = 1 << 1;
            x86.mxcsr = 0x1F80;
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[1],
                    lane,
                    32,
                    u64::from((100.0f32 + f32::from(lane)).to_bits()),
                );
                for reg in 4..=7 {
                    SmirInterpreter::set_lane(
                        &mut x86.xmm[reg],
                        lane,
                        32,
                        u64::from(1.0f32.to_bits()),
                    );
                }
            }
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0xC9, 0x9A, 0x08], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                    if lane == 1 {
                        u64::from(105.0f32.to_bits())
                    } else {
                        0
                    }
                );
            }
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[1] = 1;
            x86.xmm[1] = [0xA5A5_A5A5_A5A5_A5A5; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 32, 0x3F80_0000);
            for reg in 4..=7 {
                SmirInterpreter::set_lane(&mut x86.xmm[reg], 0, 32, 0x3F80_0000);
            }
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x09, 0x9B, 0x08], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 32), 0x40A0_0000);
            for lane in 1..4u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                    0xA5A5_A5A5
                );
            }
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        }

        // Destination ZMM5 overlaps source block ZMM4..ZMM7. Stage one must
        // observe the pre-instruction ZMM5 snapshot, not a partial result.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[5] = [0; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[5], 0, 32, 10.0f32.to_bits().into());
            for (reg, value) in [(4, 1.0f32), (6, 1.0), (7, 1.0)] {
                SmirInterpreter::set_lane(&mut x86.xmm[reg], 0, 32, u64::from(value.to_bits()));
            }
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x48, 0x9A, 0x28], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[5], 0, 32),
                u64::from(23.0f32.to_bits())
            );
        }
    }
    #[test]
    fn lifted_avx512_four_fma_orders_exceptions_and_suppresses_whole_tuple_faults() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let sentinel = [0xDEAD_BEEF_CAFE_BABEu64; 16];
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k1, 1 << 16);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x49, 0x9A, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[1][..8], &sentinel[..8]);
            assert!(x86.xmm[1][8..].iter().all(|word| *word == 0));
        }

        ctx.write_vreg(k1, 1 << 15);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x49, 0x9A, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }

        ctx.write_vreg(k1, 2);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x09, 0x9B, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.write_vreg(k1, 1);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x09, 0x9B, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));

        let tuple = [
            1.0f32.to_bits(),
            f32::INFINITY.to_bits(),
            1.0f32.to_bits(),
            1.0f32.to_bits(),
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
        memory.write(0x80, &tuple).unwrap();
        ctx.write_vreg(rax, 0x80);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F80 & !(1 << 7); // Unmask invalid only.
            x86.xmm[1] = sentinel;
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 32, 0x3F80_0000);
                SmirInterpreter::set_lane(&mut x86.xmm[4], lane, 32, 0x3380_0000);
                SmirInterpreter::set_lane(&mut x86.xmm[5], lane, 32, 0);
                SmirInterpreter::set_lane(&mut x86.xmm[6], lane, 32, 1);
                SmirInterpreter::set_lane(&mut x86.xmm[7], lane, 32, 0x3F80_0000);
            }
        }
        let before = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.xmm[1],
            _ => unreachable!(),
        };
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x48, 0x9A, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.xmm[1], before,
                "faulting FMA must not commit destination"
            );
            assert_ne!(x86.mxcsr & 1, 0, "stage 1 invalid status");
            assert_ne!(x86.mxcsr & (1 << 5), 0, "stage 0 precision status");
            assert_eq!(x86.mxcsr & (1 << 1), 0, "stage 2 must not execute");
        }
    }
    #[test]
    fn lifted_avx512_four_dot_product_wraps_saturates_masks_aliases_and_suppresses_faults() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let write_tuple = |memory: &mut FlatMemory, pairs: [(i16, i16); 4]| {
            let bytes = pairs
                .into_iter()
                .flat_map(|(low, high)| low.to_le_bytes().into_iter().chain(high.to_le_bytes()))
                .collect::<Vec<_>>();
            memory.write(0x80, &bytes).unwrap();
        };
        let seed_sources = |ctx: &mut SmirContext, pairs: [(i16, i16); 4]| {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                for (stage, (low, high)) in pairs.into_iter().enumerate() {
                    x86.xmm[4 + stage] = [0; 16];
                    for lane in 0..16u8 {
                        SmirInterpreter::set_lane(
                            &mut x86.xmm[4 + stage],
                            lane * 2,
                            16,
                            u64::from(low as u16),
                        );
                        SmirInterpreter::set_lane(
                            &mut x86.xmm[4 + stage],
                            lane * 2 + 1,
                            16,
                            u64::from(high as u16),
                        );
                    }
                }
            }
        };

        ctx.write_vreg(rax, 0x80);
        write_tuple(&mut memory, [(3, 4), (7, 8), (10, 11), (4, 5)]);
        seed_sources(&mut ctx, [(1, 2), (5, 6), (1, -1), (2, 3)]);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 32, 10);
            }
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x48, 0x52, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 32), 126);
            }
            assert!(x86.xmm[1][8..].iter().all(|word| *word == 0));
        }

        write_tuple(&mut memory, [(1, 0); 4]);
        seed_sources(&mut ctx, [(1, 0), (0, 0), (0, 0), (0, 0)]);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 32, i32::MAX as u64);
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x48, 0x52, 0x08], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], 0, 32),
                i32::MIN as u32 as u64
            );
        }

        // Signed saturation occurs after every iteration. Stage 0 clamps
        // MAX-5 + 10 to MAX before stage 1 subtracts 20.
        write_tuple(&mut memory, [(1, 0); 4]);
        seed_sources(&mut ctx, [(10, 0), (-20, 0), (0, 0), (0, 0)]);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 32, (i32::MAX - 5) as u32 as u64);
            }
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x48, 0x53, 0x08], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], 0, 32),
                (i32::MAX - 20) as u32 as u64
            );
        }

        // Packed zero masking applies to inactive dword lanes.
        seed_sources(&mut ctx, [(1, 0); 4]);
        ctx.write_vreg(k1, 1 << 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 32, 100 + u64::from(lane));
            }
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0xC9, 0x52, 0x08], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                    if lane == 1 { 105 } else { 0 }
                );
            }
        }

        // ZMM5 aliases the second register of source block ZMM4..ZMM7. Its
        // contribution must use the pre-instruction value 10.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[5] = [0; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[5], 0, 32, 10);
            for reg in [4, 6, 7] {
                x86.xmm[reg] = [0; 16];
                SmirInterpreter::set_lane(&mut x86.xmm[reg], 0, 16, 1);
            }
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x48, 0x52, 0x28], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[5], 0, 32), 23);
        }

        // Mask bits above KL=16 cannot trigger the Tuple1_4X read; bit 15 can.
        let sentinel = [0xDEAD_BEEF_CAFE_BABEu64; 16];
        ctx.write_vreg(rax, 0x200);
        ctx.write_vreg(k1, 1 << 16);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x49, 0x52, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[1][..8], &sentinel[..8]);
            assert!(x86.xmm[1][8..].iter().all(|word| *word == 0));
        }
        ctx.write_vreg(k1, 1 << 15);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x5F, 0x49, 0x52, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }
    }

