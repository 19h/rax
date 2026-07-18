//! simd::packed tests

use super::*;
use crate::smir::interpret::*;
use crate::smir::interpret::tests::*;

    #[test]
    fn lifted_packed_sse_arithmetic_and_moves_execute() {
        fn f32x4(values: [f32; 4]) -> VecValue {
            let mut result = [0u64; 16];
            result[0] = values[0].to_bits() as u64 | ((values[1].to_bits() as u64) << 32);
            result[1] = values[2].to_bits() as u64 | ((values[3].to_bits() as u64) << 32);
            result
        }
        fn f64x2(values: [f64; 2]) -> VecValue {
            let mut result = [0u64; 16];
            result[0] = values[0].to_bits();
            result[1] = values[1].to_bits();
            result
        }

        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = f32x4([1.0, 2.0, 3.0, 4.0]);
            x86.xmm[1] = f32x4([5.0, 6.0, 7.0, 8.0]);
        }
        execute_lifted_x86(&[0x0F, 0x58, 0xC1], &mut ctx, &mut memory); // ADDPS XMM0,XMM1
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], f32x4([6.0, 8.0, 10.0, 12.0]));
            assert_eq!(x86.xmm[1], f32x4([5.0, 6.0, 7.0, 8.0]));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = f64x2([8.0, 9.0]);
            x86.xmm[1] = f64x2([2.0, 3.0]);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x5E, 0xC1], &mut ctx, &mut memory); // DIVPD
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], f64x2([4.0, 3.0]));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [
                0xFFFF_0000_FFFF_0000,
                0xAAAA_AAAA_5555_5555,
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
            x86.xmm[1] = [
                0x00FF_00FF_00FF_00FF,
                0xFFFF_0000_FFFF_0000,
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
        }
        execute_lifted_x86(&[0x0F, 0x57, 0xC1], &mut ctx, &mut memory); // XORPS
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0xFF00_00FF_FF00_00FF);
            assert_eq!(x86.xmm[0][1], 0x5555_AAAA_AAAA_5555);
        }

        let bytes: Vec<u8> = (0u8..16).collect();
        memory.write(0x200, &bytes).unwrap();
        ctx.write_vreg(rbx, 0x200);
        execute_lifted_x86(&[0x0F, 0x10, 0x03], &mut ctx, &mut memory); // MOVUPS XMM0,[RBX]
        execute_lifted_x86(&[0x0F, 0x11, 0x43, 0x10], &mut ctx, &mut memory); // MOVUPS [RBX+16],XMM0
        let mut copied = [0u8; 16];
        memory.read(0x210, &mut copied).unwrap();
        assert_eq!(copied.as_slice(), bytes.as_slice());
    }
    #[test]
    fn lifted_scalar_sse_moves_and_arithmetic_preserve_or_clear_upper_lanes_exactly() {
        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;

        let mut lhs = [0u64; 16];
        lhs[0] = 1.5f32.to_bits() as u64 | (0xA1B2_C3D4u64 << 32);
        lhs[1] = 0x1122_3344_5566_7788;
        lhs[2] = 0x99AA_BBCC_DDEE_FF00;
        let mut rhs = [0u64; 16];
        rhs[0] = 2.25f32.to_bits() as u64 | (0xDEAD_BEEFu64 << 32);
        rhs[1] = u64::MAX;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = lhs;
            x86.xmm[1] = rhs;
        }
        execute_lifted_x86(&[0xF3, 0x0F, 0x58, 0xC1], &mut ctx, &mut memory); // ADDSS
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let mut expected = lhs;
            expected[0] = 3.75f32.to_bits() as u64 | (lhs[0] & 0xFFFF_FFFF_0000_0000);
            assert_eq!(x86.xmm[0], expected);
            assert_eq!(x86.xmm[1], rhs);
        }

        let upper = [
            0x0123_4567_89AB_CDEF,
            0xAABB_CCDD_EEFF_0011,
            0x2233_4455_6677_8899,
        ];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0][0] = 9.0f64.to_bits();
            x86.xmm[0][1..4].copy_from_slice(&upper);
            x86.xmm[1][0] = 3.0f64.to_bits();
        }
        execute_lifted_x86(&[0xF2, 0x0F, 0x5E, 0xC1], &mut ctx, &mut memory); // DIVSD
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 3.0f64.to_bits());
            assert_eq!(&x86.xmm[0][1..4], &upper);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = lhs;
            x86.xmm[1] = rhs;
        }
        execute_lifted_x86(&[0xF3, 0x0F, 0x10, 0xC1], &mut ctx, &mut memory); // MOVSS reg
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let mut expected = lhs;
            expected[0] = (lhs[0] & 0xFFFF_FFFF_0000_0000) | (rhs[0] & 0xFFFF_FFFF);
            assert_eq!(x86.xmm[0], expected);
        }

        memory
            .write(0x200, &6.5f32.to_bits().to_le_bytes())
            .unwrap();
        ctx.write_vreg(rbx, 0x200);
        execute_lifted_x86(&[0xF3, 0x0F, 0x10, 0x03], &mut ctx, &mut memory); // MOVSS memory
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 6.5f32.to_bits() as u64);
            assert_eq!(x86.xmm[0][1], 0);
            assert_eq!(x86.xmm[0][2..], lhs[2..], "legacy AVX upper state");
        }

        let sentinel = [0xAA; 16];
        memory.write(0x300, &sentinel).unwrap();
        ctx.write_vreg(rbx, 0x304);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 12.25f64.to_bits();
        }
        execute_lifted_x86(&[0xF2, 0x0F, 0x11, 0x0B], &mut ctx, &mut memory); // MOVSD [RBX],XMM1
        let mut stored = [0u8; 16];
        memory.read(0x300, &mut stored).unwrap();
        assert_eq!(&stored[..4], &[0xAA; 4]);
        assert_eq!(&stored[4..12], &12.25f64.to_bits().to_le_bytes());
        assert_eq!(&stored[12..], &[0xAA; 4]);

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn lifted_scalar_sse_memory_faults_preserve_destination_flags_and_memory() {
        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
        let original = [0x0123_4567_89AB_CDEFu64; 16];
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = original;
        }
        ctx.write_vreg(rbx, 0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;

        let mut short_memory = FlatMemory::new(0x204);
        let exit = execute_lifted_x86(&[0xF2, 0x0F, 0x10, 0x03], &mut ctx, &mut short_memory); // MOVSD XMM0,[RBX]
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], original, "faulting scalar load destination");
        }

        let seed = [0xA5; 16];
        let mut inner = FlatMemory::new(0x400);
        inner.write(0x200, &seed).unwrap();
        let mut read_only = StoreFaultMemory {
            inner,
            stores_before_fault: 0,
        };
        let exit = execute_lifted_x86(&[0xF3, 0x0F, 0x11, 0x03], &mut ctx, &mut read_only); // MOVSS [RBX],XMM0
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        let mut actual = [0u8; 16];
        read_only.inner.read(0x200, &mut actual).unwrap();
        assert_eq!(actual, seed);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], original, "faulting scalar store source");
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn lifted_vex_scalar_moves_and_arithmetic_merge_xmm_and_zero_avx_upper_state() {
        fn f32x4_with_upper(values: [f32; 4], upper: u64) -> VecValue {
            let mut result = [upper; 16];
            result[0] = values[0].to_bits() as u64 | ((values[1].to_bits() as u64) << 32);
            result[1] = values[2].to_bits() as u64 | ((values[3].to_bits() as u64) << 32);
            result
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;

        let src1 = f32x4_with_upper([1.5, 10.0, -3.0, 7.0], 0x1111_2222_3333_4444);
        let src2 = f32x4_with_upper([2.25, 99.0, 88.0, 77.0], 0xAAAA_BBBB_CCCC_DDDD);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1] = src1;
            x86.xmm[2] = src2;
        }
        execute_lifted_x86(&[0xC5, 0xF2, 0x58, 0xC2], &mut ctx, &mut memory); // VADDSS XMM0,XMM1,XMM2
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0] as u32, 3.75f32.to_bits());
            assert_eq!(x86.xmm[0][0] >> 32, src1[0] >> 32);
            assert_eq!(x86.xmm[0][1], src1[1]);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
            assert_eq!(x86.xmm[1], src1);
            assert_eq!(x86.xmm[2], src2);
        }

        // Destination/source1 alias: upper XMM lanes must be captured before
        // VEX zeroing rewrites the shared vector backing store.
        let alias_src = f32x4_with_upper([8.0, 2.0, 3.0, 4.0], 0xDEAD_BEEF_CAFE_BABE);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = alias_src;
            x86.xmm[1] = f32x4_with_upper([0.5, 0.0, 0.0, 0.0], 1);
        }
        execute_lifted_x86(&[0xC5, 0xFA, 0x58, 0xC1], &mut ctx, &mut memory); // VADDSS XMM0,XMM0,XMM1
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0] as u32, 8.5f32.to_bits());
            assert_eq!(x86.xmm[0][0] >> 32, alias_src[0] >> 32);
            assert_eq!(x86.xmm[0][1], alias_src[1]);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        }

        let merge = f32x4_with_upper([100.0, 20.0, 30.0, 40.0], 0x1111);
        let low = f32x4_with_upper([-6.0, 200.0, 300.0, 400.0], 0x2222);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = merge;
            x86.xmm[1] = low;
            x86.xmm[2] = [u64::MAX; 16];
        }
        execute_lifted_x86(&[0xC5, 0xFA, 0x10, 0xD1], &mut ctx, &mut memory); // VMOVSS XMM2,XMM0,XMM1
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2][0] as u32, (-6.0f32).to_bits());
            assert_eq!(x86.xmm[2][0] >> 32, merge[0] >> 32);
            assert_eq!(x86.xmm[2][1], merge[1]);
            assert!(x86.xmm[2][2..].iter().all(|word| *word == 0));
        }

        memory
            .write(0x200, &12.5f64.to_bits().to_le_bytes())
            .unwrap();
        ctx.write_vreg(rax, 0x200);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3] = [u64::MAX; 16];
        }
        execute_lifted_x86(&[0xC5, 0xFB, 0x10, 0x18], &mut ctx, &mut memory); // VMOVSD XMM3,[RAX]
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[3][0], 12.5f64.to_bits());
            assert!(x86.xmm[3][1..].iter().all(|word| *word == 0));
        }

        let packed_source = [0x5555_AAAA_1234_5678u64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1] = packed_source;
        }
        execute_lifted_x86(&[0xC5, 0xF8, 0x10, 0xC1], &mut ctx, &mut memory); // VMOVUPS XMM0,XMM1
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[0][..2], &packed_source[..2]);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn lifted_vex_scalar_memory_faults_preserve_destination_flags_and_memory() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let original = [0xABCD_EF01_2345_6789u64; 16];
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = original;
        }
        ctx.write_vreg(rax, 0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;

        let mut short_memory = FlatMemory::new(0x204);
        let exit = execute_lifted_x86(&[0xC5, 0xFB, 0x10, 0x00], &mut ctx, &mut short_memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], original);
        }

        let seed = [0x5A; 16];
        let mut inner = FlatMemory::new(0x400);
        inner.write(0x200, &seed).unwrap();
        let mut read_only = StoreFaultMemory {
            inner,
            stores_before_fault: 0,
        };
        let exit = execute_lifted_x86(&[0xC5, 0xFA, 0x11, 0x00], &mut ctx, &mut read_only);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        let mut actual = [0u8; 16];
        read_only.inner.read(0x200, &mut actual).unwrap();
        assert_eq!(actual, seed);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], original);
        }
        for bytes in [&[0x0F, 0x55, 0x00][..], &[0xC5, 0xF8, 0x55, 0x00][..]] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = original;
            }
            let exit = execute_lifted_x86(bytes, &mut ctx, &mut short_memory);
            assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ),
                "{bytes:02X?}"
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[0], original, "{bytes:02X?}");
            }
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn lifted_vex_scalar_arithmetic_all_ops_and_memory_sources_execute() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();

        for (name, bytes, is_f64, a, b, expected) in [
            (
                "vaddss",
                &[0xC5, 0xF2, 0x58, 0xC2][..],
                false,
                7.5f64,
                2.5f64,
                10.0f64,
            ),
            (
                "vmulss",
                &[0xC5, 0xF2, 0x59, 0xC2][..],
                false,
                3.0,
                4.0,
                12.0,
            ),
            (
                "vsubss",
                &[0xC5, 0xF2, 0x5C, 0xC2][..],
                false,
                7.0,
                2.0,
                5.0,
            ),
            (
                "vdivss",
                &[0xC5, 0xF2, 0x5E, 0xC2][..],
                false,
                9.0,
                3.0,
                3.0,
            ),
            (
                "vaddsd",
                &[0xC5, 0xF3, 0x58, 0xC2][..],
                true,
                7.5,
                2.5,
                10.0,
            ),
            (
                "vmulsd",
                &[0xC5, 0xF3, 0x59, 0xC2][..],
                true,
                3.0,
                4.0,
                12.0,
            ),
            ("vsubsd", &[0xC5, 0xF3, 0x5C, 0xC2][..], true, 7.0, 2.0, 5.0),
            ("vdivsd", &[0xC5, 0xF3, 0x5E, 0xC2][..], true, 9.0, 3.0, 3.0),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [u64::MAX; 16];
                x86.xmm[1] = [0x1357_9BDF_2468_ACE0; 16];
                x86.xmm[2] = [0xAAAA_BBBB_CCCC_DDDD; 16];
                if is_f64 {
                    x86.xmm[1][0] = a.to_bits();
                    x86.xmm[2][0] = b.to_bits();
                } else {
                    x86.xmm[1][0] = (a as f32).to_bits() as u64 | (0xDEAD_BEEFu64 << 32);
                    x86.xmm[2][0] = (b as f32).to_bits() as u64;
                }
            }
            execute_lifted_x86(bytes, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                if is_f64 {
                    assert_eq!(x86.xmm[0][0], expected.to_bits(), "{name}");
                    assert_eq!(x86.xmm[0][1], 0x1357_9BDF_2468_ACE0, "{name}: merge");
                } else {
                    assert_eq!(x86.xmm[0][0] as u32, (expected as f32).to_bits(), "{name}");
                    assert_eq!(x86.xmm[0][0] >> 32, 0xDEAD_BEEF, "{name}: merge");
                    assert_eq!(x86.xmm[0][1], 0x1357_9BDF_2468_ACE0, "{name}: merge");
                }
                assert!(
                    x86.xmm[0][2..].iter().all(|word| *word == 0),
                    "{name}: upper"
                );
            }
        }

        ctx.write_vreg(rax, 0x200);
        memory
            .write(0x200, &1.25f32.to_bits().to_le_bytes())
            .unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 2.75f32.to_bits() as u64 | (0x1234_5678u64 << 32);
        }
        execute_lifted_x86(&[0xC5, 0xF2, 0x58, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0] as u32, 4.0f32.to_bits());
            assert_eq!(x86.xmm[0][0] >> 32, 0x1234_5678);
        }

        memory
            .write(0x200, &2.0f64.to_bits().to_le_bytes())
            .unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 10.0f64.to_bits();
            x86.xmm[1][1] = 0xCAFE_BABE_DEAD_BEEF;
        }
        execute_lifted_x86(&[0xC5, 0xF3, 0x5E, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 5.0f64.to_bits());
            assert_eq!(x86.xmm[0][1], 0xCAFE_BABE_DEAD_BEEF);
        }
    }
    #[test]
    fn lifted_legacy_and_vex_movmsk_execute_exact_bits_zero_extend_and_preserve_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let mut memory = FlatMemory::new(1);
        let mut ctx = SmirContext::new_x86_64();
        let flags_before = 0x8D7u64;
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // MOVMSKPS: sign bits in lanes 0 and 2 become result bits 0 and 2.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0; 16];
            x86.xmm[1][0] = 0x0000_0000_8000_0000;
            x86.xmm[1][1] = 0x0000_0000_8000_0000;
        }
        ctx.write_vreg(rax, u64::MAX);
        execute_lifted_x86(&[0x0F, 0x50, 0xC1], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), 0b0101);
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

        // REX.W MOVMSKPD selects a 64-bit destination write; the two qword
        // sign bits still occupy only result bits 0 and 1.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 0x7FFF_FFFF_FFFF_FFFF;
            x86.xmm[1][1] = 0x8000_0000_0000_0000;
        }
        ctx.write_vreg(rdx, u64::MAX);
        execute_lifted_x86(&[0x66, 0x48, 0x0F, 0x50, 0xD1], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rdx), 0b10);
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

        // VEX.256.WIG uses all eight dword lanes and honors both VEX.R and
        // VEX.B register extensions. W=1 is deliberately exercised here.
        let mut ymm9 = [0u64; 16];
        for lane in 0..8usize {
            if lane & 1 != 0 {
                ymm9[lane / 2] |= 0x8000_0000u64 << ((lane & 1) * 32);
            }
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = ymm9;
        }
        ctx.write_vreg(r8, u64::MAX);
        execute_lifted_x86(&[0xC4, 0x41, 0xFC, 0x50, 0xC1], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(r8), 0xAA);
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_legacy_and_vex_pmovmskb_execute_exact_bits_and_zero_extend() {
        fn source(mask: u32, bytes: usize) -> VecValue {
            let mut raw = [0u8; 128];
            for (index, byte) in raw[..bytes].iter_mut().enumerate() {
                *byte = if mask & (1 << index) != 0 {
                    0x80 | index as u8
                } else {
                    index as u8
                };
            }
            let mut value = [0u64; 16];
            for (word, chunk) in value.iter_mut().zip(raw.chunks_exact(8)) {
                *word = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            value
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
        let flags_before = 0x8D7;
        let pattern = 0xA55A_C33C;
        let mut memory = FlatMemory::new(1);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = source(pattern, 16);
            x86.xmm[10] = source(pattern, 32);
        }

        ctx.write_vreg(rax, u64::MAX);
        assert!(matches!(
            execute_lifted_x86(&[0x66, 0x0F, 0xD7, 0xC1], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        assert_eq!(ctx.read_vreg(rax), u64::from(pattern & 0xFFFF));

        ctx.write_vreg(r9, u64::MAX);
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0x41, 0xFD, 0xD7, 0xCA], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        assert_eq!(ctx.read_vreg(r9), u64::from(pattern));
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_legacy_and_vex_lddqu_preserve_or_zero_upper_and_fault_atomically() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut memory = FlatMemory::new(0x80);
        let mut payload = [0u8; 32];
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(7).wrapping_add(3);
        }
        memory.write(3, &payload).unwrap();
        let mut ctx = SmirContext::new_x86_64();

        let legacy_old = [0xA5A5_A5A5_A5A5_A5A5u64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = legacy_old;
        }
        ctx.write_vreg(rax, 2);
        execute_lifted_x86(&[0xF2, 0x0F, 0xF0, 0x40, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let mut low = [0u8; 16];
            for (word_index, word) in x86.xmm[0][..2].iter().enumerate() {
                low[word_index * 8..word_index * 8 + 8].copy_from_slice(&word.to_le_bytes());
            }
            assert_eq!(low, payload[..16]);
            assert_eq!(&x86.xmm[0][2..], &legacy_old[2..]);
        }

        let vex_old = [u64::MAX; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = vex_old;
        }
        ctx.write_vreg(rax, 0);
        execute_lifted_x86(&[0xC5, 0xFF, 0xF0, 0x40, 0x03], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let mut low = [0u8; 32];
            for (word_index, word) in x86.xmm[0][..4].iter().enumerate() {
                low[word_index * 8..word_index * 8 + 8].copy_from_slice(&word.to_le_bytes());
            }
            assert_eq!(low, payload);
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }

        // The memory read precedes the vector write; an out-of-range operand
        // leaves both the low destination and its shared upper state unchanged.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = vex_old;
        }
        ctx.write_vreg(rax, 0x1000);
        let exit = execute_lifted_x86(&[0xC5, 0xFF, 0xF0, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], vex_old);
        }
    }
    #[test]
    fn lifted_legacy_and_vex_packed_integer_logic_executes_exact_bits_and_faults_atomically() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut memory = FlatMemory::new(0x100);
        let mut ctx = SmirContext::new_x86_64();
        let flags_before = 0xCD7;
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        let lhs = [
            0x00FF_00FF_AAAA_5555,
            0xF0F0_0F0F_1234_5678,
            0x1111_2222_3333_4444,
            0x5555_6666_7777_8888,
            0x9999_AAAA_BBBB_CCCC,
            0xDDDD_EEEE_FFFF_0000,
            0x0123_4567_89AB_CDEF,
            0xFEDC_BA98_7654_3210,
            0xA5A5_A5A5_A5A5_A5A5,
            0xA5A5_A5A5_A5A5_A5A5,
            0xA5A5_A5A5_A5A5_A5A5,
            0xA5A5_A5A5_A5A5_A5A5,
            0xA5A5_A5A5_A5A5_A5A5,
            0xA5A5_A5A5_A5A5_A5A5,
            0xA5A5_A5A5_A5A5_A5A5,
            0xA5A5_A5A5_A5A5_A5A5,
        ];
        let rhs = [
            0xFF00_FF00_0F0F_F0F0,
            0x0FF0_F00F_8765_4321,
            0xFFFF_0000_FFFF_0000,
            0x0000_FFFF_0000_FFFF,
            0x1357_9BDF_2468_ACE0,
            0xCAFE_BABE_DEAD_BEEF,
            0xFFFF_FFFF_0000_0000,
            0x0000_0000_FFFF_FFFF,
            0x5A5A_5A5A_5A5A_5A5A,
            0x5A5A_5A5A_5A5A_5A5A,
            0x5A5A_5A5A_5A5A_5A5A,
            0x5A5A_5A5A_5A5A_5A5A,
            0x5A5A_5A5A_5A5A_5A5A,
            0x5A5A_5A5A_5A5A_5A5A,
            0x5A5A_5A5A_5A5A_5A5A,
            0x5A5A_5A5A_5A5A_5A5A,
        ];

        for (opcode, apply) in [
            (0xDB, (|a: u64, b: u64| a & b) as fn(u64, u64) -> u64),
            (0xDF, (|a: u64, b: u64| !a & b) as fn(u64, u64) -> u64),
            (0xEB, (|a: u64, b: u64| a | b) as fn(u64, u64) -> u64),
            (0xEF, (|a: u64, b: u64| a ^ b) as fn(u64, u64) -> u64),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mm[0] = lhs[0];
                x86.mm[1] = rhs[0];
                x86.x87.tag_word = 0xFFFF;
                x86.x87.status_word = 4 << 11;
            }
            execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.mm[0], apply(lhs[0], rhs[0]), "MMX {opcode:02X}");
                assert_eq!(x86.x87.tag_word, 0);
                assert_eq!(x86.x87.status_word & 0x3800, 4 << 11);
            }

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = lhs;
                x86.xmm[1] = rhs;
            }
            execute_lifted_x86(&[0x66, 0x0F, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[0][0], apply(lhs[0], rhs[0]), "legacy {opcode:02X}");
                assert_eq!(x86.xmm[0][1], apply(lhs[1], rhs[1]), "legacy {opcode:02X}");
                assert_eq!(&x86.xmm[0][2..], &lhs[2..], "legacy upper {opcode:02X}");
            }

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [u64::MAX; 16];
                x86.xmm[1] = lhs;
                x86.xmm[2] = rhs;
            }
            execute_lifted_x86(&[0xC5, 0xF5, opcode, 0xC2], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                for word in 0..4 {
                    assert_eq!(
                        x86.xmm[0][word],
                        apply(lhs[word], rhs[word]),
                        "VEX {opcode:02X}, word {word}",
                    );
                }
                assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
            }
        }

        // A successful MMX memory source reads exactly 8 bytes before entering
        // MMX state and computing the non-commutative PANDN result.
        memory.write(0xF8, &rhs[0].to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0xF8);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = lhs[0];
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0xDF, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], !lhs[0] & rhs[0]);
            assert_eq!(x86.x87.tag_word, 0);
        }

        // A faulting MMX memory read precedes both the destination write and
        // the x87 tag transition.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = lhs[0];
            x86.x87.tag_word = 0xFFFF;
        }
        ctx.write_vreg(rax, 0x1000);
        let mmx_fault = execute_lifted_x86(&[0x0F, 0xDF, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], lhs[0]);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        // A memory fault occurs before the architectural destination write.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = lhs;
        }
        ctx.write_vreg(rax, 0x1000);
        let exit = execute_lifted_x86(&[0x66, 0x0F, 0xDF, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], lhs);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_saturating_packed_add_subtract_clamps_all_signed_unsigned_boundaries() {
        fn alternating_bytes(lo: u8, hi: u8) -> u64 {
            u64::from_le_bytes([lo, hi, lo, hi, lo, hi, lo, hi])
        }
        fn alternating_words(lo: u16, hi: u16) -> u64 {
            u64::from(lo) | (u64::from(hi) << 16) | (u64::from(lo) << 32) | (u64::from(hi) << 48)
        }

        let cases = [
            (
                0xEC,
                alternating_bytes(120, (-120i8) as u8),
                alternating_bytes(20, (-20i8) as u8),
                alternating_bytes(i8::MAX as u8, i8::MIN as u8),
            ),
            (
                0xED,
                alternating_words(30_000, (-30_000i16) as u16),
                alternating_words(10_000, (-10_000i16) as u16),
                alternating_words(i16::MAX as u16, i16::MIN as u16),
            ),
            (
                0xDC,
                u64::from_le_bytes([250; 8]),
                u64::from_le_bytes([10; 8]),
                u64::MAX,
            ),
            (
                0xDD,
                alternating_words(65_000, 65_000),
                alternating_words(1_000, 1_000),
                u64::MAX,
            ),
            (
                0xE8,
                alternating_bytes(120, (-120i8) as u8),
                alternating_bytes((-20i8) as u8, 20),
                alternating_bytes(i8::MAX as u8, i8::MIN as u8),
            ),
            (
                0xE9,
                alternating_words(30_000, (-30_000i16) as u16),
                alternating_words((-10_000i16) as u16, 10_000),
                alternating_words(i16::MAX as u16, i16::MIN as u16),
            ),
            (
                0xD8,
                u64::from_le_bytes([5; 8]),
                u64::from_le_bytes([10; 8]),
                0,
            ),
            (
                0xD9,
                alternating_words(500, 500),
                alternating_words(1_000, 1_000),
                0,
            ),
        ];
        let mut memory = FlatMemory::new(0x400);
        let mut ctx = SmirContext::new_x86_64();
        for (opcode, lhs_word, rhs_word, expected_word) in cases {
            let mut lhs = [0xA5A5_A5A5_A5A5_A5A5u64; 16];
            lhs[0] = lhs_word;
            lhs[1] = lhs_word;
            let rhs = [rhs_word; 16];
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mm[0] = lhs_word;
                x86.mm[1] = rhs_word;
                x86.x87.tag_word = 0xFFFF;
            }
            execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.mm[0], expected_word, "MMX {opcode:02X}");
                assert_eq!(x86.x87.tag_word, 0);
            }

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = lhs;
                x86.xmm[1] = rhs;
            }
            execute_lifted_x86(&[0x66, 0x0F, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[0][0], expected_word, "legacy {opcode:02X}");
                assert_eq!(x86.xmm[0][1], expected_word, "legacy {opcode:02X}");
                assert_eq!(&x86.xmm[0][2..], &lhs[2..], "legacy upper {opcode:02X}");
            }

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [u64::MAX; 16];
                x86.xmm[1] = [lhs_word; 16];
                x86.xmm[2] = rhs;
            }
            execute_lifted_x86(&[0xC5, 0xF5, opcode, 0xC2], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(&x86.xmm[0][..4], &[expected_word; 4], "VEX {opcode:02X}");
                assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
            }
        }

        // EVEX byte merge/zero masks preserve saturation at element granularity.
        let src1 = [alternating_bytes(120, (-120i8) as u8); 16];
        let src2 = [alternating_bytes(20, (-20i8) as u8); 16];
        let old = [0xA5A5_A5A5_A5A5_A5A5u64; 16];
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = src1;
            x86.xmm[1] = src2;
            x86.xmm[2] = old;
        }
        ctx.write_vreg(k1, 0xAAAA_AAAA_AAAA_AAAA);
        execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x49, 0xEC, 0xD1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[2][..8], &[0x80A5_80A5_80A5_80A5u64; 8]);
            assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = old;
        }
        execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0xC9, 0xEC, 0xD1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[2][..8], &[0x8000_8000_8000_8000u64; 8]);
        }

        // Masked word memory faults occur before architectural destination update.
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        memory.write(0x3FE, &1u16.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x3FE);
        ctx.write_vreg(k1, 2);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [alternating_words(30_000, 30_000); 16];
            x86.xmm[2] = old;
        }
        let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x49, 0xED, 0x10], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2], old);
        }
    }
    #[test]
    fn lifted_legacy_packed_sse_preserves_shared_state_above_bit_127() {
        fn f32x4(values: [f32; 4], upper: u64) -> VecValue {
            let mut result = [upper; 16];
            result[0] = values[0].to_bits() as u64 | ((values[1].to_bits() as u64) << 32);
            result[1] = values[2].to_bits() as u64 | ((values[3].to_bits() as u64) << 32);
            result
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let sentinel = 0xCAFE_BABE_DEAD_BEEFu64;
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;

        let mut source = [0x1111_2222_3333_4444u64; 16];
        source[0] = 0x0123_4567_89AB_CDEF;
        source[1] = 0xFEDC_BA98_7654_3210;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [sentinel; 16];
            x86.xmm[1] = source;
        }
        execute_lifted_x86(&[0x0F, 0x10, 0xC1], &mut ctx, &mut memory); // MOVUPS
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[0][..2], &source[..2]);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
        }

        let lhs = f32x4([1.0, 2.0, 3.0, 4.0], sentinel);
        let rhs = f32x4([10.0, 20.0, 30.0, 40.0], 0x9999);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = lhs;
            x86.xmm[1] = rhs;
        }
        execute_lifted_x86(&[0x0F, 0x58, 0xC1], &mut ctx, &mut memory); // ADDPS
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let expected = f32x4([11.0, 22.0, 33.0, 44.0], sentinel);
            assert_eq!(x86.xmm[0], expected);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [sentinel; 16];
            x86.xmm[0][0] = 0xFFFF_0000_F0F0_0F0F;
            x86.xmm[0][1] = 0xAAAA_5555_1234_5678;
            x86.xmm[1][0] = 0x0FF0_0FF0_FFFF_0000;
            x86.xmm[1][1] = 0xFFFF_0000_FFFF_0000;
        }
        execute_lifted_x86(&[0x0F, 0x54, 0xC1], &mut ctx, &mut memory); // ANDPS
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0x0FF0_0000_F0F0_0000);
            assert_eq!(x86.xmm[0][1], 0xAAAA_0000_1234_0000);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [sentinel; 16];
            x86.xmm[0][0] = 0xFFFF_0000_F0F0_0F0F;
            x86.xmm[0][1] = 0xAAAA_5555_1234_5678;
            x86.xmm[1][0] = 0x0FF0_0FF0_FFFF_0000;
            x86.xmm[1][1] = 0xFFFF_0000_FFFF_0000;
        }
        execute_lifted_x86(&[0x0F, 0x55, 0xC1], &mut ctx, &mut memory); // ANDNPS
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0x0000_0FF0_0F0F_0000);
            assert_eq!(x86.xmm[0][1], 0x5555_0000_EDCB_0000);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [sentinel; 16];
            x86.xmm[0][0] = 0xFFFF_0000_F0F0_0F0F;
            x86.xmm[0][1] = 0xAAAA_5555_1234_5678;
            x86.xmm[1][0] = 0x0FF0_0FF0_FFFF_0000;
            x86.xmm[1][1] = 0xFFFF_0000_FFFF_0000;
        }
        execute_lifted_x86(&[0xC5, 0xF8, 0x55, 0xC1], &mut ctx, &mut memory); // VANDNPS
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0x0000_0FF0_0F0F_0000);
            assert_eq!(x86.xmm[0][1], 0x5555_0000_EDCB_0000);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [sentinel; 16];
            x86.xmm[0][0] = 1 | (2u64 << 32);
            x86.xmm[0][1] = 3 | (4u64 << 32);
            x86.xmm[1][0] = 10 | (20u64 << 32);
            x86.xmm[1][1] = 30 | (40u64 << 32);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0xFE, 0xC1], &mut ctx, &mut memory); // PADDD
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 11 | (22u64 << 32));
            assert_eq!(x86.xmm[0][1], 33 | (44u64 << 32));
            assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [sentinel; 16];
            x86.xmm[0][0] = 2 | (3u64 << 32);
            x86.xmm[0][1] = 4 | (5u64 << 32);
            x86.xmm[1][0] = 6 | (7u64 << 32);
            x86.xmm[1][1] = 8 | (9u64 << 32);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x40, 0xC1], &mut ctx, &mut memory); // PMULLD
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 12 | (21u64 << 32));
            assert_eq!(x86.xmm[0][1], 32 | (45u64 << 32));
            assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
        }

        ctx.write_vreg(rax, 0x200);
        let bytes = 0x8877_6655_4433_2211u64
            .to_le_bytes()
            .into_iter()
            .chain(0x00FF_EEDD_CCBB_AA99u64.to_le_bytes())
            .collect::<Vec<_>>();
        memory.write(0x200, &bytes).unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [sentinel; 16];
        }
        execute_lifted_x86(&[0x0F, 0x10, 0x00], &mut ctx, &mut memory); // MOVUPS [RAX]
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0x8877_6655_4433_2211);
            assert_eq!(x86.xmm[0][1], 0x00FF_EEDD_CCBB_AA99);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn lifted_legacy_packed_sse_load_fault_preserves_full_destination() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let original = [0x0123_4567_89AB_CDEFu64; 16];
        let mut ctx = SmirContext::new_x86_64();
        ctx.write_vreg(rax, 0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = original;
        }
        let mut short_memory = FlatMemory::new(0x208);
        let exit = execute_lifted_x86(&[0x0F, 0x10, 0x00], &mut ctx, &mut short_memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], original);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn lifted_x86_packed_int_to_fp32_fp64_is_exact_masked_atomic_and_sae_aware() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
        let original = [0xCAFE_BABE_DEAD_BEEFu64; 16];
        let mut memory = FlatMemory::new(0x400);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;

        // Dynamic round-up distinguishes both signs at the first inexact I32
        // boundary. The masked lane merges, and only active lanes contribute PE.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = original;
            x86.xmm[3] = [0; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 32, 16_777_217);
            SmirInterpreter::set_lane(&mut x86.xmm[3], 1, 32, 16_777_217);
            SmirInterpreter::set_lane(&mut x86.xmm[3], 2, 32, u64::from((-16_777_217i32) as u32));
            x86.mxcsr = (0x1F80 & !(3 << 13)) | (2 << 13);
        }
        ctx.write_vreg(k2, 0b0101);
        let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x4A, 0x5B, 0xCB], &mut ctx, &mut memory);
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], 0, 32),
                u64::from(16_777_218.0f32.to_bits())
            );
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], 1, 32),
                SmirInterpreter::get_lane(&original, 1, 32)
            );
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], 2, 32),
                u64::from((-16_777_216.0f32).to_bits())
            );
            assert_ne!(x86.mxcsr & (1 << 5), 0);
        }

        // Unsigned I64 -> F32 with RZ-SAE uses the full source domain, commits
        // max-finite below 2^64, and does not alter MXCSR despite inexactness.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = original;
            x86.xmm[3] = [u64::MAX; 16];
            x86.mxcsr = 0x1F80 | 0x21;
        }
        ctx.write_vreg(k2, 1);
        let mxcsr_before = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.mxcsr,
            _ => unreachable!(),
        };
        let exit = execute_lifted_x86(&[0x62, 0xF1, 0xFF, 0x7A, 0x7A, 0xCB], &mut ctx, &mut memory);
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 32), 0x5F7F_FFFF);
            for lane in 1..8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                    SmirInterpreter::get_lane(&original, lane, 32)
                );
            }
            assert!(x86.xmm[1][4..].iter().all(|word| *word == 0));
            assert_eq!(x86.mxcsr, mxcsr_before);
        }

        // An all-zero mask suppresses an out-of-range broadcast access. One
        // active lane exposes the fault before any destination modification.
        ctx.write_vreg(rax, 0x1000);
        ctx.write_vreg(k2, 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = original;
            x86.mxcsr = 0x1F80;
        }
        let bytes = [0x62, 0xF1, 0x7F, 0xDA, 0x7A, 0x48, 0x7F];
        let exit = execute_lifted_x86(&bytes, &mut ctx, &mut memory);
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert!(x86.xmm[1].iter().all(|word| *word == 0));
        }
        ctx.write_vreg(k2, 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = original;
        }
        let exit = execute_lifted_x86(&bytes, &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], original);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn lifted_x86_packed_fp32_fp64_to_int_is_exact_atomic_and_daz_sae_aware() {
        let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
        let original = [0x0123_4567_89AB_CDEFu64; 16];
        let mut memory = FlatMemory::new(0x400);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;

        // Nearest-even signed F32 -> I64 covers half-way values, a NaN
        // indefinite result, and exact -2^63. IE and PE accumulate atomically.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = original;
            x86.xmm[3] = [0; 16];
            for (lane, value) in [1.5f32, 2.5, -1.5, f32::NAN].into_iter().enumerate() {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[3],
                    lane as u8,
                    32,
                    u64::from(value.to_bits()),
                );
            }
            SmirInterpreter::set_lane(
                &mut x86.xmm[3],
                4,
                32,
                u64::from((-9_223_372_036_854_775_808.0f32).to_bits()),
            );
            x86.mxcsr = 0x1F80;
        }
        ctx.write_vreg(k2, 0b1_1111);
        let bytes = [0x62, 0xF1, 0x7D, 0x4A, 0x7B, 0xCB];
        let exit = execute_lifted_x86(&bytes, &mut ctx, &mut memory);
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 64), 2);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 1, 64), 2);
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], 2, 64),
                (-2i64) as u64
            );
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 3, 64), 1u64 << 63);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 4, 64), 1u64 << 63);
            assert_eq!(x86.mxcsr & ((1 << 0) | (1 << 5)), (1 << 0) | (1 << 5));
        }

        // Unmasking Invalid updates MXCSR but leaves the complete destination
        // unchanged when any active lane cannot be represented.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = original;
            x86.xmm[3] = [0; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 32, u64::from(f32::NAN.to_bits()));
            x86.mxcsr = 0x1F80 & !(1 << 7);
        }
        ctx.write_vreg(k2, 1);
        let exit = execute_lifted_x86(&bytes, &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], original);
            assert_ne!(x86.mxcsr & 1, 0);
        }

        // Unsigned truncation with SAE commits the all-ones indefinite value
        // for a negative input, truncates positive input, and preserves MXCSR.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = original;
            x86.xmm[3] = [0; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 64, (-1.0f64).to_bits());
            SmirInterpreter::set_lane(&mut x86.xmm[3], 1, 64, 1.9f64.to_bits());
            SmirInterpreter::set_lane(&mut x86.xmm[3], 2, 64, f64::MAX.to_bits());
            x86.mxcsr = 0x1F80 | 0x21;
        }
        ctx.write_vreg(k2, 0b111);
        let mxcsr_before = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.mxcsr,
            _ => unreachable!(),
        };
        let exit = execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0x1A, 0x78, 0xCB], &mut ctx, &mut memory);
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 64), u64::MAX);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 1, 64), 1);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 2, 64), u64::MAX);
            assert_eq!(x86.mxcsr, mxcsr_before);
        }

        // DAZ converts a binary32 subnormal to signed zero before conversion.
        // Without DAZ it is inexact (PE), but this instruction does not raise DE.
        for (daz, expected_status) in [(false, 1 << 5), (true, 0)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1] = original;
                x86.xmm[3] = [0; 16];
                SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 32, 1);
                x86.mxcsr = (0x1F80 & !0x3F) | if daz { 1 << 6 } else { 0 };
            }
            let exit = execute_lifted_x86(&[0xC5, 0xFE, 0x5B, 0xCB], &mut ctx, &mut memory);
            assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 32), 0);
                assert_eq!(x86.mxcsr & 0x3F, expected_status, "DAZ={daz}");
            }
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn lifted_x86_packed_fp_convert_honors_lanes_rounding_and_upper_state() {
        fn pack_f32(values: [f32; 4], upper: u64) -> VecValue {
            let mut result = [upper; 16];
            result[0] = u64::from(values[0].to_bits()) | (u64::from(values[1].to_bits()) << 32);
            result[1] = u64::from(values[2].to_bits()) | (u64::from(values[3].to_bits()) << 32);
            result
        }

        let sentinel = 0xCAFE_BABE_DEAD_BEEFu64;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x1000);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [sentinel; 16];
            x86.xmm[1] = pack_f32([1.5, -2.25, 99.0, 100.0], 0x1111);
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(&[0x0F, 0x5A, 0xC1], &mut ctx, &mut memory); // CVTPS2PD
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 1.5f64.to_bits());
            assert_eq!(x86.xmm[0][1], (-2.25f64).to_bits());
            assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
        }

        let midpoint = 1.0f64 + 2.0f64.powi(-24);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [sentinel; 16];
            x86.xmm[1][0] = midpoint.to_bits();
            x86.xmm[1][1] = 2.25f64.to_bits();
            x86.mxcsr = (0x1F80 & !(3 << 13)) | (2 << 13);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x5A, 0xC1], &mut ctx, &mut memory); // CVTPD2PS
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0] as u32, 1.0f32.to_bits() + 1);
            assert_eq!((x86.xmm[0][0] >> 32) as u32, 2.25f32.to_bits());
            assert_eq!(x86.xmm[0][1], 0, "legacy narrowing clears bits 127:64");
            assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [sentinel; 16];
            x86.xmm[1] = pack_f32([1.0, 2.0, 3.0, 4.0], 0x2222);
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(&[0xC5, 0xFC, 0x5A, 0xC1], &mut ctx, &mut memory); // VCVTPS2PD ymm
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                &x86.xmm[0][..4],
                &[
                    1.0f64.to_bits(),
                    2.0f64.to_bits(),
                    3.0f64.to_bits(),
                    4.0f64.to_bits(),
                ]
            );
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [sentinel; 16];
            x86.xmm[1][..4].copy_from_slice(&[
                1.0f64.to_bits(),
                2.0f64.to_bits(),
                3.0f64.to_bits(),
                4.0f64.to_bits(),
            ]);
        }
        execute_lifted_x86(&[0xC5, 0xFD, 0x5A, 0xC1], &mut ctx, &mut memory); // VCVTPD2PS xmm,ymm
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[0][..2], &pack_f32([1.0, 2.0, 3.0, 4.0], 0)[..2]);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn lifted_x86_packed_fp_convert_memory_fault_preserves_destination_and_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let original = [0xA5A5_5A5A_0123_4567u64; 16];
        for (name, bytes) in [
            ("legacy CVTPS2PD", &[0x0F, 0x5A, 0x00][..]),
            ("VEX VCVTPD2PS ymm source", &[0xC5, 0xFD, 0x5A, 0x00][..]),
        ] {
            let mut ctx = SmirContext::new_x86_64();
            ctx.write_vreg(rax, 0x200);
            ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
            ctx.flags.lazy = None;
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = original;
            }
            let mut short_memory = FlatMemory::new(0x204);
            let exit = execute_lifted_x86(bytes, &mut ctx, &mut short_memory);
            assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ),
                "{name}: {exit:?}"
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[0], original, "{name}: destination");
            }
            ctx.flags.materialize_all();
            assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7, "{name}: flags");
        }
    }
    #[test]
    fn lifted_legacy_and_vex_mxcsr_roundtrip_and_fault_atomicity() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rsi = VReg::Arch(ArchReg::X86(X86Reg::Rsi));
        let rdi = VReg::Arch(ArchReg::X86(X86Reg::Rdi));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.write_vreg(rax, 0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;

        memory.write(0x200, &0x5F80u32.to_le_bytes()).unwrap();
        execute_lifted_x86(&[0x0F, 0xAE, 0x10], &mut ctx, &mut memory); // LDMXCSR [RAX]
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr, 0x5F80);
        }
        execute_lifted_x86(&[0x0F, 0xAE, 0x58, 0x04], &mut ctx, &mut memory); // STMXCSR [RAX+4]
        let mut stored = [0u8; 4];
        memory.read(0x204, &mut stored).unwrap();
        assert_eq!(u32::from_le_bytes(stored), 0x5F80);

        memory.write(0x208, &0x3F80u32.to_le_bytes()).unwrap();
        execute_lifted_x86(&[0xC5, 0xF8, 0xAE, 0x50, 0x08], &mut ctx, &mut memory); // VLDMXCSR [RAX+8]
        execute_lifted_x86(&[0xC5, 0xF8, 0xAE, 0x58, 0x0C], &mut ctx, &mut memory); // VSTMXCSR [RAX+12]
        memory.read(0x20C, &mut stored).unwrap();
        assert_eq!(u32::from_le_bytes(stored), 0x3F80);

        memory.write(0x200, &0x7F80u32.to_le_bytes()).unwrap();
        ctx.write_vreg(rdi, 0xAAAA_BBBB_0000_0100);
        ctx.write_vreg(rsi, 0xCCCC_DDDD_0000_0080);
        execute_lifted_x86(&[0x67, 0x0F, 0xAE, 0x14, 0x77], &mut ctx, &mut memory); // LDMXCSR [edi+esi*2]
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr, 0x7F80);
        }

        let mut short_memory = FlatMemory::new(0x202);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F80;
        }
        let exit = execute_lifted_x86(&[0x0F, 0xAE, 0x10], &mut ctx, &mut short_memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr, 0x1F80, "faulting load changed MXCSR");
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn interprets_vpermute_single_and_two_table_domains() {
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Ymm(0)));
        let table = VReg::Arch(ArchReg::X86(X86Reg::Ymm(1)));
        let indices = VReg::Arch(ArchReg::X86(X86Reg::Ymm(2)));
        let second = VReg::Arch(ArchReg::X86(X86Reg::Xmm(3)));
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] =
                vec_from_bytes(&(10u32..18).flat_map(u32::to_le_bytes).collect::<Vec<_>>());
            x86.xmm[2] = vec_from_bytes(
                &[7u32, 0, 6, 1, 13, 2, 12, 3]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
            x86.xmm[3] = vec_from_bytes(&(0x80u8..0x90).collect::<Vec<_>>());
        }
        let interp = SmirInterpreter::new();
        let mut memory = FlatMemory::new(0x1000);
        interp
            .execute_op(
                &mut ctx,
                &mut memory,
                &SmirOp::new(
                    OpId(0),
                    0x1000,
                    OpKind::VPermute {
                        dst,
                        src1: table,
                        src2: None,
                        indices,
                        elem: VecElementType::I32,
                        width: VecWidth::V256,
                        overwrite_table: false,
                    },
                ),
            )
            .unwrap();
        let expected = [17u32, 10, 16, 11, 15, 12, 14, 13]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(
            SmirInterpreter::read_vec(&ctx, dst),
            vec_from_bytes(&expected)
        );

        let byte_indices = VReg::Virtual(VirtualId(200));
        let byte_dst = VReg::Virtual(VirtualId(201));
        SmirInterpreter::write_vec(
            &mut ctx,
            byte_indices,
            vec_from_bytes(&[0, 15, 16, 31, 32, 47, 48, 63, 8, 24, 40, 56, 1, 17, 33, 49]),
        );
        interp
            .execute_op(
                &mut ctx,
                &mut memory,
                &SmirOp::new(
                    OpId(1),
                    0x1000,
                    OpKind::VPermute {
                        dst: byte_dst,
                        src1: table,
                        src2: Some(second),
                        indices: byte_indices,
                        elem: VecElementType::I8,
                        width: VecWidth::V128,
                        overwrite_table: false,
                    },
                ),
            )
            .unwrap();
        let out = SmirInterpreter::read_vec(&ctx, byte_dst);
        let table1 = SmirInterpreter::read_vec(&ctx, table);
        let table2 = SmirInterpreter::read_vec(&ctx, second);
        for (lane, selected) in [0u8, 15, 16, 31, 0, 15, 16, 31, 8, 24, 8, 24, 1, 17, 1, 17]
            .into_iter()
            .enumerate()
        {
            let expected = if selected < 16 {
                SmirInterpreter::get_lane(&table1, selected, 8)
            } else {
                SmirInterpreter::get_lane(&table2, selected - 16, 8)
            };
            assert_eq!(SmirInterpreter::get_lane(&out, lane as u8, 8), expected);
        }
    }
    #[test]
    fn executes_avx_permute_domains_masks_aliases_and_fault_suppression() {
        fn vec_u32(values: &[u32]) -> VecValue {
            vec_from_bytes(
                &values
                    .iter()
                    .copied()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
        }

        fn vec_u64(values: &[u64]) -> VecValue {
            vec_from_bytes(
                &values
                    .iter()
                    .copied()
                    .flat_map(u64::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
        }

        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = vec_u32(&[7, 0, 6, 1, 5, 2, 4, 3]);
            x86.xmm[3] = vec_u32(&[10, 11, 12, 13, 14, 15, 16, 17]);
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0x6D, 0x36, 0xCB], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], vec_u32(&[17, 10, 16, 11, 15, 12, 14, 13]));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = vec_u32(&[100, 101, 102, 103, 104, 105, 106, 107]);
            x86.xmm[3] = vec_u32(&[0, 1, 2, 3, 4, 5, 6, 7]);
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0x69, 0x0C, 0xD3], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2], vec_u32(&[100, 101, 102, 103]));
            assert_eq!(&x86.xmm[2][2..], &[0; 14]);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[5] = vec_u64(&[10, 11, 20, 21]);
            x86.xmm[6] = vec_u64(&[0, 2, 2, 0]);
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0x55, 0x0D, 0xE6], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[4], vec_u64(&[10, 11, 21, 20]));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = vec_u64(&[1, 2, 3, 4]);
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE3, 0xFD, 0x00, 0xCA, 0x1B], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], vec_u64(&[4, 3, 2, 1]));
            assert_eq!(&x86.xmm[1][4..], &[0; 12]);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[18] = vec_u64(&[100, 101, 102, 103, 104, 105, 106, 107]);
            x86.xmm[19] = vec_u64(&[10, 11, 12, 13, 14, 15, 16, 17]);
            x86.k[5] = 0x55;
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xA3, 0xFD, 0x4D, 0x05, 0xD3, 0xA5],
                &mut ctx,
                &mut memory
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[18], vec_u64(&[11, 101, 13, 103, 14, 105, 16, 107]));
        }

        memory
            .write(0x200, &0x1122_3344_5566_7788u64.to_le_bytes())
            .unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x200);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = [u64::MAX; 16];
            x86.k[4] = 0xA5;
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xE3, 0xFD, 0xDC, 0x00, 0x08, 0x1B],
                &mut ctx,
                &mut memory
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 64),
                    if 0xA5 & (1 << lane) != 0 {
                        0x1122_3344_5566_7788
                    } else {
                        0
                    }
                );
            }
        }

        memory.write(0x3FC, &0xA1B2_C3D4u32.to_le_bytes()).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[20] = [0xCCCC_CCCC_CCCC_CCCC; 16];
            x86.xmm[21] = vec_u32(&[
                0, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
            ]);
            x86.k[5] = 1;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xE2, 0x55, 0xC5, 0x36, 0x20], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[20], 0, 32), 0xA1B2_C3D4);
            for lane in 1..16 {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[20], lane, 32), 0);
            }
        }

        let sentinel = [0x5A5A_5A5A_5A5A_5A5A; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[20] = sentinel;
            x86.k[5] = 2;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xE2, 0x55, 0xC5, 0x36, 0x20], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[20], sentinel);
        }
    }
    #[test]
    fn executes_vnni_dot_signedness_saturation_masks_broadcast_and_fault_atomicity() {
        fn vec_bytes(bytes: &[u8]) -> VecValue {
            vec_from_bytes(bytes)
        }
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = vec_bytes(&100i32.to_le_bytes().repeat(4));
            x86.xmm[2] = vec_bytes(&[1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4]);
            x86.xmm[3] = vec_bytes(&[-1i8 as u8, 2, -3i8 as u8, 4].repeat(4));
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x69, 0x50, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..4 {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 32), 110);
            }
        }

        let broadcast = [1i16, -2]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>();
        memory.write(0x3FC, &broadcast).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[20] = vec_bytes(&i32::MAX.to_le_bytes().repeat(8));
            x86.xmm[21] = vec_bytes(
                &[2i16, -3]
                    .into_iter()
                    .cycle()
                    .take(16)
                    .flat_map(i16::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
            x86.k[3] = 0x55;
        }
        execute_lifted_x86(&[0x62, 0xE2, 0x55, 0x33, 0x53, 0x20], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[20], lane, 32),
                    if lane % 2 == 0 {
                        i32::MAX as u64
                    } else {
                        i32::MAX as u64
                    }
                );
            }
        }

        let sentinel = [0x4242_4242_4242_4242; 16];
        memory
            .write(0x3FC, &1.0f32.to_bits().to_le_bytes())
            .unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 1;
        }
        let sparse =
            execute_lifted_x86(&[0x62, 0xF2, 0x7E, 0x09, 0x72, 0x08], &mut ctx, &mut memory);
        assert!(matches!(sparse, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16), 0x3F80);
            for lane in 1..4 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 16),
                    SmirInterpreter::get_lane(&sentinel, lane, 16)
                );
            }
            assert_eq!(&x86.xmm[1][1..], &[0; 15]);
        }

        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x400);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[20] = sentinel;
            x86.k[3] = 1;
        }
        let exit = execute_lifted_x86(&[0x62, 0xE2, 0x55, 0x33, 0x53, 0x20], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[20], sentinel);
        }
    }
    #[test]
    fn executes_avx_vnni_int8_int16_signedness_saturation_aliases_memory_and_faults() {
        fn reference(
            acc: u32,
            lhs: &[u64],
            rhs: &[u64],
            bits: u32,
            lhs_signed: bool,
            rhs_signed: bool,
            saturate: bool,
        ) -> u32 {
            let signed = |value: u64| -> i128 {
                let shift = 128 - bits;
                (i128::from(value) << shift) >> shift
            };
            let unsigned_result = !lhs_signed && !rhs_signed;
            let mut sum = if unsigned_result {
                i128::from(acc)
            } else {
                i128::from(acc as i32)
            };
            for (&a, &b) in lhs.iter().zip(rhs) {
                let a = if lhs_signed { signed(a) } else { i128::from(a) };
                let b = if rhs_signed { signed(b) } else { i128::from(b) };
                sum += a * b;
            }
            if !saturate {
                sum as u32
            } else if unsigned_result {
                sum.clamp(0, i128::from(u32::MAX)) as u32
            } else {
                sum.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32 as u32
            }
        }

        let cases = [
            (
                &[0xC4, 0xE2, 0x6B, 0x50, 0xCB][..],
                8,
                true,
                true,
                false,
                4usize,
            ),
            (&[0xC4, 0xE2, 0x57, 0x51, 0xE6][..], 8, true, true, true, 8),
            (
                &[0xC4, 0xE2, 0x6A, 0x50, 0xCB][..],
                8,
                true,
                false,
                false,
                4,
            ),
            (&[0xC4, 0xE2, 0x56, 0x51, 0xE6][..], 8, true, false, true, 8),
            (
                &[0xC4, 0xE2, 0x68, 0x50, 0xCB][..],
                8,
                false,
                false,
                false,
                4,
            ),
            (
                &[0xC4, 0xE2, 0x54, 0x51, 0xE6][..],
                8,
                false,
                false,
                true,
                8,
            ),
            (
                &[0xC4, 0xE2, 0x6A, 0xD2, 0xCB][..],
                16,
                true,
                false,
                false,
                4,
            ),
            (
                &[0xC4, 0xE2, 0x56, 0xD3, 0xE6][..],
                16,
                true,
                false,
                true,
                8,
            ),
            (
                &[0xC4, 0xE2, 0x69, 0xD2, 0xCB][..],
                16,
                false,
                true,
                false,
                4,
            ),
            (
                &[0xC4, 0xE2, 0x55, 0xD3, 0xE6][..],
                16,
                false,
                true,
                true,
                8,
            ),
            (
                &[0xC4, 0xE2, 0x68, 0xD2, 0xCB][..],
                16,
                false,
                false,
                false,
                4,
            ),
            (
                &[0xC4, 0xE2, 0x54, 0xD3, 0xE6][..],
                16,
                false,
                false,
                true,
                8,
            ),
        ];
        for (bytes, bits, lhs_signed, rhs_signed, saturate, lanes) in cases {
            let dst_reg = if lanes == 4 { 1 } else { 4 };
            let lhs_reg = if lanes == 4 { 2 } else { 5 };
            let rhs_reg = if lanes == 4 { 3 } else { 6 };
            let terms = 32 / bits;
            let mask = (1u64 << bits) - 1;
            let positive = (1u64 << (bits - 1)) - 1;
            let negative = 1u64 << (bits - 1);
            let unsigned_max = mask;
            let mut acc = Vec::with_capacity(lanes);
            let mut lhs = Vec::with_capacity(lanes * terms as usize);
            let mut rhs = Vec::with_capacity(lanes * terms as usize);
            for lane in 0..lanes {
                acc.push(match lane % 4 {
                    0 => {
                        if !lhs_signed && !rhs_signed {
                            u32::MAX
                        } else {
                            i32::MAX as u32
                        }
                    }
                    1 => {
                        if !lhs_signed && !rhs_signed {
                            0
                        } else {
                            i32::MIN as u32
                        }
                    }
                    2 => 100,
                    _ => 0xA5A5_5A5A,
                });
                for term in 0..terms {
                    let (a, b) = match lane % 4 {
                        0 => (
                            if lhs_signed { positive } else { unsigned_max },
                            if rhs_signed { positive } else { unsigned_max },
                        ),
                        1 if lhs_signed => {
                            (negative, if rhs_signed { positive } else { unsigned_max })
                        }
                        1 if rhs_signed => {
                            (if lhs_signed { positive } else { unsigned_max }, negative)
                        }
                        1 => (unsigned_max, unsigned_max),
                        2 => ((term as u64 + 1) & mask, (term as u64 + 2) & mask),
                        _ => (mask.wrapping_sub(term as u64) & mask, term as u64 & mask),
                    };
                    lhs.push(a);
                    rhs.push(b);
                }
            }
            let expected = (0..lanes)
                .map(|lane| {
                    let start = lane * terms as usize;
                    reference(
                        acc[lane],
                        &lhs[start..start + terms as usize],
                        &rhs[start..start + terms as usize],
                        bits,
                        lhs_signed,
                        rhs_signed,
                        saturate,
                    )
                })
                .collect::<Vec<_>>();
            let mut ctx = SmirContext::new_x86_64();
            let mut memory = FlatMemory::new(0x400);
            ctx.flags.materialized = MaterializedFlags::from_rflags(0x8D5);
            ctx.flags.lazy = None;
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                let mut dst = [0xDEAD_BEEF_DEAD_BEEF; 16];
                let mut first = [0x1111_1111_1111_1111; 16];
                let mut second = [0x2222_2222_2222_2222; 16];
                for lane in 0..lanes {
                    SmirInterpreter::set_lane(&mut dst, lane as u8, 32, u64::from(acc[lane]));
                }
                for (lane, value) in lhs.iter().enumerate() {
                    SmirInterpreter::set_lane(&mut first, lane as u8, bits, *value);
                }
                for (lane, value) in rhs.iter().enumerate() {
                    SmirInterpreter::set_lane(&mut second, lane as u8, bits, *value);
                }
                x86.xmm[dst_reg] = dst;
                x86.xmm[lhs_reg] = first;
                x86.xmm[rhs_reg] = second;
            }
            execute_lifted_x86(bytes, &mut ctx, &mut memory);
            ctx.flags.materialize_all();
            assert_eq!(
                ctx.flags.materialized.to_rflags(),
                MaterializedFlags::from_rflags(0x8D5).to_rflags()
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                for (lane, expected) in expected.into_iter().enumerate() {
                    assert_eq!(
                        SmirInterpreter::get_lane(&x86.xmm[dst_reg], lane as u8, 32),
                        u64::from(expected),
                        "case {bytes:02X?}, lane {lane}"
                    );
                }
                assert_eq!(&x86.xmm[dst_reg][lanes / 2..], &[0; 16][..16 - lanes / 2]);
            }
        }

        // dst == src1 == src2 must use snapshots of the original vector.
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        let alias = vec_from_bytes(&[1u8, 2, 3, 4].repeat(4));
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = alias;
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x73, 0x50, 0xC9], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..4 {
                let original_acc = SmirInterpreter::get_lane(&alias, lane, 32) as u32;
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                    u64::from(original_acc.wrapping_add(30))
                );
            }
        }

        // Type-4 memory operands are unaligned, read the complete vector, and
        // fault before modifying the architectural accumulator.
        let source = [0xFFu8; 32];
        memory.write(0x101, &source).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[4] = vec_from_bytes(&u32::MAX.to_le_bytes().repeat(8));
            x86.xmm[5] = vec_from_bytes(&[0xFF; 32]);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x54, 0x51, 0x20], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[4], lane, 32),
                    u32::MAX as u64
                );
            }
        }

        let sentinel = [0x4242_4242_4242_4242; 16];
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3F0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[4] = sentinel;
        }
        let exit = execute_lifted_x86(&[0xC4, 0xE2, 0x54, 0x51, 0x20], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[4], sentinel);
        }
    }
    #[test]
    fn executes_vpshufbitqmb_lane_domains_opmask_zeroing_memory_and_faults() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        let mut data = [0u64; 16];
        let mut controls = [0u64; 16];
        for qword in 0..8u8 {
            SmirInterpreter::set_lane(
                &mut data,
                qword,
                64,
                0x8040_2010_0804_0201u64.rotate_left(u32::from(qword)),
            );
            for byte in 0..8u8 {
                SmirInterpreter::set_lane(
                    &mut controls,
                    qword * 8 + byte,
                    8,
                    u64::from((byte * 9 + qword) & 0x3F) | 0xC0,
                );
            }
        }
        let reference = |width: VecWidth, mask: u64| -> u64 {
            let mut result = 0u64;
            for qword in 0..(width.bytes() / 8) as u8 {
                let source = SmirInterpreter::get_lane(&data, qword, 64);
                for byte in 0..8u8 {
                    let output = qword * 8 + byte;
                    if mask & (1u64 << output) != 0 {
                        let control = SmirInterpreter::get_lane(&controls, output, 8) & 0x3F;
                        result |= ((source >> control) & 1) << output;
                    }
                }
            }
            result
        };

        for (bytes, width, dst, src, indices, mask) in [
            (
                &[0x62, 0xF2, 0x6D, 0x08, 0x8F, 0xCB][..],
                VecWidth::V128,
                1usize,
                2usize,
                3usize,
                u64::MAX,
            ),
            (
                &[0x62, 0xF2, 0x55, 0x2B, 0x8F, 0xE6][..],
                VecWidth::V256,
                4,
                5,
                6,
                0xA5A5_5A5A,
            ),
            (
                &[0x62, 0xB2, 0x6D, 0x42, 0x8F, 0xFB][..],
                VecWidth::V512,
                7,
                18,
                19,
                0xF0F0_0F0F_AA55_55AA,
            ),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[src] = data;
                x86.xmm[indices] = controls;
                x86.k[dst] = u64::MAX;
                x86.k[2] = if dst == 7 { mask } else { x86.k[2] };
                x86.k[3] = if dst == 4 { mask } else { x86.k[3] };
            }
            execute_lifted_x86(bytes, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.k[dst], reference(width, mask));
            }
        }

        // A sparse E4 mask may read the last mapped byte without touching any
        // masked-off byte beyond the memory boundary.
        memory.write(0x3FF, &[5]).unwrap();
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3FF);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = [0; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 64, 1 << 5);
            x86.k[1] = 0xDEAD_BEEF_DEAD_BEEF;
            x86.k[3] = 1;
        }
        let sparse =
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x0B, 0x8F, 0x08], &mut ctx, &mut memory);
        assert!(matches!(sparse, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[1], 1);
        }

        // Activating the next byte faults before the K destination is changed.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[1] = 0xDEAD_BEEF_DEAD_BEEF;
            x86.k[3] = 2;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x0B, 0x8F, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[1], 0xDEAD_BEEF_DEAD_BEEF);
        }
    }
    #[test]
    fn executes_packed_variable_shifts_boundaries_signedness_masks_and_aliases() {
        let cases = [
            (
                &[0x62, 0xF2, 0xED, 0x08, 0x10, 0xCB][..],
                16u32,
                ShiftOp::Lsr,
            ),
            (&[0x62, 0xF2, 0x6D, 0x08, 0x46, 0xCB][..], 32, ShiftOp::Asr),
            (&[0x62, 0xF2, 0xED, 0x08, 0x47, 0xCB][..], 64, ShiftOp::Lsl),
        ];
        let mut memory = FlatMemory::new(0x100);
        for (bytes, bits, shift) in cases {
            let mut ctx = SmirContext::new_x86_64();
            let lanes = 128 / bits;
            let counts = [
                0u64,
                u64::from(bits - 1),
                u64::from(bits),
                u64::from(bits + 1),
            ];
            let mask = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                for lane in 0..lanes as u8 {
                    SmirInterpreter::set_lane(&mut x86.xmm[2], lane, bits, mask ^ u64::from(lane));
                    SmirInterpreter::set_lane(
                        &mut x86.xmm[3],
                        lane,
                        bits,
                        counts[usize::from(lane) & 3],
                    );
                }
            }
            execute_lifted_x86(bytes, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                for lane in 0..lanes as u8 {
                    let value = mask ^ u64::from(lane);
                    let amount = counts[usize::from(lane) & 3];
                    let expected = if amount >= u64::from(bits) {
                        if shift == ShiftOp::Asr && value & (1u64 << (bits - 1)) != 0 {
                            mask
                        } else {
                            0
                        }
                    } else {
                        match shift {
                            ShiftOp::Lsr => value >> amount,
                            ShiftOp::Lsl => (value << amount) & mask,
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
                    };
                    assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, bits), expected);
                }
                assert_eq!(&x86.xmm[1][2..], &[0; 14]);
            }
        }
    }
    #[test]
    fn executes_load_broadcasts_tuple_order_masks_gpr_aliases_and_e6_fault_suppression() {
        let mut memory = FlatMemory::new(0x400);
        let flags_before = 0xCD7;

        // Register tuple broadcast repeats lanes 0,1 and preserves inactive
        // destination lanes under merging masking.
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        let control = 0xA55Au64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 32, 0x1122_3344);
            SmirInterpreter::set_lane(&mut x86.xmm[2], 1, 32, 0xAABB_CCDD);
            for lane in 0..16u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 32, 0xDEAD_0000 + u64::from(lane));
            }
            x86.k[1] = control;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x49, 0x19, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                let expected = if control & (1u64 << lane) != 0 {
                    if lane & 1 == 0 {
                        0x1122_3344
                    } else {
                        0xAABB_CCDD
                    }
                } else {
                    0xDEAD_0000 + u64::from(lane)
                };
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 32), expected);
            }
        }

        // The GPR source may alias an extended destination encoding; zeroing
        // masking applies at qword granularity.
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::R9)), 0x0123_4567_89AB_CDEF);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = [u64::MAX; 16];
            x86.k[3] = 0b0101_1010;
        }
        execute_lifted_x86(&[0x62, 0xC2, 0xFD, 0xCB, 0x7C, 0xC9], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[17], lane, 64),
                    if 0b0101_1010 & (1u64 << lane) != 0 {
                        0x0123_4567_89AB_CDEF
                    } else {
                        0
                    }
                );
            }
        }

        // Mask-to-vector broadcasts zero-extend only the low byte/word of the
        // K source before repeating it at qword/dword granularity.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[7] = 0x1234;
            x86.k[3] = 0x12_3456;
        }
        execute_lifted_x86(&[0x62, 0xE2, 0xFE, 0x48, 0x2A, 0xCF], &mut ctx, &mut memory);
        execute_lifted_x86(&[0x62, 0xF2, 0x7E, 0x28, 0x3A, 0xD3], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8u8 {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[17], lane, 64), 0x34);
            }
            for lane in 0..8u8 {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[2], lane, 32), 0x3456);
            }
        }

        // Compressed displacement is scaled by the complete 16-byte tuple.
        let tuple = [0x1111_1111u32, 0x2222_2222, 0x3333_3333, 0x4444_4444];
        for (lane, value) in tuple.into_iter().enumerate() {
            memory
                .write(0x100 + lane as u64 * 4, &value.to_le_bytes())
                .unwrap();
        }
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x80);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[1] = u64::MAX;
        }
        execute_lifted_x86(
            &[0x62, 0xF2, 0x7D, 0xC9, 0x1A, 0x48, 0x08],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                    u64::from(tuple[usize::from(lane % 4)])
                );
            }
        }

        // Type E6 suppresses the complete tuple read for an all-zero effective
        // mask. Any active destination lane requires the complete tuple and
        // faults before the architectural destination changes.
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3F8);
        let sentinel = [0xA5A5_A5A5_A5A5_A5A5; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 0;
        }
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0xC9, 0x1A, 0x08], &mut ctx, &mut memory);
        assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], [0; 16]);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 1;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0xC9, 0x1A, 0x08], &mut ctx, &mut memory);
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
    fn smir_bextr_bzhi_result_ops_preserve_x86_flags_and_edge_counts() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let flags = 0x2 | 0x1 | 0x40 | 0x80 | 0x800;

        let (value, got_flags) = exec_x86_rax_op(
            OpKind::Bextr {
                dst: rax,
                src: rax,
                control: rcx,
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            0xf0f0,
            (8 << 8) | 4,
            flags,
        );
        assert_eq!(value, 0x0f);
        assert_eq!(got_flags, flags);

        let (value, got_flags) = exec_x86_rax_op(
            OpKind::Bextr {
                dst: rax,
                src: rax,
                control: rcx,
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            0x1234_5678,
            64,
            flags,
        );
        assert_eq!(value, 0);
        assert_eq!(got_flags, flags);

        let (value, got_flags) = exec_x86_rax_op(
            OpKind::Bzhi {
                dst: rax,
                src: rax,
                index: rcx,
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            0xffff_1234_5678_9abc,
            16,
            flags,
        );
        assert_eq!(value, 0x9abc);
        assert_eq!(got_flags, flags);

        let (value, got_flags) = exec_x86_rax_op(
            OpKind::Bzhi {
                dst: rax,
                src: rax,
                index: rcx,
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            0xffff_1234_5678_9abc,
            64,
            flags,
        );
        assert_eq!(value, 0xffff_1234_5678_9abc);
        assert_eq!(got_flags, flags);
    }
    #[test]
    fn smir_andn_updates_only_defined_x86_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        const CF: u64 = 1 << 0;
        const PF: u64 = 1 << 2;
        const AF: u64 = 1 << 4;
        const ZF: u64 = 1 << 6;
        const SF: u64 = 1 << 7;
        const OF: u64 = 1 << 11;
        let defined = FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF);
        let initial = 0x2 | CF | PF | AF | ZF | OF;

        let (value, got_flags) = exec_x86_rax_op(
            OpKind::AndNot {
                dst: rax,
                src1: rax,
                src2: SrcOperand::Reg(rcx),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(defined),
            },
            0x8000_0000_0000_0000,
            0,
            initial,
        );
        assert_eq!(value, 0x8000_0000_0000_0000);
        assert_eq!(got_flags & CF, 0, "ANDN clears CF");
        assert_eq!(got_flags & ZF, 0, "nonzero ANDN clears ZF");
        assert_ne!(got_flags & SF, 0, "ANDN sets SF from its result");
        assert_eq!(got_flags & OF, 0, "ANDN clears OF");
        assert_ne!(got_flags & PF, 0, "ANDN preserves undefined PF");
        assert_ne!(got_flags & AF, 0, "ANDN preserves undefined AF");

        let (value, got_flags) = exec_x86_rax_op(
            OpKind::AndNot {
                dst: rax,
                src1: rax,
                src2: SrcOperand::Reg(rcx),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            0xffff_ffff,
            0xffff_fff0,
            initial,
        );
        assert_eq!(value, 0x0f);
        assert_eq!(got_flags, initial, "APX NF ANDN preserves every flag");
    }
    #[test]
    fn test_vwidenmul_byte_layout() {
        // V0 bytes = [3,7,3,7,...], V1 = [5,2,5,2,...].
        // lo.h[i] = even_byte products = 3*5 = 15; hi.h[i] = odd = 7*2 = 14.
        let v0 = [0x0703_0703_0703_0703u64; 16];
        let v1 = [0x0205_0205_0205_0205u64; 16];
        let (lo, hi) = run_widenmul(v0, v1, VecElementType::I8, true, true);
        assert_eq!(lo, [0x000F_000F_000F_000Fu64; 16]); // 15 per halfword
        assert_eq!(hi, [0x000E_000E_000E_000Eu64; 16]); // 14 per halfword
    }
    #[test]
    fn test_vwidenmul_half_to_word() {
        // half*half -> word pair. V0 half = 0x0003, V1 half = 0x0005 -> 15.
        let v0 = [0x0003_0003_0003_0003u64; 16];
        let v1 = [0x0005_0005_0005_0005u64; 16];
        let (lo, hi) = run_widenmul(v0, v1, VecElementType::I16, true, true);
        assert_eq!(lo, [0x0000_000F_0000_000Fu64; 16]); // word = 15
        assert_eq!(hi, [0x0000_000F_0000_000Fu64; 16]);
    }
    #[test]
    fn test_vnarrowshiftsat_wh_interleave() {
        // word->half, signed src, no round, no shift (rt=0), saturate signed.
        // V0 (src_lo/Vv) word = 0x0000_1234, V1 (src_hi/Vu) word = 0x0000_5678.
        // out half[2i] = sat(0x1234) = 0x1234 (even <- Vv);
        // out half[2i+1] = sat(0x5678) = 0x5678 (odd <- Vu).
        let v0 = [0x0000_1234_0000_1234u64; 16];
        let v1 = [0x0000_5678_0000_5678u64; 16];
        let out = run_narrow_shift_sat(v0, v1, 0, VecElementType::I32, true, false, 1);
        // each 32-bit out word = [Vv-half | Vu-half<<16] = 0x5678_1234
        assert_eq!(out, [0x5678_1234_5678_1234u64; 16]);
    }
    #[test]
    fn test_vwidenaddsub_byte_layout() {
        // V0 bytes = [3,7,...], V1 = [5,2,...]. Even-byte add -> lo.h = 3+5=8,
        // odd-byte add -> hi.h = 7+2=9. Sub: lo.h = 3-5=-2=0xFFFE, hi.h=7-2=5.
        let v0 = [0x0703_0703_0703_0703u64; 16];
        let v1 = [0x0205_0205_0205_0205u64; 16];
        let run = |sub: bool, s1: bool, s2: bool, acc: bool| -> ([u64; 16], [u64; 16]) {
            let mut ctx = SmirContext::new_hexagon();
            let mut memory = FlatMemory::new(0x1000);
            let interp = SmirInterpreter::new();
            if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
                hex.set_v(0, v0);
                hex.set_v(1, v1);
                hex.set_v(2, [0u64; 16]);
                hex.set_v(3, [0u64; 16]);
            }
            let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
            let block = SmirBlock {
                id: BlockId(0),
                guest_pc: 0x1000,
                phis: vec![],
                ops: vec![SmirOp {
                    id: OpId(0),
                    guest_pc: 0x1000,
                    kind: OpKind::VWidenAddSub {
                        dst_lo: mkv(2),
                        dst_hi: mkv(3),
                        src1: mkv(0),
                        src2: mkv(1),
                        src_elem: VecElementType::I8,
                        signed1: s1,
                        signed2: s2,
                        sub,
                        acc,
                    },
                    x86_hint: None,
                }],
                terminator: Terminator::Trap {
                    kind: TrapKind::Halt,
                },
                exec_count: 0,
            };
            interp.execute_block(&mut ctx, &mut memory, &block);
            match &ctx.arch_regs {
                ArchRegState::Hexagon(hex) => (hex.get_v(2), hex.get_v(3)),
                _ => panic!("not hexagon"),
            }
        };
        let (lo, hi) = run(false, false, false, false);
        assert_eq!(lo, [0x0008_0008_0008_0008u64; 16]); // 3+5=8
        assert_eq!(hi, [0x0009_0009_0009_0009u64; 16]); // 7+2=9
        let (lo, hi) = run(true, false, false, false);
        assert_eq!(lo, [0xFFFE_FFFE_FFFE_FFFEu64; 16]); // 3-5=-2
        assert_eq!(hi, [0x0005_0005_0005_0005u64; 16]); // 7-2=5
    }
    #[test]
    fn test_vreducemul_byte4_to_word() {
        // 4-tap byte dot product -> word. Every byte of V0 = 2, V1 = 3.
        // Each word lane = sum of 4 products = 4 * (2*3) = 24 = 0x18.
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
            hex.set_v(0, [0x0202_0202_0202_0202u64; 16]);
            hex.set_v(1, [0x0303_0303_0303_0303u64; 16]);
        }
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let mk = |op| SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: op,
                x86_hint: None,
            }],
            terminator: Terminator::Trap {
                kind: TrapKind::Halt,
            },
            exec_count: 0,
        };
        interp.execute_block(
            &mut ctx,
            &mut memory,
            &mk(OpKind::VReduceMul {
                dst: mkv(2),
                src1: mkv(0),
                src2: mkv(1),
                src1_elem: VecElementType::I8,
                src2_elem: VecElementType::I8,
                out_elem: VecElementType::I32,
                taps: 4,
                sat: false,
                set_ovf: false,
                signed1: false,
                signed2: false,
                acc: false,
            }),
        );
        if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
            assert_eq!(hex.get_v(2), [0x0000_0018_0000_0018u64; 16]); // word = 24
        }
        // Accumulate: dst already holds 24 per word; +24 -> 48 = 0x30.
        interp.execute_block(
            &mut ctx,
            &mut memory,
            &mk(OpKind::VReduceMul {
                dst: mkv(2),
                src1: mkv(0),
                src2: mkv(1),
                src1_elem: VecElementType::I8,
                src2_elem: VecElementType::I8,
                out_elem: VecElementType::I32,
                taps: 4,
                sat: false,
                set_ovf: false,
                signed1: false,
                signed2: false,
                acc: true,
            }),
        );
        if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
            assert_eq!(hex.get_v(2), [0x0000_0030_0000_0030u64; 16]); // word = 48
        }
    }
    #[test]
    fn test_vwidenext_interleave_zero() {
        // vzb: every byte = 0xAB. Interleaved zero-extend byte->half.
        // lo.h[i] = ZE(byte 2i) = 0x00AB; hi.h[i] = ZE(byte 2i+1) = 0x00AB.
        let (lo, hi) = run_widenext(
            [0xABAB_ABAB_ABAB_ABABu64; 16],
            VecElementType::I8,
            false,
            true,
        );
        assert_eq!(lo, [0x00AB_00AB_00AB_00ABu64; 16]);
        assert_eq!(hi, [0x00AB_00AB_00AB_00ABu64; 16]);
    }
    #[test]
    fn test_vwidenext_interleave_sign() {
        // vsb: every byte = 0x80 (-128). Sign-extend byte->half = 0xFF80.
        let (lo, hi) = run_widenext(
            [0x8080_8080_8080_8080u64; 16],
            VecElementType::I8,
            true,
            true,
        );
        assert_eq!(lo, [0xFF80_FF80_FF80_FF80u64; 16]);
        assert_eq!(hi, [0xFF80_FF80_FF80_FF80u64; 16]);
    }
    #[test]
    fn vshuffle_uses_explicit_lanes_two_source_indices_and_zeroes_inactive_state() {
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
            let mut first = [u64::MAX; 16];
            first[0] = 0x0013_0012_0011_0010;
            let mut second = [u64::MAX; 16];
            second[0] = 0x0023_0022_0021_0020;
            let mut indices = [0u64; 16];
            indices[0] = 0x0007_0003_0004_0000;
            hex.set_v(0, first);
            hex.set_v(1, second);
            hex.set_v(3, indices);
            hex.set_v(2, [0xA5A5_A5A5_A5A5_A5A5; 16]);
        }
        let block = SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp::new(
                OpId(0),
                0x1000,
                OpKind::VShuffle {
                    dst: mkv(2),
                    src1: mkv(0),
                    src2: Some(mkv(1)),
                    indices: mkv(3),
                    elem: VecElementType::I16,
                    lanes: 4,
                },
            )],
            terminator: Terminator::Trap {
                kind: TrapKind::Halt,
            },
            exec_count: 0,
        };
        SmirInterpreter::new().execute_block(&mut ctx, &mut memory, &block);
        if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
            assert_eq!(hex.get_v(2)[0], 0x0023_0013_0020_0010);
            assert!(hex.get_v(2)[1..].iter().all(|word| *word == 0));
        }
    }
    #[test]
    fn vinterleave_selects_halves_independently_in_each_lane_block() {
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let mut first = [0u64; 16];
        first[..4].copy_from_slice(&[
            0x1111_1111_0000_0000,
            0x3333_3333_2222_2222,
            0x5555_5555_4444_4444,
            0x7777_7777_6666_6666,
        ]);
        let mut second = [0u64; 16];
        second[..4].copy_from_slice(&[
            0xBBBB_BBBB_AAAA_AAAA,
            0xDDDD_DDDD_CCCC_CCCC,
            0xFFFF_FFFF_EEEE_EEEE,
            0x9999_9999_8888_8888,
        ]);

        for (high, expected) in [
            (
                false,
                [
                    0xAAAA_AAAA_0000_0000,
                    0xBBBB_BBBB_1111_1111,
                    0xEEEE_EEEE_4444_4444,
                    0xFFFF_FFFF_5555_5555,
                ],
            ),
            (
                true,
                [
                    0xCCCC_CCCC_2222_2222,
                    0xDDDD_DDDD_3333_3333,
                    0x8888_8888_6666_6666,
                    0x9999_9999_7777_7777,
                ],
            ),
        ] {
            let out = run_vec2(
                first,
                second,
                OpKind::VInterleave {
                    dst: mkv(2),
                    src1: mkv(0),
                    src2: mkv(1),
                    elem: VecElementType::I32,
                    lanes: 8,
                    block_lanes: 4,
                    high,
                },
            );
            assert_eq!(out[..4], expected);
            assert!(out[4..].iter().all(|word| *word == 0));
        }
    }
    #[test]
    fn test_vshuffle2_byte_roundtrip() {
        // shuffle then deal must be identity. Use a distinguishable per-byte pattern.
        let mut v0 = [0u64; 16];
        for (i, w) in v0.iter_mut().enumerate() {
            // each byte = its global index (mod 256)
            let mut x = 0u64;
            for b in 0..8 {
                x |= (((i * 8 + b) as u64) & 0xff) << (b * 8);
            }
            *w = x;
        }
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        // shuffle V0 -> V2
        let shuffled = run_vec2(
            v0,
            [0u64; 16],
            OpKind::VShuffle2 {
                dst: mkv(2),
                src: mkv(0),
                elem: VecElementType::I8,
                deal: false,
            },
        );
        // deal the shuffled value -> should recover v0
        let dealt = run_vec2(
            shuffled,
            [0u64; 16],
            OpKind::VShuffle2 {
                dst: mkv(2),
                src: mkv(0),
                elem: VecElementType::I8,
                deal: true,
            },
        );
        assert_eq!(dealt, v0, "deal(shuffle(x)) must equal x");
        // explicit check: shuffle out[0]=src.b[0], out[1]=src.b[64].
        assert_eq!((shuffled[0] & 0xff) as u8, 0); // src byte 0
        assert_eq!(((shuffled[0] >> 8) & 0xff) as u8, 64); // src byte 64
    }
    #[test]
    fn test_vshuffleeo_even_byte() {
        // vshuffeb: out.b[2i] = Vv.b[2i], out.b[2i+1] = Vu.b[2i].
        // V0(=Vu) halfwords = 0x__11 (byte0=0x11), V1(=Vv) = 0x__22 (byte0=0x22).
        let v0 = [0xAA11_AA11_AA11_AA11u64; 16];
        let v1 = [0xBB22_BB22_BB22_BB22u64; 16];
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let out = run_vec2(
            v0,
            v1,
            OpKind::VShuffleEO {
                dst: mkv(2),
                src1: mkv(0),
                src2: mkv(1),
                elem: VecElementType::I8,
                odd: false,
            },
        );
        // every output halfword = Vv.b0(0x22) | Vu.b0(0x11)<<8 = 0x1122.
        assert_eq!(out, [0x1122_1122_1122_1122u64; 16]);
    }
    #[test]
    fn test_vbroadcast_gpr_to_words() {
        // Splat GPR R5 = 0xDEADBEEF into every word lane of V2.
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::R(5)), 0xDEAD_BEEF);
        let block = SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: OpKind::VBroadcast {
                    dst: VReg::Arch(ArchReg::Hexagon(HexagonReg::V(2))),
                    scalar: VReg::Arch(ArchReg::Hexagon(HexagonReg::R(5))),
                    elem: VecElementType::I32,
                    lanes: 32,
                },
                x86_hint: None,
            }],
            terminator: Terminator::Trap {
                kind: TrapKind::Halt,
            },
            exec_count: 0,
        };
        interp.execute_block(&mut ctx, &mut memory, &block);
        if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
            assert_eq!(hex.get_v(2), [0xDEAD_BEEF_DEAD_BEEFu64; 16]);
        }
    }
    #[test]
    fn test_vlanecond_byte() {
        // if (Q0) V0.b += V1.b: byte0 Q-set -> add, byte1 Q-clear -> unchanged.
        let mut vx = [0u64; 16];
        vx[0] = 0x0000_0000_0000_2010; // byte0=0x10, byte1=0x20
        let mut vu = [0u64; 16];
        vu[0] = 0x0000_0000_0000_0505; // byte0=0x05, byte1=0x05
        let mut q = [0u64; 16];
        q[0] = 0b01; // only Q bit0 set (covers byte0)
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let out = run_lanecond(
            vx,
            vu,
            q,
            OpKind::VLaneCond {
                dst: mkv(0),
                src: mkv(1),
                mask_q: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(0))),
                elem: VecElementType::I8,
                lanes: 128,
                sub: false,
                negate: false,
            },
        );
        // byte0: 0x10+0x05=0x15 (Q set); byte1: 0x20 unchanged (Q clear).
        assert_eq!(out[0] & 0xffff, 0x2015);
        // negate: byte0 unchanged, byte1 adds.
        let out_n = run_lanecond(
            vx,
            vu,
            q,
            OpKind::VLaneCond {
                dst: mkv(0),
                src: mkv(1),
                mask_q: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(0))),
                elem: VecElementType::I8,
                lanes: 128,
                sub: false,
                negate: true,
            },
        );
        assert_eq!(out_n[0] & 0xffff, 0x2510); // byte0=0x10, byte1=0x20+0x05=0x25
    }
    #[test]
    fn lifted_mmx_pmovmskb_extracts_byte_signs_and_enters_mmx_state() {
        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let flags_before = 0xCD7;
        let mut memory = FlatMemory::new(0x100);
        let mut ctx = SmirContext::new_x86_64();
        ctx.write_vreg(r8, u64::MAX);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            // Little-endian byte sign bits are 10101010b from byte 7 to 0.
            x86.mm[1] = 0x80_7F_FF_00_81_01_FE_7E;
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 3 << 11;
        }

        let exit = execute_lifted_x86(&[0x4C, 0x0F, 0xD7, 0xC1], &mut ctx, &mut memory);
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(ctx.read_vreg(r8), 0xAA);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[1], 0x80_7F_FF_00_81_01_FE_7E);
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 3 << 11);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_mmx_pinsrw_pextrw_execute_rex_lanes_state_memory_and_faults() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
        let flags_before = 0xCD7;
        let mut memory = FlatMemory::new(0x100);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[1] = 0x4444_3333_2222_1111;
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 6 << 11;
        }
        ctx.write_vreg(r8, 0xDEAD_BEEF_CAFE_A1B2);

        // REX.B selects R8 as the scalar source, REX.R is ignored for MM1,
        // and only imm8[1:0] selects one of four words.
        execute_lifted_x86(&[0x45, 0x0F, 0xC4, 0xC8, 0xFF], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[1], 0xA1B2_3333_2222_1111);
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 6 << 11);
        }

        // REX.R selects R9 as the destination, REX.B is ignored for MM1, and
        // PEXTRW clears every destination bit above the selected word.
        ctx.write_vreg(r9, u64::MAX);
        execute_lifted_x86(&[0x45, 0x0F, 0xC5, 0xC9, 0xFE], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(r9), 0x3333);

        // An unaligned m16 source reads exactly two bytes before entering MMX
        // state or changing its destination.
        memory.write(0x41, &0x7788u16.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x40);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[1] = 0x4444_3333_2222_1111;
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0xC4, 0x48, 0x01, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[1], 0x4444_3333_2222_7788);
            assert_eq!(x86.x87.tag_word, 0);
        }

        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[1] = 0xA5A5_5A5A_C3C3_3C3C;
            x86.x87.tag_word = 0xFFFF;
        }
        let fault = execute_lifted_x86(&[0x0F, 0xC4, 0x08, 0x03], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[1], 0xA5A5_5A5A_C3C3_3C3C);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
        assert_eq!(ctx.read_vreg(r8), 0xDEAD_BEEF_CAFE_A1B2);
    }
    #[test]
    fn lifted_mmx_movq_executes_directions_rex_memory_state_and_faults() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let flags_before = 0xCD7;
        let mut memory = FlatMemory::new(0x100);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xAAAA_AAAA_AAAA_AAAA;
            x86.mm[1] = 0x0123_4567_89AB_CDEF;
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 4 << 11;
        }

        // REX.R/REX.B do not extend either three-bit MM field.
        execute_lifted_x86(&[0x45, 0x0F, 0x6F, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0x0123_4567_89AB_CDEF);
            assert_eq!(x86.mm[1], 0x0123_4567_89AB_CDEF);
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 4 << 11);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xFEDC_BA98_7654_3210;
        }
        execute_lifted_x86(&[0x45, 0x0F, 0x7F, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[1], 0xFEDC_BA98_7654_3210);
        }

        let memory_value = 0x8877_6655_4433_2211u64;
        memory.write(0x41, &memory_value.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x40);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x6F, 0x48, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[1], memory_value);
            assert_eq!(x86.x87.tag_word, 0);
        }
        ctx.write_vreg(rax, 0x50);
        execute_lifted_x86(&[0x0F, 0x7F, 0x48, 0x01], &mut ctx, &mut memory);
        let mut stored = [0u8; 8];
        memory.read(0x51, &mut stored).unwrap();
        assert_eq!(u64::from_le_bytes(stored), memory_value);

        for (bytes, write) in [
            (&[0x0F, 0x6F, 0x08][..], false),
            (&[0x0F, 0x7F, 0x08][..], true),
        ] {
            ctx.write_vreg(rax, 0x100);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mm[1] = 0xA5A5_5A5A_C3C3_3C3C;
                x86.x87.tag_word = 0xFFFF;
            }
            let fault = execute_lifted_x86(bytes, &mut ctx, &mut memory);
            assert!(matches!(
                fault,
                BlockResult::Exit(ExitReason::MemoryFault {
                    write: actual,
                    ..
                }) if actual == write
            ));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.mm[1], 0xA5A5_5A5A_C3C3_3C3C);
                assert_eq!(x86.x87.tag_word, 0xFFFF);
            }
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_legacy_and_vex_packed_integer_compares_execute_signedness_widths_and_faults() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let flags_before = 0xCD7;
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let mut memory = FlatMemory::new(0x400);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        let mmx_cases = [
            (
                0x64,
                0x3412_00FF_0505_7F80,
                0x3512_01FF_0605_807F,
                0x0000_0000_0000_FF00,
            ),
            (
                0x65,
                0xFFFE_FFFF_7FFF_8000,
                0xFFFF_FFFE_8000_7FFF,
                0x0000_FFFF_FFFF_0000,
            ),
            (
                0x66,
                0x7FFF_FFFF_8000_0000,
                0x8000_0000_7FFF_FFFF,
                0xFFFF_FFFF_0000_0000,
            ),
            (
                0x74,
                0xAA22_CC44_5566_7788,
                0xAA00_CCFF_5500_77FF,
                0xFF00_FF00_FF00_FF00,
            ),
            (
                0x75,
                0xAAAA_BBBB_CCCC_DDDD,
                0xAAAA_0000_CCCC_1111,
                0xFFFF_0000_FFFF_0000,
            ),
            (
                0x76,
                0xAAAA_BBBB_CCCC_DDDD,
                0xAAAA_BBBB_1111_2222,
                0xFFFF_FFFF_0000_0000,
            ),
        ];
        for (opcode, lhs, rhs, expected) in mmx_cases {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mm[0] = lhs;
                x86.mm[1] = rhs;
                x86.x87.tag_word = 0xFFFF;
                x86.x87.status_word = 2 << 11;
            }
            execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.mm[0], expected, "MMX {opcode:02X}");
                assert_eq!(x86.x87.tag_word, 0);
                assert_eq!(x86.x87.status_word & 0x3800, 2 << 11);
            }
        }

        // The MMX memory form consumes exactly 8 bytes before entering MMX
        // state and committing the compare result.
        memory
            .write(0x3F8, &0xAA00_CCFF_5500_77FFu64.to_le_bytes())
            .unwrap();
        ctx.write_vreg(rax, 0x3F8);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xAA22_CC44_5566_7788;
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x74, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xFF00_FF00_FF00_FF00);
            assert_eq!(x86.x87.tag_word, 0);
        }

        // A faulting source changes neither the destination nor the x87 tags.
        ctx.write_vreg(rax, 0x1000);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xA5A5_5A5A_C3C3_3C3C;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0x74, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xA5A5_5A5A_C3C3_3C3C);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        let signed_cases = [
            (
                &[0x66, 0x0F, 0x64, 0xC1][..],
                0x3412_00FF_0505_7F80,
                0x3512_01FF_0605_807F,
                0x0000_0000_0000_FF00,
                0,
            ),
            (
                &[0x66, 0x0F, 0x65, 0xC1][..],
                0xFFFE_FFFF_7FFF_8000,
                0xFFFF_FFFE_8000_7FFF,
                0x0000_FFFF_FFFF_0000,
                0,
            ),
            (
                &[0x66, 0x0F, 0x66, 0xC1][..],
                0x7FFF_FFFF_8000_0000,
                0x8000_0000_7FFF_FFFF,
                0xFFFF_FFFF_0000_0000,
                0,
            ),
            (
                &[0x66, 0x0F, 0x38, 0x37, 0xC1][..],
                u64::MAX,
                u64::MAX - 1,
                u64::MAX,
                0,
            ),
        ];
        for (bytes, lhs, rhs, expected0, expected1) in signed_cases {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [upper; 16];
                x86.xmm[0][0] = lhs;
                x86.xmm[0][1] = 0;
                x86.xmm[1][0] = rhs;
                x86.xmm[1][1] = 1;
            }
            execute_lifted_x86(bytes, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[0][0], expected0, "{bytes:02X?}");
                assert_eq!(x86.xmm[0][1], expected1, "{bytes:02X?}");
                assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
            }
        }

        // Equality produces an all-ones lane, including qwords, and treats
        // signed zero identically because it is an integer comparison.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [upper; 16];
            x86.xmm[0][0] = 0x0123_4567_89AB_CDEF;
            x86.xmm[0][1] = 0x8000_0000_0000_0000;
            x86.xmm[1][0] = 0x0123_4567_89AB_CDEF;
            x86.xmm[1][1] = 0;
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x29, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], u64::MAX);
            assert_eq!(x86.xmm[0][1], 0);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        // VEX.256 compares all 256 bits and clears the shared backing state
        // above bit 255. Destination/source aliasing is safe because operands
        // are read before the single VCmp write.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1][..4].copy_from_slice(&[
                0xA000_0000_0000_0001,
                0xB000_0000_0000_0002,
                0xC000_0000_0000_0003,
                0xD000_0000_0000_0004,
            ]);
            x86.xmm[2][..4].copy_from_slice(&[
                0xA100_0000_0000_0001,
                0xB000_0000_0000_0009,
                0xC000_0000_0000_0003,
                0xD100_0000_0000_0008,
            ]);
        }
        execute_lifted_x86(&[0xC5, 0xF5, 0x76, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                &x86.xmm[0][..4],
                &[0x0000_0000_FFFF_FFFF, 0xFFFF_FFFF_0000_0000, u64::MAX, 0,]
            );
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }

        // EVEX fixed comparisons write a k-mask, apply the input writemask,
        // zero inactive/high result bits, and remain correct when dst==mask.
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 2u64 << 32 | 1;
            x86.xmm[1][1] = 4u64 << 32 | 3;
            x86.xmm[2][0] = 9u64 << 32 | 1;
            x86.xmm[2][1] = 4u64 << 32 | 3;
        }
        ctx.write_vreg(k1, 0b1011);
        ctx.write_vreg(k2, u64::MAX);
        execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x09, 0x76, 0xD2], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(k2), 0b1001);

        ctx.write_vreg(k1, 0b1111);
        execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x09, 0x76, 0xCA], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(k1), 0b1101);

        // A zero writemask suppresses every memory access. Enabling one lane
        // exposes the fault, without committing the k destination.
        ctx.write_vreg(rax, 0x1000);
        ctx.write_vreg(k1, 0);
        ctx.write_vreg(k2, 0xDEAD_BEEF);
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x09, 0x76, 0x10], &mut ctx, &mut memory);
        assert!(!matches!(
            suppressed,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        assert_eq!(ctx.read_vreg(k2), 0);

        ctx.write_vreg(k1, 1);
        ctx.write_vreg(k2, 0xDEAD_BEEF);
        let exposed =
            execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x09, 0x76, 0x10], &mut ctx, &mut memory);
        assert!(matches!(
            exposed,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        assert_eq!(ctx.read_vreg(k2), 0xDEAD_BEEF);

        // A masked EVEX.512 broadcast reads one dword for each active lane and
        // compares it against all selected source lanes.
        memory.write(0x100, &7u32.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k1, (1 << 0) | (1 << 5) | (1 << 15));
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0; 16];
            x86.xmm[1][0] = 7;
            x86.xmm[1][2] = 7u64 << 32;
            x86.xmm[1][7] = 8u64 << 32;
        }
        execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x59, 0x76, 0x10], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(k2), (1 << 0) | (1 << 5));

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

        // A memory fault precedes the architectural compare destination write.
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        ctx.write_vreg(rax, 0x1000);
        let exit = execute_lifted_x86(&[0xC5, 0xF5, 0x74, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }
    }
    #[test]
    fn lifted_legacy_and_vex_packed_unpacks_interleave_per_128_bit_lane() {
        fn seeded(bytes: &[u8], fill: u64) -> VecValue {
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

        fn expected(
            first: &[u8],
            second: &[u8],
            elem: usize,
            block_bytes: usize,
            high: bool,
        ) -> Vec<u8> {
            let mut result = Vec::with_capacity(first.len());
            for (a, b) in first
                .chunks_exact(block_bytes)
                .zip(second.chunks_exact(block_bytes))
            {
                let half = block_bytes / 2;
                let start = if high { half } else { 0 };
                for offset in (start..start + half).step_by(elem) {
                    result.extend_from_slice(&a[offset..offset + elem]);
                    result.extend_from_slice(&b[offset..offset + elem]);
                }
            }
            result
        }

        let first = (1u8..=32).collect::<Vec<_>>();
        let second = (0x81u8..=0xA0).collect::<Vec<_>>();
        let cases = [
            (0x60, 1, false),
            (0x61, 2, false),
            (0x62, 4, false),
            (0x6C, 8, false),
            (0x68, 1, true),
            (0x69, 2, true),
            (0x6A, 4, true),
            (0x6D, 8, true),
        ];
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for (opcode, elem, high) in [
            (0x60, 1, false),
            (0x61, 2, false),
            (0x62, 4, false),
            (0x68, 1, true),
            (0x69, 2, true),
            (0x6A, 4, true),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
                x86.mm[1] = u64::from_le_bytes(second[..8].try_into().unwrap());
                x86.x87.tag_word = 0xFFFF;
                x86.x87.status_word = 4 << 11;
            }
            execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    x86.mm[0].to_le_bytes(),
                    expected(&first[..8], &second[..8], elem, 8, high).as_slice(),
                    "MMX opcode {opcode:02X}"
                );
                assert_eq!(x86.x87.tag_word, 0);
                assert_eq!(x86.x87.status_word & 0x3800, 4 << 11);
            }
        }

        // Low MMX memory forms access only m32. Placing the operand at the
        // exact end of memory distinguishes that architectural width from an
        // incorrect 8-byte read.
        let low_memory = [0xA1, 0xA2, 0xA3, 0xA4];
        memory.write(0x3FC, &low_memory).unwrap();
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        ctx.write_vreg(rax, 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x60, 0x00], &mut ctx, &mut memory);
        let mut low_source = [0u8; 8];
        low_source[..4].copy_from_slice(&low_memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0].to_le_bytes(),
                expected(&first[..8], &low_source, 1, 8, false).as_slice()
            );
            assert_eq!(x86.x87.tag_word, 0);
        }

        // High MMX memory forms consume the complete m64 source.
        let high_memory = [0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8];
        memory.write(0x3F8, &high_memory).unwrap();
        ctx.write_vreg(rax, 0x3F8);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x68, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0].to_le_bytes(),
                expected(&first[..8], &high_memory, 1, 8, true).as_slice()
            );
            assert_eq!(x86.x87.tag_word, 0);
        }

        // A source fault precedes both the MMX-state transition and result.
        ctx.write_vreg(rax, 0x1000);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xA5A5_5A5A_C3C3_3C3C;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0x60, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xA5A5_5A5A_C3C3_3C3C);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        for (opcode, elem, high) in cases {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = seeded(&first[..16], upper);
                x86.xmm[1] = seeded(&second[..16], 0);
            }
            execute_lifted_x86(&[0x66, 0x0F, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[0], 16),
                    expected(&first[..16], &second[..16], elem, 16, high),
                    "legacy opcode {opcode:02X}"
                );
                assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
            }

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [u64::MAX; 16];
                x86.xmm[1] = seeded(&first, 0);
                x86.xmm[2] = seeded(&second, 0);
            }
            execute_lifted_x86(&[0xC5, 0xF5, opcode, 0xC2], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[0], 32),
                    expected(&first, &second, elem, 16, high),
                    "VEX opcode {opcode:02X}"
                );
                assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
            }
        }

        // Same-register legacy sources are captured before the destination
        // merge, so each selected element is duplicated exactly.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&first[..16], upper);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x60, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                expected(&first[..16], &first[..16], 1, 16, false)
            );
        }

        // EVEX merge/zero masks apply to output elements after each 128-bit
        // lane-local interleave and clear backing state above VL.
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let raw = expected(&first, &second, 2, 16, true);
        let mask = 0xA55Au64;
        for (p2, zeroing) in [(0x29, false), (0xA9, true)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = seeded(&[0xEE; 32], u64::MAX);
                x86.xmm[1] = seeded(&first, 0);
                x86.xmm[2] = seeded(&second, 0);
            }
            ctx.write_vreg(k1, mask);
            execute_lifted_x86(&[0x62, 0xF1, 0x75, p2, 0x69, 0xC2], &mut ctx, &mut memory);
            let mut masked = Vec::with_capacity(32);
            for lane in 0..16 {
                if mask >> lane & 1 != 0 {
                    masked.extend_from_slice(&raw[lane * 2..lane * 2 + 2]);
                } else if zeroing {
                    masked.extend_from_slice(&[0, 0]);
                } else {
                    masked.extend_from_slice(&[0xEE, 0xEE]);
                }
            }
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 32), masked);
                assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
            }
        }

        // EVEX integer unpack is E4NF/E4NF.nb: every memory form performs the
        // complete vector access before masking, including even-only or
        // all-zero masks.
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&first, 0);
        }
        ctx.write_vreg(rax, 0x1000);
        for mask in [0x55, 0x00] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = sentinel;
            }
            ctx.write_vreg(k1, mask);
            let fault =
                execute_lifted_x86(&[0x62, 0xF1, 0xF5, 0x49, 0x6D, 0x00], &mut ctx, &mut memory);
            assert!(matches!(
                fault,
                BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
            ));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[0], sentinel);
            }
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

        // VEX.256 memory forms require the complete 32-byte operand before any
        // architectural destination write.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        ctx.write_vreg(rax, 0x3F0);
        let exit = execute_lifted_x86(&[0xC5, 0xF5, 0x60, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }
    }
    #[test]
    fn lifted_saturating_packs_execute_lane_groups_masks_and_fault_suppression() {
        fn seeded(bytes: &[u8], fill: u64) -> VecValue {
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

        fn pack_reference(
            first: &[u8],
            second: &[u8],
            src_bytes: usize,
            block_bytes: usize,
            to_unsigned: bool,
        ) -> Vec<u8> {
            let dst_bytes = src_bytes / 2;
            let block_lanes = block_bytes / src_bytes;
            let source_lanes = first.len() / src_bytes;
            let read_signed = |source: &[u8], lane: usize| -> i64 {
                let at = lane * src_bytes;
                match src_bytes {
                    2 => i16::from_le_bytes(source[at..at + 2].try_into().unwrap()) as i64,
                    4 => i32::from_le_bytes(source[at..at + 4].try_into().unwrap()) as i64,
                    _ => unreachable!(),
                }
            };
            let saturate = |value: i64| -> u64 {
                if to_unsigned {
                    value.clamp(0, (1i64 << (dst_bytes * 8)) - 1) as u64
                } else {
                    let high = (1i64 << (dst_bytes * 8 - 1)) - 1;
                    let low = -(1i64 << (dst_bytes * 8 - 1));
                    value.clamp(low, high) as u64
                }
            };
            let mut result = Vec::with_capacity(first.len());
            for block_base in (0..source_lanes).step_by(block_lanes) {
                for source in [first, second] {
                    for lane in block_base..block_base + block_lanes {
                        result.extend_from_slice(
                            &saturate(read_signed(source, lane)).to_le_bytes()[..dst_bytes],
                        );
                    }
                }
            }
            result
        }

        let words1 = [
            -400i16,
            -129,
            -128,
            -1,
            0,
            1,
            127,
            128,
            255,
            256,
            i16::MAX,
            i16::MIN,
            42,
            -42,
            300,
            -300,
        ];
        let words2 = [
            500i16, 129, 128, 2, -2, 126, -127, -500, 254, 257, 1000, -1000, 7, -7, 200, -200,
        ];
        let dwords1 = [-100_000i32, -32_769, -32_768, -1, 0, 32_767, 65_535, 65_536];
        let dwords2 = [i32::MAX, i32::MIN, 1, 32_768, 65_534, 70_000, -2, 1234];
        let words1_bytes = words1
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        let words2_bytes = words2
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        let dwords1_bytes = dwords1
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        let dwords2_bytes = dwords2
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for (opcode, first, second, src_bytes, to_unsigned) in [
            (
                0x63,
                words1_bytes.as_slice(),
                words2_bytes.as_slice(),
                2,
                false,
            ),
            (
                0x67,
                words1_bytes.as_slice(),
                words2_bytes.as_slice(),
                2,
                true,
            ),
            (
                0x6B,
                dwords1_bytes.as_slice(),
                dwords2_bytes.as_slice(),
                4,
                false,
            ),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
                x86.mm[1] = u64::from_le_bytes(second[..8].try_into().unwrap());
                x86.x87.tag_word = 0xFFFF;
                x86.x87.status_word = 6 << 11;
            }
            execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    x86.mm[0].to_le_bytes().as_slice(),
                    pack_reference(&first[..8], &second[..8], src_bytes, 8, to_unsigned),
                    "MMX opcode {opcode:02X}",
                );
                assert_eq!(x86.x87.tag_word, 0);
                assert_eq!(x86.x87.status_word & 0x3800, 6 << 11);
            }
        }

        // MMX memory packs consume exactly one 8-byte source before entering
        // MMX state and committing their destructive result.
        memory.write(0x3F8, &words2_bytes[..8]).unwrap();
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        ctx.write_vreg(rax, 0x3F8);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(words1_bytes[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x63, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0].to_le_bytes().as_slice(),
                pack_reference(&words1_bytes[..8], &words2_bytes[..8], 2, 8, false)
            );
            assert_eq!(x86.x87.tag_word, 0);
        }

        // A source fault changes neither MMX data nor the x87 tag word.
        ctx.write_vreg(rax, 0x1000);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xA5A5_5A5A_C3C3_3C3C;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0x63, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xA5A5_5A5A_C3C3_3C3C);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        for (legacy, vex, first, second, src_bytes, to_unsigned) in [
            (
                &[0x66, 0x0F, 0x63, 0xC1][..],
                &[0xC5, 0xF5, 0x63, 0xC2][..],
                words1_bytes.as_slice(),
                words2_bytes.as_slice(),
                2,
                false,
            ),
            (
                &[0x66, 0x0F, 0x67, 0xC1][..],
                &[0xC5, 0xF5, 0x67, 0xC2][..],
                words1_bytes.as_slice(),
                words2_bytes.as_slice(),
                2,
                true,
            ),
            (
                &[0x66, 0x0F, 0x6B, 0xC1][..],
                &[0xC5, 0xF5, 0x6B, 0xC2][..],
                dwords1_bytes.as_slice(),
                dwords2_bytes.as_slice(),
                4,
                false,
            ),
            (
                &[0x66, 0x0F, 0x38, 0x2B, 0xC1][..],
                &[0xC4, 0xE2, 0x75, 0x2B, 0xC2][..],
                dwords1_bytes.as_slice(),
                dwords2_bytes.as_slice(),
                4,
                true,
            ),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = seeded(&first[..16], upper);
                x86.xmm[1] = seeded(&second[..16], 0);
            }
            execute_lifted_x86(legacy, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[0], 16),
                    pack_reference(&first[..16], &second[..16], src_bytes, 16, to_unsigned),
                    "legacy {legacy:02X?}",
                );
                assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
            }

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [u64::MAX; 16];
                x86.xmm[1] = seeded(first, 0);
                x86.xmm[2] = seeded(second, 0);
            }
            execute_lifted_x86(vex, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[0], 32),
                    pack_reference(first, second, src_bytes, 16, to_unsigned),
                    "VEX {vex:02X?}",
                );
                assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
            }
        }

        // EVEX masking is applied to the packed word result, after independent
        // 128-bit groups. Merging and zeroing both clear backing state above VL.
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let raw = pack_reference(&dwords1_bytes, &dwords2_bytes, 4, 16, true);
        let mask = 0xA55Au64;
        for (p2, zeroing) in [(0x29, false), (0xA9, true)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = seeded(&[0xEE; 32], u64::MAX);
                x86.xmm[1] = seeded(&dwords1_bytes, 0);
                x86.xmm[2] = seeded(&dwords2_bytes, 0);
            }
            ctx.write_vreg(k1, mask);
            execute_lifted_x86(&[0x62, 0xF2, 0x75, p2, 0x2B, 0xC2], &mut ctx, &mut memory);
            let mut expected = Vec::with_capacity(32);
            for lane in 0..16 {
                if mask >> lane & 1 != 0 {
                    expected.extend_from_slice(&raw[lane * 2..lane * 2 + 2]);
                } else if zeroing {
                    expected.extend_from_slice(&[0, 0]);
                } else {
                    expected.extend_from_slice(&[0xEE, 0xEE]);
                }
            }
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 32), expected);
                assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
            }
        }

        // Only second-half output lanes in each 128-bit group consume the r/m
        // source. First-half-only masks suppress an invalid address; selecting
        // a second-half lane exposes the read fault before destination commit.
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        ctx.write_vreg(rax, 0x1000);
        for (insn, first_half_bit, second_half_bit) in [
            (
                &[0x62, 0xF1, 0x75, 0x49, 0x63, 0x00][..],
                1u64 << 0,
                1u64 << 8,
            ),
            (
                &[0x62, 0xF1, 0x75, 0x49, 0x6B, 0x00][..],
                1u64 << 0,
                1u64 << 4,
            ),
            (
                &[0x62, 0xF1, 0x75, 0x59, 0x6B, 0x00][..],
                1u64 << 0,
                1u64 << 4,
            ),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = sentinel;
                x86.xmm[1] = seeded(&dwords1_bytes, 0);
            }
            ctx.write_vreg(k1, first_half_bit);
            let suppressed = execute_lifted_x86(insn, &mut ctx, &mut memory);
            assert!(!matches!(
                suppressed,
                BlockResult::Exit(ExitReason::MemoryFault { .. })
            ));

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = sentinel;
            }
            ctx.write_vreg(k1, second_half_bit);
            let exposed = execute_lifted_x86(insn, &mut ctx, &mut memory);
            assert!(
                matches!(
                    exposed,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ),
                "second-source mask must expose memory fault for {insn:02X?}, got {exposed:?}"
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[0], sentinel);
            }
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

        // Non-EVEX memory forms retain their full-width all-or-fault boundary.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        ctx.write_vreg(rax, 0x3F0);
        let exit = execute_lifted_x86(&[0xC5, 0xF5, 0x63, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }
    }
    #[test]
    fn lifted_pshufb_executes_msb_zeroing_lane_locality_masks_and_faults() {
        fn seeded(bytes: &[u8], fill: u64) -> VecValue {
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

        fn reference(source: &[u8], control: &[u8]) -> Vec<u8> {
            let mut result = vec![0; source.len()];
            for block_base in (0..source.len()).step_by(16) {
                for lane in 0..16 {
                    let selector = control[block_base + lane];
                    if selector & 0x80 == 0 {
                        result[block_base + lane] =
                            source[block_base + usize::from(selector & 0x0F)];
                    }
                }
            }
            result
        }

        let source = (0x10u8..=0x4F).collect::<Vec<_>>();
        let control_block = [
            0x00, 0x01, 0x0F, 0x10, 0x1F, 0x7F, 0x80, 0x8F, 0x02, 0x0E, 0x12, 0x2E, 0xFF, 0x04,
            0x08, 0x0C,
        ];
        let control = control_block.repeat(4);
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        let mmx_source = u64::from_le_bytes([0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17]);
        let mmx_control = u64::from_le_bytes([0x00, 0x07, 0x08, 0x87, 0x02, 0x06, 0x80, 0x03]);
        let mmx_expected = [0x10, 0x17, 0x10, 0x00, 0x12, 0x16, 0x00, 0x13];

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = mmx_source;
            x86.mm[1] = mmx_control;
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 6 << 11;
        }
        execute_lifted_x86(&[0x0F, 0x38, 0x00, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0].to_le_bytes(), mmx_expected);
            assert_eq!(x86.mm[1], mmx_control);
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 6 << 11);
        }

        // The MMX control source is m64 and has no mandatory #GP alignment.
        // A faulting complete load leaves the destructive destination and the
        // x87/MMX state unchanged.
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        memory.write(0x81, &mmx_control.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x80);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = mmx_source;
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x38, 0x00, 0x40, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0].to_le_bytes(), mmx_expected);
            assert_eq!(x86.x87.tag_word, 0);
        }

        ctx.write_vreg(rax, 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0x38, 0x00, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&source[..16], upper);
            x86.xmm[1] = seeded(&control[..16], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x00, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                reference(&source[..16], &control[..16])
            );
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        // The legacy memory form checks its mandatory 16-byte alignment before
        // reading controls or modifying the destructive destination.
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        memory.write(0x101, &control[..16]).unwrap();
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x00, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        // Destructive legacy aliasing must snapshot both data and controls
        // before the first architectural destination-byte write.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&source[..16], upper);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x00, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                reference(&source[..16], &source[..16])
            );
        }

        // VEX.256 uses two independent 16-byte tables and clears all backing
        // state above bit 255.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1] = seeded(&source[..32], 0);
            x86.xmm[2] = seeded(&control[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x00, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                reference(&source[..32], &control[..32])
            );
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }

        // EVEX masking operates on the shuffled byte result. Both merge and
        // zero forms clear backing state above the selected vector length.
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let raw = reference(&source, &control);
        let mask = 0xA55A_F00F_1234_89ABu64;
        for (p2, zeroing) in [(0x49, false), (0xC9, true)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = seeded(&[0xEE; 64], u64::MAX);
                x86.xmm[1] = seeded(&source, 0);
                x86.xmm[2] = seeded(&control, 0);
            }
            ctx.write_vreg(k1, mask);
            execute_lifted_x86(&[0x62, 0xF2, 0x75, p2, 0x00, 0xC2], &mut ctx, &mut memory);
            let mut expected = Vec::with_capacity(64);
            for lane in 0..64 {
                expected.push(if mask >> lane & 1 != 0 {
                    raw[lane]
                } else if zeroing {
                    0
                } else {
                    0xEE
                });
            }
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 64), expected);
                assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
            }
        }

        // A masked memory control byte is accessed iff its corresponding
        // output byte is active. Put byte 0 at the final valid address so lane
        // 0 succeeds while lane 1 demonstrates precise fault suppression.
        memory.write(0x3FF, &[0]).unwrap();
        ctx.write_vreg(rax, 0x3FF);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&source[..16], 0);
        }
        ctx.write_vreg(k1, 1);
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x09, 0x00, 0x00], &mut ctx, &mut memory);
        assert!(!matches!(
            suppressed,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        ctx.write_vreg(k1, 1 << 1);
        let exposed =
            execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x09, 0x00, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            exposed,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

        // VEX memory controls are full-width all-or-fault loads.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        ctx.write_vreg(rax, 0x3F0);
        let fault = execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x00, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }
    }
    #[test]
    fn vdotproduct_executes_vnni_accumulation_wrapping_saturation_and_signed_words() {
        fn seeded(bytes: &[u8]) -> VecValue {
            let mut value = [0; 16];
            for (index, chunk) in bytes.chunks_exact(8).enumerate() {
                value[index] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            value
        }

        fn run(
            acc: VecValue,
            src1: VecValue,
            src2: VecValue,
            src_elem: VecElementType,
            src1_unsigned: bool,
            saturate: bool,
            masking: Option<(u64, bool)>,
        ) -> VecValue {
            let dst = VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)));
            let first = VReg::Arch(ArchReg::X86(X86Reg::Zmm(1)));
            let second = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
            let k4 = VReg::Arch(ArchReg::X86(X86Reg::K(4)));
            let mut ctx = SmirContext::new_x86_64();
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = acc;
                x86.xmm[1] = src1;
                x86.xmm[2] = src2;
                x86.k[4] = masking.map_or(0, |(mask, _)| mask);
            }
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::VDotProduct {
                    dst,
                    acc: dst,
                    src1: first,
                    src2: second,
                    mask: masking.map(|_| k4),
                    src_elem,
                    acc_elem: VecElementType::I32,
                    width: VecWidth::V128,
                    src1_unsigned,
                    saturate,
                    zeroing: masking.is_some_and(|(_, zeroing)| zeroing),
                },
            );
            builder.set_terminator(Terminator::Trap {
                kind: TrapKind::Halt,
            });
            let block = &builder.finish().blocks[0];
            let exit =
                SmirInterpreter::new().execute_block(&mut ctx, &mut FlatMemory::new(0x100), block);
            assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
            match &ctx.arch_regs {
                ArchRegState::X86_64(x86) => x86.xmm[0],
                _ => unreachable!(),
            }
        }

        let acc = [1_000i32, i32::MAX - 10, i32::MIN + 10, -100];
        let first = [
            1u8, 2, 3, 4, 255, 255, 255, 255, 255, 255, 255, 255, 0, 128, 255, 4,
        ];
        let second = [
            1i8, -2, 3, -4, 127, 127, 127, 127, -128, -128, -128, -128, -1, 1, -1, 127,
        ];
        let acc_vec = seeded(
            &acc.iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let first_vec = seeded(&first);
        let second_vec = seeded(&second.iter().map(|value| *value as u8).collect::<Vec<_>>());
        let sums = (0..4)
            .map(|lane| {
                i64::from(acc[lane])
                    + (0..4)
                        .map(|term| {
                            i64::from(first[lane * 4 + term]) * i64::from(second[lane * 4 + term])
                        })
                        .sum::<i64>()
            })
            .collect::<Vec<_>>();
        let saturated = run(
            acc_vec,
            first_vec,
            second_vec,
            VecElementType::I8,
            true,
            true,
            None,
        );
        let wrapping = run(
            acc_vec,
            first_vec,
            second_vec,
            VecElementType::I8,
            true,
            false,
            None,
        );
        for lane in 0..4 {
            assert_eq!(
                SmirInterpreter::get_lane(&saturated, lane as u8, 32) as u32,
                sums[lane].clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32 as u32
            );
            assert_eq!(
                SmirInterpreter::get_lane(&wrapping, lane as u8, 32) as u32,
                sums[lane] as i32 as u32
            );
        }
        assert!(saturated[2..].iter().all(|word| *word == 0));

        let word_acc = [17i32, -33, 44, -55];
        let word_first = [-32768i16, 32767, -123, 456, 1000, -2000, 3000, -4000];
        let word_second = [-1i16, 2, 300, -400, -30, 40, -50, 60];
        let word_result = run(
            seeded(
                &word_acc
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
            seeded(
                &word_first
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
            seeded(
                &word_second
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
            VecElementType::I16,
            false,
            false,
            None,
        );
        for lane in 0..4 {
            let expected = word_acc[lane].wrapping_add(
                i32::from(word_first[lane * 2]) * i32::from(word_second[lane * 2])
                    + i32::from(word_first[lane * 2 + 1]) * i32::from(word_second[lane * 2 + 1]),
            );
            assert_eq!(
                SmirInterpreter::get_lane(&word_result, lane as u8, 32) as u32,
                expected as u32
            );
        }

        for (zeroing, masked_off) in [(false, acc[1] as u32), (true, 0)] {
            let masked = run(
                acc_vec,
                first_vec,
                second_vec,
                VecElementType::I8,
                true,
                false,
                Some((0b0101, zeroing)),
            );
            assert_eq!(
                SmirInterpreter::get_lane(&masked, 0, 32) as u32,
                sums[0] as i32 as u32
            );
            assert_eq!(SmirInterpreter::get_lane(&masked, 1, 32) as u32, masked_off);
            assert_eq!(
                SmirInterpreter::get_lane(&masked, 2, 32) as u32,
                sums[2] as i32 as u32
            );
            assert_eq!(
                SmirInterpreter::get_lane(&masked, 3, 32) as u32,
                if zeroing { 0 } else { acc[3] as u32 }
            );
        }
    }
    #[test]
    fn lifted_pmaddubsw_executes_products_saturation_masks_aliases_and_faults() {
        fn seeded(bytes: &[u8], fill: u64) -> VecValue {
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

        fn reference(unsigned: &[u8], signed: &[u8]) -> Vec<u8> {
            unsigned
                .chunks_exact(2)
                .zip(signed.chunks_exact(2))
                .flat_map(|(a, b)| {
                    let sum = i32::from(a[0]) * i32::from(b[0] as i8)
                        + i32::from(a[1]) * i32::from(b[1] as i8);
                    (sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16).to_le_bytes()
                })
                .collect()
        }

        let mut unsigned = (0..64)
            .map(|lane| (lane * 37 + 11) as u8)
            .collect::<Vec<_>>();
        let mut signed = (0..64)
            .map(|lane| ((lane as i8).wrapping_mul(29)).wrapping_sub(93) as u8)
            .collect::<Vec<_>>();
        unsigned[0..4].copy_from_slice(&[255, 255, 255, 255]);
        signed[0..4].copy_from_slice(&[127, 127, 0x80, 0x80]);
        let expected = reference(&unsigned, &signed);
        assert_eq!(
            i16::from_le_bytes(expected[0..2].try_into().unwrap()),
            i16::MAX
        );
        assert_eq!(
            i16::from_le_bytes(expected[2..4].try_into().unwrap()),
            i16::MIN
        );

        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(unsigned[..8].try_into().unwrap());
            x86.mm[1] = u64::from_le_bytes(signed[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 3 << 11;
        }
        execute_lifted_x86(&[0x0F, 0x38, 0x04, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                u64::from_le_bytes(expected[..8].try_into().unwrap())
            );
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 3 << 11);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&unsigned[..16], upper);
            x86.xmm[1] = seeded(&signed[..16], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x04, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 16), expected[..16]);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&unsigned[..32], 0);
            x86.xmm[2] = seeded(&signed[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x04, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 32), expected[..32]);
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }

        // Destructive legacy aliasing must read every unsigned and signed byte
        // before merging any result word back into the shared destination.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&unsigned[..16], upper);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x04, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                reference(&unsigned[..16], &unsigned[..16])
            );
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(unsigned[..8].try_into().unwrap());
        }
        execute_lifted_x86(&[0x0F, 0x38, 0x04, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                u64::from_le_bytes(
                    reference(&unsigned[..8], &unsigned[..8])
                        .try_into()
                        .unwrap()
                )
            );
        }

        // EVEX applies each mask bit to one signed-word result. Validate both
        // merge and zero modes over all 32 ZMM result lanes.
        let raw = reference(&unsigned, &signed);
        let mask = 0xA55A_89ABu64;
        for (p2, zeroing) in [(0x49, false), (0xC9, true)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
                x86.xmm[1] = seeded(&unsigned, 0);
                x86.xmm[2] = seeded(&signed, 0);
            }
            ctx.write_vreg(k1, mask);
            execute_lifted_x86(&[0x62, 0xF2, 0x75, p2, 0x04, 0xC2], &mut ctx, &mut memory);
            let mut masked = vec![0; 64];
            for lane in 0..32 {
                let at = lane * 2;
                if mask >> lane & 1 != 0 {
                    masked[at..at + 2].copy_from_slice(&raw[at..at + 2]);
                } else if !zeroing {
                    masked[at..at + 2].copy_from_slice(&[0x6B, 0x6B]);
                }
            }
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 64), masked);
                assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
            }
        }

        // Independently encoded high-register EVEX form: zmm16 := zmm17,zmm18.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = sentinel;
            x86.xmm[17] = seeded(&unsigned, 0);
            x86.xmm[18] = seeded(&signed, 0);
        }
        execute_lifted_x86(&[0x62, 0xA2, 0x75, 0x40, 0x04, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[16], 64), expected);
        }

        memory.write(0x101, &signed).unwrap();
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x04, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(unsigned[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x38, 0x04, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                u64::from_le_bytes(expected[..8].try_into().unwrap())
            );
            assert_eq!(x86.x87.tag_word, 0);
        }

        ctx.write_vreg(rax, 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0x38, 0x04, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        // VEX accepts the identical unaligned address and performs a complete
        // all-or-fault 32-byte load.
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&unsigned[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x04, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 32), expected[..32]);
        }

        // E4NF: destination masking never suppresses the complete memory read.
        // Only two of the required 16 bytes are mapped, so both a single-bit
        // mask and an all-zero mask fault before modifying the destination.
        memory.write(0x3FE, &signed[..2]).unwrap();
        ctx.write_vreg(rax, 0x3FE);
        for mask in [1, 0] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = sentinel;
                x86.xmm[1] = seeded(&unsigned[..16], 0);
            }
            ctx.write_vreg(k1, mask);
            let fault =
                execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x09, 0x04, 0x00], &mut ctx, &mut memory);
            assert!(matches!(
                fault,
                BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
            ));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[0], sentinel);
            }
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        ctx.write_vreg(rax, 0x3F0);
        let fault = execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x04, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }
    }
    #[test]
    fn lifted_pmulhrsw_executes_rounding_masks_aliases_and_faults() {
        fn seeded(bytes: &[u8], fill: u64) -> VecValue {
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

        fn reference(first: &[u8], second: &[u8]) -> Vec<u8> {
            first
                .chunks_exact(2)
                .zip(second.chunks_exact(2))
                .flat_map(|(a, b)| {
                    let a = i32::from(i16::from_le_bytes(a.try_into().unwrap()));
                    let b = i32::from(i16::from_le_bytes(b.try_into().unwrap()));
                    (((a * b + 0x4000) >> 15) as i16).to_le_bytes()
                })
                .collect()
        }

        let first_words = [
            i16::MIN,
            i16::MAX,
            0x4000,
            -0x4000,
            1,
            -1,
            0x1234,
            -0x2345,
            i16::MIN,
            i16::MAX,
            0x2000,
            -0x2000,
            0x7FFE,
            -0x7FFF,
            17,
            -29,
            i16::MIN,
            i16::MAX,
            0x4000,
            -0x4000,
            1,
            -1,
            0x1234,
            -0x2345,
            i16::MIN,
            i16::MAX,
            0x2000,
            -0x2000,
            0x7FFE,
            -0x7FFF,
            17,
            -29,
        ];
        let second_words = [
            i16::MIN,
            i16::MIN,
            0x4000,
            0x4000,
            i16::MAX,
            i16::MAX,
            -0x3456,
            0x4567,
            i16::MIN,
            i16::MAX,
            -0x2000,
            -0x2000,
            0x7FFF,
            -0x7FFF,
            -31,
            43,
            i16::MIN,
            i16::MIN,
            0x4000,
            0x4000,
            i16::MAX,
            i16::MAX,
            -0x3456,
            0x4567,
            i16::MIN,
            i16::MAX,
            -0x2000,
            -0x2000,
            0x7FFF,
            -0x7FFF,
            -31,
            43,
        ];
        let first = first_words
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let second = second_words
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let expected = reference(&first, &second);
        assert_eq!(expected[..2], i16::MIN.to_le_bytes());

        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
            x86.mm[1] = u64::from_le_bytes(second[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 3 << 11;
        }
        execute_lifted_x86(&[0x0F, 0x38, 0x0B, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                u64::from_le_bytes(expected[..8].try_into().unwrap())
            );
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 3 << 11);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&first[..16], upper);
            x86.xmm[1] = seeded(&second[..16], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x0B, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 16), expected[..16]);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&first[..32], 0);
            x86.xmm[2] = seeded(&second[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x0B, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 32), expected[..32]);
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&first[..16], upper);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x0B, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                reference(&first[..16], &first[..16])
            );
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
        }
        execute_lifted_x86(&[0x0F, 0x38, 0x0B, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                u64::from_le_bytes(reference(&first[..8], &first[..8]).try_into().unwrap())
            );
        }

        let raw = reference(&first, &second);
        let mask = 0xA55A_89ABu64;
        for (p2, zeroing) in [(0x49, false), (0xC9, true)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
                x86.xmm[1] = seeded(&first, 0);
                x86.xmm[2] = seeded(&second, 0);
            }
            ctx.write_vreg(k1, mask);
            execute_lifted_x86(&[0x62, 0xF2, 0x75, p2, 0x0B, 0xC2], &mut ctx, &mut memory);
            let mut masked = vec![0; 64];
            for lane in 0..32 {
                let at = lane * 2;
                if mask >> lane & 1 != 0 {
                    masked[at..at + 2].copy_from_slice(&raw[at..at + 2]);
                } else if !zeroing {
                    masked[at..at + 2].copy_from_slice(&[0x6B, 0x6B]);
                }
            }
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 64), masked);
                assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
            }
        }

        memory.write(0x101, &second).unwrap();
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x0B, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x38, 0x0B, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                u64::from_le_bytes(expected[..8].try_into().unwrap())
            );
            assert_eq!(x86.x87.tag_word, 0);
        }

        ctx.write_vreg(rax, 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0x38, 0x0B, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&first[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x0B, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 32), expected[..32]);
        }

        memory.write(0x3FE, &second[..2]).unwrap();
        ctx.write_vreg(rax, 0x3FE);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&first[..16], 0);
        }
        ctx.write_vreg(k1, 1);
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x09, 0x0B, 0x00], &mut ctx, &mut memory);
        assert!(!matches!(
            suppressed,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 2), expected[..2]);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        ctx.write_vreg(k1, 1 << 1);
        let exposed =
            execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x09, 0x0B, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            exposed,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        ctx.write_vreg(rax, 0x3F0);
        let fault = execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x0B, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }
    }
    #[test]
    fn lifted_pabs_family_executes_minima_masks_broadcasts_and_faults() {
        fn seeded(bytes: &[u8], fill: u64) -> VecValue {
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

        fn reference(input: &[u8], elem_bytes: usize) -> Vec<u8> {
            input
                .chunks_exact(elem_bytes)
                .flat_map(|lane| match elem_bytes {
                    1 => vec![(lane[0] as i8).wrapping_abs() as u8],
                    2 => i16::from_le_bytes(lane.try_into().unwrap())
                        .wrapping_abs()
                        .to_le_bytes()
                        .to_vec(),
                    4 => i32::from_le_bytes(lane.try_into().unwrap())
                        .wrapping_abs()
                        .to_le_bytes()
                        .to_vec(),
                    8 => i64::from_le_bytes(lane.try_into().unwrap())
                        .wrapping_abs()
                        .to_le_bytes()
                        .to_vec(),
                    _ => unreachable!(),
                })
                .collect()
        }

        let mut byte_input = (0..64)
            .map(|lane| (lane * 37 + 0x41) as u8)
            .collect::<Vec<_>>();
        byte_input[0] = i8::MIN as u8;
        byte_input[1] = (-1i8) as u8;
        byte_input[2] = 0;
        byte_input[3] = i8::MAX as u8;
        let word_input = [
            i16::MIN,
            -1,
            0,
            i16::MAX,
            -0x1234,
            0x2345,
            -17,
            29,
            i16::MIN,
            -1,
            0,
            i16::MAX,
            -0x3456,
            0x4567,
            -31,
            43,
            i16::MIN,
            -1,
            0,
            i16::MAX,
            -0x1234,
            0x2345,
            -17,
            29,
            i16::MIN,
            -1,
            0,
            i16::MAX,
            -0x3456,
            0x4567,
            -31,
            43,
        ]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
        let dword_input = [
            i32::MIN,
            -1,
            0,
            i32::MAX,
            -0x1234_567,
            0x2345_678,
            -17,
            29,
            i32::MIN,
            -1,
            0,
            i32::MAX,
            -0x3456_789,
            0x4567_89A,
            -31,
            43,
        ]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
        let qword_input = [
            i64::MIN,
            -1,
            0,
            i64::MAX,
            -0x1234_5678_9ABC,
            0x2345_6789_ABCD,
            -17,
            29,
        ]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
        let cases = [
            (0x1C, 1usize, byte_input.as_slice()),
            (0x1D, 2, word_input.as_slice()),
            (0x1E, 4, dword_input.as_slice()),
            (0x1F, 8, qword_input.as_slice()),
        ];

        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Prefix-free SSSE3 PABS operates on an MMX destination and an mm/m64
        // source.  Exercise every element width, the wrapping minimum value,
        // the x87/MMX state transition, and a destructive register alias.
        for &(opcode, elem_bytes, input) in &cases {
            if opcode == 0x1F {
                continue;
            }
            let input = &input[..8];
            let source = u64::from_le_bytes(input.try_into().unwrap());
            let expected = u64::from_le_bytes(reference(input, elem_bytes).try_into().unwrap());
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mm[0] = source;
                x86.x87.tag_word = 0xFFFF;
                x86.x87.status_word = 5 << 11;
            }
            execute_lifted_x86(&[0x0F, 0x38, opcode, 0xC0], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.mm[0], expected, "MMX opcode={opcode:02X}");
                assert_eq!(x86.x87.tag_word, 0);
                assert_eq!(x86.x87.status_word & 0x3800, 5 << 11);
            }
        }

        // The m64 form has no mandatory 16-byte #GP alignment.  Its complete
        // source load faults before either the destination or MMX state changes.
        let mmx_word_expected =
            u64::from_le_bytes(reference(&word_input[..8], 2).try_into().unwrap());
        memory.write(0x81, &word_input[..8]).unwrap();
        ctx.write_vreg(rax, 0x80);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x38, 0x1D, 0x40, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], mmx_word_expected);
            assert_eq!(x86.x87.tag_word, 0);
        }

        ctx.write_vreg(rax, 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0x38, 0x1D, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        for (opcode, elem_bytes, input) in cases {
            let expected = reference(input, elem_bytes);
            assert_eq!(&expected[..elem_bytes], &input[..elem_bytes]);

            if opcode != 0x1F {
                if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                    x86.xmm[0] = [upper; 16];
                    x86.xmm[1] = seeded(&input[..16], 0);
                }
                execute_lifted_x86(&[0x66, 0x0F, 0x38, opcode, 0xC1], &mut ctx, &mut memory);
                if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                    assert_eq!(bytes(&x86.xmm[0], 16), expected[..16]);
                    assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
                }

                let vex_p2 = 0x7D;
                if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                    x86.xmm[0] = sentinel;
                    x86.xmm[2] = seeded(&input[..32], 0);
                }
                execute_lifted_x86(&[0xC4, 0xE2, vex_p2, opcode, 0xC2], &mut ctx, &mut memory);
                if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                    assert_eq!(bytes(&x86.xmm[0], 32), expected[..32]);
                    assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
                }
            }

            let evex_w = if opcode == 0x1F { 0xFD } else { 0x7D };
            let lanes = 64 / elem_bytes;
            let mask = 0xA55A_89AB_F00F_1357u64;
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
                x86.xmm[2] = seeded(input, 0);
            }
            ctx.write_vreg(k1, mask);
            execute_lifted_x86(
                &[0x62, 0xF2, evex_w, 0x49, opcode, 0xC2],
                &mut ctx,
                &mut memory,
            );
            let mut masked = vec![0; 64];
            for lane in 0..lanes {
                let at = lane * elem_bytes;
                if mask >> lane & 1 != 0 {
                    masked[at..at + elem_bytes].copy_from_slice(&expected[at..at + elem_bytes]);
                } else {
                    masked[at..at + elem_bytes].fill(0x6B);
                }
            }
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 64), masked);
                assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
            }
        }

        // Dword broadcast repeats one wrapping absolute value across all lanes.
        memory.write(0x100, &i32::MIN.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x100);
        execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x58, 0x1E, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 64),
                i32::MIN
                    .to_le_bytes()
                    .into_iter()
                    .cycle()
                    .take(64)
                    .collect::<Vec<_>>()
            );
        }

        // A zero mask suppresses a broadcast memory fault; any active lane
        // requires the single scalar read and exposes it.
        ctx.write_vreg(rax, 0x1000);
        ctx.write_vreg(k1, 0);
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x59, 0x1E, 0x00], &mut ctx, &mut memory);
        assert!(!matches!(
            suppressed,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        ctx.write_vreg(k1, 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let exposed =
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x59, 0x1E, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            exposed,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        // Normal masked memory accesses are per element.
        memory.write(0x3FF, &[i8::MIN as u8]).unwrap();
        ctx.write_vreg(rax, 0x3FF);
        ctx.write_vreg(k1, 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let lane0 =
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x09, 0x1C, 0x00], &mut ctx, &mut memory);
        assert!(!matches!(
            lane0,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 1), vec![i8::MIN as u8]);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        }
        ctx.write_vreg(k1, 1 << 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let lane1 =
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x09, 0x1C, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            lane1,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        memory.write(0x101, &word_input).unwrap();
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x1D, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x1D, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 32), reference(&word_input[..32], 2));
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_palignr_executes_immediates_grouping_masks_aliases_and_faults() {
        fn seeded(bytes: &[u8], fill: u64) -> VecValue {
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

        fn reference(high: &[u8], low: &[u8], imm: u8) -> Vec<u8> {
            let mut result = vec![0; high.len()];
            let block_bytes = usize::min(16, high.len());
            for block in 0..high.len() / block_bytes {
                let base = block * block_bytes;
                for lane in 0..block_bytes {
                    let index = usize::from(imm) + lane;
                    result[base + lane] = if index < block_bytes {
                        low[base + index]
                    } else if index < block_bytes * 2 {
                        high[base + index - block_bytes]
                    } else {
                        0
                    };
                }
            }
            result
        }

        let high = (0..64).map(|lane| (lane + 1) as u8).collect::<Vec<_>>();
        let low = (0..64).map(|lane| (0x80 + lane) as u8).collect::<Vec<_>>();
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for imm in [0u8, 1, 7, 8, 9, 15, 16, 255] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mm[0] = u64::from_le_bytes(high[..8].try_into().unwrap());
                x86.mm[1] = u64::from_le_bytes(low[..8].try_into().unwrap());
                x86.x87.tag_word = 0xFFFF;
            }
            execute_lifted_x86(&[0x0F, 0x3A, 0x0F, 0xC1, imm], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    x86.mm[0],
                    u64::from_le_bytes(reference(&high[..8], &low[..8], imm).try_into().unwrap()),
                    "MMX imm={imm}"
                );
                assert_eq!(x86.x87.tag_word, 0);
            }
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(high[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x3A, 0x0F, 0xC0, 5], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                u64::from_le_bytes(reference(&high[..8], &high[..8], 5).try_into().unwrap())
            );
        }

        memory.write(0x181, &low[..8]).unwrap();
        ctx.write_vreg(rax, 0x180);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(high[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x3A, 0x0F, 0x40, 0x01, 5], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                u64::from_le_bytes(reference(&high[..8], &low[..8], 5).try_into().unwrap())
            );
            assert_eq!(x86.x87.tag_word, 0);
        }

        ctx.write_vreg(rax, 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0x3A, 0x0F, 0x00, 5], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        for imm in [0u8, 1, 15, 16, 17, 31, 32, 255] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = seeded(&high[..16], upper);
                x86.xmm[1] = seeded(&low[..16], 0);
            }
            execute_lifted_x86(&[0x66, 0x0F, 0x3A, 0x0F, 0xC1, imm], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[0], 16),
                    reference(&high[..16], &low[..16], imm),
                    "legacy imm={imm}"
                );
                assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
            }

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = sentinel;
                x86.xmm[1] = seeded(&high[..32], 0);
                x86.xmm[2] = seeded(&low[..32], 0);
            }
            execute_lifted_x86(&[0xC4, 0xE3, 0x75, 0x0F, 0xC2, imm], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[0], 32),
                    reference(&high[..32], &low[..32], imm),
                    "VEX imm={imm}"
                );
                assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
            }
        }

        // Legacy destructive alias and both VEX destination alias directions.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&high[..16], upper);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x3A, 0x0F, 0xC0, 0x05], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                reference(&high[..16], &high[..16], 5)
            );
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&high[..32], 0);
            x86.xmm[2] = seeded(&low[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE3, 0x7D, 0x0F, 0xC2, 0x05], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                reference(&high[..32], &low[..32], 5)
            );
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&low[..32], 0);
            x86.xmm[1] = seeded(&high[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE3, 0x75, 0x0F, 0xC0, 0x05], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                reference(&high[..32], &low[..32], 5)
            );
        }

        let raw = reference(&high, &low, 5);
        let mask = 0xA55A_89AB_F00F_1357u64;
        for (p2, zeroing) in [(0x49, false), (0xC9, true)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
                x86.xmm[1] = seeded(&high, 0);
                x86.xmm[2] = seeded(&low, 0);
            }
            ctx.write_vreg(k1, mask);
            execute_lifted_x86(
                &[0x62, 0xF3, 0x75, p2, 0x0F, 0xC2, 0x05],
                &mut ctx,
                &mut memory,
            );
            let expected = (0..64)
                .map(|lane| {
                    if mask >> lane & 1 != 0 {
                        raw[lane]
                    } else if zeroing {
                        0
                    } else {
                        0x6B
                    }
                })
                .collect::<Vec<_>>();
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 64), expected);
                assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
            }
        }

        memory.write(0x101, &low).unwrap();
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let misaligned =
            execute_lifted_x86(&[0x66, 0x0F, 0x3A, 0x0F, 0x00, 0x01], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&high[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE3, 0x75, 0x0F, 0x00, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                reference(&high[..32], &low[..32], 1)
            );
        }

        // At imm=0, output byte n consumes memory byte n. Put byte 0 at the
        // final valid address to distinguish suppressed lane 0 from lane 1.
        memory.write(0x3FF, &[low[0]]).unwrap();
        ctx.write_vreg(rax, 0x3FF);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&high[..16], 0);
        }
        ctx.write_vreg(k1, 1);
        let lane0 = execute_lifted_x86(
            &[0x62, 0xF3, 0x75, 0x09, 0x0F, 0x00, 0x00],
            &mut ctx,
            &mut memory,
        );
        assert!(!matches!(
            lane0,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 1), vec![low[0]]);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        }
        ctx.write_vreg(k1, 1 << 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let lane1 = execute_lifted_x86(
            &[0x62, 0xF3, 0x75, 0x09, 0x0F, 0x00, 0x00],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            lane1,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        // imm=16 selects only src1: active output bytes do not consume the
        // memory concatenand, so the invalid address remains suppressed.
        ctx.write_vreg(rax, 0x1000);
        ctx.write_vreg(k1, u64::MAX);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = seeded(&high[..16], 0);
        }
        let shifted_out = execute_lifted_x86(
            &[0x62, 0xF3, 0x75, 0x09, 0x0F, 0x00, 0x10],
            &mut ctx,
            &mut memory,
        );
        assert!(!matches!(
            shifted_out,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 16), high[..16]);
        }

        // Without a writemask, the complete memory operand is still read.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let full_fault = execute_lifted_x86(
            &[0x62, 0xF3, 0x75, 0x08, 0x0F, 0x00, 0x10],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            full_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_packed_extend_families_execute_sign_zero_masks_aliases_and_faults() {
        fn seeded(input: &[u8], fill: u64) -> VecValue {
            let mut value = [fill; 16];
            for (index, byte) in input.iter().copied().enumerate() {
                let word = index / 8;
                let shift = (index % 8) * 8;
                value[word] = (value[word] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
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

        fn reference(
            input: &[u8],
            source_bytes: usize,
            destination_bytes: usize,
            signed: bool,
            destination_len: usize,
        ) -> Vec<u8> {
            let lanes = destination_len / destination_bytes;
            assert_eq!(input.len(), lanes * source_bytes);
            let source_bits = source_bytes * 8;
            let mut result = Vec::with_capacity(destination_len);
            for lane in input.chunks_exact(source_bytes) {
                let mut raw_bytes = [0u8; 8];
                raw_bytes[..source_bytes].copy_from_slice(lane);
                let raw = u64::from_le_bytes(raw_bytes);
                let extended = if signed {
                    let shift = 64 - source_bits;
                    ((raw << shift) as i64 >> shift) as u64
                } else {
                    raw
                };
                result.extend_from_slice(&extended.to_le_bytes()[..destination_bytes]);
            }
            assert_eq!(result.len(), destination_len);
            result
        }

        let cases = [
            (0x20, 1usize, 2usize, true),
            (0x21, 1, 4, true),
            (0x22, 1, 8, true),
            (0x23, 2, 4, true),
            (0x24, 2, 8, true),
            (0x25, 4, 8, true),
            (0x30, 1, 2, false),
            (0x31, 1, 4, false),
            (0x32, 1, 8, false),
            (0x33, 2, 4, false),
            (0x34, 2, 8, false),
            (0x35, 4, 8, false),
        ];
        // Keeping the high bit set in every byte guarantees discriminating
        // negative lanes for each 8-, 16-, and 32-bit source element width.
        let source = (0..32)
            .map(|index| 0x80 | (((index * 29 + 3) as u8) & 0x7F))
            .collect::<Vec<_>>();
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for (opcode, source_bytes, destination_bytes, signed) in cases {
            let legacy_source_len = 16 / destination_bytes * source_bytes;
            let legacy_expected = reference(
                &source[..legacy_source_len],
                source_bytes,
                destination_bytes,
                signed,
                16,
            );
            if signed {
                assert_ne!(
                    legacy_expected,
                    reference(
                        &source[..legacy_source_len],
                        source_bytes,
                        destination_bytes,
                        false,
                        16,
                    ),
                    "signed opcode {opcode:02X} lacks a discriminating lane"
                );
            }
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [upper; 16];
                x86.xmm[1] = seeded(&source[..legacy_source_len], 0);
            }
            execute_lifted_x86(&[0x66, 0x0F, 0x38, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[0], 16),
                    legacy_expected,
                    "legacy {opcode:02X}"
                );
                assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
            }

            let vex_source_len = 32 / destination_bytes * source_bytes;
            let vex_expected = reference(
                &source[..vex_source_len],
                source_bytes,
                destination_bytes,
                signed,
                32,
            );
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = sentinel;
                x86.xmm[2] = seeded(&source[..vex_source_len], 0);
            }
            execute_lifted_x86(&[0xC4, 0xE2, 0x7D, opcode, 0xC2], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 32), vex_expected, "VEX {opcode:02X}");
                assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
            }

            let evex_source_len = 64 / destination_bytes * source_bytes;
            let evex_raw = reference(
                &source[..evex_source_len],
                source_bytes,
                destination_bytes,
                signed,
                64,
            );
            let mask = 0xA5A5u64;
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
                x86.xmm[2] = seeded(&source[..evex_source_len], 0);
            }
            ctx.write_vreg(k1, mask);
            execute_lifted_x86(
                &[0x62, 0xF2, 0x7D, 0x49, opcode, 0xC2],
                &mut ctx,
                &mut memory,
            );
            let mut evex_expected = vec![0x6B; 64];
            for lane in 0..64 / destination_bytes {
                if mask >> lane & 1 != 0 {
                    let at = lane * destination_bytes;
                    evex_expected[at..at + destination_bytes]
                        .copy_from_slice(&evex_raw[at..at + destination_bytes]);
                }
            }
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 64), evex_expected, "EVEX {opcode:02X}");
                assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
            }
        }

        // The source and destination may alias. All source bytes must be
        // captured before the VEX zeroing destination write begins.
        let alias_source = &source[..16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(alias_source, upper);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x20, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                reference(alias_source, 1, 2, true, 32)
            );
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }

        // Legacy PMOVSXBQ reads exactly two bytes, accepts an unaligned
        // address, and commits no destination state when the second read faults.
        memory.write(0x3FE, &[0x80, 0x7F]).unwrap();
        ctx.write_vreg(rax, 0x3FE);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [upper; 16];
        }
        let exact = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x22, 0x00], &mut ctx, &mut memory);
        assert!(!matches!(
            exact,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0xFFFF_FFFF_FFFF_FF80);
            assert_eq!(x86.xmm[0][1], 0x7F);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        ctx.write_vreg(rax, 0x3FF);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let partial_fault =
            execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x22, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            partial_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        // EVEX VPMOVZXBQ maps destination mask bit n to source byte n.
        // A masked-off byte is not read, including at an invalid address.
        ctx.write_vreg(rax, 0x3FF);
        ctx.write_vreg(k1, 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
        }
        let lane0 =
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x49, 0x32, 0x00], &mut ctx, &mut memory);
        assert!(!matches!(
            lane0,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0x7F);
            assert!(
                x86.xmm[0][1..8]
                    .iter()
                    .all(|word| *word == 0x6B6B_6B6B_6B6B_6B6B)
            );
            assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
        }

        ctx.write_vreg(k1, 1 << 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let lane1 =
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x49, 0x32, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            lane1,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        ctx.write_vreg(rax, 0x1000);
        ctx.write_vreg(k1, 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let all_suppressed =
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x49, 0x32, 0x00], &mut ctx, &mut memory);
        assert!(!matches!(
            all_suppressed,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[0][..8], &sentinel[..8]);
            assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_packed_minmax_executes_signedness_masks_aliases_broadcasts_and_faults() {
        fn seeded(input: &[u8], fill: u64) -> VecValue {
            let mut value = [fill; 16];
            for (index, byte) in input.iter().copied().enumerate() {
                let word = index / 8;
                let shift = (index % 8) * 8;
                value[word] = (value[word] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
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

        fn reference(
            lhs: &[u8],
            rhs: &[u8],
            elem_bytes: usize,
            signed: bool,
            min: bool,
        ) -> Vec<u8> {
            assert_eq!(lhs.len(), rhs.len());
            let bits = elem_bytes * 8;
            let signed_value = |raw: u64| {
                let shift = 64 - bits;
                ((raw << shift) as i64) >> shift
            };
            lhs.chunks_exact(elem_bytes)
                .zip(rhs.chunks_exact(elem_bytes))
                .flat_map(|(a, b)| {
                    let mut a_bytes = [0u8; 8];
                    let mut b_bytes = [0u8; 8];
                    a_bytes[..elem_bytes].copy_from_slice(a);
                    b_bytes[..elem_bytes].copy_from_slice(b);
                    let av = u64::from_le_bytes(a_bytes);
                    let bv = u64::from_le_bytes(b_bytes);
                    let take_a = if signed {
                        if min {
                            signed_value(av) < signed_value(bv)
                        } else {
                            signed_value(av) > signed_value(bv)
                        }
                    } else if min {
                        av < bv
                    } else {
                        av > bv
                    };
                    if take_a { a.to_vec() } else { b.to_vec() }
                })
                .collect()
        }

        let lhs = (0..64)
            .map(|index| [0x80, 0x7F, 0xFF, 0x00, 0x01, 0xFE, 0x40, 0xC0][index % 8])
            .collect::<Vec<_>>();
        let rhs = (0..64)
            .map(|index| [0x7F, 0x80, 0x00, 0xFF, 0xFE, 0x01, 0xC0, 0x40][index % 8])
            .collect::<Vec<_>>();
        let cases = [
            (0x38, 1usize, true, true),
            (0x39, 4, true, true),
            (0x3A, 2, false, true),
            (0x3B, 4, false, true),
            (0x3C, 1, true, false),
            (0x3D, 4, true, false),
            (0x3E, 2, false, false),
            (0x3F, 4, false, false),
        ];
        let qword_cases = [
            (0x39, true, true),
            (0x3B, false, true),
            (0x3D, true, false),
            (0x3F, false, false),
        ];
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for (opcode, elem_bytes, signed, min) in cases {
            let legacy_expected = reference(&lhs[..16], &rhs[..16], elem_bytes, signed, min);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = seeded(&lhs[..16], upper);
                x86.xmm[1] = seeded(&rhs[..16], 0);
            }
            execute_lifted_x86(&[0x66, 0x0F, 0x38, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[0], 16),
                    legacy_expected,
                    "legacy {opcode:02X}"
                );
                assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
            }

            let vex_expected = reference(&lhs[..32], &rhs[..32], elem_bytes, signed, min);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = sentinel;
                x86.xmm[1] = seeded(&lhs[..32], 0);
                x86.xmm[2] = seeded(&rhs[..32], 0);
            }
            execute_lifted_x86(&[0xC4, 0xE2, 0x75, opcode, 0xC2], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 32), vex_expected, "VEX {opcode:02X}");
                assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
            }

            let raw = reference(&lhs, &rhs, elem_bytes, signed, min);
            let mask = 0xA55Au64;
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
                x86.xmm[1] = seeded(&lhs, 0);
                x86.xmm[2] = seeded(&rhs, 0);
            }
            ctx.write_vreg(k1, mask);
            execute_lifted_x86(
                &[0x62, 0xF2, 0x75, 0x49, opcode, 0xC2],
                &mut ctx,
                &mut memory,
            );
            let mut expected = vec![0x6B; 64];
            for lane in 0..64 / elem_bytes {
                if mask >> lane & 1 != 0 {
                    let at = lane * elem_bytes;
                    expected[at..at + elem_bytes].copy_from_slice(&raw[at..at + elem_bytes]);
                }
            }
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 64), expected, "EVEX {opcode:02X}");
                assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
            }
        }

        for (opcode, signed, min) in qword_cases {
            let raw = reference(&lhs, &rhs, 8, signed, min);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = sentinel;
                x86.xmm[1] = seeded(&lhs, 0);
                x86.xmm[2] = seeded(&rhs, 0);
            }
            ctx.write_vreg(k1, u64::MAX);
            execute_lifted_x86(
                &[0x62, 0xF2, 0xF5, 0x49, opcode, 0xC2],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 64), raw, "EVEX qword {opcode:02X}");
                assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
            }
        }

        // VEX permits destination aliasing with either input.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&lhs[..32], 0);
            x86.xmm[2] = seeded(&rhs[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x38, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                reference(&lhs[..32], &rhs[..32], 1, true, true)
            );
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&rhs[..32], 0);
            x86.xmm[1] = seeded(&lhs[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x38, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                reference(&lhs[..32], &rhs[..32], 1, true, true)
            );
        }

        memory.write(0x100, &rhs).unwrap();
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x38, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&lhs[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x38, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                reference(&lhs[..32], &rhs[1..33], 1, true, true)
            );
        }

        // Masked byte memory accesses are fault-suppressed per destination lane.
        memory.write(0x3FF, &[rhs[0]]).unwrap();
        ctx.write_vreg(rax, 0x3FF);
        ctx.write_vreg(k1, 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
            x86.xmm[1] = seeded(&lhs, 0);
        }
        let lane0 =
            execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x49, 0x38, 0x00], &mut ctx, &mut memory);
        assert!(!matches!(
            lane0,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        ctx.write_vreg(k1, 1 << 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let lane1 =
            execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x49, 0x38, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            lane1,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        // A zero mask suppresses a qword-broadcast fault. Any active lane
        // requires the scalar eight-byte read.
        ctx.write_vreg(rax, 0x1000);
        ctx.write_vreg(k1, 0);
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF2, 0xF5, 0x59, 0x3F, 0x00], &mut ctx, &mut memory);
        assert!(!matches!(
            suppressed,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        ctx.write_vreg(k1, 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let exposed =
            execute_lifted_x86(&[0x62, 0xF2, 0xF5, 0x59, 0x3F, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            exposed,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_mmx_packed_multiply_executes_widths_aliases_state_and_faults() {
        fn words(value: u64) -> [u16; 4] {
            let bytes = value.to_le_bytes();
            std::array::from_fn(|lane| {
                u16::from_le_bytes(bytes[lane * 2..lane * 2 + 2].try_into().unwrap())
            })
        }

        fn pack_words(value: [u16; 4]) -> u64 {
            u64::from_le_bytes(
                value
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
            )
        }

        fn reference(opcode: u8, lhs: u64, rhs: u64) -> u64 {
            if opcode == 0xF4 {
                return u64::from(lhs as u32) * u64::from(rhs as u32);
            }
            let lhs = words(lhs);
            let rhs = words(rhs);
            pack_words(std::array::from_fn(|lane| match opcode {
                0xD5 => lhs[lane].wrapping_mul(rhs[lane]),
                0xE4 => ((u32::from(lhs[lane]) * u32::from(rhs[lane])) >> 16) as u16,
                0xE5 => {
                    let product = i32::from(lhs[lane] as i16) * i32::from(rhs[lane] as i16);
                    (product >> 16) as i16 as u16
                }
                _ => unreachable!(),
            }))
        }

        let lhs = pack_words([0xFFFF, 0x8000, 0x1234, 0x7FFF]);
        let rhs = pack_words([0x0002, 0xFFFF, 0xFEDC, 0x8000]);
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for opcode in [0xF4, 0xD5, 0xE4, 0xE5] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mm[0] = lhs;
                x86.mm[1] = rhs;
                x86.x87.tag_word = 0xFFFF;
                x86.x87.status_word = 3 << 11;
            }
            execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    x86.mm[0],
                    reference(opcode, lhs, rhs),
                    "opcode={opcode:02X}"
                );
                assert_eq!(x86.x87.tag_word, 0);
                assert_eq!(x86.x87.status_word & 0x3800, 3 << 11);
            }
        }

        // Destructive register aliases snapshot every input lane before the
        // first architectural write.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = lhs;
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0xD5, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], reference(0xD5, lhs, lhs));
            assert_eq!(x86.x87.tag_word, 0);
        }

        // PMULUDQ fetches the complete unaligned m64 source even though only
        // its low doubleword participates in the single qword product.
        memory.write(0x181, &rhs.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x180);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = lhs;
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0xF4, 0x40, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], reference(0xF4, lhs, rhs));
            assert_eq!(x86.x87.tag_word, 0);
        }

        // A source fault precedes both the destructive result and EnterMmx.
        ctx.write_vreg(rax, 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
            x86.x87.tag_word = 0xFFFF;
        }
        let fault = execute_lifted_x86(&[0x0F, 0xF4, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_mmx_average_and_pmaddwd_execute_rounding_wrap_aliases_and_faults() {
        fn pavg(opcode: u8, lhs: u64, rhs: u64) -> u64 {
            if opcode == 0xE0 {
                let lhs = lhs.to_le_bytes();
                let rhs = rhs.to_le_bytes();
                return u64::from_le_bytes(std::array::from_fn(|lane| {
                    ((u16::from(lhs[lane]) + u16::from(rhs[lane]) + 1) >> 1) as u8
                }));
            }
            let lhs = lhs.to_le_bytes();
            let rhs = rhs.to_le_bytes();
            u64::from_le_bytes(
                (0..4)
                    .flat_map(|lane| {
                        let at = lane * 2;
                        let a = u16::from_le_bytes(lhs[at..at + 2].try_into().unwrap());
                        let b = u16::from_le_bytes(rhs[at..at + 2].try_into().unwrap());
                        ((u32::from(a) + u32::from(b) + 1) >> 1)
                            .to_le_bytes()
                            .into_iter()
                            .take(2)
                    })
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
            )
        }

        fn pmaddwd(lhs: u64, rhs: u64) -> u64 {
            let lhs = lhs.to_le_bytes();
            let rhs = rhs.to_le_bytes();
            u64::from_le_bytes(
                (0..2)
                    .flat_map(|lane| {
                        let at = lane * 4;
                        let word = |bytes: &[u8], offset: usize| {
                            i32::from(i16::from_le_bytes(
                                bytes[offset..offset + 2].try_into().unwrap(),
                            ))
                        };
                        let sum = word(&lhs, at)
                            .wrapping_mul(word(&rhs, at))
                            .wrapping_add(word(&lhs, at + 2).wrapping_mul(word(&rhs, at + 2)));
                        sum.to_le_bytes()
                    })
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
            )
        }

        let lhs = 0xFFFF_8000_0100_00FFu64;
        let rhs = 0x8000_FFFF_00FF_0002u64;
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for opcode in [0xE0, 0xE3] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mm[0] = lhs;
                x86.mm[1] = rhs;
                x86.x87.tag_word = 0xFFFF;
                x86.x87.status_word = 5 << 11;
            }
            execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.mm[0], pavg(opcode, lhs, rhs), "opcode={opcode:02X}");
                assert_eq!(x86.x87.tag_word, 0);
                assert_eq!(x86.x87.status_word & 0x3800, 5 << 11);
            }
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = lhs;
            x86.mm[1] = rhs;
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0xF5, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], pmaddwd(lhs, rhs));
            assert_eq!(x86.x87.tag_word, 0);
        }

        // The only overflowing PMADDWD input wraps each pairwise sum to
        // 0x8000_0000 rather than saturating.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0x8000_8000_8000_8000;
            x86.mm[1] = 0x8000_8000_8000_8000;
        }
        execute_lifted_x86(&[0x0F, 0xF5, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0x8000_0000_8000_0000);
        }

        // A destructive self-alias remains unchanged under rounded averaging.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = lhs;
        }
        execute_lifted_x86(&[0x0F, 0xE0, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], lhs);
        }

        memory.write(0x181, &rhs.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x180);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = lhs;
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0xE3, 0x40, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], pavg(0xE3, lhs, rhs));
            assert_eq!(x86.x87.tag_word, 0);
        }

        ctx.write_vreg(rax, 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
            x86.x87.tag_word = 0xFFFF;
        }
        let fault = execute_lifted_x86(&[0x0F, 0xF5, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_original_packed_minmax_executes_values_masks_e4_and_faults() {
        fn seeded(input: &[u8], fill: u64) -> VecValue {
            let mut value = [fill; 16];
            for (index, byte) in input.iter().copied().enumerate() {
                let shift = (index % 8) * 8;
                value[index / 8] =
                    (value[index / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
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

        fn reference(opcode: u8, lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
            match opcode {
                0xDA | 0xDE => lhs
                    .iter()
                    .zip(rhs)
                    .map(|(a, b)| {
                        if opcode == 0xDA {
                            (*a).min(*b)
                        } else {
                            (*a).max(*b)
                        }
                    })
                    .collect(),
                0xEA | 0xEE => lhs
                    .chunks_exact(2)
                    .zip(rhs.chunks_exact(2))
                    .flat_map(|(a, b)| {
                        let a = i16::from_le_bytes(a.try_into().unwrap());
                        let b = i16::from_le_bytes(b.try_into().unwrap());
                        if opcode == 0xEA { a.min(b) } else { a.max(b) }.to_le_bytes()
                    })
                    .collect(),
                _ => unreachable!(),
            }
        }

        let lhs = (0..64)
            .map(|lane| [0x80, 0x7F, 0xFF, 0x00, 0x01, 0xFE, 0x40, 0xC0][lane % 8])
            .collect::<Vec<_>>();
        let rhs = (0..64)
            .map(|lane| [0x7F, 0x80, 0x00, 0xFF, 0xFE, 0x01, 0xC0, 0x40][lane % 8])
            .collect::<Vec<_>>();
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let sentinel = [0x6B6B_6B6B_6B6B_6B6Bu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for opcode in [0xDA, 0xDE, 0xEA, 0xEE] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mm[0] = u64::from_le_bytes(lhs[..8].try_into().unwrap());
                x86.mm[1] = u64::from_le_bytes(rhs[..8].try_into().unwrap());
                x86.x87.tag_word = 0xFFFF;
                x86.x87.status_word = 3 << 11;
            }
            execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    x86.mm[0],
                    u64::from_le_bytes(reference(opcode, &lhs[..8], &rhs[..8]).try_into().unwrap())
                );
                assert_eq!(x86.x87.tag_word, 0);
                assert_eq!(x86.x87.status_word & 0x3800, 3 << 11);
            }

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = seeded(&lhs[..16], upper);
                x86.xmm[1] = seeded(&rhs[..16], 0);
            }
            execute_lifted_x86(&[0x66, 0x0F, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[0], 16),
                    reference(opcode, &lhs[..16], &rhs[..16])
                );
                assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
            }

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[8] = sentinel;
                x86.xmm[9] = seeded(&lhs[..32], 0);
                x86.xmm[10] = seeded(&rhs[..32], 0);
            }
            execute_lifted_x86(&[0xC4, 0x41, 0x35, opcode, 0xC2], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[8], 32),
                    reference(opcode, &lhs[..32], &rhs[..32])
                );
                assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
            }

            let elem_bytes = if opcode < 0xE0 { 1 } else { 2 };
            let raw = reference(opcode, &lhs, &rhs);
            let mask = 0xA55Au64;
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[16] = sentinel;
                x86.xmm[17] = seeded(&lhs, 0);
                x86.xmm[18] = seeded(&rhs, 0);
            }
            ctx.write_vreg(k1, mask);
            execute_lifted_x86(
                &[0x62, 0xA1, 0x75, 0x41, opcode, 0xC2],
                &mut ctx,
                &mut memory,
            );
            let mut expected = vec![0x6B; 64];
            for lane in 0..64 / elem_bytes {
                if mask >> lane & 1 != 0 {
                    let at = lane * elem_bytes;
                    expected[at..at + elem_bytes].copy_from_slice(&raw[at..at + elem_bytes]);
                }
            }
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[16], 64), expected);
                assert!(x86.xmm[16][8..].iter().all(|word| *word == 0));
            }
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(lhs[..8].try_into().unwrap());
        }
        execute_lifted_x86(&[0x0F, 0xDA, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                u64::from_le_bytes(reference(0xDA, &lhs[..8], &lhs[..8]).try_into().unwrap())
            );
        }

        memory.write(0x3FF, &rhs[..1]).unwrap();
        ctx.write_vreg(rax, 0x3FF);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&lhs, 0);
        }
        ctx.write_vreg(k1, 1);
        let lane0 =
            execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xDA, 0x00], &mut ctx, &mut memory);
        assert!(!matches!(
            lane0,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));

        ctx.write_vreg(k1, 2);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let lane1 =
            execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xDA, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            lane1,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        ctx.write_vreg(rax, 0x1000);
        ctx.write_vreg(k1, 0);
        let suppressed =
            execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xEA, 0x00], &mut ctx, &mut memory);
        assert!(!matches!(
            suppressed,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));

        memory.write(0x181, &rhs[..8]).unwrap();
        ctx.write_vreg(rax, 0x181);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(lhs[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0xEE, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                u64::from_le_bytes(reference(0xEE, &lhs[..8], &rhs[..8]).try_into().unwrap())
            );
            assert_eq!(x86.x87.tag_word, 0);
        }

        ctx.write_vreg(rax, 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0xDA, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        memory.write(0x100, &rhs[..16]).unwrap();
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0xEE, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_ptest_vptest_executes_flag_truth_table_widths_alignment_and_faults() {
        fn vec_from(input: &[u8], fill: u64) -> VecValue {
            let mut value = [fill; 16];
            for (index, byte) in input.iter().copied().enumerate() {
                let word = index / 8;
                let shift = (index % 8) * 8;
                value[word] = (value[word] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
            }
            value
        }

        fn expected_flags(before: u64, first: &[u8], second: &[u8]) -> u64 {
            assert_eq!(first.len(), second.len());
            let zf = first.iter().zip(second).all(|(a, b)| (*a & *b) == 0);
            let cf = first.iter().zip(second).all(|(a, b)| ((!*a) & *b) == 0);
            (before & !0x8D5) | u64::from(cf) | (u64::from(zf) << 6)
        }

        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);

        let truth_table = [
            ([0x00; 16], [0x00; 16]), // ZF=1, CF=1
            ([0xFF; 16], [0xFF; 16]), // ZF=0, CF=1
            ([0x00; 16], [0xFF; 16]), // ZF=1, CF=0
            ([0x0F; 16], [0xFF; 16]), // ZF=0, CF=0
        ];
        for (first, second) in truth_table {
            ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
            ctx.flags.lazy = None;
            let first_state = vec_from(&first, 0xA5A5_A5A5_A5A5_A5A5);
            let second_state = vec_from(&second, 0x5A5A_5A5A_5A5A_5A5A);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = first_state;
                x86.xmm[1] = second_state;
            }
            execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x17, 0xC1], &mut ctx, &mut memory);
            ctx.flags.materialize_all();
            assert_eq!(
                ctx.flags.materialized.to_rflags(),
                expected_flags(flags_before, &first, &second)
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[0], first_state);
                assert_eq!(x86.xmm[1], second_state);
            }
        }

        // Low 128 bits satisfy both zero reductions. Only the upper 128 bits
        // make both reductions nonzero, distinguishing VPTEST.128 from .256.
        let mut first = [0xFFu8; 32];
        let mut second = [0u8; 32];
        first[16..].fill(0x0F);
        second[16..].fill(0xFF);
        let first_state = vec_from(&first, 0xA5A5_A5A5_A5A5_A5A5);
        let second_state = vec_from(&second, 0x5A5A_5A5A_5A5A_5A5A);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = first_state;
            x86.xmm[1] = second_state;
        }
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        execute_lifted_x86(&[0xC4, 0xE2, 0x79, 0x17, 0xC1], &mut ctx, &mut memory);
        ctx.flags.materialize_all();
        assert_eq!(
            ctx.flags.materialized.to_rflags(),
            expected_flags(flags_before, &first[..16], &second[..16])
        );
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x17, 0xC1], &mut ctx, &mut memory);
        ctx.flags.materialize_all();
        assert_eq!(
            ctx.flags.materialized.to_rflags(),
            expected_flags(flags_before, &first, &second)
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], first_state);
            assert_eq!(x86.xmm[1], second_state);
        }

        memory.write(0x101, &second).unwrap();
        ctx.write_vreg(rax, 0x101);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x17, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x17, 0x00], &mut ctx, &mut memory);
        ctx.flags.materialize_all();
        assert_eq!(
            ctx.flags.materialized.to_rflags(),
            expected_flags(flags_before, &first, &second)
        );

        // A faulting source read cannot expose any part of the flag update.
        ctx.write_vreg(rax, 0x3F0);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        let fault = execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x17, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], first_state);
        }
    }
    #[test]
    fn lifted_vperm2x128_executes_controls_aliases_memory_and_faults() {
        fn reference(src1: &[u64; 4], src2: &[u64; 4], imm: u8) -> [u64; 4] {
            let mut out = [0; 4];
            for (output_half, control_shift, zero_bit) in [(0usize, 0u8, 3u8), (1, 4, 7)] {
                if (imm >> zero_bit) & 1 != 0 {
                    continue;
                }
                let control = (imm >> control_shift) & 3;
                let source = if control < 2 { src1 } else { src2 };
                let source_half = usize::from(control & 1);
                out[output_half * 2..output_half * 2 + 2]
                    .copy_from_slice(&source[source_half * 2..source_half * 2 + 2]);
            }
            out
        }

        let src1 = [10, 11, 12, 13];
        let src2 = [20, 21, 22, 23];
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for imm in [0x00, 0x31, 0x88, 0x82, 0xFF] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [upper; 16];
                x86.xmm[1][..4].copy_from_slice(&src1);
                x86.xmm[2][..4].copy_from_slice(&src2);
            }
            assert!(matches!(
                execute_lifted_x86(&[0xC4, 0xE3, 0x75, 0x06, 0xC2, imm], &mut ctx, &mut memory,),
                BlockResult::Exit(ExitReason::Halt)
            ));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(&x86.xmm[0][..4], &reference(&src1, &src2, imm));
                assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
            }
        }

        // Destination aliases SRC2; all selected halves must be captured
        // before the architectural YMM write clears upper state.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0][..4].copy_from_slice(&src2);
            x86.xmm[1][..4].copy_from_slice(&src1);
        }
        execute_lifted_x86(&[0xC4, 0xE3, 0x75, 0x06, 0xC0, 0x23], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[0][..4], &reference(&src1, &src2, 0x23));
        }

        let memory_source = [30u64, 31, 32, 33];
        let raw = memory_source
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        memory.write(0x21, &raw).unwrap();
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        ctx.write_vreg(rax, 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = [upper; 16];
            x86.xmm[9][..4].copy_from_slice(&src1);
        }
        execute_lifted_x86(
            &[0xC4, 0x63, 0x35, 0x46, 0x40, 0x20, 0x82],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[8][..4], &reference(&src1, &memory_source, 0x82));
        }

        ctx.write_vreg(rax, 0xF0);
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = sentinel;
        }
        let fault = execute_lifted_x86(
            &[0xC4, 0x63, 0x35, 0x46, 0x40, 0x20, 0x82],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[8], sentinel);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_variable_blends_execute_mask_bits_aliases_widths_and_faults() {
        fn vec_from(input: &[u8], fill: u64) -> VecValue {
            let mut value = [fill; 16];
            for (index, byte) in input.iter().copied().enumerate() {
                let word = index / 8;
                let shift = (index % 8) * 8;
                value[word] = (value[word] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
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

        fn mask_for(elem_bytes: usize, len: usize) -> Vec<u8> {
            let mut mask = vec![0x5A; len];
            for (lane, chunk) in mask.chunks_exact_mut(elem_bytes).enumerate() {
                chunk[elem_bytes - 1] = if lane % 3 == 1 { 0x80 } else { 0x7F };
            }
            mask
        }

        fn reference(src1: &[u8], src2: &[u8], mask: &[u8], elem_bytes: usize) -> Vec<u8> {
            assert_eq!(src1.len(), src2.len());
            assert_eq!(src1.len(), mask.len());
            src1.chunks_exact(elem_bytes)
                .zip(src2.chunks_exact(elem_bytes))
                .zip(mask.chunks_exact(elem_bytes))
                .flat_map(|((a, b), m)| {
                    if m[elem_bytes - 1] & 0x80 != 0 {
                        b.to_vec()
                    } else {
                        a.to_vec()
                    }
                })
                .collect()
        }

        let src1 = (0..32)
            .map(|index| (index * 29 + 0x13) as u8)
            .collect::<Vec<_>>();
        let src2 = (0..32)
            .map(|index| (0xF1u8).wrapping_sub((index * 17) as u8))
            .collect::<Vec<_>>();
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for (legacy_opcode, vex_opcode, elem_bytes) in
            [(0x10, 0x4C, 1usize), (0x14, 0x4A, 4), (0x15, 0x4B, 8)]
        {
            let mask128 = mask_for(elem_bytes, 16);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = vec_from(&mask128, 0);
                x86.xmm[1] = vec_from(&src2[..16], 0);
                x86.xmm[2] = vec_from(&src1[..16], upper);
            }
            execute_lifted_x86(
                &[0x66, 0x0F, 0x38, legacy_opcode, 0xD1],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[2], 16),
                    reference(&src1[..16], &src2[..16], &mask128, elem_bytes),
                    "legacy opcode {legacy_opcode:02X}"
                );
                assert!(x86.xmm[2][2..].iter().all(|word| *word == upper));
            }

            for (p2, width) in [(0x61, 16usize), (0x65, 32)] {
                let mask = mask_for(elem_bytes, width);
                if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                    x86.xmm[1] = sentinel;
                    x86.xmm[2] = vec_from(&src2[..width], 0);
                    x86.xmm[3] = vec_from(&src1[..width], 0);
                    x86.xmm[4] = vec_from(&mask, 0);
                }
                execute_lifted_x86(
                    &[0xC4, 0xE3, p2, vex_opcode, 0xCA, 0x40],
                    &mut ctx,
                    &mut memory,
                );
                if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                    assert_eq!(
                        bytes(&x86.xmm[1], width),
                        reference(&src1[..width], &src2[..width], &mask, elem_bytes),
                        "VEX opcode {vex_opcode:02X} width {width}"
                    );
                    assert!(x86.xmm[1][width / 8..].iter().all(|word| *word == 0));
                }
            }
        }

        // Legacy destination=XMM0 aliases the implicit mask. Its original bits
        // are both source 1 data and the lane-selection mask.
        let alias_mask = mask_for(1, 16);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = vec_from(&alias_mask, upper);
            x86.xmm[1] = vec_from(&src2[..16], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x10, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                reference(&alias_mask, &src2[..16], &alias_mask, 1)
            );
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        // VEX destination aliases the explicit mask register encoded by /is4.
        let explicit_mask = mask_for(1, 32);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = vec_from(&explicit_mask, 0);
            x86.xmm[2] = vec_from(&src2, 0);
            x86.xmm[3] = vec_from(&src1, 0);
        }
        execute_lifted_x86(&[0xC4, 0xE3, 0x65, 0x4C, 0xCA, 0x10], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[1], 32),
                reference(&src1, &src2, &explicit_mask, 1)
            );
            assert!(x86.xmm[1][4..].iter().all(|word| *word == 0));
        }

        memory.write(0x101, &src2).unwrap();
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = vec_from(&mask_for(1, 16), 0);
            x86.xmm[2] = sentinel;
        }
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x10, 0x10], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2], sentinel);
        }

        let mask256 = mask_for(4, 32);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = sentinel;
            x86.xmm[3] = vec_from(&src1, 0);
            x86.xmm[4] = vec_from(&mask256, 0);
        }
        execute_lifted_x86(&[0xC4, 0xE3, 0x65, 0x4A, 0x10, 0x40], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[2], 32), reference(&src1, &src2, &mask256, 4));
        }

        ctx.write_vreg(rax, 0x3F0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = sentinel;
        }
        let fault =
            execute_lifted_x86(&[0xC4, 0xE3, 0x65, 0x4A, 0x10, 0x40], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2], sentinel);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_pmuldq_executes_even_signed_lanes_masks_aliases_and_faults() {
        fn packed(values: &[i32], fill: u64) -> VecValue {
            let bytes = values
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>();
            let mut out = [fill; 16];
            for (i, byte) in bytes.into_iter().enumerate() {
                let shift = (i % 8) * 8;
                out[i / 8] = (out[i / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
            }
            out
        }
        fn products(a: &[i32], b: &[i32]) -> Vec<u8> {
            a.iter()
                .step_by(2)
                .zip(b.iter().step_by(2))
                .flat_map(|(x, y)| (i64::from(*x) * i64::from(*y)).to_le_bytes())
                .collect()
        }
        fn bytes(v: &VecValue, len: usize) -> Vec<u8> {
            v.iter().flat_map(|w| w.to_le_bytes()).take(len).collect()
        }

        let a = [
            -1,
            0x1111,
            2,
            0x2222,
            i32::MIN,
            7,
            i32::MAX,
            -9,
            -3,
            4,
            5,
            6,
            -7,
            8,
            9,
            10,
        ];
        let b = [
            3,
            -1,
            i32::MAX,
            2,
            -1,
            5,
            2,
            6,
            -11,
            12,
            -13,
            14,
            15,
            -16,
            -17,
            18,
        ];
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = packed(&a[..4], upper);
            x86.xmm[1] = packed(&b[..4], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x28, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 16), products(&a[..4], &b[..4]));
            assert!(x86.xmm[0][2..].iter().all(|w| *w == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = packed(&a[..8], 0);
            x86.xmm[2] = packed(&b[..8], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x28, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 32), products(&a[..8], &b[..8]));
            assert!(x86.xmm[0][4..].iter().all(|w| *w == 0));
        }

        let raw = products(&a, &b);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
            x86.xmm[1] = packed(&a, 0);
            x86.xmm[2] = packed(&b, 0);
        }
        ctx.write_vreg(k1, 0x55);
        execute_lifted_x86(&[0x62, 0xF2, 0xF5, 0x49, 0x28, 0xC2], &mut ctx, &mut memory);
        let mut expected = vec![0x6B; 64];
        for lane in 0..8 {
            if (0x55 >> lane) & 1 != 0 {
                expected[lane * 8..lane * 8 + 8].copy_from_slice(&raw[lane * 8..lane * 8 + 8]);
            }
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 64), expected);
        }

        // Same-register VEX source/destination must be captured before zeroing.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = packed(&a[..8], 0);
            x86.xmm[2] = packed(&b[..8], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x28, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 32), products(&a[..8], &b[..8]));
        }

        memory
            .write(
                0x3F8,
                &b[..2]
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        ctx.write_vreg(rax, 0x3F8);
        ctx.write_vreg(k1, 1);
        let ok = execute_lifted_x86(&[0x62, 0xF2, 0xF5, 0x49, 0x28, 0x00], &mut ctx, &mut memory);
        assert!(!matches!(
            ok,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        ctx.write_vreg(k1, 2);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF2, 0xF5, 0x49, 0x28, 0x00], &mut ctx, &mut memory);
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
    fn lifted_pmuludq_executes_even_unsigned_lanes_widths_and_masks() {
        fn packed(values: &[u32], fill: u64) -> VecValue {
            let raw = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            let mut out = [fill; 16];
            for (index, byte) in raw.into_iter().enumerate() {
                let shift = (index % 8) * 8;
                out[index / 8] =
                    (out[index / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
            }
            out
        }

        fn products(a: &[u32], b: &[u32]) -> Vec<u8> {
            a.iter()
                .step_by(2)
                .zip(b.iter().step_by(2))
                .flat_map(|(x, y)| (u64::from(*x) * u64::from(*y)).to_le_bytes())
                .collect()
        }

        fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(len)
                .collect()
        }

        let a = [
            u32::MAX,
            1,
            0x8000_0000,
            2,
            3,
            4,
            0xFFFF_0001,
            5,
            7,
            6,
            11,
            8,
            13,
            9,
            17,
            10,
        ];
        let b = [
            u32::MAX,
            10,
            2,
            11,
            0xF000_0000,
            12,
            0x8000_0001,
            13,
            19,
            14,
            23,
            15,
            29,
            16,
            31,
            17,
        ];
        let flags_before = 0xCD7;
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(1);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = packed(&a[..4], upper);
            x86.xmm[1] = packed(&b[..4], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0xF4, 0xD1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[2], 16), products(&a[..4], &b[..4]));
            assert!(x86.xmm[2][2..].iter().all(|word| *word == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = [upper; 16];
            x86.xmm[9] = packed(&a[..8], 0);
            x86.xmm[10] = packed(&b[..8], 0);
        }
        execute_lifted_x86(&[0xC4, 0x41, 0x35, 0xF4, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[8], 32), products(&a[..8], &b[..8]));
            assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = [0x6B6B_6B6B_6B6B_6B6B; 16];
            x86.xmm[17] = packed(&a, 0);
            x86.xmm[18] = packed(&b, 0);
            x86.k[1] = 0x55;
        }
        execute_lifted_x86(&[0x62, 0xA1, 0xF5, 0x41, 0xF4, 0xC2], &mut ctx, &mut memory);
        let raw = products(&a, &b);
        let mut expected = vec![0x6B; 64];
        for lane in 0..8 {
            if (0x55 >> lane) & 1 != 0 {
                expected[lane * 8..lane * 8 + 8].copy_from_slice(&raw[lane * 8..lane * 8 + 8]);
            }
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[16], 64), expected);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_pmulld_pmulq_execute_widths_masks_broadcasts_and_fault_suppression() {
        fn packed32(values: &[u32], fill: u64) -> VecValue {
            let raw = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            let mut out = [fill; 16];
            for (index, byte) in raw.into_iter().enumerate() {
                let shift = (index % 8) * 8;
                out[index / 8] =
                    (out[index / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
            }
            out
        }

        fn lanes32(value: &VecValue, count: usize) -> Vec<u32> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 4)
                .collect::<Vec<_>>()
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }

        let a32 = (0..16)
            .map(|lane| 0x8000_0001u32.wrapping_add(lane * 0x1111_1111))
            .collect::<Vec<_>>();
        let b32 = (0..16)
            .map(|lane| 0xFFFF_0001u32.wrapping_sub(lane * 0x0101_0101))
            .collect::<Vec<_>>();
        let a64 = [u64::MAX, 0x8000_0000_0000_0001, 3, 5, 7, 11, 13, 17];
        let b64 = [19, 23, u64::MAX, 29, 31, 37, 41, 43];
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = packed32(&a32, 0);
            x86.xmm[18] = packed32(&b32, 0);
        }
        execute_lifted_x86(&[0x62, 0xA2, 0x75, 0x40, 0x40, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                lanes32(&x86.xmm[16], 16),
                a32.iter()
                    .zip(&b32)
                    .map(|(a, b)| a.wrapping_mul(*b))
                    .collect::<Vec<_>>(),
            );
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[20][..8].copy_from_slice(&a64);
            x86.xmm[21][..8].copy_from_slice(&b64);
        }
        execute_lifted_x86(&[0x62, 0xA2, 0xDD, 0x40, 0x40, 0xDD], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                &x86.xmm[19][..8],
                &a64.iter()
                    .zip(&b64)
                    .map(|(a, b)| a.wrapping_mul(*b))
                    .collect::<Vec<_>>(),
            );
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        memory.write(0xFC, &7u32.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0xF8);
        ctx.write_vreg(k1, 0xA55A);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
            x86.xmm[1] = packed32(&a32, 0);
        }
        execute_lifted_x86(
            &[0x62, 0xF2, 0x75, 0xD9, 0x40, 0x40, 0x01],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let actual = lanes32(&x86.xmm[0], 16);
            for lane in 0..16 {
                assert_eq!(
                    actual[lane],
                    if (0xA55A >> lane) & 1 != 0 {
                        a32[lane].wrapping_mul(7)
                    } else {
                        0
                    },
                );
            }
        }

        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k1, 0);
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xF2, 0x75, 0xD9, 0x40, 0x40, 0x01],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        ctx.write_vreg(k1, 1);
        let fault = execute_lifted_x86(
            &[0x62, 0xF2, 0x75, 0xD9, 0x40, 0x40, 0x01],
            &mut ctx,
            &mut memory,
        );
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
    fn lifted_pmullw_executes_widths_masks_alignment_and_fault_suppression() {
        fn packed(values: &[u16], fill: u64) -> VecValue {
            let raw = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            let mut out = [fill; 16];
            for (index, byte) in raw.into_iter().enumerate() {
                let shift = (index % 8) * 8;
                out[index / 8] =
                    (out[index / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
            }
            out
        }

        fn lanes(value: &VecValue, count: usize) -> Vec<u16> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 2)
                .collect::<Vec<_>>()
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }

        let a = (0..32)
            .map(|lane| 0x8001u16.wrapping_add((lane as u16).wrapping_mul(0x1111)))
            .collect::<Vec<_>>();
        let b = (0..32)
            .map(|lane| 0xFFF1u16.wrapping_sub((lane as u16).wrapping_mul(0x0101)))
            .collect::<Vec<_>>();
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = packed(&a[..8], upper);
            x86.xmm[1] = packed(&b[..8], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0xD5, 0xD1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                lanes(&x86.xmm[2], 8),
                a[..8]
                    .iter()
                    .zip(&b[..8])
                    .map(|(a, b)| a.wrapping_mul(*b))
                    .collect::<Vec<_>>(),
            );
            assert!(x86.xmm[2][2..].iter().all(|word| *word == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = [upper; 16];
            x86.xmm[9] = packed(&a[..16], 0);
            x86.xmm[10] = packed(&b[..16], 0);
        }
        execute_lifted_x86(&[0xC4, 0x41, 0x35, 0xD5, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                lanes(&x86.xmm[8], 16),
                a[..16]
                    .iter()
                    .zip(&b[..16])
                    .map(|(a, b)| a.wrapping_mul(*b))
                    .collect::<Vec<_>>(),
            );
            assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = [upper; 16];
            x86.xmm[17] = packed(&a, 0);
            x86.xmm[18] = packed(&b, 0);
            x86.k[1] = 0xA5A5_5A5A;
        }
        execute_lifted_x86(&[0x62, 0xA1, 0x75, 0x41, 0xD5, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let actual = lanes(&x86.xmm[16], 32);
            for lane in 0..32 {
                assert_eq!(
                    actual[lane],
                    if (0xA5A5_5A5Au64 >> lane) & 1 != 0 {
                        a[lane].wrapping_mul(b[lane])
                    } else {
                        0xA5A5
                    },
                );
            }
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let memory_words = (0..8).map(|lane| lane as u16 + 3).collect::<Vec<_>>();
        let raw = memory_words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        memory.write(0xF0, &raw).unwrap();
        ctx.write_vreg(rax, 0xF0);
        ctx.write_vreg(k1, 0xFF);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0; 16];
            x86.xmm[1] = packed(&a, 0);
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xD5, 0x00], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.write_vreg(k1, 1 << 8);
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xD5, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        // Legacy SSE requires 16-byte alignment before its source load.
        ctx.write_vreg(rax, 0x71);
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0xD5, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_pmulhw_pmulhuw_execute_signedness_masks_alignment_and_faults() {
        fn packed(values: &[u16], fill: u64) -> VecValue {
            let raw = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            let mut out = [fill; 16];
            for (index, byte) in raw.into_iter().enumerate() {
                let shift = (index % 8) * 8;
                out[index / 8] =
                    (out[index / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
            }
            out
        }

        fn lanes(value: &VecValue, count: usize) -> Vec<u16> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 2)
                .collect::<Vec<_>>()
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }

        fn signed_high(a: u16, b: u16) -> u16 {
            let product = i32::from(a as i16).wrapping_mul(i32::from(b as i16));
            ((product as u32) >> 16) as u16
        }

        fn unsigned_high(a: u16, b: u16) -> u16 {
            ((u32::from(a) * u32::from(b)) >> 16) as u16
        }

        let a = (0..32)
            .map(|lane| 0x8001u16.wrapping_add((lane as u16).wrapping_mul(0x1111)))
            .collect::<Vec<_>>();
        let b = (0..32)
            .map(|lane| 0xFFF1u16.wrapping_sub((lane as u16).wrapping_mul(0x0101)))
            .collect::<Vec<_>>();
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for (opcode, reference) in [
            (0xE5u8, signed_high as fn(u16, u16) -> u16),
            (0xE4, unsigned_high as fn(u16, u16) -> u16),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[2] = packed(&a[..8], upper);
                x86.xmm[1] = packed(&b[..8], 0);
            }
            execute_lifted_x86(&[0x66, 0x0F, opcode, 0xD1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    lanes(&x86.xmm[2], 8),
                    a[..8]
                        .iter()
                        .zip(&b[..8])
                        .map(|(a, b)| reference(*a, *b))
                        .collect::<Vec<_>>(),
                );
                assert!(x86.xmm[2][2..].iter().all(|word| *word == upper));
            }
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = [upper; 16];
            x86.xmm[9] = packed(&a[..16], 0);
            x86.xmm[10] = packed(&b[..16], 0);
        }
        execute_lifted_x86(&[0xC4, 0x41, 0x35, 0xE5, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                lanes(&x86.xmm[8], 16),
                a[..16]
                    .iter()
                    .zip(&b[..16])
                    .map(|(a, b)| signed_high(*a, *b))
                    .collect::<Vec<_>>(),
            );
            assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = [upper; 16];
            x86.xmm[17] = packed(&a, 0);
            x86.xmm[18] = packed(&b, 0);
            x86.k[1] = 0xA5A5_5A5A;
        }
        execute_lifted_x86(&[0x62, 0xA1, 0x75, 0x41, 0xE4, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let actual = lanes(&x86.xmm[16], 32);
            for lane in 0..32 {
                assert_eq!(
                    actual[lane],
                    if (0xA5A5_5A5Au64 >> lane) & 1 != 0 {
                        unsigned_high(a[lane], b[lane])
                    } else {
                        0xA5A5
                    },
                );
            }
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let memory_words = (0..8).map(|lane| lane as u16 + 3).collect::<Vec<_>>();
        let raw = memory_words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        memory.write(0xF0, &raw).unwrap();
        ctx.write_vreg(rax, 0xF0);
        ctx.write_vreg(k1, 0xFF);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0; 16];
            x86.xmm[1] = packed(&a, 0);
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xE5, 0x00], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.write_vreg(k1, 1 << 8);
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xE5, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }
        ctx.write_vreg(rax, 0xF1);
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0xE4, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn vfma_executes_fused_f32_f64_and_sign_controls() {
        let regs = [
            X86Reg::Xmm(0),
            X86Reg::Xmm(1),
            X86Reg::Xmm(2),
            X86Reg::Xmm(3),
        ]
        .map(|reg| VReg::Arch(ArchReg::X86(reg)));
        for (elem, values, expected) in [
            (
                VecElementType::F32,
                [
                    f32::to_bits(1.5) as u64,
                    f32::to_bits(4.0) as u64,
                    f32::to_bits(2.0) as u64,
                ],
                f32::to_bits(-8.0) as u64,
            ),
            (
                VecElementType::F64,
                [f64::to_bits(1.5), f64::to_bits(4.0), f64::to_bits(2.0)],
                f64::to_bits(-8.0),
            ),
        ] {
            let mut ctx = SmirContext::new_x86_64();
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0][0] = values[0];
                x86.xmm[1][0] = values[1];
                x86.xmm[2][0] = values[2];
            }
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::VFma {
                    dst: regs[3],
                    src1: regs[0],
                    src2: regs[1],
                    acc: regs[2],
                    elem,
                    lanes: 1,
                    negate_product: true,
                    negate_acc: true,
                },
            );
            builder.set_terminator(Terminator::Trap {
                kind: TrapKind::Halt,
            });
            let func = builder.finish();
            let mut memory = FlatMemory::new(0x100);
            assert!(matches!(
                SmirInterpreter::new().execute_block(&mut ctx, &mut memory, &func.blocks[0]),
                BlockResult::Exit(ExitReason::Halt)
            ));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[3][0], expected);
                assert!(x86.xmm[3][1..].iter().all(|word| *word == 0));
            }
        }
    }
    #[test]
    fn reciprocal_estimates_execute_special_cases_and_accuracy_bound() {
        fn packed_f32(bits: &[u32]) -> VecValue {
            let mut out = [0; 16];
            for (lane, value) in bits.iter().copied().enumerate() {
                SmirInterpreter::set_lane(&mut out, lane as u8, 32, u64::from(value));
            }
            out
        }

        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = packed_f32(&[0, 0x8000_0000, 1, 0x8000_0001]);
            x86.xmm[1] = packed_f32(&[
                f32::INFINITY.to_bits(),
                f32::NEG_INFINITY.to_bits(),
                0x7FA1_2345,
                0xFFC5_4321,
            ]);
            x86.xmm[2] = packed_f32(&[
                7.0f32.to_bits(),
                (-11.0f32).to_bits(),
                f32::MAX.to_bits(),
                f32::MIN_POSITIVE.to_bits(),
            ]);
            x86.xmm[3] = packed_f32(&[
                4.0f32.to_bits(),
                (-4.0f32).to_bits(),
                f32::INFINITY.to_bits(),
                f32::NEG_INFINITY.to_bits(),
            ]);
        }
        let regs = (0..8)
            .map(|index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index))))
            .collect::<Vec<_>>();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        for (dst, src, op) in [
            (4usize, 0usize, VecUnaryOp::FRecipEstimate),
            (5, 1, VecUnaryOp::FRecipEstimate),
            (6, 2, VecUnaryOp::FRecipEstimate),
            (7, 3, VecUnaryOp::FRsqrtEstimate),
        ] {
            builder.push_op(
                0x1000,
                OpKind::VUnary {
                    dst: regs[dst],
                    src: regs[src],
                    elem: VecElementType::F32,
                    lanes: 4,
                    op,
                },
            );
        }
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let func = builder.finish();
        let mut memory = FlatMemory::new(0x100);
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut ctx, &mut memory, &func.blocks[0]),
            BlockResult::Exit(ExitReason::Halt)
        ));

        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                (0..4)
                    .map(|lane| SmirInterpreter::get_lane(&x86.xmm[4], lane, 32) as u32)
                    .collect::<Vec<_>>(),
                [
                    f32::INFINITY.to_bits(),
                    f32::NEG_INFINITY.to_bits(),
                    f32::INFINITY.to_bits(),
                    f32::NEG_INFINITY.to_bits(),
                ]
            );
            assert_eq!(
                (0..4)
                    .map(|lane| SmirInterpreter::get_lane(&x86.xmm[5], lane, 32) as u32)
                    .collect::<Vec<_>>(),
                [0, 0x8000_0000, 0x7FE1_2345, 0xFFC5_4321]
            );
            // Exact binary32 evaluation is a valid deterministic member of the
            // architectural estimate set. Verify the architectural error bound
            // independently in binary64 rather than requiring a hardware bit pattern.
            for (lane, input) in [(0u8, 7.0f64), (1, -11.0)] {
                let actual =
                    f64::from(f32::from_bits(
                        SmirInterpreter::get_lane(&x86.xmm[6], lane, 32) as u32,
                    ));
                let exact = 1.0f64 / input;
                let relative_error = ((actual - exact) / exact).abs();
                assert!(relative_error <= 1.5 * 2.0f64.powi(-12));
            }
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[6], 2, 32), 0);
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[6], 3, 32),
                u64::from((1.0f32 / f32::MIN_POSITIVE).to_bits())
            );
            assert_eq!(
                (0..4)
                    .map(|lane| SmirInterpreter::get_lane(&x86.xmm[7], lane, 32) as u32)
                    .collect::<Vec<_>>(),
                [0.5f32.to_bits(), 0xFFC0_0000, 0, 0xFFC0_0000]
            );
        }
    }
    #[test]
    fn lifted_vex_fma3_executes_orders_sign_families_alternation_scalar_and_faults() {
        fn packed_f32(values: &[f32], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (lane, value) in values.iter().copied().enumerate() {
                SmirInterpreter::set_lane(&mut out, lane as u8, 32, u64::from(value.to_bits()));
            }
            out
        }

        let old = [1.5, -2.0, 3.25, -4.5, 5.0, -6.25, 7.5, -8.0];
        let vex = [0.5, 1.25, -1.5, 2.0, -2.5, 3.0, -3.5, 4.0];
        let rm = [2.0, -3.0, 4.0, -5.0, 6.0, -7.0, 8.0, -9.0];
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        for opcode in [
            0x96u8, 0x97, 0x98, 0x9A, 0x9C, 0x9E, 0xA6, 0xA7, 0xA8, 0xAA, 0xAC, 0xAE, 0xB6, 0xB7,
            0xB8, 0xBA, 0xBC, 0xBE,
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[2] = packed_f32(&old, 0xA5A5_A5A5_A5A5_A5A5);
                x86.xmm[1] = packed_f32(&vex, 0);
                x86.xmm[3] = packed_f32(&rm, 0);
            }
            execute_lifted_x86(&[0xC4, 0xE2, 0x75, opcode, 0xD3], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                for lane in 0..8u8 {
                    let (a, b, c) = match opcode >> 4 {
                        9 => (old[lane as usize], rm[lane as usize], vex[lane as usize]),
                        10 => (vex[lane as usize], old[lane as usize], rm[lane as usize]),
                        11 => (vex[lane as usize], rm[lane as usize], old[lane as usize]),
                        _ => unreachable!(),
                    };
                    let low = opcode & 0xF;
                    let alternating = matches!(low, 6 | 7);
                    let negate_product = matches!(low, 0xC | 0xE);
                    let negate_acc = if alternating {
                        (lane & 1 == 0) == (low == 6)
                    } else {
                        matches!(low, 0xA | 0xE)
                    };
                    let expected = (if negate_product { -a } else { a })
                        .mul_add(b, if negate_acc { -c } else { c });
                    assert_eq!(
                        SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                        u64::from(expected.to_bits()),
                        "opcode {opcode:02X}, lane {lane}",
                    );
                }
                assert!(x86.xmm[2][4..].iter().all(|word| *word == 0));
            }
        }

        // Scalar FMA replaces only lane zero, preserves the old destination's
        // remaining XMM lanes, and clears state above bit 127.
        let upper = [9.0f32, -10.0, 11.0];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = packed_f32(&[2.0, upper[0], upper[1], upper[2]], u64::MAX);
            x86.xmm[1] = packed_f32(&[3.0], 0);
            x86.xmm[3] = packed_f32(&[5.0], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x71, 0xB9, 0xD3], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[2], 0, 32),
                17.0f32.to_bits() as u64
            );
            for lane in 1..4u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                    u64::from(upper[(lane - 1) as usize].to_bits())
                );
            }
            assert!(x86.xmm[2][2..].iter().all(|word| *word == 0));
        }

        // A faulting source load occurs before any destination commit.
        let rdi = VReg::Arch(ArchReg::X86(X86Reg::Rdi));
        ctx.write_vreg(rdi, 0x1000);
        let sentinel = [0x6B6B_6B6B_6B6B_6B6B; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0x71, 0x99, 0x17], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2], sentinel);
        }

        // EVEX packed masking merges or zeroes per lane after the fused result.
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let evex_mask = 0xA55Au64;
        ctx.write_vreg(k1, evex_mask);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = packed_f32(&old, 0);
            x86.xmm[1] = packed_f32(&vex, 0);
            x86.xmm[3] = packed_f32(&rm, 0);
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x49, 0x98, 0xD3], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..16u8 {
                let expected = if lane < 8 && evex_mask >> lane & 1 != 0 {
                    old[lane as usize].mul_add(rm[lane as usize], vex[lane as usize])
                } else if lane < 8 {
                    old[lane as usize]
                } else {
                    0.0
                };
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                    u64::from(expected.to_bits())
                );
            }
        }

        // A zero EVEX mask suppresses a broadcast source fault and zeroing
        // clears every destination lane. Activating one lane exposes the fault
        // without committing the destination.
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        ctx.write_vreg(rax, 0x1000);
        ctx.write_vreg(k1, 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x75, 0xD9, 0x98, 0x10], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert!(x86.xmm[2].iter().all(|word| *word == 0));
        }
        ctx.write_vreg(k1, 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x75, 0xD9, 0x98, 0x10], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2], sentinel);
        }
    }
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
