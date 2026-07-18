//! packed part 4 tests

use super::*;
use crate::smir::interpret::*;
use crate::smir::interpret::tests::*;

    #[test]
    fn lifted_sse3_addsub_horizontal_executes_values_lane_groups_alignment_and_upper_state() {
        fn packed_f32(values: &[f32], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (lane, value) in values.iter().copied().enumerate() {
                SmirInterpreter::set_lane(&mut out, lane as u8, 32, u64::from(value.to_bits()));
            }
            out
        }
        fn packed_f64(values: &[f64], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (lane, value) in values.iter().copied().enumerate() {
                SmirInterpreter::set_lane(&mut out, lane as u8, 64, value.to_bits());
            }
            out
        }

        let a32 = [1.0, -2.0, 3.5, -4.5, 5.25, -6.75, 7.0, -8.0];
        let b32 = [0.5, 1.25, -2.5, 3.75, -4.25, 5.5, -6.5, 7.75];
        let a64 = [1.5, -2.25, 3.75, -4.5];
        let b64 = [0.25, 1.75, -2.5, 3.125];
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);

        for opcode in [0xD0u8, 0x7C, 0x7D] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = packed_f32(&a32, 0);
                x86.xmm[1] = packed_f32(&b32, 0);
            }
            execute_lifted_x86(&[0xC5, 0xFF, opcode, 0xD1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                let expected = if opcode == 0xD0 {
                    (0..8)
                        .map(|lane| {
                            if lane & 1 == 0 {
                                a32[lane] - b32[lane]
                            } else {
                                a32[lane] + b32[lane]
                            }
                        })
                        .collect::<Vec<_>>()
                } else {
                    let op = |x: f32, y: f32| if opcode == 0x7C { x + y } else { x - y };
                    vec![
                        op(a32[0], a32[1]),
                        op(a32[2], a32[3]),
                        op(b32[0], b32[1]),
                        op(b32[2], b32[3]),
                        op(a32[4], a32[5]),
                        op(a32[6], a32[7]),
                        op(b32[4], b32[5]),
                        op(b32[6], b32[7]),
                    ]
                };
                for lane in 0..8u8 {
                    assert_eq!(
                        SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                        u64::from(expected[lane as usize].to_bits()),
                        "PS opcode {opcode:02X}, lane {lane}",
                    );
                }
                assert!(x86.xmm[2][4..].iter().all(|word| *word == 0));
            }

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = packed_f64(&a64, 0);
                x86.xmm[1] = packed_f64(&b64, 0);
            }
            execute_lifted_x86(&[0xC5, 0xFD, opcode, 0xD1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                let expected = if opcode == 0xD0 {
                    vec![
                        a64[0] - b64[0],
                        a64[1] + b64[1],
                        a64[2] - b64[2],
                        a64[3] + b64[3],
                    ]
                } else {
                    let op = |x: f64, y: f64| if opcode == 0x7C { x + y } else { x - y };
                    vec![
                        op(a64[0], a64[1]),
                        op(b64[0], b64[1]),
                        op(a64[2], a64[3]),
                        op(b64[2], b64[3]),
                    ]
                };
                for lane in 0..4u8 {
                    assert_eq!(
                        SmirInterpreter::get_lane(&x86.xmm[2], lane, 64),
                        expected[lane as usize].to_bits(),
                        "PD opcode {opcode:02X}, lane {lane}",
                    );
                }
            }
        }

        // Legacy destructive semantics retain all state above bit 127.
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = packed_f32(&a32[..4], upper);
            x86.xmm[1] = packed_f32(&b32[..4], 0);
        }
        execute_lifted_x86(&[0xF2, 0x0F, 0xD0, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        ctx.write_vreg(rax, 1);
        assert!(matches!(
            execute_lifted_x86(&[0x66, 0x0F, 0x7C, 0x00], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
    }
    #[test]
    fn lifted_reciprocal_estimates_execute_merges_upper_state_alignment_and_faults() {
        fn packed_f32(values: &[f32], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (lane, value) in values.iter().copied().enumerate() {
                SmirInterpreter::set_lane(&mut out, lane as u8, 32, u64::from(value.to_bits()));
            }
            out
        }

        let flags_before = 0xCD7;
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Legacy packed RCPPS is destructive below bit 128 and preserves all
        // architectural vector state above bit 127.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = packed_f32(&[99.0; 4], upper);
            x86.xmm[1] = packed_f32(&[2.0, 4.0, 8.0, 16.0], 0);
        }
        assert!(matches!(
            execute_lifted_x86(&[0x0F, 0x53, 0xC1], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for (lane, expected) in [0.5f32, 0.25, 0.125, 0.0625].into_iter().enumerate() {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[0], lane as u8, 32),
                    u64::from(expected.to_bits())
                );
            }
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        // Legacy scalar RSQRTSS sources lane zero from r/m32, merges lanes
        // 1..3 from the old destination, and also preserves state above XMM.
        let scalar_merge = [10.0f32, 11.0, 12.0, 13.0];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = packed_f32(&scalar_merge, upper);
            x86.xmm[1] = packed_f32(&[16.0], 0);
        }
        execute_lifted_x86(&[0xF3, 0x0F, 0x52, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[0], 0, 32),
                u64::from(0.25f32.to_bits())
            );
            for lane in 1..4u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[0], lane, 32),
                    u64::from(scalar_merge[lane as usize].to_bits())
                );
            }
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        // VEX.256 computes eight lanes and clears state above bit 255.
        let packed = [1.0f32, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed_f32(&packed, 0);
            x86.xmm[2] = [u64::MAX; 16];
        }
        execute_lifted_x86(&[0xC5, 0xFC, 0x53, 0xD1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for (lane, input) in packed.into_iter().enumerate() {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[2], lane as u8, 32),
                    u64::from((1.0f32 / input).to_bits())
                );
            }
            assert!(x86.xmm[2][4..].iter().all(|word| *word == 0));
        }

        // VEX scalar merge comes from vvvv, not the old destination or r/m32;
        // all architectural state above bit 127 is cleared.
        let vex_merge = [20.0f32, 21.0, 22.0, 23.0];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed_f32(&vex_merge, 0);
            x86.xmm[2] = [u64::MAX; 16];
            x86.xmm[3] = packed_f32(&[4.0, 31.0, 32.0, 33.0], 0);
        }
        execute_lifted_x86(&[0xC5, 0xF2, 0x53, 0xD3], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[2], 0, 32),
                u64::from(0.25f32.to_bits())
            );
            for lane in 1..4u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                    u64::from(vex_merge[lane as usize].to_bits())
                );
            }
            assert!(x86.xmm[2][2..].iter().all(|word| *word == 0));
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        ctx.write_vreg(rax, 1);
        let sentinel = [0x6B6B_6B6B_6B6B_6B6B; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x0F, 0x53, 0x00], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        // The scalar m32 form has no alignment requirement.
        memory.write(1, &4.0f32.to_le_bytes()).unwrap();
        assert!(matches!(
            execute_lifted_x86(&[0xF3, 0x0F, 0x53, 0x00], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[0], 0, 32),
                u64::from(0.25f32.to_bits())
            );
        }

        // All loads precede the destination commit.
        ctx.write_vreg(rax, 0x1000);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC5, 0xFC, 0x53, 0x10], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2], sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_aesni_vaes_executes_fips_vectors_lanes_upper_state_and_faults() {
        use crate::isa::x86_64::execute::crypto::aes;

        fn block(bytes: [u8; 16]) -> [u64; 2] {
            [
                u64::from_le_bytes(bytes[..8].try_into().unwrap()),
                u64::from_le_bytes(bytes[8..].try_into().unwrap()),
            ]
        }
        fn packed_blocks(blocks: &[[u64; 2]], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (lane, words) in blocks.iter().enumerate() {
                out[lane * 2..lane * 2 + 2].copy_from_slice(words);
            }
            out
        }

        // FIPS 197 AES-128 round-one and final-round intermediate values.
        let round0 = block([
            0x00, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xA0, 0xB0, 0xC0, 0xD0,
            0xE0, 0xF0,
        ]);
        let round1_key = block([
            0xD6, 0xAA, 0x74, 0xFD, 0xD2, 0xAF, 0x72, 0xFA, 0xDA, 0xA6, 0x78, 0xF1, 0xD6, 0xAB,
            0x76, 0xFE,
        ]);
        let round1 = block([
            0x89, 0xD8, 0x10, 0xE8, 0x85, 0x5A, 0xCE, 0x68, 0x2D, 0x18, 0x43, 0xD8, 0xCB, 0x12,
            0x8F, 0xE4,
        ]);
        let round9 = block([
            0xBD, 0x6E, 0x7C, 0x3D, 0xF2, 0xB5, 0x77, 0x9E, 0x0B, 0x61, 0x21, 0x6E, 0x8B, 0x10,
            0xB6, 0x89,
        ]);
        let round10_key = block([
            0x13, 0x11, 0x1D, 0x7F, 0xE3, 0x94, 0x4A, 0x17, 0xF3, 0x07, 0xA7, 0x8B, 0x4D, 0x2B,
            0x30, 0xC5,
        ]);
        let ciphertext = block([
            0x69, 0xC4, 0xE0, 0xD8, 0x6A, 0x7B, 0x04, 0x30, 0xD8, 0xCD, 0xB7, 0x80, 0x70, 0xB4,
            0xC5, 0x5A,
        ]);
        let flags_before = 0xCD7;
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Legacy AESENC is destructive and preserves architectural state above XMM.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = packed_blocks(&[round0], upper);
            x86.xmm[1] = packed_blocks(&[round1_key], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0xDC, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[0][..2], &round1);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        // Legacy AESKEYGENASSIST is checked against the explicit SubWord,
        // RotWord, and RCON=1 result for the AES-128 key 00..0f.
        let original_key = block([
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
            0x0E, 0x0F,
        ]);
        let keygen_expected = block([
            0xF2, 0x6B, 0x6F, 0xC5, 0x6A, 0x6F, 0xC5, 0xF2, 0xFE, 0xD7, 0xAB, 0x76, 0xD6, 0xAB,
            0x76, 0xFE,
        ]);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [upper; 16];
            x86.xmm[1] = packed_blocks(&[original_key], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x3A, 0xDF, 0xC1, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[0][..2], &keygen_expected);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        // VEX.256 applies AESENCLAST independently to each 128-bit lane and
        // clears all state above YMM. Lane zero is the FIPS final round.
        let lane1_state = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
        let lane1_key = [0x0F1E_2D3C_4B5A_6978, 0x8796_A5B4_C3D2_E1F0];
        let lane1_expected =
            aes::aesenclast(lane1_state[0], lane1_state[1], lane1_key[0], lane1_key[1]);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed_blocks(&[round9, lane1_state], 0);
            x86.xmm[2] = [u64::MAX; 16];
            x86.xmm[3] = packed_blocks(&[round10_key, lane1_key], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0xDD, 0xD3], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[2][..2], &ciphertext);
            assert_eq!(x86.xmm[2][2], lane1_expected.0);
            assert_eq!(x86.xmm[2][3], lane1_expected.1);
            assert!(x86.xmm[2][4..].iter().all(|word| *word == 0));
        }

        // EVEX.512 handles four lanes and high registers without masking.
        let states = [round0, round9, lane1_state, original_key];
        let keys = [round1_key, round10_key, lane1_key, keygen_expected];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = packed_blocks(&keys, 0);
            x86.xmm[18] = packed_blocks(&states, 0);
            x86.xmm[19] = [u64::MAX; 16];
        }
        execute_lifted_x86(&[0x62, 0xA2, 0x6D, 0x40, 0xDC, 0xD9], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..4 {
                let expected = aes::aesenc(
                    states[lane][0],
                    states[lane][1],
                    keys[lane][0],
                    keys[lane][1],
                );
                assert_eq!(x86.xmm[19][lane * 2], expected.0);
                assert_eq!(x86.xmm[19][lane * 2 + 1], expected.1);
            }
            assert!(x86.xmm[19][8..].iter().all(|word| *word == 0));
        }

        // AESIMC and VEX AESKEYGENASSIST use the same primitive with different
        // legacy/VEX upper-state rules.
        let imc_expected = aes::aesimc(round10_key[0], round10_key[1]);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = [upper; 16];
            x86.xmm[3] = packed_blocks(&[round10_key], 0);
            x86.xmm[10] = packed_blocks(&[original_key], 0);
            x86.xmm[11] = [u64::MAX; 16];
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0xDB, 0xD3], &mut ctx, &mut memory);
        execute_lifted_x86(&[0xC4, 0x43, 0x79, 0xDF, 0xDA, 0x5A], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[2][..2], &[imc_expected.0, imc_expected.1]);
            assert!(x86.xmm[2][2..].iter().all(|word| *word == upper));
            let expected = aes::aeskeygenassist(original_key[0], original_key[1], 0x5A);
            assert_eq!(&x86.xmm[11][..2], &[expected.0, expected.1]);
            assert!(x86.xmm[11][2..].iter().all(|word| *word == 0));
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let sentinel = [0x6B6B_6B6B_6B6B_6B6B; 16];
        ctx.write_vreg(rax, 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x66, 0x0F, 0x38, 0xDE, 0x00], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        // VEX memory is unaligned-capable, and any load fault precedes the
        // destination commit.
        memory.write(1, &round10_key[0].to_le_bytes()).unwrap();
        memory.write(9, &round10_key[1].to_le_bytes()).unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = packed_blocks(&[round9], 0);
            x86.xmm[3] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0x69, 0xDD, 0x18], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[3][..2], &ciphertext);
        }
        ctx.write_vreg(rax, 0x1000);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0x69, 0xDD, 0x18], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[3], sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_vex_masked_memory_executes_zeroing_stores_and_element_faults() {
        fn packed_lanes(values: &[u64], bits: u32, fill: u64) -> VecValue {
            let mut value = [fill; 16];
            for (lane, scalar) in values.iter().copied().enumerate() {
                SmirInterpreter::set_lane(&mut value, lane as u8, bits, scalar);
            }
            value
        }

        let flags_before = 0xCD7;
        let rdi = VReg::Arch(ArchReg::X86(X86Reg::Rdi));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        let dwords = (0..8)
            .map(|lane| 0x1020_3040u32.wrapping_add(lane * 0x1111_1111))
            .collect::<Vec<_>>();
        let bytes = dwords
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        memory.write(0x40, &bytes).unwrap();
        ctx.write_vreg(rdi, 0x40);
        let mask32 = (0..8)
            .map(|lane| {
                if lane % 2 == 0 {
                    0x8000_0000
                } else {
                    0x7FFF_FFFF
                }
            })
            .collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed_lanes(&mask32, 32, 0);
            x86.xmm[2] = [0xA5A5_A5A5_A5A5_A5A5; 16];
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x2C, 0x17], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                    if lane % 2 == 0 {
                        u64::from(dwords[lane as usize])
                    } else {
                        0
                    }
                );
            }
            assert!(x86.xmm[2][4..].iter().all(|word| *word == 0));
        }

        let qwords = [0x1122_3344_5566_7788, 0x99AA_BBCC_DDEE_FF00];
        let mask64 = [u64::MAX, i64::MAX as u64];
        memory.write(0x80, &[0x55; 16]).unwrap();
        ctx.write_vreg(rdi, 0x80);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed_lanes(&mask64, 64, 0);
            x86.xmm[2] = packed_lanes(&qwords, 64, 0);
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0xF1, 0x8E, 0x17], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let mut stored = [0; 16];
        memory.read(0x80, &mut stored).unwrap();
        assert_eq!(&stored[..8], &qwords[0].to_le_bytes());
        assert_eq!(&stored[8..], &[0x55; 8]);

        // Only an active qword may fault. A faulting load leaves the vector
        // destination unchanged because all memory reads precede its commit.
        ctx.write_vreg(rdi, 0x1F8);
        memory.write(0x1F8, &qwords[0].to_le_bytes()).unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed_lanes(&[u64::MAX, 0], 64, 0);
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0xF1, 0x8C, 0x17], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2][0], qwords[0]);
            assert_eq!(x86.xmm[2][1], 0);
        }

        let sentinel = [0x6B6B_6B6B_6B6B_6B6B; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = packed_lanes(&[0, u64::MAX], 64, 0);
            x86.xmm[2] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0xF1, 0x8C, 0x17], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2], sentinel);
        }

        ctx.write_vreg(rdi, 0x1_0000);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0; 16];
            x86.xmm[2] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0xF1, 0x8C, 0x17], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert!(x86.xmm[2].iter().all(|word| *word == 0));
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_pmaddwd_executes_wrap_widths_masks_complete_memory_and_faults() {
        fn packed_words(values: &[u16], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (lane, value) in values.iter().copied().enumerate() {
                let shift = (lane % 4) * 16;
                out[lane / 4] =
                    (out[lane / 4] & !(0xFFFFu64 << shift)) | (u64::from(value) << shift);
            }
            out
        }

        fn dwords(value: &VecValue, count: usize) -> Vec<u32> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 4)
                .collect::<Vec<_>>()
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }
        fn reference(a: &[u16], b: &[u16]) -> Vec<u32> {
            a.chunks_exact(2)
                .zip(b.chunks_exact(2))
                .map(|(a, b)| {
                    let lo = i32::from(a[0] as i16).wrapping_mul(i32::from(b[0] as i16));
                    let hi = i32::from(a[1] as i16).wrapping_mul(i32::from(b[1] as i16));
                    lo.wrapping_add(hi) as u32
                })
                .collect()
        }

        let mut a = (0..32)
            .map(|lane| 0x8101u16.wrapping_add((lane as u16).wrapping_mul(0x1237)))
            .collect::<Vec<_>>();
        let mut b = (0..32)
            .map(|lane| 0xFEDCu16.wrapping_sub((lane as u16).wrapping_mul(0x091D)))
            .collect::<Vec<_>>();
        a[..2].copy_from_slice(&[0x8000, 0x8000]);
        b[..2].copy_from_slice(&[0x8000, 0x8000]);
        let expected = reference(&a, &b);
        assert_eq!(expected[0], 0x8000_0000);

        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let sentinel = [0x6B6B_6B6B_6B6B_6B6Bu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = packed_words(&a[..8], upper);
            x86.xmm[1] = packed_words(&b[..8], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0xF5, 0xD1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(dwords(&x86.xmm[2], 4), expected[..4]);
            assert!(x86.xmm[2][2..].iter().all(|word| *word == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = sentinel;
            x86.xmm[9] = packed_words(&a[..16], 0);
            x86.xmm[10] = packed_words(&b[..16], 0);
        }
        execute_lifted_x86(&[0xC4, 0x41, 0x35, 0xF5, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(dwords(&x86.xmm[8], 8), expected[..8]);
            assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
        }

        let mask = 0xA55Au64;
        for (p2, zeroing) in [(0x41, false), (0xC1, true)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[16] = sentinel;
                x86.xmm[17] = packed_words(&a, 0);
                x86.xmm[18] = packed_words(&b, 0);
            }
            ctx.write_vreg(k1, mask);
            execute_lifted_x86(&[0x62, 0xA1, 0x75, p2, 0xF5, 0xC2], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                let actual = dwords(&x86.xmm[16], 16);
                for lane in 0..16 {
                    assert_eq!(
                        actual[lane],
                        if mask >> lane & 1 != 0 {
                            expected[lane]
                        } else if zeroing {
                            0
                        } else {
                            0x6B6B_6B6B
                        },
                    );
                }
            }
        }

        let memory_bytes = b[..8]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        memory.write(0x80, &memory_bytes).unwrap();
        ctx.write_vreg(rax, 0x80);
        ctx.write_vreg(k1, 0b0101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = packed_words(&a[..8], 0);
        }
        execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x09, 0xF5, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let actual = dwords(&x86.xmm[0], 4);
            for lane in 0..4 {
                assert_eq!(
                    actual[lane],
                    if 0b0101 >> lane & 1 != 0 {
                        expected[lane]
                    } else {
                        0x6B6B_6B6B
                    },
                );
            }
        }

        // E4NF requires the entire m128 even with an all-zero destination mask.
        ctx.write_vreg(rax, 0xF8);
        ctx.write_vreg(k1, 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x09, 0xF5, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        ctx.write_vreg(rax, 0x81);
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0xF5, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_packed_shift_count_executes_full_counts_masks_mem128_and_faults() {
        fn packed(values: &[u64], bits: u32, fill: u64) -> VecValue {
            let mut out = [fill; 16];
            let mask = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            for (lane, value) in values.iter().copied().enumerate() {
                let bit = lane * bits as usize;
                let word = bit / 64;
                let shift = bit % 64;
                out[word] = (out[word] & !(mask << shift)) | ((value & mask) << shift);
            }
            out
        }

        fn lanes(value: &VecValue, bits: u32, count: usize) -> Vec<u64> {
            (0..count)
                .map(|lane| SmirInterpreter::get_lane(value, lane as u8, bits))
                .collect()
        }

        fn source(bits: u32, count: usize) -> Vec<u64> {
            let mask = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            (0..count)
                .map(|lane| {
                    (0xA55A_C33C_F00F_8111u64 ^ (lane as u64).wrapping_mul(0x1111_2222_3333_4445))
                        & mask
                })
                .collect()
        }

        fn shifted(value: u64, bits: u32, amount: u64, shift: ShiftOp) -> u64 {
            let mask = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            let value = value & mask;
            if amount >= u64::from(bits) {
                return if shift == ShiftOp::Asr && value & (1u64 << (bits - 1)) != 0 {
                    mask
                } else {
                    0
                };
            }
            match shift {
                ShiftOp::Lsl => (value << amount) & mask,
                ShiftOp::Lsr => value >> amount,
                ShiftOp::Asr => {
                    let signed = if bits == 64 {
                        value as i64
                    } else {
                        ((value << (64 - bits)) as i64) >> (64 - bits)
                    };
                    ((signed >> amount) as u64) & mask
                }
                _ => unreachable!(),
            }
        }

        let flags_before = 0xCD7;
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let sentinel = [0x6B6B_6B6B_6B6B_6B6Bu64; 16];
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // The prefix-free forms shift a 64-bit MMX destination by the complete
        // low 64-bit count from mm/m64, then enter MMX state without changing
        // x87 TOP or integer flags.
        for (opcode, bits, shift) in [
            (0xD1, 16, ShiftOp::Lsr),
            (0xD2, 32, ShiftOp::Lsr),
            (0xD3, 64, ShiftOp::Lsr),
            (0xE1, 16, ShiftOp::Asr),
            (0xE2, 32, ShiftOp::Asr),
            (0xF1, 16, ShiftOp::Lsl),
            (0xF2, 32, ShiftOp::Lsl),
            (0xF3, 64, ShiftOp::Lsl),
        ] {
            let input = source(bits, 64 / bits as usize);
            let packed_input = packed(&input, bits, 0)[0];
            for amount in [
                0,
                u64::from(bits - 1),
                u64::from(bits),
                u64::from(bits + 1),
                1 << 40,
            ] {
                if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                    x86.mm[0] = packed_input;
                    x86.mm[1] = amount;
                    x86.x87.tag_word = 0xFFFF;
                    x86.x87.status_word = 5 << 11;
                }
                execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
                if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                    let actual = packed(&[x86.mm[0]], 64, 0);
                    assert_eq!(
                        lanes(&actual, bits, input.len()),
                        input
                            .iter()
                            .map(|value| shifted(*value, bits, amount, shift))
                            .collect::<Vec<_>>(),
                        "MMX opcode {opcode:02X}, count {amount}",
                    );
                    assert_eq!(x86.x87.tag_word, 0);
                    assert_eq!(x86.x87.status_word & 0x3800, 5 << 11);
                }
            }
        }

        // Destructive aliases must read the complete original destination as
        // the count before committing the result.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 4;
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0xF1, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 64);
            assert_eq!(x86.x87.tag_word, 0);
        }

        // An unaligned m64 count is legal and is fully read before either the
        // destination or x87 tags are changed.
        memory.write(0x81, &3u64.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x81);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0x8001_7FFF_F00F_00F0;
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0xD1, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0x1000_0FFF_1E01_001E);
            assert_eq!(x86.x87.tag_word, 0);
        }

        ctx.write_vreg(rax, 0xFC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xA55A_C33C_F00F_8111;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0xD1, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xA55A_C33C_F00F_8111);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        for (opcode, bits, shift) in [
            (0xD1, 16, ShiftOp::Lsr),
            (0xD2, 32, ShiftOp::Lsr),
            (0xD3, 64, ShiftOp::Lsr),
            (0xE1, 16, ShiftOp::Asr),
            (0xE2, 32, ShiftOp::Asr),
            (0xF1, 16, ShiftOp::Lsl),
            (0xF2, 32, ShiftOp::Lsl),
            (0xF3, 64, ShiftOp::Lsl),
        ] {
            let input = source(bits, 128 / bits as usize);
            for amount in [
                0,
                u64::from(bits - 1),
                u64::from(bits),
                u64::from(bits + 1),
                1 << 40,
            ] {
                if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                    x86.xmm[0] = sentinel;
                    x86.xmm[1] = packed(&input, bits, 0);
                    x86.xmm[2] = [amount, u64::MAX, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
                }
                execute_lifted_x86(&[0xC5, 0xF1, opcode, 0xC2], &mut ctx, &mut memory);
                if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                    assert_eq!(
                        lanes(&x86.xmm[0], bits, input.len()),
                        input
                            .iter()
                            .map(|value| shifted(*value, bits, amount, shift))
                            .collect::<Vec<_>>(),
                        "opcode {opcode:02X}, count {amount}",
                    );
                    assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
                }
            }
        }

        // Legacy destructive semantics preserve all state above bit 127.
        let input16 = source(16, 8);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = packed(&input16, 16, upper);
            x86.xmm[1] = [16, u64::MAX, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        }
        execute_lifted_x86(&[0x66, 0x0F, 0xE1, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                lanes(&x86.xmm[0], 16, 8),
                input16
                    .iter()
                    .map(|value| shifted(*value, 16, 16, ShiftOp::Asr))
                    .collect::<Vec<_>>(),
            );
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        // A 256-bit data source still takes its single count from xmm10; the
        // upper 64 bits of that XMM count operand are ignored.
        let input32 = source(32, 8);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = sentinel;
            x86.xmm[9] = packed(&input32, 32, 0);
            x86.xmm[10] = [3, u64::MAX, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        }
        execute_lifted_x86(&[0xC4, 0x41, 0x35, 0xD2, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                lanes(&x86.xmm[8], 32, 8),
                input32
                    .iter()
                    .map(|value| shifted(*value, 32, 3, ShiftOp::Lsr))
                    .collect::<Vec<_>>(),
            );
            assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
        }

        // EVEX.W=1 selects VPSRAQ. Validate both merge and zero destination
        // masks over independently encoded high registers.
        let input64 = source(64, 8);
        let mask = 0xA5u64;
        for (p2, zeroing) in [(0x41, false), (0xC1, true)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[16] = sentinel;
                x86.xmm[17] = packed(&input64, 64, 0);
                x86.xmm[18] = [4, u64::MAX, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            }
            ctx.write_vreg(k1, mask);
            execute_lifted_x86(&[0x62, 0xA1, 0xF5, p2, 0xE2, 0xC2], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                let actual = lanes(&x86.xmm[16], 64, 8);
                for lane in 0..8 {
                    assert_eq!(
                        actual[lane],
                        if mask >> lane & 1 != 0 {
                            shifted(input64[lane], 64, 4, ShiftOp::Asr)
                        } else if zeroing {
                            0
                        } else {
                            0x6B6B_6B6B_6B6B_6B6B
                        },
                    );
                }
            }
        }

        let count_bytes = [3u64, u64::MAX]
            .into_iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        memory.write(0x81, &count_bytes).unwrap();
        ctx.write_vreg(rax, 0x81);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = packed(&input32, 32, 0);
        }
        execute_lifted_x86(&[0xC5, 0xF5, 0xD2, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                lanes(&x86.xmm[0], 32, 8),
                input32
                    .iter()
                    .map(|value| shifted(*value, 32, 3, ShiftOp::Lsr))
                    .collect::<Vec<_>>(),
            );
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0xD2, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        // E4NF Mem128 is all-or-fault even when every destination mask bit is 0.
        ctx.write_vreg(rax, 0xF8);
        ctx.write_vreg(k1, 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = packed(&input16, 16, 0);
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x09, 0xE1, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_pclmulqdq_executes_selectors_blocks_aliases_memory_and_faults() {
        fn clmul(a: u64, b: u64) -> (u64, u64) {
            let mut product = 0u128;
            for bit in 0..64 {
                if (b >> bit) & 1 != 0 {
                    product ^= u128::from(a) << bit;
                }
            }
            (product as u64, (product >> 64) as u64)
        }
        fn selected(src1: &[u64], src2: &[u64], blocks: usize, imm: u8) -> Vec<u64> {
            let mut out = Vec::with_capacity(blocks * 2);
            for block in 0..blocks {
                let a = src1[block * 2 + usize::from(imm & 1)];
                let b = src2[block * 2 + usize::from((imm >> 4) & 1)];
                let (lo, hi) = clmul(a, b);
                out.extend([lo, hi]);
            }
            out
        }

        let src1: [u64; 8] = [
            0xFEDC_BA98_7654_3210,
            0x0123_4567_89AB_CDEF,
            u64::MAX,
            0x8000_0000_0000_0001,
            0x1111_2222_3333_4444,
            0xAAAA_5555_AAAA_5555,
            0xDEAD_BEEF_CAFE_BABE,
            0x0000_0000_0000_0001,
        ];
        let src2: [u64; 8] = [
            0x1357_9BDF_2468_ACE0,
            0x0F0E_0D0C_0B0A_0908,
            0x5555_5555_5555_5555,
            0x7FFF_FFFF_FFFF_FFFF,
            0x0101_0101_0101_0101,
            0xFFFF_0000_FFFF_0000,
            0x3141_5926_5358_9793,
            0xFFFF_FFFF_FFFF_FFFF,
        ];
        let memory_src: [u64; 8] = [
            0x2222_3333_4444_5555,
            0xABCD_EF01_2345_6789,
            0x0102_0304_0506_0708,
            0x8877_6655_4433_2211,
            0x8000_0000_0000_0000,
            3,
            0xF0F0_F0F0_0F0F_0F0F,
            0x1234_0000_5678_0000,
        ];
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        let memory_bytes = memory_src
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        memory.write(0x101, &memory_bytes).unwrap();
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Legacy form intrinsically aliases destination and source 1. Exercise
        // all selector combinations plus ignored immediate bits.
        for imm in [0x00u8, 0x01, 0x10, 0x11, 0xEF] {
            let mut first = [upper; 16];
            first[..2].copy_from_slice(&src1[..2]);
            let mut second = [0u64; 16];
            second[..2].copy_from_slice(&src2[..2]);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = first;
                x86.xmm[10] = second;
            }
            execute_lifted_x86(
                &[0x66, 0x45, 0x0F, 0x3A, 0x44, 0xCA, imm],
                &mut ctx,
                &mut memory,
            );
            let expected = selected(&src1[..2], &src2[..2], 1, imm);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(&x86.xmm[9][..2], expected.as_slice());
                assert!(x86.xmm[9][2..].iter().all(|word| *word == upper));
            }
        }

        // VEX.256 destination/source aliasing remains block-local and zeros all
        // state above the active vector length.
        let mut first = [upper; 16];
        first[..4].copy_from_slice(&src1[..4]);
        let mut second = [0u64; 16];
        second[..4].copy_from_slice(&src2[..4]);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = first;
            x86.xmm[10] = second;
        }
        execute_lifted_x86(&[0xC4, 0x43, 0x35, 0x44, 0xCA, 0x11], &mut ctx, &mut memory);
        let expected = selected(&src1[..4], &src2[..4], 2, 0x11);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[9][..4], expected.as_slice());
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
        }

        // EVEX.512 high registers perform four independent products.
        let mut first = [0u64; 16];
        first[..8].copy_from_slice(&src1);
        let mut second = [0u64; 16];
        second[..8].copy_from_slice(&src2);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = second;
            x86.xmm[17] = sentinel;
            x86.xmm[18] = first;
        }
        execute_lifted_x86(
            &[0x62, 0xA3, 0x6D, 0x40, 0x44, 0xC8, 0x10],
            &mut ctx,
            &mut memory,
        );
        let expected = selected(&src1, &src2, 4, 0x10);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[17][..8], expected.as_slice());
            assert!(x86.xmm[17][8..].iter().all(|word| *word == 0));
        }

        // EVEX Full-Mem is unaligned-capable and uses one plain, non-fault-
        // suppressed vector load.
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
        }
        execute_lifted_x86(
            &[0x62, 0xE3, 0x6D, 0x40, 0x44, 0x08, 0x11],
            &mut ctx,
            &mut memory,
        );
        let expected = selected(&src1, &memory_src, 4, 0x11);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[17][..8], expected.as_slice());
        }

        // Legacy memory misalignment is #GP(0) before destination mutation.
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
        }
        let misaligned = execute_lifted_x86(
            &[0x66, 0x44, 0x0F, 0x3A, 0x44, 0x08, 0x11],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[9], sentinel);
        }

        // EVEX has no fault suppression: an unavailable byte in the full
        // memory operand faults before any destination write.
        ctx.write_vreg(rax, 0x3E1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
        }
        let fault = execute_lifted_x86(
            &[0x62, 0xE3, 0x6D, 0x40, 0x44, 0x08, 0x11],
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
    fn lifted_immediate_blends_execute_raw_masks_repetition_aliases_and_faults() {
        fn vector(bytes: &[u8], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (word, chunk) in bytes.chunks_exact(8).enumerate() {
                out[word] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            out
        }
        fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(len)
                .collect()
        }
        fn blend(
            first: &[u8],
            second: &[u8],
            elem_bytes: usize,
            imm: u8,
            repeat_128: bool,
        ) -> Vec<u8> {
            let lanes = first.len() / elem_bytes;
            let block_lanes = 16 / elem_bytes;
            let mut out = Vec::with_capacity(first.len());
            for lane in 0..lanes {
                let bit = if repeat_128 { lane % block_lanes } else { lane };
                let source = if (imm >> bit) & 1 != 0 { second } else { first };
                out.extend_from_slice(&source[lane * elem_bytes..(lane + 1) * elem_bytes]);
            }
            out
        }

        let first = (0..32).map(|i| (i * 29 + 3) as u8).collect::<Vec<_>>();
        let second = (0..32)
            .map(|i| (0xF1u16.wrapping_sub((i * 17) as u16)) as u8)
            .collect::<Vec<_>>();
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        memory.write(0x100, &second[..16]).unwrap();
        memory.write(0x181, &second).unwrap();
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for (opcode, elem_bytes, imm, repeat) in [
            (0x0C, 4usize, 0x5A, false),
            (0x0D, 8, 0x02, false),
            (0x0E, 2, 0xA5, true),
        ] {
            let mut dst = vector(&first[..16], upper);
            dst[2..].fill(upper);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = dst;
                x86.xmm[10] = vector(&second[..16], 0);
            }
            execute_lifted_x86(
                &[0x66, 0x45, 0x0F, 0x3A, opcode, 0xCA, imm],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[9], 16),
                    blend(&first[..16], &second[..16], elem_bytes, imm, repeat)
                );
                assert!(x86.xmm[9][2..].iter().all(|word| *word == upper));
            }
        }

        for (opcode, elem_bytes, imm, repeat) in [
            (0x0C, 4usize, 0xA5, false),
            (0x0D, 8, 0x05, false),
            (0x0E, 2, 0xA5, true),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = sentinel;
                x86.xmm[10] = vector(&second, 0);
                x86.xmm[11] = vector(&first, 0);
            }
            execute_lifted_x86(
                &[0xC4, 0x43, 0x25, opcode, 0xCA, imm],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[9], 32),
                    blend(&first, &second, elem_bytes, imm, repeat)
                );
                assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
            }
        }

        // VEX source1/destination aliasing must read all old lanes before the
        // architectural zeroing write.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector(&first, upper);
            x86.xmm[10] = vector(&second, 0);
        }
        execute_lifted_x86(&[0xC4, 0x43, 0x35, 0x0C, 0xCA, 0xA5], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[9], 32),
                blend(&first, &second, 4, 0xA5, false)
            );
        }

        // VEX memory is unaligned-capable.
        ctx.write_vreg(rax, 0x181);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[3] = vector(&first, 0);
        }
        execute_lifted_x86(&[0xC4, 0xE3, 0x65, 0x0C, 0x08, 0xA5], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[1], 32),
                blend(&first, &second, 4, 0xA5, false)
            );
        }

        // Legacy Type-4 alignment faults before the load and destination write.
        ctx.write_vreg(rax, 0x181);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
        }
        let misaligned = execute_lifted_x86(
            &[0x66, 0x44, 0x0F, 0x3A, 0x0D, 0x08, 0x02],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[9], sentinel);
        }

        ctx.write_vreg(rax, 0x3F0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
        }
        let fault =
            execute_lifted_x86(&[0xC4, 0xE3, 0x65, 0x0C, 0x08, 0xA5], &mut ctx, &mut memory);
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
    fn lifted_mpsadbw_executes_block_selectors_aliases_alignment_faults_and_flags() {
        fn vector(bytes: &[u8], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (word, chunk) in bytes.chunks_exact(8).enumerate() {
                out[word] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            out
        }
        fn words(value: &VecValue, count: usize) -> Vec<u16> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 2)
                .collect::<Vec<_>>()
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }
        fn reference(first: &[u8], second: &[u8], imm: u8) -> Vec<u16> {
            let blocks = first.len() / 16;
            let mut out = Vec::with_capacity(blocks * 8);
            for block in 0..blocks {
                let control = if block & 1 == 0 { imm } else { imm >> 3 };
                let first_select = ((control >> 2) & 1) * 4;
                let second_select = (control & 3) * 4;
                let base = block * 16;
                for output in 0..8usize {
                    let mut sum = 0u16;
                    for tap in 0..4usize {
                        let a = first[base + usize::from(first_select) + output + tap];
                        let b = second[base + usize::from(second_select) + tap];
                        sum += u16::from(a.abs_diff(b));
                    }
                    out.push(sum);
                }
            }
            out
        }

        let first = (0..32).map(|i| (i * 37 + 11) as u8).collect::<Vec<_>>();
        let second = (0..32)
            .map(|i| (0xF7u16.wrapping_sub((i * 19) as u16)) as u8)
            .collect::<Vec<_>>();
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        memory.write(0x101, &second).unwrap();
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Exercise every legacy selector combination. Bits 7:3 are ignored,
        // destination/source1 aliasing is intrinsic, and upper state survives.
        for selector in 0..8u8 {
            let imm = selector | 0xF8;
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = vector(&first[..16], upper);
                x86.xmm[10] = vector(&second[..16], 0);
            }
            execute_lifted_x86(
                &[0x66, 0x45, 0x0F, 0x3A, 0x42, 0xCA, imm],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    words(&x86.xmm[9], 8),
                    reference(&first[..16], &second[..16], imm)
                );
                assert!(x86.xmm[9][2..].iter().all(|word| *word == upper));
            }
        }

        // VEX.256 uses independent selector fields for each 128-bit block.
        // Source1/destination aliasing must snapshot both blocks before writeback.
        let imm = 0x38;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector(&first, upper);
            x86.xmm[10] = vector(&second, 0);
        }
        execute_lifted_x86(&[0xC4, 0x43, 0x35, 0x42, 0xCA, imm], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(words(&x86.xmm[9], 16), reference(&first, &second, imm));
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
        }

        // VEX memory is unaligned-capable and reads a complete 256-bit operand.
        ctx.write_vreg(rax, 0xF0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[11] = vector(&first, 0);
        }
        execute_lifted_x86(
            &[0xC4, 0x63, 0x25, 0x42, 0x48, 0x11, imm],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(words(&x86.xmm[9], 16), reference(&first, &second, imm));
        }

        // EVEX.128 merge masking preserves inactive low words but clears all
        // architectural destination state above the selected VL.
        let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
        let xmm_mask = 0x55u64;
        ctx.write_vreg(k2, xmm_mask);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[10] = vector(&first[..16], upper);
            x86.xmm[11] = vector(&second[..16], upper);
        }
        execute_lifted_x86(
            &[0x62, 0x53, 0x2E, 0x0A, 0x42, 0xCB, 0xE7],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let mut expected = reference(&first[..16], &second[..16], 0xE7);
            for lane in 0..8 {
                if xmm_mask & (1u64 << lane) == 0 {
                    expected[lane] = 0xCCCC;
                }
            }
            assert_eq!(words(&x86.xmm[9], 8), expected);
            assert!(x86.xmm[9][2..].iter().all(|word| *word == 0));
        }

        // AVX10.2 VL=512 repeats the low/high selector fields across even/odd
        // 128-bit lanes and masks all 32 destination words. This merge case
        // also aliases dst/src1, requiring both source and merge snapshots.
        let first512 = (0..64).map(|i| (i * 29 + 7) as u8).collect::<Vec<_>>();
        let second512 = (0..64)
            .map(|i| (0xFBu16.wrapping_sub((i * 23) as u16)) as u8)
            .collect::<Vec<_>>();
        let mask_bits = 0xA55A_C33Cu64;
        let k3 = VReg::Arch(ArchReg::X86(X86Reg::K(3)));
        ctx.write_vreg(k3, mask_bits);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = vector(&first512, 0);
            x86.xmm[18] = vector(&second512, 0);
        }
        execute_lifted_x86(
            &[0x62, 0xA3, 0x7E, 0x43, 0x42, 0xC2, 0x3F],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let mut expected = reference(&first512, &second512, 0x3F);
            let old_words = words(&vector(&first512, 0), 32);
            for lane in 0..32 {
                if mask_bits & (1u64 << lane) == 0 {
                    expected[lane] = old_words[lane];
                }
            }
            assert_eq!(words(&x86.xmm[16], 32), expected);
        }

        // Zero masking clears inactive words. The result maximum remains
        // bounded by 4 * |0 - 255| = 1020, representable in u16.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = sentinel;
            x86.xmm[17] = vector(&first512, 0);
            x86.xmm[18] = vector(&second512, 0);
        }
        execute_lifted_x86(
            &[0x62, 0xA3, 0x76, 0xC3, 0x42, 0xC2, 0x3F],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let mut expected = reference(&first512, &second512, 0x3F);
            for lane in 0..32 {
                if mask_bits & (1u64 << lane) == 0 {
                    expected[lane] = 0;
                }
            }
            assert_eq!(words(&x86.xmm[16], 32), expected);
            assert!(expected.iter().all(|&word| word <= 1020));
        }

        // E4NF does not provide memory fault suppression. Even an all-zero
        // write mask performs the complete 64-byte FULLMEM load before dst.
        ctx.write_vreg(k2, 0);
        ctx.write_vreg(rax, 0x3F0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[3] = vector(&first512, 0);
        }
        let fault = execute_lifted_x86(
            &[0x62, 0xF3, 0x66, 0x4A, 0x42, 0x08, 0x3F],
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

        // The same address violates the legacy 16-byte alignment requirement
        // before either the load or destination mutation.
        ctx.write_vreg(rax, 0xF1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
        }
        let misaligned = execute_lifted_x86(
            &[0x66, 0x44, 0x0F, 0x3A, 0x42, 0x08, 0x07],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[9], sentinel);
        }

        // An incomplete VEX memory operand faults before upper-state clearing.
        ctx.write_vreg(rax, 0x3F0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[11] = vector(&first, 0);
        }
        let fault = execute_lifted_x86(&[0xC4, 0x63, 0x25, 0x42, 0x08, imm], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[9], sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_vdbpsadbw_executes_dword_shuffle_sad_masks_aliases_and_faults() {
        fn vector(bytes: &[u8], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (word, chunk) in bytes.chunks_exact(8).enumerate() {
                out[word] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            out
        }
        fn words(value: &VecValue, count: usize) -> Vec<u16> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 2)
                .collect::<Vec<_>>()
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }
        fn reference(first: &[u8], second: &[u8], imm: u8) -> Vec<u16> {
            let mut shuffled = vec![0u8; second.len()];
            for block in (0..second.len()).step_by(16) {
                for dword in 0..4usize {
                    let selector = usize::from((imm >> (2 * dword)) & 3);
                    shuffled[block + dword * 4..block + dword * 4 + 4]
                        .copy_from_slice(&second[block + selector * 4..block + selector * 4 + 4]);
                }
            }

            let mut result = Vec::with_capacity(first.len() / 2);
            for block in (0..first.len()).step_by(8) {
                for (first_offset, shuffled_offset) in [(0, 0), (0, 1), (4, 2), (4, 3)] {
                    let mut sum = 0u16;
                    for byte in 0..4usize {
                        sum += u16::from(
                            first[block + first_offset + byte]
                                .abs_diff(shuffled[block + shuffled_offset + byte]),
                        );
                    }
                    result.push(sum);
                }
            }
            result
        }

        let first = (0..64)
            .map(|lane| (lane as u8).wrapping_mul(0x5D).wrapping_add(0x21))
            .collect::<Vec<_>>();
        let second = (0..64)
            .map(|lane| (lane as u8).wrapping_mul(0x97).wrapping_add(0x53))
            .collect::<Vec<_>>();
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Exhaust all four independent two-bit dword selectors in imm8.
        for imm in 0..=u8::MAX {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1] = sentinel;
                x86.xmm[2] = vector(&first[..16], 0);
                x86.xmm[3] = vector(&second[..16], 0);
            }
            execute_lifted_x86(
                &[0x62, 0xF3, 0x6D, 0x08, 0x42, 0xCB, imm],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    words(&x86.xmm[1], 8),
                    reference(&first[..16], &second[..16], imm),
                    "VDBPSADBW imm8={imm:#04x}",
                );
                assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
            }
        }

        // imm8=E4 selects all four source dwords and a zeroing word mask
        // applies after all 32 ZMM results are computed from high registers.
        let mask = 0xA55A_C33Cu64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.xmm[18] = vector(&first, 0);
            x86.xmm[19] = vector(&second, 0);
            x86.k[3] = mask;
        }
        execute_lifted_x86(
            &[0x62, 0xA3, 0x6D, 0xC3, 0x42, 0xCB, 0xE4],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let actual = words(&x86.xmm[17], 32);
            let expected = reference(&first, &second, 0xE4);
            for lane in 0..32 {
                assert_eq!(
                    actual[lane],
                    if mask & (1u64 << lane) != 0 {
                        expected[lane]
                    } else {
                        0
                    },
                    "VDBPSADBW lane {lane}",
                );
            }
            assert!(expected.iter().all(|word| *word <= 4 * 255));
        }

        // A YMM destination may alias the shuffled second source; all input
        // bytes are consumed before writeback and state above VL is cleared.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = vector(&second[..32], 0xA5A5_A5A5_A5A5_A5A5);
            x86.xmm[9] = vector(&first[..32], 0);
        }
        execute_lifted_x86(
            &[0x62, 0x53, 0x35, 0x28, 0x42, 0xC0, 0x1B],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                words(&x86.xmm[8], 16),
                reference(&first[..32], &second[..32], 0x1B)
            );
            assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
        }

        // E4NF performs the complete FULLMEM read even when every write-mask
        // bit is clear, and the fault precedes all destination state changes.
        ctx.write_vreg(rax, 0x3F0);
        ctx.write_vreg(k2, 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[2] = vector(&first, 0);
        }
        let fault = execute_lifted_x86(
            &[0x62, 0xF3, 0x6D, 0x4A, 0x42, 0x08, 0xE4],
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
    fn lifted_psadbw_executes_all_widths_aliases_upper_state_and_faults() {
        fn vector(bytes: &[u8], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (word, chunk) in bytes.chunks_exact(8).enumerate() {
                out[word] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            out
        }
        fn reference(first: &[u8], second: &[u8]) -> Vec<u64> {
            first
                .chunks_exact(8)
                .zip(second.chunks_exact(8))
                .map(|(a, b)| {
                    a.iter()
                        .zip(b)
                        .map(|(&x, &y)| u64::from(x.abs_diff(y)))
                        .sum()
                })
                .collect()
        }

        let mut first = (0..64).map(|i| (i * 37 + 11) as u8).collect::<Vec<_>>();
        let mut second = (0..64)
            .map(|i| (0xF7u16.wrapping_sub((i * 19) as u16)) as u8)
            .collect::<Vec<_>>();
        // Include the architectural maximum 8 * |0 - 255| = 2040 in block 0.
        first[..8].fill(0);
        second[..8].fill(255);
        let expected = reference(&first, &second);
        assert_eq!(expected[0], 2040);

        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        memory.write(0x101, &second).unwrap();
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = vector(&first[..8], 0)[0];
            x86.mm[1] = vector(&second[..8], 0)[0];
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 6 << 11;
        }
        execute_lifted_x86(&[0x0F, 0xF6, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], expected[0]);
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 6 << 11);
        }

        // The destructive same-register form snapshots both inputs before
        // writing and therefore produces an exact zero sum.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = vector(&first[..8], 0)[0];
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0xF6, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0);
            assert_eq!(x86.x87.tag_word, 0);
        }

        // The m64 source is unaligned-capable and completes before MMX state
        // entry; a crossing fault preserves both destination and x87 tags.
        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = vector(&first[..8], 0)[0];
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0xF6, 0x40, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], expected[0]);
            assert_eq!(x86.x87.tag_word, 0);
        }

        ctx.write_vreg(rax, 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xA55A_C33C_F00F_8111;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0xF6, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xA55A_C33C_F00F_8111);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        // Legacy PSADBW intrinsically aliases destination/source1 and preserves
        // the architectural state above bit 127.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector(&first[..16], upper);
            x86.xmm[10] = vector(&second[..16], 0);
        }
        execute_lifted_x86(&[0x66, 0x45, 0x0F, 0xF6, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[9][..2], &expected[..2]);
            assert!(x86.xmm[9][2..].iter().all(|word| *word == upper));
        }

        // VEX.128 destination/source1 and destination/source2 aliases both
        // require complete input snapshots; VEX clears all state above bit 127.
        for (insn, src1, src2) in [
            (
                &[0xC4, 0x41, 0x31, 0xF6, 0xCA][..],
                vector(&first[..16], upper),
                vector(&second[..16], 0),
            ),
            (
                &[0xC4, 0x41, 0x21, 0xF6, 0xC9][..],
                vector(&first[..16], 0),
                vector(&second[..16], upper),
            ),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = if insn[4] == 0xCA { src1 } else { src2 };
                x86.xmm[10] = vector(&second[..16], 0);
                x86.xmm[11] = vector(&first[..16], 0);
            }
            execute_lifted_x86(insn, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(&x86.xmm[9][..2], &expected[..2]);
                assert!(x86.xmm[9][2..].iter().all(|word| *word == 0));
            }
        }

        // VEX.256 and EVEX.512 repeat independently for every 64-bit block and
        // clear state above their active vector length.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[10] = vector(&second[..32], 0);
            x86.xmm[11] = vector(&first[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0x41, 0x25, 0xF6, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[9][..4], &expected[..4]);
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.xmm[18] = vector(&second, 0);
            x86.xmm[19] = vector(&first, 0);
        }
        execute_lifted_x86(&[0x62, 0xA1, 0x65, 0x40, 0xF6, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[17][..8], &expected[..8]);
            assert!(x86.xmm[17][8..].iter().all(|word| *word == 0));
        }

        // VEX and EVEX memory operands are unaligned-capable.
        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[11] = vector(&first[..16], 0);
            x86.xmm[19] = sentinel;
            x86.xmm[20] = vector(&first, 0);
        }
        execute_lifted_x86(&[0xC5, 0x21, 0xF6, 0x48, 0x01], &mut ctx, &mut memory);
        ctx.write_vreg(rax, 0x101);
        execute_lifted_x86(&[0x62, 0xE1, 0x5D, 0x40, 0xF6, 0x18], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[9][..2], &expected[..2]);
            assert_eq!(&x86.xmm[19][..8], &expected[..8]);
        }

        // Legacy alignment and vector load faults precede all destination
        // mutation, including VEX/EVEX upper-state clearing.
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
        }
        let misaligned = execute_lifted_x86(&[0x66, 0x44, 0x0F, 0xF6, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[9], sentinel);
        }

        ctx.write_vreg(rax, 0x3F0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[11] = vector(&first[..32], 0);
        }
        let fault = execute_lifted_x86(&[0xC4, 0x61, 0x25, 0xF6, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[9], sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_sha512_executes_schedule_rounds_aliases_and_preserves_flags() {
        fn vector(values: &[u64], fill: u64) -> VecValue {
            let mut result = [fill; 16];
            result[..values.len()].copy_from_slice(values);
            result
        }
        fn msg1_reference(old: [u64; 4], source0: u64) -> [u64; 4] {
            let sigma = |x: u64| x.rotate_right(1) ^ x.rotate_right(8) ^ (x >> 7);
            [
                old[0].wrapping_add(sigma(old[1])),
                old[1].wrapping_add(sigma(old[2])),
                old[2].wrapping_add(sigma(old[3])),
                old[3].wrapping_add(sigma(source0)),
            ]
        }
        fn msg2_reference(old: [u64; 4], source: [u64; 4]) -> [u64; 4] {
            let sigma = |x: u64| x.rotate_right(19) ^ x.rotate_right(61) ^ (x >> 6);
            let w16 = old[0].wrapping_add(sigma(source[2]));
            let w17 = old[1].wrapping_add(sigma(source[3]));
            let w18 = old[2].wrapping_add(sigma(w16));
            let w19 = old[3].wrapping_add(sigma(w17));
            [w16, w17, w18, w19]
        }
        fn rounds_reference(cdgh: [u64; 4], abef: [u64; 4], wk: [u64; 2]) -> [u64; 4] {
            let (mut a, mut b, mut c, mut d) = (abef[3], abef[2], cdgh[3], cdgh[2]);
            let (mut e, mut f, mut g, mut h) = (abef[1], abef[0], cdgh[1], cdgh[0]);
            for round_constant in wk {
                let choose = (e & f) ^ (g & !e);
                let majority = (a & b) ^ (a & c) ^ (b & c);
                let big1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
                let big0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
                let t1 = h
                    .wrapping_add(big1)
                    .wrapping_add(choose)
                    .wrapping_add(round_constant);
                let next_a = t1.wrapping_add(big0).wrapping_add(majority);
                let next_e = d.wrapping_add(t1);
                (h, g, f, e, d, c, b, a) = (g, f, e, next_e, c, b, a, next_a);
            }
            [f, e, b, a]
        }

        let old = [
            0x0123_4567_89AB_CDEF,
            0xFEDC_BA98_7654_3210,
            0x0F1E_2D3C_4B5A_6978,
            0x8877_6655_4433_2211,
        ];
        let source = [
            0x1122_3344_5566_7788,
            0x99AA_BBCC_DDEE_FF00,
            0x1357_9BDF_2468_ACE0,
            0xF0E1_D2C3_B4A5_9687,
        ];
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector(&old, 0xA5A5_A5A5_A5A5_A5A5);
            x86.xmm[10] = vector(&source, 0);
        }
        execute_lifted_x86(&[0xC4, 0x42, 0x7F, 0xCC, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                &x86.xmm[9][..4],
                &[
                    0x6F90_7DEB_1D5C_B34D,
                    0x7E7A_EF81_D7C5_0C17,
                    0xE3C1_57BC_A930_2DE6,
                    0x0919_E64D_2A7F_B36D,
                ]
            );
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector(&old, u64::MAX);
            x86.xmm[10] = vector(&source, 0);
        }
        execute_lifted_x86(&[0xC4, 0x42, 0x7F, 0xCD, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                &x86.xmm[9][..4],
                &[
                    0x9090_C864_365A_EF2D,
                    0x34FA_A9E3_07FA_8701,
                    0xEA3F_DF4E_865C_FD93,
                    0x805D_E976_2B2A_D4FB,
                ]
            );
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
        }

        // FIPS 180-4 SHA-512 initial state and the first two W+K values for
        // the padded "abc" message. The constants below are independently
        // calculated compression-round outputs [F2,E2,B2,A2].
        let cdgh = [
            0x5BE0_CD19_137E_2179,
            0x1F83_D9AB_FB41_BD6B,
            0xA54F_F53A_5F1D_36F1,
            0x3C6E_F372_FE94_F82B,
        ];
        let abef = [
            0x9B05_688C_2B3E_6C1F,
            0x510E_527F_ADE6_82D1,
            0xBB67_AE85_84CA_A73B,
            0x6A09_E667_F3BC_C908,
        ];
        let wk = [0xA3EC_9318_D728_AE22, 0x7137_4491_23EF_65CD];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector(&cdgh, 0xDEAD_BEEF_DEAD_BEEF);
            x86.xmm[10] = vector(&wk, 0);
            x86.xmm[11] = vector(&abef, 0);
        }
        execute_lifted_x86(&[0xC4, 0x42, 0x27, 0xCB, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                &x86.xmm[9][..4],
                &[
                    0x58CB_0234_7AB5_1F91,
                    0xC3D4_EBFD_4865_0FFA,
                    0xF6AF_CEB8_BCFC_DDF5,
                    0x1320_F8C9_FB87_2CC0,
                ]
            );
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
        }

        // Destructive sources and explicit sources may all alias destination.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector(&old, u64::MAX);
        }
        execute_lifted_x86(&[0xC4, 0x42, 0x7F, 0xCC, 0xC9], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[9][..4], &msg1_reference(old, old[0]));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector(&old, u64::MAX);
        }
        execute_lifted_x86(&[0xC4, 0x42, 0x7F, 0xCD, 0xC9], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[9][..4], &msg2_reference(old, old));
        }

        let alias_input = [sentinel[0]; 4];
        let alias_expected = rounds_reference(alias_input, alias_input, [sentinel[0]; 2]);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
        }
        execute_lifted_x86(&[0xC4, 0x42, 0x37, 0xCB, 0xC9], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[9][..4], &alias_expected);
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_sm3_executes_schedule_rounds_aliases_memory_and_faults() {
        fn vector(values: &[u32], fill: u64) -> VecValue {
            let mut bytes = Vec::with_capacity(values.len() * 4);
            for value in values {
                bytes.extend(value.to_le_bytes());
            }
            let mut result = [fill; 16];
            for (word, chunk) in bytes.chunks_exact(8).enumerate() {
                result[word] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            result
        }
        fn lanes(value: &VecValue) -> Vec<u32> {
            value[..2]
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect::<Vec<_>>()
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }
        fn msg1(old: [u32; 4], first: [u32; 4], second: [u32; 4]) -> [u32; 4] {
            let p1 = |x: u32| x ^ x.rotate_left(15) ^ x.rotate_left(23);
            std::array::from_fn(|index| {
                p1(old[index]
                    ^ second[index]
                    ^ if index < 3 {
                        first[index].rotate_left(15)
                    } else {
                        0
                    })
            })
        }
        fn msg2(old: [u32; 4], first: [u32; 4], second: [u32; 4]) -> [u32; 4] {
            let mut result = std::array::from_fn(|index| {
                first[index].rotate_left(7) ^ second[index] ^ old[index]
            });
            result[3] ^=
                result[0].rotate_left(6) ^ result[0].rotate_left(15) ^ result[0].rotate_left(30);
            result
        }

        let old = [0x0123_4567, 0x89AB_CDEF, 0xFEDC_BA98, 0x7654_3210];
        let first = [0x0F1E_2D3C, 0x4B5A_6978, 0x8877_6655, 0x4433_2211];
        let second = [0x1122_3344, 0x5566_7788, 0x99AA_BBCC, 0xDDEE_FF00];
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for (insn, expected) in [
            (
                &[0xC4, 0x42, 0x20, 0xDA, 0xCA][..],
                [0x684A_3D5B, 0xC2E0_D33D, 0x0101_0123, 0x4567_45AB],
            ),
            (
                &[0xC4, 0x42, 0x21, 0xDA, 0xCA][..],
                [0x9F17_E824, 0x71F9_0642, 0x5CC5_2B90, 0xA406_7917],
            ),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = vector(&old, u64::MAX);
                x86.xmm[10] = vector(&second, 0);
                x86.xmm[11] = vector(&first, 0);
            }
            execute_lifted_x86(insn, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(lanes(&x86.xmm[9]), expected);
                assert!(x86.xmm[9][2..].iter().all(|word| *word == 0));
            }
        }

        // SM3 initial state, W0/W1/W4/W5 for padded "abc", and independently
        // calculated state [F2,E2,B2,A2] after rounds 0 and 1.
        let cdgh = [0xB0FB_0E4E, 0xE38D_EE4D, 0xDA8A_0600, 0x1724_42D7];
        let abef = [0x1631_38AA, 0xA96F_30BC, 0x4914_B2B9, 0x7380_166F];
        let words = [0x6162_6380, 0, 0, 0];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector(&cdgh, u64::MAX);
            x86.xmm[10] = vector(&words, 0);
            x86.xmm[11] = vector(&abef, 0);
        }
        execute_lifted_x86(&[0xC4, 0x43, 0x21, 0xDE, 0xCA, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                lanes(&x86.xmm[9]),
                [0x81F4_A5F8, 0x054E_9506, 0x37CF_E1D7, 0xE0FC_D39D]
            );
            assert!(x86.xmm[9][2..].iter().all(|word| *word == 0));
        }

        // Full aliasing snapshots all three logical inputs before writeback.
        for (insn, expected) in [
            (&[0xC4, 0x42, 0x30, 0xDA, 0xC9][..], msg1(old, old, old)),
            (&[0xC4, 0x42, 0x31, 0xDA, 0xC9][..], msg2(old, old, old)),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = vector(&old, u64::MAX);
            }
            execute_lifted_x86(insn, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(lanes(&x86.xmm[9]), expected);
            }
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector(&old, u64::MAX);
        }
        execute_lifted_x86(&[0xC4, 0x43, 0x31, 0xDE, 0xC9, 0x3F], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                lanes(&x86.xmm[9]),
                [0xF4B4_23DA, 0xF7AE_3424, 0xAC48_C0E4, 0xF3C9_4CFA]
            );
        }

        // Unaligned memory succeeds; a short read faults before destination or
        // flags can change. imm=1 is equivalent to imm=0 after masking.
        let bytes = second
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        memory.write(0x101, &bytes).unwrap();
        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector(&cdgh, u64::MAX);
            x86.xmm[11] = vector(&abef, 0);
        }
        execute_lifted_x86(
            &[0xC4, 0x63, 0x21, 0xDE, 0x48, 0x01, 0x01],
            &mut ctx,
            &mut memory,
        );

        ctx.write_vreg(rax, 0x1F8);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
        }
        let fault =
            execute_lifted_x86(&[0xC4, 0x63, 0x21, 0xDE, 0x08, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[9], sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_sm4_executes_standard_vectors_widths_aliases_memory_and_faults() {
        fn vector(values: &[u32], fill: u64) -> VecValue {
            let mut bytes = Vec::with_capacity(values.len() * 4);
            for value in values {
                bytes.extend(value.to_le_bytes());
            }
            let mut result = [fill; 16];
            for (word, chunk) in bytes.chunks_exact(8).enumerate() {
                result[word] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            result
        }
        fn lanes(value: &VecValue, count: usize) -> Vec<u32> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 4)
                .collect::<Vec<_>>()
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }

        let expanded_key = [0xA292_FFA1, 0xDF01_FEBF, 0x99A1_2B0F, 0xC424_10CC];
        let constants = [0x0007_0E15, 0x1C23_2A31, 0x383F_464D, 0x545B_6269];
        let round_keys = [0xF121_86F9, 0x4166_2B61, 0x5A6A_B19A, 0x7BA9_2077];
        let plaintext = [0x0123_4567, 0x89AB_CDEF, 0xFEDC_BA98, 0x7654_3210];
        let after_four = [0x27FA_D345, 0xA18B_4CB2, 0x11C1_E22A, 0xCC13_E2EE];
        let alternate = [0xDEAD_BEEF, 0x0123_9876, 0xA5A5_5A5A, 0x0F1E_2D3C];
        let alternate_keys = [0x1020_3040, 0x5060_7080, 0x90A0_B0C0, 0xD0E0_F000];
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for (insn, first, second, expected) in [
            (
                &[0xC4, 0x42, 0x22, 0xDA, 0xCA][..],
                expanded_key,
                constants,
                round_keys,
            ),
            (
                &[0xC4, 0x42, 0x23, 0xDA, 0xCA][..],
                plaintext,
                round_keys,
                after_four,
            ),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = sentinel;
                x86.xmm[10] = vector(&second, 0);
                x86.xmm[11] = vector(&first, 0);
            }
            execute_lifted_x86(insn, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(lanes(&x86.xmm[9], 4), expected);
                assert!(x86.xmm[9][2..].iter().all(|word| *word == 0));
            }
        }

        // Compute an alternate lane independently with VEX.128, then verify the
        // same lane is unchanged when paired with the standard vector in VEX.256.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[10] = vector(&alternate_keys, 0);
            x86.xmm[11] = vector(&alternate, 0);
        }
        execute_lifted_x86(&[0xC4, 0x42, 0x23, 0xDA, 0xCA], &mut ctx, &mut memory);
        let alternate_expected = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => lanes(&x86.xmm[9], 4),
            _ => unreachable!(),
        };
        let combined_first = plaintext.into_iter().chain(alternate).collect::<Vec<_>>();
        let combined_keys = round_keys
            .into_iter()
            .chain(alternate_keys)
            .collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[10] = vector(&combined_keys, 0);
            x86.xmm[11] = vector(&combined_first, 0);
        }
        execute_lifted_x86(&[0xC4, 0x42, 0x27, 0xDA, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&lanes(&x86.xmm[9], 8)[..4], &after_four);
            assert_eq!(&lanes(&x86.xmm[9], 8)[4..], alternate_expected);
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
        }

        // Every logical input may alias the destination.
        for insn in [
            &[0xC4, 0x42, 0x32, 0xDA, 0xC9][..],
            &[0xC4, 0x42, 0x37, 0xDA, 0xC9][..],
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = vector(&combined_first, u64::MAX);
            }
            execute_lifted_x86(insn, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_ne!(x86.xmm[9], vector(&combined_first, u64::MAX));
                assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
            }
        }

        // VEX.256 memory is unaligned-capable; a short read is atomic.
        let source_bytes = combined_keys
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        memory.write(0x101, &source_bytes).unwrap();
        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[11] = vector(&combined_first, 0);
        }
        execute_lifted_x86(&[0xC4, 0x62, 0x27, 0xDA, 0x48, 0x01], &mut ctx, &mut memory);

        ctx.write_vreg(rax, 0x1F0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
        }
        let fault = execute_lifted_x86(&[0xC4, 0x62, 0x27, 0xDA, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[9], sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_avx_ne_convert_executes_exact_bits_widths_mxcsr_and_faults() {
        fn words(values: &[u16]) -> Vec<u8> {
            values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect()
        }
        fn f32_bits(value: &VecValue, count: usize) -> Vec<u32> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 4)
                .collect::<Vec<_>>()
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }

        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let mxcsr_before = 0xBFC0;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        ctx.write_vreg(rax, 0x100);

        // BF16 conversion is the exact bit operation input << 16, including
        // signaling-NaN payloads; the YMM form zeroes all state above 256 bits.
        memory.write(0x101, &0x3F80u16.to_le_bytes()).unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.mxcsr = mxcsr_before;
        }
        execute_lifted_x86(&[0xC4, 0x62, 0x7E, 0xB1, 0x48, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(f32_bits(&x86.xmm[9], 8), vec![0x3F80_0000; 8]);
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
            assert_eq!(x86.mxcsr, mxcsr_before);
        }

        // FP16 signaling NaNs are quieted with sign/payload preservation and
        // no SIMD exception or MXCSR status update under the instruction's SAE.
        memory.write(0x101, &0xFC01u16.to_le_bytes()).unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
        }
        execute_lifted_x86(&[0xC4, 0x62, 0x79, 0xB1, 0x48, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(f32_bits(&x86.xmm[9], 4), vec![0xFFC0_2000; 4]);
            assert!(x86.xmm[9][2..].iter().all(|word| *word == 0));
            assert_eq!(x86.mxcsr, mxcsr_before);
        }

        let bf16 = [
            0x0000, 0x8000, 0x3F80, 0xBF80, 0x7F80, 0xFF80, 0x7F81, 0xFFFF,
        ];
        memory.write(0x101, &words(&bf16)).unwrap();
        for (insn, expected) in [
            (
                &[0xC4, 0x62, 0x7A, 0xB0, 0x48, 0x01][..],
                [0x0000_0000, 0x3F80_0000, 0x7F80_0000, 0x7F81_0000],
            ),
            (
                &[0xC4, 0x62, 0x7B, 0xB0, 0x48, 0x01][..],
                [0x8000_0000, 0xBF80_0000, 0xFF80_0000, 0xFFFF_0000],
            ),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = sentinel;
            }
            execute_lifted_x86(insn, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(f32_bits(&x86.xmm[9], 4), expected);
                assert!(x86.xmm[9][2..].iter().all(|word| *word == 0));
            }
        }

        // Alternating words cover signed zero, minimum/maximum subnormal,
        // minimum/maximum normal, finite fractions, infinities, and both NaN
        // classes. Even and odd instructions select one word from each dword.
        let fp16 = [
            0x0000, 0x8000, 0x0001, 0x03FF, 0x0400, 0x3C00, 0x7C00, 0xFC00, 0x7C01, 0x7E55, 0xFC01,
            0xFE55, 0x3555, 0xB555, 0x7BFF, 0xFBFF,
        ];
        memory.write(0x101, &words(&fp16)).unwrap();
        for (insn, expected) in [
            (
                &[0xC4, 0x62, 0x7D, 0xB0, 0x48, 0x01][..],
                [
                    0x0000_0000,
                    0x3380_0000,
                    0x3880_0000,
                    0x7F80_0000,
                    0x7FC0_2000,
                    0xFFC0_2000,
                    0x3EAA_A000,
                    0x477F_E000,
                ],
            ),
            (
                &[0xC4, 0x62, 0x7C, 0xB0, 0x48, 0x01][..],
                [
                    0x8000_0000,
                    0x387F_C000,
                    0x3F80_0000,
                    0xFF80_0000,
                    0x7FCA_A000,
                    0xFFCA_A000,
                    0xBEAA_A000,
                    0xC77F_E000,
                ],
            ),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = sentinel;
            }
            execute_lifted_x86(insn, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(f32_bits(&x86.xmm[9], 8), expected);
                assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
                assert_eq!(x86.mxcsr, mxcsr_before);
            }
        }

        // Both scalar and full-vector memory faults precede destination writes.
        for (address, insn) in [
            (0x1FF, &[0xC4, 0x62, 0x7A, 0xB1, 0x08][..]),
            (0x1F0, &[0xC4, 0x62, 0x7E, 0xB0, 0x08][..]),
        ] {
            ctx.write_vreg(rax, address);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = sentinel;
            }
            let result = execute_lifted_x86(insn, &mut ctx, &mut memory);
            assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
            ));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[9], sentinel);
                assert_eq!(x86.mxcsr, mxcsr_before);
            }
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_vex_packed_immediate_shifts_execute_overshifts_and_128_bit_lanes() {
        fn bytes(value: &VecValue, count: usize) -> Vec<u8> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count)
                .collect()
        }

        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(1);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        let word_source = (0..16)
            .flat_map(|index| if index % 2 == 0 { 0x8001u16 } else { 0x7FFF }.to_le_bytes())
            .collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[10] = vec_from_bytes(&word_source);
        }
        execute_lifted_x86(&[0xC4, 0xC1, 0x35, 0x71, 0xE2, 17], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let expected = (0..16)
                .flat_map(|index| if index % 2 == 0 { u16::MAX } else { 0 }.to_le_bytes())
                .collect::<Vec<_>>();
            assert_eq!(bytes(&x86.xmm[9], 32), expected);
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
        }

        // Logical element overshifts produce zero rather than modulo-count
        // results; this covers 16-, 32-, and 64-bit element sizes.
        for insn in [
            &[0xC4, 0xC1, 0x31, 0x71, 0xF2, 17][..],
            &[0xC4, 0xC1, 0x31, 0x72, 0xD2, 33][..],
            &[0xC4, 0xC1, 0x31, 0x73, 0xF2, 65][..],
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = sentinel;
                x86.xmm[10] = vec_from_bytes(&[0xFF; 16]);
            }
            execute_lifted_x86(insn, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert!(x86.xmm[9].iter().all(|word| *word == 0));
            }
        }

        let source = (0u8..32).collect::<Vec<_>>();
        for (insn, expected) in [
            (
                &[0xC4, 0xC1, 0x35, 0x73, 0xDA, 1][..],
                (1u8..16)
                    .chain([0])
                    .chain(17u8..32)
                    .chain([0])
                    .collect::<Vec<_>>(),
            ),
            (
                &[0xC4, 0xC1, 0x35, 0x73, 0xFA, 1][..],
                [0].into_iter()
                    .chain(0u8..15)
                    .chain([0])
                    .chain(16u8..31)
                    .collect::<Vec<_>>(),
            ),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = sentinel;
                x86.xmm[10] = vec_from_bytes(&source);
            }
            execute_lifted_x86(insn, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[9], 32), expected);
                assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
            }
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_packed_immediate_shuffles_execute_lanes_aliases_memory_and_faults() {
        fn vector_u16(values: &[u16], fill: u64) -> VecValue {
            let bytes = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            let mut result = [fill; 16];
            for (index, chunk) in bytes.chunks(8).enumerate() {
                let mut word = [0u8; 8];
                word[..chunk.len()].copy_from_slice(chunk);
                result[index] = u64::from_le_bytes(word);
            }
            result
        }
        fn words(value: &VecValue, count: usize) -> Vec<u16> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 2)
                .collect::<Vec<_>>()
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }
        fn dwords(value: &VecValue, count: usize) -> Vec<u32> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 4)
                .collect::<Vec<_>>()
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }
        fn mmx_words(value: u64) -> [u16; 4] {
            std::array::from_fn(|lane| ((value >> (lane * 16)) & 0xFFFF) as u16)
        }

        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Prefix-free PSHUFW uses four 16-bit lanes, snapshots an aliased
        // register source, and enters MMX state without changing TOP.
        let mmx_source = vector_u16(&[0, 1, 2, 3], 0)[0];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[1] = mmx_source;
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 5 << 11;
        }
        execute_lifted_x86(&[0x0F, 0x70, 0xC9, 0x1B], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(mmx_words(x86.mm[1]), [3, 2, 1, 0]);
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 5 << 11);
        }

        // The m64 source may be unaligned. A fault leaves both the destination
        // and MMX/x87 state untouched because the load precedes writeback.
        let mmx_memory_source = vector_u16(&[10, 11, 12, 13], 0)[0];
        memory
            .write(0x81, &mmx_memory_source.to_le_bytes())
            .unwrap();
        ctx.write_vreg(rax, 0x80);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::MAX;
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x70, 0x40, 0x01, 0x1B], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(mmx_words(x86.mm[0]), [13, 12, 11, 10]);
            assert_eq!(x86.x87.tag_word, 0);
        }

        ctx.write_vreg(rax, 0x1FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0x70, 0x00, 0x1B], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        for (insn, expected) in [
            (
                &[0xF3, 0x45, 0x0F, 0x70, 0xCA, 0x1B][..],
                [0, 1, 2, 3, 7, 6, 5, 4],
            ),
            (
                &[0xF2, 0x45, 0x0F, 0x70, 0xCA, 0x1B][..],
                [3, 2, 1, 0, 4, 5, 6, 7],
            ),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = sentinel;
                x86.xmm[10] = vector_u16(&(0..8).collect::<Vec<_>>(), 0);
            }
            execute_lifted_x86(insn, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(words(&x86.xmm[9], 8), expected);
                assert!(x86.xmm[9][2..].iter().all(|word| *word == sentinel[0]));
            }
        }

        let dword_source = (0u32..8)
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[10] = vec_from_bytes(&dword_source);
        }
        execute_lifted_x86(&[0xC4, 0x41, 0x7D, 0x70, 0xCA, 0x1B], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(dwords(&x86.xmm[9], 8), [3, 2, 1, 0, 7, 6, 5, 4]);
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
        }

        // Destination/source aliasing snapshots the complete source before
        // writeback, and each 128-bit lane uses the same immediate selectors.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vec_from_bytes(&dword_source);
        }
        execute_lifted_x86(&[0xC4, 0x41, 0x7D, 0x70, 0xC9, 0x1B], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(dwords(&x86.xmm[9], 8), [3, 2, 1, 0, 7, 6, 5, 4]);
        }

        memory.write(0x101, &dword_source[..16]).unwrap();
        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
        }
        execute_lifted_x86(&[0xC5, 0x79, 0x70, 0x48, 0x01, 0x1B], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(dwords(&x86.xmm[9], 4), [3, 2, 1, 0]);
            assert!(x86.xmm[9][2..].iter().all(|word| *word == 0));
        }

        // EVEX masking is applied after lane-local shuffling. E4NF memory
        // semantics perform complete loads even when every mask bit is zero.
        let evex_source = (0u32..16)
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.xmm[18] = vec_from_bytes(&evex_source);
            x86.k[3] = 0x5555;
        }
        execute_lifted_x86(
            &[0x62, 0xA1, 0x7D, 0x4B, 0x70, 0xCA, 0x1B],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let actual = dwords(&x86.xmm[17], 16);
            for lane in 0..16 {
                let expected = if lane % 2 == 0 {
                    (lane / 4 * 4 + (3 - lane % 4)) as u32
                } else {
                    0xCCCC_CCCC
                };
                assert_eq!(actual[lane], expected, "masked dword lane {lane}");
            }
        }

        memory.write(0x104, &0x1234_5678u32.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.k[3] = u64::MAX;
        }
        execute_lifted_x86(
            &[0x62, 0xE1, 0x7D, 0x5B, 0x70, 0x48, 0x01, 0x1B],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(dwords(&x86.xmm[17], 16), vec![0x1234_5678; 16]);
        }

        ctx.write_vreg(rax, 0x1F0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.k[3] = 0;
        }
        let evex_fault = execute_lifted_x86(
            &[0x62, 0xE1, 0x7D, 0x4B, 0x70, 0x08, 0x1B],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            evex_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[17], sentinel);
        }

        ctx.write_vreg(rax, 0x1F8);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
        }
        let fault = execute_lifted_x86(&[0xC5, 0x79, 0x70, 0x08, 0x1B], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[9], sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_two_source_shuffles_execute_selectors_masks_broadcast_and_faults() {
        fn vector_u32(values: &[u32], fill: u64) -> VecValue {
            let mut result = [fill; 16];
            let bytes = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            for (index, chunk) in bytes.chunks(8).enumerate() {
                result[index] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            result
        }
        fn dwords(value: &VecValue, count: usize) -> Vec<u32> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 4)
                .collect::<Vec<_>>()
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }
        fn qwords(value: &VecValue, count: usize) -> Vec<u64> {
            value[..count].to_vec()
        }

        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector_u32(&[0, 1, 2, 3], sentinel[0]);
            x86.xmm[10] = vector_u32(&[10, 11, 12, 13], 0);
        }
        execute_lifted_x86(&[0x45, 0x0F, 0xC6, 0xCA, 0xE4], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(dwords(&x86.xmm[9], 4), [0, 1, 12, 13]);
            assert!(x86.xmm[9][2..].iter().all(|word| *word == sentinel[0]));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[10] = vector_u32(&(10..18).collect::<Vec<_>>(), 0);
            x86.xmm[11] = vector_u32(&(20..28).collect::<Vec<_>>(), 0);
        }
        execute_lifted_x86(&[0xC4, 0x41, 0x2C, 0xC6, 0xCB, 0xE4], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(dwords(&x86.xmm[9], 8), [10, 11, 22, 23, 14, 15, 26, 27]);
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[10][..4].copy_from_slice(&[10, 11, 12, 13]);
            x86.xmm[11][..4].copy_from_slice(&[20, 21, 22, 23]);
        }
        execute_lifted_x86(&[0xC4, 0x41, 0x2D, 0xC6, 0xCB, 0x0A], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(qwords(&x86.xmm[9], 4), [10, 21, 12, 23]);
        }

        memory.write(0x104, &0x1234_5678u32.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.xmm[18] = vector_u32(&(100..116).collect::<Vec<_>>(), 0);
            x86.k[3] = u64::MAX;
        }
        execute_lifted_x86(
            &[0x62, 0xE1, 0x6C, 0x53, 0xC6, 0x48, 0x01, 0xE4],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                dwords(&x86.xmm[17], 16),
                [
                    100,
                    101,
                    0x1234_5678,
                    0x1234_5678,
                    104,
                    105,
                    0x1234_5678,
                    0x1234_5678,
                    108,
                    109,
                    0x1234_5678,
                    0x1234_5678,
                    112,
                    113,
                    0x1234_5678,
                    0x1234_5678
                ]
            );
        }

        ctx.write_vreg(rax, 0x1F0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.k[3] = 0;
        }
        let fault = execute_lifted_x86(
            &[0x62, 0xE1, 0x6C, 0x43, 0xC6, 0x08, 0xE4],
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
    fn lifted_fp_unpack_family_executes_lane_blocks_masks_broadcast_and_e4nf_faults() {
        fn vector(values: &[u32], fill: u64) -> VecValue {
            let bytes = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            let mut out = [fill; 16];
            for (index, chunk) in bytes.chunks_exact(8).enumerate() {
                out[index] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            out
        }
        fn lanes(value: &VecValue, count: usize) -> Vec<u32> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 4)
                .collect::<Vec<_>>()
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = vector(&[0, 1, 2, 3], sentinel[0]);
            x86.xmm[2] = vector(&[10, 11, 12, 13], 0);
        }
        execute_lifted_x86(&[0x0F, 0x14, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(lanes(&x86.xmm[1], 4), [0, 10, 1, 11]);
            assert!(x86.xmm[1][2..].iter().all(|word| *word == sentinel[0]));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[2] = vector(&(0..8).collect::<Vec<_>>(), 0);
            x86.xmm[3] = vector(&(10..18).collect::<Vec<_>>(), 0);
        }
        execute_lifted_x86(&[0xC5, 0xEC, 0x14, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(lanes(&x86.xmm[1], 8), [0, 10, 1, 11, 4, 14, 5, 15]);
            assert!(x86.xmm[1][4..].iter().all(|word| *word == 0));
        }

        memory.write(0x100, &99u32.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.xmm[18] = vector(&(0..16).collect::<Vec<_>>(), 0);
            x86.k[3] = 0x7777;
        }
        execute_lifted_x86(
            &[0x62, 0xE1, 0x6C, 0x53, 0x14, 0x48, 0x00],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let actual = lanes(&x86.xmm[17], 16);
            for lane in 0..16 {
                assert_eq!(
                    actual[lane],
                    if lane % 4 == 0 {
                        (lane / 4 * 4 + lane % 4 / 2) as u32
                    } else if lane % 4 == 1 {
                        99
                    } else if lane % 4 == 2 {
                        (lane / 4 * 4 + lane % 4 / 2) as u32
                    } else {
                        0xCCCC_CCCC
                    }
                );
            }
        }

        // E4NF performs the complete memory access even with an all-zero mask.
        ctx.write_vreg(rax, 0x1F8);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[3] = 0;
            x86.xmm[17] = sentinel;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xE1, 0x6C, 0x43, 0x14, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[17], sentinel);
        }

        // Integer VPUNPCK* shares E4NF/E4NF.nb: its all-zero mask likewise
        // cannot suppress the complete vector memory access.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[3] = 0;
            x86.xmm[1] = sentinel;
        }
        let integer_fault =
            execute_lifted_x86(&[0x62, 0xF1, 0x6D, 0x4B, 0x60, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            integer_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }

        ctx.write_vreg(rax, 0x101);
        let aligned = execute_lifted_x86(&[0x0F, 0x15, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            aligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
    }
    #[test]
    fn x86_rsqrt14_matches_intel_reference_all_segments_mxcsr_and_special_values() {
        // FNV-1a accumulation over outputs generated by Intel's RECIP14.c
        // RSQRT14S/RSQRT14D implementation. The corpus covers all 32 segments
        // in both exponent-parity branches, four exponent scales, and four
        // positions within each segment.
        const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
        let mut hash32 = FNV_OFFSET;
        let mut count32 = 0usize;
        for exponents in [[1u32, 63, 127, 253], [2u32, 64, 128, 254]] {
            for exponent in exponents {
                for segment in 0u32..32 {
                    for tail in [0u32, 1, (1 << 17) - 1, (1 << 18) - 1] {
                        let bits = (exponent << 23) | (segment << 18) | tail;
                        let result =
                            SmirInterpreter::x86_simd_rsqrt14(u64::from(bits), X86_SIMD_F32, 0);
                        hash32 = (hash32 ^ result.bits).wrapping_mul(FNV_PRIME);
                        assert_eq!(result.status, 0);
                        count32 += 1;
                    }
                }
            }
        }
        assert_eq!(count32, 1_024);
        assert_eq!(hash32, 0xE26E_279D_4CE5_1F25);

        let mut hash64 = FNV_OFFSET;
        let mut count64 = 0usize;
        for exponents in [[1u64, 255, 1023, 2045], [2u64, 256, 1024, 2046]] {
            for exponent in exponents {
                for segment in 0u64..32 {
                    for tail in [0u64, 1, (1 << 46) - 1, (1 << 47) - 1] {
                        let bits = (exponent << 52) | (segment << 47) | tail;
                        let result = SmirInterpreter::x86_simd_rsqrt14(bits, X86_SIMD_F64, 0);
                        hash64 = (hash64 ^ result.bits).wrapping_mul(FNV_PRIME);
                        assert_eq!(result.status, 0);
                        count64 += 1;
                    }
                }
            }
        }
        assert_eq!(count64, 1_024);
        assert_eq!(hash64, 0x0F83_22C7_DF28_7325);

        for (bits, format, mxcsr, expected) in [
            (0, X86_SIMD_F32, 0, 0x7F80_0000),
            (0x8000_0000, X86_SIMD_F32, 0, 0xFF80_0000),
            (1, X86_SIMD_F32, 0, 0x64B5_0280),
            (1, X86_SIMD_F32, 1 << 6, 0x7F80_0000),
            (0x8000_0001, X86_SIMD_F32, 0, 0xFFC0_0000),
            (0xBF80_0000, X86_SIMD_F32, 0, 0xFFC0_0000),
            (0xFF80_0000, X86_SIMD_F32, 0, 0xFFC0_0000),
            (0x7F80_0000, X86_SIMD_F32, 0, 0),
            (0x7FC1_2345, X86_SIMD_F32, 0, 0x7FC1_2345),
            (0x7F81_2345, X86_SIMD_F32, 0, 0x7FC1_2345),
            (0x3F80_0000, X86_SIMD_F32, 0, 0x3F80_0000),
            (0x4000_0000, X86_SIMD_F32, 0, 0x3F35_0280),
            (0x0080_0000, X86_SIMD_F32, 0, 0x5F00_0000),
            (0x7F7F_FFFF, X86_SIMD_F32, 1 << 15, 0x1F80_0000),
            (0, X86_SIMD_F64, 0, 0x7FF0_0000_0000_0000),
            (
                0x8000_0000_0000_0000,
                X86_SIMD_F64,
                0,
                0xFFF0_0000_0000_0000,
            ),
            (1, X86_SIMD_F64, 0, 0x6180_0000_0000_0000),
            (1, X86_SIMD_F64, 1 << 6, 0x7FF0_0000_0000_0000),
            (
                0x8000_0000_0000_0001,
                X86_SIMD_F64,
                0,
                0xFFF8_0000_0000_0000,
            ),
            (
                0xBFF0_0000_0000_0000,
                X86_SIMD_F64,
                0,
                0xFFF8_0000_0000_0000,
            ),
            (
                0xFFF0_0000_0000_0000,
                X86_SIMD_F64,
                0,
                0xFFF8_0000_0000_0000,
            ),
            (0x7FF0_0000_0000_0000, X86_SIMD_F64, 0, 0),
            (
                0x7FF8_1234_5678_9ABC,
                X86_SIMD_F64,
                0,
                0x7FF8_1234_5678_9ABC,
            ),
            (
                0x7FF0_1234_5678_9ABC,
                X86_SIMD_F64,
                0,
                0x7FF8_1234_5678_9ABC,
            ),
            (
                0x3FF0_0000_0000_0000,
                X86_SIMD_F64,
                0,
                0x3FF0_0000_0000_0000,
            ),
            (
                0x4000_0000_0000_0000,
                X86_SIMD_F64,
                0,
                0x3FE6_A050_0000_0000,
            ),
            (
                0x0010_0000_0000_0000,
                X86_SIMD_F64,
                0,
                0x5FE0_0000_0000_0000,
            ),
            (
                0x7FEF_FFFF_FFFF_FFFF,
                X86_SIMD_F64,
                1 << 15,
                0x1FF0_0000_0000_0000,
            ),
        ] {
            assert_eq!(
                SmirInterpreter::x86_simd_rsqrt14(bits, format, mxcsr),
                X86SimdFpResult {
                    bits: expected,
                    status: 0,
                }
            );
        }

        let limit = 2.0f64.powi(-14);
        for exponent in [1u64, 2, 256, 1023, 1024, 1792, 2045, 2046] {
            for segment in 0u64..32 {
                for tail in [1u64, (1 << 46) - 1, (1 << 47) - 1] {
                    let bits = (exponent << 52) | (segment << 47) | tail;
                    let input = f64::from_bits(bits);
                    let actual = f64::from_bits(
                        SmirInterpreter::x86_simd_rsqrt14(bits, X86_SIMD_F64, 0).bits,
                    );
                    let reference = input.sqrt().recip();
                    let relative_error = ((actual - reference) / reference).abs();
                    assert!(
                        relative_error < limit,
                        "VRSQRT14D {input:e}: relative error {relative_error:e}"
                    );
                }
            }
        }
    }
    #[test]
    fn lifted_x86_rsqrt14_preserves_widths_scalar_merge_masks_daz_and_fault_suppression() {
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
            x86.xmm[3][0] = u64::from(4.0f32.to_bits());
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0x4F, 0xCB], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_3F00_0000);
            assert_eq!(x86.xmm[1][1], 0x0FED_CBA9_8765_4321);
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 0xAAAA_AAAA_DEAD_BEEF;
            x86.k[1] = 0;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0x4F, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_DEAD_BEEF);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 0xAAAA_AAAA_DEAD_BEEF;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x89, 0x4F, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_0000_0000);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3][0] = 1;
            x86.mxcsr = 0;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0x4F, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] as u32, 0x64B5_0280);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 1 << 6;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0x4F, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] as u32, 0x7F80_0000);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3][0] = u64::from((-4.0f32).to_bits());
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0x4F, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] as u32, 0xFFC0_0000);
            assert_eq!(x86.mxcsr, 0x1F80);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0xDEAD_BEEF_CAFE_BABE; 16];
            x86.xmm[3][0] = (u64::from(4.0f32.to_bits()) << 32) | u64::from(1.0f32.to_bits());
            x86.xmm[3][1] = (u64::from(64.0f32.to_bits()) << 32) | u64::from(16.0f32.to_bits());
            x86.mxcsr = 0;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x08, 0x4E, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x3F00_0000_3F80_0000);
            assert_eq!(x86.xmm[1][1], 0x3E00_0000_3E80_0000);
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k1, 0);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0x4F, 0x08], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.write_vreg(k1, 1);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0x4F, 0x08], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
    }
    #[test]
    fn x86_rsqrt28_matches_intel_reference_all_segments_and_special_values() {
        // FNV-1a-style accumulation over outputs and status flags generated by
        // Intel's RECIP28EXP2.c RSQRT28S/RSQRT28D implementation. The corpus
        // exercises all 512 polynomial segments, three exponent scales of the
        // parity selecting each table half, and three in-segment positions.
        const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
        let mut hash32 = FNV_OFFSET;
        let mut count32 = 0usize;
        for segment in 0u32..512 {
            let exponents = if segment >> 8 == 0 {
                [2u32, 128, 254]
            } else {
                [1u32, 127, 253]
            };
            for exponent in exponents {
                for tail in [1u32, 0x3FFF, 0x7FFF] {
                    let fraction = ((segment & 0xFF) << 15) | tail;
                    let bits = (exponent << 23) | fraction;
                    let result = SmirInterpreter::x86_simd_rsqrt28(u64::from(bits), X86_SIMD_F32);
                    hash32 = (hash32 ^ result.bits).wrapping_mul(FNV_PRIME);
                    hash32 = (hash32 ^ u64::from(result.status)).wrapping_mul(FNV_PRIME);
                    count32 += 1;
                }
            }
        }
        assert_eq!(count32, 4_608);
        assert_eq!(hash32, 0x4FDD_6AA5_109F_3F46);

        let mut hash64 = FNV_OFFSET;
        let mut count64 = 0usize;
        for segment in 0u64..512 {
            let exponents = if segment >> 8 == 0 {
                [2u64, 1024, 2046]
            } else {
                [1u64, 1023, 2045]
            };
            for exponent in exponents {
                for tail in [1u64, 0x1F_FFFF, 0x3F_FFFF] {
                    let fraction = ((segment & 0xFF) << 44) | (tail << 22) | (tail & 0x3F_FFFF);
                    let bits = (exponent << 52) | fraction;
                    let result = SmirInterpreter::x86_simd_rsqrt28(bits, X86_SIMD_F64);
                    hash64 = (hash64 ^ result.bits).wrapping_mul(FNV_PRIME);
                    hash64 = (hash64 ^ u64::from(result.status)).wrapping_mul(FNV_PRIME);
                    count64 += 1;
                }
            }
        }
        assert_eq!(count64, 4_608);
        assert_eq!(hash64, 0xDE3A_1C91_6E9A_4AD5);

        for (bits, format, expected, status) in [
            (0, X86_SIMD_F32, 0x7F80_0000, 1 << 2),
            (0x8000_0000, X86_SIMD_F32, 0xFF80_0000, 1 << 2),
            (1, X86_SIMD_F32, 0x7F80_0000, 1 << 2),
            (0x8000_0001, X86_SIMD_F32, 0xFF80_0000, 1 << 2),
            (0x7F80_0000, X86_SIMD_F32, 0, 0),
            (0xFF80_0000, X86_SIMD_F32, 0xFFC0_0000, 1),
            (0xBF80_0000, X86_SIMD_F32, 0xFFC0_0000, 1),
            (0, X86_SIMD_F64, 0x7FF0_0000_0000_0000, 1 << 2),
            (
                0x8000_0000_0000_0000,
                X86_SIMD_F64,
                0xFFF0_0000_0000_0000,
                1 << 2,
            ),
            (1, X86_SIMD_F64, 0x7FF0_0000_0000_0000, 1 << 2),
            (
                0x8000_0000_0000_0001,
                X86_SIMD_F64,
                0xFFF0_0000_0000_0000,
                1 << 2,
            ),
            (0x7FF0_0000_0000_0000, X86_SIMD_F64, 0, 0),
            (
                0xFFF0_0000_0000_0000,
                X86_SIMD_F64,
                0xFFF8_0000_0000_0000,
                1,
            ),
            (
                0xBFF0_0000_0000_0000,
                X86_SIMD_F64,
                0xFFF8_0000_0000_0000,
                1,
            ),
        ] {
            assert_eq!(
                SmirInterpreter::x86_simd_rsqrt28(bits, format),
                X86SimdFpResult {
                    bits: expected,
                    status,
                }
            );
        }

        for (input, expected) in [(0.25f64, 2.0f64), (1.0, 1.0), (4.0, 0.5), (16.0, 0.25)] {
            assert_eq!(
                SmirInterpreter::x86_simd_rsqrt28(input.to_bits(), X86_SIMD_F64),
                X86SimdFpResult {
                    bits: expected.to_bits(),
                    status: 0,
                }
            );
        }

        let qnan = SmirInterpreter::x86_simd_rsqrt28(0xFFC1_2345, X86_SIMD_F32);
        assert_eq!(
            qnan,
            X86SimdFpResult {
                bits: 0xFFC1_2345,
                status: 0
            }
        );
        let snan = SmirInterpreter::x86_simd_rsqrt28(0xFF81_2345, X86_SIMD_F32);
        assert_eq!(
            snan,
            X86SimdFpResult {
                bits: 0xFFC1_2345,
                status: 1
            }
        );

        let limit = 2.0f64.powi(-28);
        for exponent in [1u64, 256, 1023, 1792, 2046] {
            for segment in 0u64..256 {
                for tail in [1u64, 0x1F_FFFF, 0x3F_FFFF] {
                    let bits = (exponent << 52) | (segment << 44) | (tail << 22);
                    let input = f64::from_bits(bits);
                    let result = SmirInterpreter::x86_simd_rsqrt28(bits, X86_SIMD_F64);
                    if result.bits == 0 {
                        continue;
                    }
                    let actual = f64::from_bits(result.bits);
                    let reference = 1.0 / input.sqrt();
                    let relative_error = ((actual - reference) / reference).abs();
                    assert!(
                        relative_error < limit,
                        "VRSQRT28D {input:e}: relative error {relative_error:e}"
                    );
                }
            }
        }
    }
    #[test]
    fn lifted_x86_rsqrt28_preserves_scalar_merge_masks_sae_and_fault_atomicity() {
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
            x86.xmm[3][0] = u64::from(4.0f32.to_bits());
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0xCD, 0xCB], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_3F00_0000);
            assert_eq!(x86.xmm[1][1], 0x0FED_CBA9_8765_4321);
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 0xAAAA_AAAA_DEAD_BEEF;
            x86.k[1] = 0;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0xCD, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_DEAD_BEEF);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 0xAAAA_AAAA_DEAD_BEEF;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x89, 0xCD, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_0000_0000);
        }

        let sentinel = [0xDEAD_BEEF_CAFE_BABEu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[3][0] = u64::from((-4.0f32).to_bits());
            x86.k[1] = 1;
            x86.mxcsr = 0;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0xCD, 0xCB], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
            assert_ne!(x86.mxcsr & 1, 0);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.mxcsr = 0;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x19, 0xCD, 0xCB], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_FFC0_0000);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k1, 0);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0xCD, 0x08], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.write_vreg(k1, 1);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0xCD, 0x08], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
    }

