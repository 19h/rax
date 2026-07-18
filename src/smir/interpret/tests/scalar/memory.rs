//! scalar::memory tests

use super::*;
use crate::smir::interpret::*;
use crate::smir::interpret::tests::*;

    #[test]
    fn lifted_bit_tests_execute_partial_register_and_signed_memory_offsets() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let mut memory = FlatMemory::new(0x4000);
        let mut ctx = SmirContext::new_x86_64();

        ctx.write_vreg(rax, 0x1122_3344_5566_0000);
        ctx.write_vreg(rcx, 15);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD6);
        ctx.flags.lazy = None;
        execute_lifted_x86(&[0x66, 0x0F, 0xAB, 0xC8], &mut ctx, &mut memory); // BTS AX,CX
        ctx.flags.materialize_all();
        assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_8000);
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD6);

        memory
            .write(0x2008, &0x8000_0000_0000_0000u64.to_le_bytes())
            .unwrap();
        ctx.write_vreg(rax, 0x2010);
        ctx.write_vreg(rcx, u64::MAX); // signed bit index -1 => [base-8], bit 63
        execute_lifted_x86(&[0x48, 0x0F, 0xA3, 0x08], &mut ctx, &mut memory);
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);

        memory.write(0x2008, &1u64.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x2000);
        ctx.write_vreg(rcx, 64); // +1 qword, bit 0
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD6);
        ctx.flags.lazy = None;
        execute_lifted_x86(&[0x48, 0x0F, 0xA3, 0x08], &mut ctx, &mut memory);
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);

        memory.write(0x200C, &0x8000_0000u32.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x2010);
        ctx.write_vreg(rcx, 0xFFFF_FFFF); // signed 32-bit index -1 => [base-4], bit31
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD6);
        ctx.flags.lazy = None;
        execute_lifted_x86(&[0x0F, 0xA3, 0x08], &mut ctx, &mut memory);
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn lifted_moffs_pop_rm_and_group4_execute_memory_register_effects() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
        let mut memory = FlatMemory::new(0x5000);
        let mut ctx = SmirContext::new_x86_64();

        memory.write(0x2000, &[0xA5]).unwrap();
        ctx.write_vreg(rax, 0x1122_3344_5566_7788);
        execute_lifted_x86(&[0xA0, 0x00, 0x20, 0, 0, 0, 0, 0, 0], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_77A5, "MOV AL,moffs8");

        ctx.write_vreg(rax, 0xAABB_CCDD_EEFF_1020);
        execute_lifted_x86(&[0xA3, 0x00, 0x21, 0, 0, 0, 0, 0, 0], &mut ctx, &mut memory);
        let mut dword = [0u8; 4];
        memory.read(0x2100, &mut dword).unwrap();
        assert_eq!(u32::from_le_bytes(dword), 0xEEFF_1020, "MOV moffs32,EAX");

        ctx.write_vreg(rsp, 0x3000);
        memory
            .write(0x3000, &0x0123_4567_89AB_CDEFu64.to_le_bytes())
            .unwrap();
        execute_lifted_x86(&[0x8F, 0x44, 0x24, 0x08], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rsp), 0x3008);
        let mut popped = [0u8; 8];
        memory.read(0x3010, &mut popped).unwrap();
        assert_eq!(u64::from_le_bytes(popped), 0x0123_4567_89AB_CDEF);

        ctx.write_vreg(rax, 0x1122_3344_5566_77FF);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0x1); // CF set
        ctx.flags.lazy = None;
        execute_lifted_x86(&[0xFE, 0xC0], &mut ctx, &mut memory); // INC AL
        ctx.flags.materialize_all();
        assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_7700);
        assert!(ctx.flags.materialized.cf, "INC must preserve CF");
        assert!(ctx.flags.materialized.zf, "FF + 1 must set ZF");

        memory.write(0x2200, &[0]).unwrap();
        ctx.write_vreg(rax, 0x2200);
        execute_lifted_x86(&[0xFE, 0x08], &mut ctx, &mut memory); // DEC byte [RAX]
        let mut byte = [0u8; 1];
        memory.read(0x2200, &mut byte).unwrap();
        assert_eq!(byte[0], 0xFF);

        let mut inner = FlatMemory::new(0x5000);
        inner
            .write(0x3000, &0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes())
            .unwrap();
        let mut read_only = StoreFaultMemory {
            inner,
            stores_before_fault: 0,
        };
        ctx.write_vreg(rsp, 0x3000);
        let exit = execute_lifted_x86(&[0x8F, 0x44, 0x24, 0x08], &mut ctx, &mut read_only);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        assert_eq!(
            ctx.read_vreg(rsp),
            0x3000,
            "faulting POP r/m destination must not commit RSP"
        );

        let mut inner = FlatMemory::new(0x5000);
        inner.write(0x2200, &[0]).unwrap();
        let mut read_only = StoreFaultMemory {
            inner,
            stores_before_fault: 0,
        };
        ctx.write_vreg(rax, 0x2200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;
        let exit = execute_lifted_x86(&[0xFE, 0x08], &mut ctx, &mut read_only);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        ctx.flags.materialize_all();
        assert_eq!(
            ctx.flags.materialized.to_rflags(),
            0xCD7,
            "faulting Group-4 memory update must not commit flags"
        );

        let mut wide_memory = FlatMemory::new(0x30000);
        wide_memory
            .write(0x1FFFF, &0xABCDu16.to_le_bytes())
            .unwrap();
        ctx.write_vreg(rsp, 0x1FFFF);
        execute_lifted_x86(&[0x66, 0x8F, 0xC4], &mut ctx, &mut wide_memory); // POP SP
        assert_eq!(
            ctx.read_vreg(rsp),
            0x2ABCD,
            "POP SP must take upper bits from the incremented RSP"
        );
    }
    #[test]
    fn lifted_memory_rmw_store_faults_preserve_flags_memory_and_registers() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let cases: &[(&str, &[u8])] = &[
            ("add [rax],ecx", &[0x01, 0x08]),
            ("adc [rax],1", &[0x83, 0x10, 0x01]),
            ("shl dword [rax],1", &[0xC1, 0x20, 0x01]),
            ("ror byte [rax],1", &[0xD0, 0x08]),
            ("rcr qword [rax],cl", &[0x48, 0xD3, 0x18]),
            ("neg dword [rax]", &[0xF7, 0x18]),
            ("inc qword [rax]", &[0x48, 0xFF, 0x00]),
            ("dec word [rax]", &[0x66, 0xFF, 0x08]),
        ];

        for (name, bytes) in cases {
            let seed = 0x0123_4567_89AB_CDEFu64.to_le_bytes();
            let mut inner = FlatMemory::new(0x1000);
            inner.write(0x200, &seed).unwrap();
            let mut memory = StoreFaultMemory {
                inner,
                stores_before_fault: 0,
            };
            let mut ctx = SmirContext::new_x86_64();
            ctx.write_vreg(rax, 0x200);
            ctx.write_vreg(rcx, 1);
            ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
            ctx.flags.lazy = None;

            let exit = execute_lifted_x86(bytes, &mut ctx, &mut memory);
            assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
                ),
                "{name}",
            );
            ctx.flags.materialize_all();
            assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7, "{name}: flags");
            assert_eq!(ctx.read_vreg(rax), 0x200, "{name}: RAX");
            assert_eq!(ctx.read_vreg(rcx), 1, "{name}: RCX");
            let mut actual = [0u8; 8];
            memory.inner.read(0x200, &mut actual).unwrap();
            assert_eq!(actual, seed, "{name}: memory");
        }
    }
    #[test]
    fn lifted_memory_rmw_success_commits_results_and_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();
        ctx.write_vreg(rax, 0x200);

        memory.write(0x200, &1u32.to_le_bytes()).unwrap();
        ctx.write_vreg(rcx, 2);
        execute_lifted_x86(&[0x01, 0x08], &mut ctx, &mut memory); // ADD [RAX],ECX
        let mut dword = [0u8; 4];
        memory.read(0x200, &mut dword).unwrap();
        assert_eq!(u32::from_le_bytes(dword), 3);
        ctx.flags.materialize_all();
        assert!(!ctx.flags.materialized.zf);
        assert!(!ctx.flags.materialized.cf);

        memory.write(0x200, &[0x80]).unwrap();
        execute_lifted_x86(&[0xD0, 0x20], &mut ctx, &mut memory); // SHL byte [RAX],1
        let mut byte = [0u8; 1];
        memory.read(0x200, &mut byte).unwrap();
        assert_eq!(byte[0], 0);
        ctx.flags.materialize_all();
        assert!(ctx.flags.materialized.zf);
        assert!(ctx.flags.materialized.cf);

        memory.write(0x200, &1u32.to_le_bytes()).unwrap();
        execute_lifted_x86(&[0xF7, 0x18], &mut ctx, &mut memory); // NEG dword [RAX]
        memory.read(0x200, &mut dword).unwrap();
        assert_eq!(u32::from_le_bytes(dword), u32::MAX);
        ctx.flags.materialize_all();
        assert!(ctx.flags.materialized.cf);
        assert!(ctx.flags.materialized.sf);

        memory.write(0x200, &u64::MAX.to_le_bytes()).unwrap();
        ctx.flags.materialized.cf = true;
        ctx.flags.lazy = None;
        execute_lifted_x86(&[0x48, 0xFF, 0x00], &mut ctx, &mut memory); // INC qword [RAX]
        let mut qword = [0u8; 8];
        memory.read(0x200, &mut qword).unwrap();
        assert_eq!(u64::from_le_bytes(qword), 0);
        ctx.flags.materialize_all();
        assert!(ctx.flags.materialized.cf, "INC preserves CF");
        assert!(ctx.flags.materialized.zf);
    }
    #[test]
    fn lifted_locked_memory_rmw_faults_preserve_flags_memory_and_registers() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        for (name, bytes) in [
            ("LOCK ADD", &[0xF0, 0x01, 0x08][..]),
            ("LOCK ADC immediate", &[0xF0, 0x83, 0x10, 0x01][..]),
            ("LOCK INC", &[0xF0, 0x48, 0xFF, 0x00][..]),
            ("LOCK NOT", &[0xF0, 0xF7, 0x10][..]),
            ("LOCK NEG", &[0xF0, 0xF7, 0x18][..]),
        ] {
            let seed = 0x0123_4567_89AB_CDEFu64.to_le_bytes();
            let mut inner = FlatMemory::new(0x1000);
            inner.write(0x200, &seed).unwrap();
            let mut memory = StoreFaultMemory {
                inner,
                stores_before_fault: 0,
            };
            let mut ctx = SmirContext::new_x86_64();
            ctx.write_vreg(rax, 0x200);
            ctx.write_vreg(rcx, 1);
            ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
            ctx.flags.lazy = None;

            let exit = execute_lifted_x86(bytes, &mut ctx, &mut memory);
            assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
                ),
                "{name}: {exit:?}",
            );
            ctx.flags.materialize_all();
            assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7, "{name}: flags");
            assert_eq!(ctx.read_vreg(rax), 0x200, "{name}: RAX");
            assert_eq!(ctx.read_vreg(rcx), 1, "{name}: RCX");
            let mut actual = [0u8; 8];
            memory.inner.read(0x200, &mut actual).unwrap();
            assert_eq!(actual, seed, "{name}: memory");
        }
    }
    #[test]
    fn lifted_locked_memory_rmw_success_commits_atomic_results_and_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();
        ctx.write_vreg(rax, 0x200);

        memory.write(0x200, &u32::MAX.to_le_bytes()).unwrap();
        ctx.write_vreg(rcx, 1);
        execute_lifted_x86(&[0xF0, 0x01, 0x08], &mut ctx, &mut memory);
        let mut dword = [0u8; 4];
        memory.read(0x200, &mut dword).unwrap();
        assert_eq!(u32::from_le_bytes(dword), 0);
        ctx.flags.materialize_all();
        assert!(ctx.flags.materialized.zf);
        assert!(ctx.flags.materialized.cf);

        memory.write(0x200, &5u32.to_le_bytes()).unwrap();
        ctx.flags.materialized = MaterializedFlags::from_rflags(1);
        ctx.flags.lazy = None;
        execute_lifted_x86(&[0xF0, 0x83, 0x10, 0x01], &mut ctx, &mut memory);
        memory.read(0x200, &mut dword).unwrap();
        assert_eq!(u32::from_le_bytes(dword), 7, "ADC includes incoming CF");
        ctx.flags.materialize_all();
        assert!(!ctx.flags.materialized.cf);
        assert!(!ctx.flags.materialized.zf);

        memory.write(0x200, &u64::MAX.to_le_bytes()).unwrap();
        ctx.flags.materialized = MaterializedFlags::from_rflags(1);
        ctx.flags.lazy = None;
        execute_lifted_x86(&[0xF0, 0x48, 0xFF, 0x00], &mut ctx, &mut memory);
        let mut qword = [0u8; 8];
        memory.read(0x200, &mut qword).unwrap();
        assert_eq!(u64::from_le_bytes(qword), 0);
        ctx.flags.materialize_all();
        assert!(ctx.flags.materialized.cf, "LOCK INC preserves CF");
        assert!(ctx.flags.materialized.zf);

        memory.write(0x200, &[0x0F]).unwrap();
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;
        execute_lifted_x86(&[0xF0, 0xF6, 0x10], &mut ctx, &mut memory);
        let mut byte = [0u8; 1];
        memory.read(0x200, &mut byte).unwrap();
        assert_eq!(byte[0], 0xF0);
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7, "LOCK NOT flags");

        memory.write(0x200, &1u32.to_le_bytes()).unwrap();
        execute_lifted_x86(&[0xF0, 0xF7, 0x18], &mut ctx, &mut memory);
        memory.read(0x200, &mut dword).unwrap();
        assert_eq!(u32::from_le_bytes(dword), u32::MAX);
        ctx.flags.materialize_all();
        assert!(ctx.flags.materialized.cf);
        assert!(ctx.flags.materialized.sf);
    }
    #[test]
    fn lifted_xchg_swaps_legacy_high_bytes_register_and_memory_forms() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
        let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();

        ctx.write_vreg(rax, 0x1122_3344_5566_1256);
        ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_3456);
        execute_lifted_x86(&[0x86, 0xFC], &mut ctx, &mut memory); // XCHG AH,BH
        assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_3456);
        assert_eq!(ctx.read_vreg(rbx), 0xAABB_CCDD_EEFF_1256);

        ctx.write_vreg(rax, 0x1122_3344_5566_1234);
        execute_lifted_x86(&[0x86, 0xE0], &mut ctx, &mut memory); // XCHG AL,AH
        assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_3412);

        ctx.write_vreg(rax, 0x1122_3344_5566_A5CC);
        ctx.write_vreg(rbx, 0x200);
        memory.write(0x200, &[0x6D]).unwrap();
        execute_lifted_x86(&[0x86, 0x23], &mut ctx, &mut memory); // XCHG [RBX],AH
        assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_6DCC);
        let mut byte = [0u8; 1];
        memory.read(0x200, &mut byte).unwrap();
        assert_eq!(byte[0], 0xA5);

        ctx.write_vreg(rax, 0x11);
        ctx.write_vreg(rsp, 0x1234_5678_9ABC_DE22);
        execute_lifted_x86(&[0x40, 0x86, 0xC4], &mut ctx, &mut memory); // XCHG SPL,AL
        assert_eq!(ctx.read_vreg(rax), 0x22);
        assert_eq!(ctx.read_vreg(rsp), 0x1234_5678_9ABC_DE11);
    }
    #[test]
    fn lifted_x86_minmax_preserves_source2_nan_zero_and_lane_semantics() {
        fn f32x4(bits: [u32; 4], upper: u64) -> VecValue {
            let mut value = [upper; 16];
            value[0] = u64::from(bits[0]) | (u64::from(bits[1]) << 32);
            value[1] = u64::from(bits[2]) | (u64::from(bits[3]) << 32);
            value
        }

        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;

        let mut legacy_dst = [0xCAFE_BABE_DEAD_BEEFu64; 16];
        legacy_dst[0] = 0x1122_3344_0000_0000;
        let mut legacy_src = [0u64; 16];
        legacy_src[0] = 0x8000_0000; // -0.0f32
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = legacy_dst;
            x86.xmm[1] = legacy_src;
        }
        execute_lifted_x86(&[0xF3, 0x0F, 0x5D, 0xC1], &mut ctx, &mut memory); // MINSS
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let mut expected = legacy_dst;
            expected[0] = (legacy_dst[0] & 0xFFFF_FFFF_0000_0000) | 0x8000_0000;
            assert_eq!(x86.xmm[0], expected, "equal zeros select source 2");
        }

        let qnan2 = 0x7FF8_1234_5678_9ABCu64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0][0] = 1.0f64.to_bits();
            x86.xmm[1][0] = qnan2;
        }
        execute_lifted_x86(&[0xF2, 0x0F, 0x5F, 0xC1], &mut ctx, &mut memory); // MAXSD
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], qnan2, "source-2 NaN payload/sign bits");
        }

        let src1 = f32x4(
            [
                1.0f32.to_bits(),
                9.0f32.to_bits(),
                0.0f32.to_bits(),
                0x7FC1_2345,
            ],
            0x1111_2222_3333_4444,
        );
        let src2 = f32x4(
            [
                2.0f32.to_bits(),
                3.0f32.to_bits(),
                (-0.0f32).to_bits(),
                4.0f32.to_bits(),
            ],
            0xAAAA_BBBB_CCCC_DDDD,
        );
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = src1;
            x86.xmm[1] = src2;
        }
        execute_lifted_x86(&[0x0F, 0x5D, 0xC1], &mut ctx, &mut memory); // MINPS
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0] as u32, 1.0f32.to_bits());
            assert_eq!((x86.xmm[0][0] >> 32) as u32, 3.0f32.to_bits());
            assert_eq!(x86.xmm[0][1] as u32, (-0.0f32).to_bits());
            assert_eq!((x86.xmm[0][1] >> 32) as u32, 4.0f32.to_bits());
            assert_eq!(
                &x86.xmm[0][2..],
                &src1[2..],
                "legacy MINPS changed shared state above bit 127"
            );
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1] = src1;
            x86.xmm[2] = src2;
        }
        execute_lifted_x86(&[0xC5, 0xF0, 0x5D, 0xC2], &mut ctx, &mut memory); // VMINPS
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0] as u32, 1.0f32.to_bits());
            assert_eq!((x86.xmm[0][0] >> 32) as u32, 3.0f32.to_bits());
            assert_eq!(x86.xmm[0][1] as u32, (-0.0f32).to_bits());
            assert_eq!((x86.xmm[0][1] >> 32) as u32, 4.0f32.to_bits());
            assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        }

        // A source-1 NaN still selects finite source 2, and scalar VEX merges
        // the non-low XMM lanes from source 1 while clearing wider AVX state.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1] = [
                0x7FF8_0000_0000_0001,
                0x0123_4567_89AB_CDEF,
                7,
                7,
                7,
                7,
                7,
                7,
                7,
                7,
                7,
                7,
                7,
                7,
                7,
                7,
            ];
            x86.xmm[2][0] = 5.0f64.to_bits();
        }
        execute_lifted_x86(&[0xC5, 0xF3, 0x5F, 0xC2], &mut ctx, &mut memory); // VMAXSD
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 5.0f64.to_bits());
            assert_eq!(x86.xmm[0][1], 0x0123_4567_89AB_CDEF);
            assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        }

        // False EVEX mask suppresses an invalid memory access and preserves or
        // zeros only the scalar destination lane as requested.
        ctx.write_vreg(rax, 0x2000);
        ctx.write_vreg(k1, 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = src1;
            x86.xmm[2] = src2;
        }
        let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x09, 0x5D, 0x10], &mut ctx, &mut memory);
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2][0] as u32, src2[0] as u32);
            assert_eq!(x86.xmm[2][0] >> 32, src1[0] >> 32);
            assert_eq!(x86.xmm[2][1], src1[1]);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = src2;
        }
        let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x89, 0x5D, 0x10], &mut ctx, &mut memory);
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2][0] as u32, 0);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn lifted_x86_minmax_memory_faults_preserve_destinations_and_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        for (name, bytes, dst) in [
            ("legacy MINSS", &[0xF3, 0x0F, 0x5D, 0x00][..], 0usize),
            ("VEX VMAXSD", &[0xC5, 0xF3, 0x5F, 0x00][..], 0usize),
            (
                "EVEX VMINSS k1",
                &[0x62, 0xF1, 0x7E, 0x09, 0x5D, 0x10][..],
                2usize,
            ),
        ] {
            let original = [0x89AB_CDEF_0123_4567u64; 16];
            let mut ctx = SmirContext::new_x86_64();
            ctx.write_vreg(rax, 0x200);
            ctx.write_vreg(k1, 1);
            ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
            ctx.flags.lazy = None;
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[dst] = original;
            }
            let mut short_memory = FlatMemory::new(0x202);
            let exit = execute_lifted_x86(bytes, &mut ctx, &mut short_memory);
            assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ),
                "{name}: {exit:?}"
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[dst], original, "{name}: destination changed");
            }
            ctx.flags.materialize_all();
            assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7, "{name}: flags");
        }
    }
    #[test]
    fn vector_lane_insert_extract_preserve_other_bits_and_extend_exactly() {
        let interp = SmirInterpreter::new();
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x1000);
        let xmm0 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let xmm1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
        let mut original = [0u64; 16];
        original[0] = 0x8001_3344_5566_7788;
        original[1] = 0x99AA_BBCC_DDEE_FF00;
        SmirInterpreter::write_vec(&mut ctx, xmm0, original);
        ctx.write_vreg(rax, 0xDEAD_BEEF);

        interp
            .execute_op(
                &mut ctx,
                &mut memory,
                &SmirOp::new(
                    OpId(0),
                    0x1000,
                    OpKind::VInsertLane {
                        dst: xmm1,
                        vec: xmm0,
                        scalar: rax,
                        lane: 1,
                        elem: VecElementType::I32,
                    },
                ),
            )
            .unwrap();
        let inserted = SmirInterpreter::read_vec(&ctx, xmm1);
        assert_eq!(inserted[0], 0xDEAD_BEEF_5566_7788);
        assert_eq!(inserted[1..], original[1..]);
        assert_eq!(SmirInterpreter::read_vec(&ctx, xmm0), original);

        for (dst, sign, expected) in [
            (rcx, SignExtend::Zero, 0x8001),
            (rdx, SignExtend::Sign, 0xFFFF_FFFF_FFFF_8001),
        ] {
            interp
                .execute_op(
                    &mut ctx,
                    &mut memory,
                    &SmirOp::new(
                        OpId(1),
                        0x1000,
                        OpKind::VExtractLane {
                            dst,
                            vec: xmm0,
                            lane: 3,
                            elem: VecElementType::I16,
                            sign,
                        },
                    ),
                )
                .unwrap();
            assert_eq!(ctx.read_vreg(dst), expected);
        }
    }
    #[test]
    fn atomic_cmpxadd_updates_memory_dst_and_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let mut ctx = SmirContext::new_x86_64();
        ctx.write_vreg(rax, 0x20);
        ctx.write_vreg(rbx, 7);
        ctx.write_vreg(rcx, 3);
        let mut memory = FlatMemory::new(0x1000);
        memory
            .atomic_store(0x20, 5, MemWidth::B4, MemoryOrder::SeqCst)
            .unwrap();
        let interp = SmirInterpreter::new();

        interp
            .execute_op(
                &mut ctx,
                &mut memory,
                &SmirOp::new(
                    OpId(0),
                    0x1000,
                    OpKind::AtomicCmpXadd {
                        dst_old: rbx,
                        addr: Address::Direct(rax),
                        cmp: rbx,
                        add: rcx,
                        cond: Condition::Ule,
                        width: MemWidth::B4,
                        order: MemoryOrder::SeqCst,
                    },
                ),
            )
            .unwrap();

        assert_eq!(
            memory
                .atomic_load(0x20, MemWidth::B4, MemoryOrder::SeqCst)
                .unwrap(),
            8
        );
        assert_eq!(ctx.read_vreg(rbx), 5);
        ctx.flags.materialize_all();
        assert!(ctx.flags.materialized.cf);
        assert!(ctx.flags.materialized.sf);
        assert!(ctx.flags.materialized.af);
        assert!(!ctx.flags.materialized.zf);
        assert!(!ctx.flags.materialized.of);
        assert!(!ctx.flags.materialized.pf);
    }
    #[test]
    fn atomic_cmpxadd_false_condition_stores_old_and_preserves_add_alias() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
        let mut ctx = SmirContext::new_x86_64();
        ctx.write_vreg(rax, 0x20);
        ctx.write_vreg(rbx, 2);
        let mut memory = FlatMemory::new(0x1000);
        memory
            .atomic_store(0x20, 1, MemWidth::B4, MemoryOrder::SeqCst)
            .unwrap();
        let interp = SmirInterpreter::new();

        interp
            .execute_op(
                &mut ctx,
                &mut memory,
                &SmirOp::new(
                    OpId(0),
                    0x1000,
                    OpKind::AtomicCmpXadd {
                        dst_old: rbx,
                        addr: Address::Direct(rax),
                        cmp: rbx,
                        add: rbx,
                        cond: Condition::Ugt,
                        width: MemWidth::B4,
                        order: MemoryOrder::SeqCst,
                    },
                ),
            )
            .unwrap();

        assert_eq!(
            memory
                .atomic_load(0x20, MemWidth::B4, MemoryOrder::SeqCst)
                .unwrap(),
            1
        );
        assert_eq!(ctx.read_vreg(rbx), 1);
        ctx.flags.materialize_all();
        assert!(ctx.flags.materialized.cf);
        assert!(ctx.flags.materialized.sf);
        assert!(!ctx.flags.materialized.zf);
    }
    #[test]
    fn test_memory_operations() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x2000);
        let mut interp = SmirInterpreter::new();

        // Build: store 42 to [0x1000], load it back
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        let v0 = builder.alloc_vreg();
        let v1 = builder.alloc_vreg();

        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: v0,
                src: SrcOperand::Imm(42),
                width: OpWidth::W64,
            },
        );

        builder.push_op(
            0x1004,
            OpKind::Store {
                src: v0,
                addr: Address::Absolute(0x1800),
                width: MemWidth::B8,
            },
        );

        builder.push_op(
            0x1008,
            OpKind::Load {
                dst: v1,
                addr: Address::Absolute(0x1800),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );

        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });

        let func = builder.finish();
        let block = func.blocks[0].clone();

        interp.add_block(0x1000, block);
        ctx.pc = 0x1000;

        let exit = interp.run(&mut ctx, &mut memory);

        assert!(matches!(exit, ExitReason::Halt));
        assert_eq!(ctx.read_vreg(v1), 42);
    }
    #[test]
    fn test_apply_lane_op_byte() {
        use VLaneOp::*;
        let f = SmirInterpreter::apply_lane_op;
        // wrapping add/sub/mul (signedness-agnostic)
        assert_eq!(f(Add, 0xFF, 0x02, 8, false), 0x01);
        assert_eq!(f(Sub, 0x01, 0x02, 8, false), 0xFF);
        assert_eq!(f(Mul, 0x10, 0x10, 8, false), 0x00); // 256 & 0xFF
        // bitwise
        assert_eq!(f(And, 0xF0, 0x3C, 8, false), 0x30);
        assert_eq!(f(Or, 0xF0, 0x0F, 8, false), 0xFF);
        assert_eq!(f(Xor, 0xFF, 0x0F, 8, false), 0xF0);
        assert_eq!(f(AndNot, 0xF0, 0x0F, 8, false), 0xF0);
        // min/max signed vs unsigned: 0xFF = -1 (signed) / 255 (unsigned)
        assert_eq!(f(Max, 0xFF, 0x01, 8, false), 0xFF); // umax(255,1)
        assert_eq!(f(Max, 0xFF, 0x01, 8, true), 0x01); // smax(-1,1)
        assert_eq!(f(Min, 0xFF, 0x01, 8, false), 0x01); // umin(255,1)
        assert_eq!(f(Min, 0xFF, 0x01, 8, true), 0xFF); // smin(-1,1)
        // saturating
        assert_eq!(f(AddSat, 0xFF, 0x10, 8, false), 0xFF); // u8 clamp
        assert_eq!(f(AddSat, 0x7F, 0x01, 8, true), 0x7F); // i8 +overflow -> 127
        assert_eq!(f(SubSat, 0x01, 0x02, 8, false), 0x00); // u8 underflow -> 0
        assert_eq!(f(SubSat, 0x80, 0x01, 8, true), 0x80); // i8 -128-1 -> -128
        // average (truncating vs rounding)
        assert_eq!(f(Avg, 0xFF, 0x01, 8, false), 0x80); // (255+1)/2
        assert_eq!(f(Avg, 0x02, 0x03, 8, false), 0x02); // (5)/2 trunc
        assert_eq!(f(AvgRnd, 0x02, 0x03, 8, false), 0x03); // (5+1)/2
        // absolute difference
        assert_eq!(f(AbsDiff, 0x01, 0x03, 8, false), 0x02);
        assert_eq!(f(AbsDiff, 0xFF, 0x01, 8, true), 0x02); // |-1 - 1|
    }
    #[test]
    fn test_apply_lane_op_word() {
        use VLaneOp::*;
        let f = SmirInterpreter::apply_lane_op;
        assert_eq!(f(Add, 0xFFFF_FFFF, 1, 32, false), 0);
        assert_eq!(f(Max, 0xFFFF_FFFF, 1, 32, true), 1); // smax(-1,1)
        assert_eq!(f(Max, 0xFFFF_FFFF, 1, 32, false), 0xFFFF_FFFF); // umax
        assert_eq!(f(AddSat, 0x7FFF_FFFF, 1, 32, true), 0x7FFF_FFFF);
        assert_eq!(f(SubSat, 0x8000_0000, 1, 32, true), 0x8000_0000);
        assert_eq!(f(Avg, 0xFFFF_FFFF, 1, 32, false), 0x8000_0000);
    }
    #[test]
    fn vcmp_executes_all_integer_conditions_and_ieee_float_lane_masks() {
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        for (cond, expected) in [
            (VecCmpCond::Eq, false),
            (VecCmpCond::Ne, true),
            (VecCmpCond::Lt, true),
            (VecCmpCond::Le, true),
            (VecCmpCond::Gt, false),
            (VecCmpCond::Ge, false),
            (VecCmpCond::Ltu, false),
            (VecCmpCond::Leu, false),
            (VecCmpCond::Gtu, true),
            (VecCmpCond::Geu, true),
        ] {
            let out = run_vec2(
                [0xFF; 16],
                [1; 16],
                OpKind::VCmp {
                    dst: mkv(2),
                    src1: mkv(0),
                    src2: mkv(1),
                    cond,
                    elem: VecElementType::I8,
                    lanes: 1,
                },
            );
            assert_eq!(out[0] & 0xFF, if expected { 0xFF } else { 0 }, "{cond:?}");
            assert!(out[1..].iter().all(|word| *word == 0));
        }

        for (elem, lhs, rhs, cond, expected) in [
            (VecElementType::F16, 0x8000, 0, VecCmpCond::Eq, true),
            (VecElementType::F16, 0xC000, 0x3C00, VecCmpCond::Lt, true),
            (VecElementType::F16, 0x7E00, 0x3C00, VecCmpCond::Ne, true),
            (
                VecElementType::F32,
                u64::from((-2.0f32).to_bits()),
                u64::from(1.0f32.to_bits()),
                VecCmpCond::Gt,
                false,
            ),
            (
                VecElementType::F64,
                3.0f64.to_bits(),
                2.0f64.to_bits(),
                VecCmpCond::Ge,
                true,
            ),
        ] {
            let mut a = [0u64; 16];
            let mut b = [0u64; 16];
            a[0] = lhs;
            b[0] = rhs;
            let out = run_vec2(
                a,
                b,
                OpKind::VCmp {
                    dst: mkv(2),
                    src1: mkv(0),
                    src2: mkv(1),
                    cond,
                    elem,
                    lanes: 1,
                },
            );
            let mask = if elem.bytes() == 8 {
                u64::MAX
            } else {
                (1u64 << (elem.bytes() * 8)) - 1
            };
            assert_eq!(out[0] & mask, if expected { mask } else { 0 });
        }
    }
    #[test]
    fn test_pred_load_commit_and_cancel() {
        // cond bit0 set -> loads memory value (commits).
        assert_eq!(
            run_pred_load(0x8000, 0xDEAD_BEEF, 0x01, 0x1111_1111),
            0xDEAD_BEEF
        );
        // full predicate byte (0xff) also commits (only bit0 matters).
        assert_eq!(
            run_pred_load(0x8000, 0xDEAD_BEEF, 0xff, 0x1111_1111),
            0xDEAD_BEEF
        );
        // cond bit0 clear -> dst UNCHANGED (cancel, no memory read).
        assert_eq!(
            run_pred_load(0x8000, 0xDEAD_BEEF, 0x00, 0x1111_1111),
            0x1111_1111
        );
        // even byte 0xfe (bit0 clear) -> cancel.
        assert_eq!(
            run_pred_load(0x8000, 0xDEAD_BEEF, 0xfe, 0x1111_1111),
            0x1111_1111
        );
    }
    #[test]
    fn test_pred_store_commit_and_cancel() {
        // cond bit0 set -> stores R1 (commits).
        assert_eq!(
            run_pred_store(0x8000, 0xCAFE_F00D, 0x01, 0x2222_2222),
            0xCAFE_F00D
        );
        assert_eq!(
            run_pred_store(0x8000, 0xCAFE_F00D, 0xff, 0x2222_2222),
            0xCAFE_F00D
        );
        // cond bit0 clear -> memory UNCHANGED (cancel).
        assert_eq!(
            run_pred_store(0x8000, 0xCAFE_F00D, 0x00, 0x2222_2222),
            0x2222_2222
        );
        assert_eq!(
            run_pred_store(0x8000, 0xCAFE_F00D, 0xfe, 0x2222_2222),
            0x2222_2222
        );
    }
    // Regression for issue #112: PredStore writes memory, so the O2 redundant-load
    // elimination pass must drop cached loads across it. This builds `Load X;
    // PredStore X (committing); Load X`, runs the FULL optimizer, then executes:
    // the second load must observe the value the PredStore wrote, not a stale value
    // forwarded from the first load.
    #[test]
    fn issue_112_optimized_load_after_pred_store_reads_fresh_memory() {
        use crate::smir::optimize::{OptLevel, optimize_function};

        let addr = 0x800u64;
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        memory.write(addr, &0x1111_1111u32.to_le_bytes()).unwrap();

        let r0 = VReg::Arch(ArchReg::Hexagon(HexagonReg::R(0))); // address
        let r1 = VReg::Arch(ArchReg::Hexagon(HexagonReg::R(1))); // first load dst
        let r2 = VReg::Arch(ArchReg::Hexagon(HexagonReg::R(2))); // store value
        let r3 = VReg::Arch(ArchReg::Hexagon(HexagonReg::R(3))); // second load dst
        let p0 = VReg::Arch(ArchReg::Hexagon(HexagonReg::P(0))); // predicate
        ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::R(0)), addr);
        ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::R(2)), 0x2222_2222);
        ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::P(0)), 1); // commit the store

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: r1,
                addr: Address::Direct(r0),
                width: MemWidth::B4,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1004,
            OpKind::PredStore {
                src: SrcOperand::Reg(r2),
                cond: p0,
                addr: Address::Direct(r0),
                width: MemWidth::B4,
            },
        );
        builder.push_op(
            0x1008,
            OpKind::Load {
                dst: r3,
                addr: Address::Direct(r0),
                width: MemWidth::B4,
                sign: SignExtend::Zero,
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let mut func = builder.finish();
        optimize_function(&mut func, OptLevel::O2);

        let interp = SmirInterpreter::new();
        interp.execute_block(&mut ctx, &mut memory, &func.blocks[0]);

        assert_eq!(
            ctx.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(3))) as u32,
            0x2222_2222,
            "a load after a committing PredStore must read fresh memory, not a stale forwarded load",
        );
    }
    #[test]
    fn lifted_maskmovdqu_executes_selected_stores_addresses_and_fault_suppression() {
        fn packed_bytes(bytes: &[u8], fill: u64) -> VecValue {
            let mut value = [fill; 16];
            for (lane, byte) in bytes.iter().copied().enumerate() {
                let shift = (lane % 8) * 8;
                value[lane / 8] =
                    (value[lane / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
            }
            value
        }

        let data = (0..16).map(|lane| 0xA0 + lane).collect::<Vec<_>>();
        let alternating_mask = (0..16)
            .map(|lane| if lane % 2 == 0 { 0x80 } else { 0x7F })
            .collect::<Vec<_>>();
        let flags_before = 0xCD7;
        let rdi = VReg::Arch(ArchReg::X86(X86Reg::Rdi));
        let fs_base = VReg::Arch(ArchReg::X86(X86Reg::FsBase));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        memory.write(0x40, &[0x55; 16]).unwrap();
        ctx.write_vreg(rdi, 0x40);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = packed_bytes(&data, 0x1111_1111_1111_1111);
            x86.xmm[9] = packed_bytes(&alternating_mask, 0x2222_2222_2222_2222);
        }
        assert!(matches!(
            execute_lifted_x86(&[0x66, 0x45, 0x0F, 0xF7, 0xC1], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let mut stored = [0; 16];
        memory.read(0x40, &mut stored).unwrap();
        for lane in 0..16 {
            assert_eq!(stored[lane], if lane % 2 == 0 { data[lane] } else { 0x55 });
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[8], packed_bytes(&data, 0x1111_1111_1111_1111));
            assert_eq!(
                x86.xmm[9],
                packed_bytes(&alternating_mask, 0x2222_2222_2222_2222)
            );
        }

        // Prefix-free MASKMOVQ performs the same selection over eight MMX
        // bytes and commits the x87-to-MMX tag transition after the stores.
        memory.write(0x80, &[0x66; 8]).unwrap();
        ctx.write_vreg(rdi, 0x80);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(data[..8].try_into().unwrap());
            x86.mm[1] = u64::from_le_bytes(alternating_mask[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x0F, 0xF7, 0xC1], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let mut mmx_stored = [0; 8];
        memory.read(0x80, &mut mmx_stored).unwrap();
        for lane in 0..8 {
            assert_eq!(
                mmx_stored[lane],
                if lane % 2 == 0 { data[lane] } else { 0x66 }
            );
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], u64::from_le_bytes(data[..8].try_into().unwrap()));
            assert_eq!(
                x86.mm[1],
                u64::from_le_bytes(alternating_mask[..8].try_into().unwrap())
            );
            assert_eq!(x86.x87.tag_word, 0);
        }

        // Address-size override truncates RDI to EDI before adding the lane.
        memory.write(0x80, &[0x33; 16]).unwrap();
        ctx.write_vreg(rdi, 0xFFFF_FFFF_0000_0080);
        assert!(matches!(
            execute_lifted_x86(&[0x67, 0xC4, 0x41, 0x79, 0xF7, 0xC1], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let mut stored = [0; 16];
        memory.read(0x80, &mut stored).unwrap();
        for lane in 0..16 {
            assert_eq!(stored[lane], if lane % 2 == 0 { data[lane] } else { 0x33 });
        }

        // FS contributes its architectural base after the implicit RDI address.
        memory.write(0x120, &[0x44; 16]).unwrap();
        ctx.write_vreg(fs_base, 0x100);
        ctx.write_vreg(rdi, 0x20);
        assert!(matches!(
            execute_lifted_x86(&[0x64, 0xC4, 0x41, 0x79, 0xF7, 0xC1], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let mut stored = [0; 16];
        memory.read(0x120, &mut stored).unwrap();
        for lane in 0..16 {
            assert_eq!(stored[lane], if lane % 2 == 0 { data[lane] } else { 0x44 });
        }

        // Inactive bytes beyond the mapped boundary perform no access. Making
        // the first out-of-range byte active exposes a write fault.
        let low_half_mask = [0x80; 8].into_iter().chain([0; 8]).collect::<Vec<_>>();
        ctx.write_vreg(fs_base, 0);
        ctx.write_vreg(rdi, 0x1F8);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = packed_bytes(&low_half_mask, 0);
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0x41, 0x79, 0xF7, 0xC1], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let mut stored = [0; 8];
        memory.read(0x1F8, &mut stored).unwrap();
        assert_eq!(stored, data[..8]);

        let lane8_mask = [0; 8]
            .into_iter()
            .chain([0x80])
            .chain([0; 7])
            .collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = packed_bytes(&lane8_mask, 0);
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0x41, 0x79, 0xF7, 0xC1], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));

        // Intel specifies all-zero-mask addressing faults as implementation
        // dependent. SMIR selects the permitted fully suppressed behavior.
        ctx.write_vreg(rdi, 0x1_0000);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = [0; 16];
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0x41, 0x79, 0xF7, 0xC1], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));

        // MASKMOVQ also takes the permitted fully suppressed all-zero-mask
        // path, but still enters MMX state on successful completion.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[1] = 0;
            x86.x87.tag_word = 0xFFFF;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x0F, 0xF7, 0xC1], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.tag_word, 0);
        }

        // Earlier active bytes may be stored before a later active byte
        // faults. EnterMmx remains after the complete predicated-store series,
        // so the fault does not commit the x87 tag transition.
        ctx.write_vreg(rdi, 0x1FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(data[..8].try_into().unwrap());
            x86.mm[1] = 0x0000_0080_0000_0080;
            x86.x87.tag_word = 0xFFFF;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x0F, 0xF7, 0xC1], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        let mut first = [0];
        memory.read(0x1FC, &mut first).unwrap();
        assert_eq!(first[0], data[0]);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_movntdqa_executes_widths_alignment_upper_state_and_faults() {
        let data = (0..64).map(|i| (i * 37 + 5) as u8).collect::<Vec<_>>();
        let words = |input: &[u8], fill: u64| {
            let mut out = [fill; 16];
            for (i, chunk) in input.chunks_exact(8).enumerate() {
                out[i] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            out
        };
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        memory.write(0x100, &data).unwrap();
        ctx.write_vreg(rax, 0x100);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [upper; 16];
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x2A, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[0][..2], &words(&data[..16], 0)[..2]);
            assert!(x86.xmm[0][2..].iter().all(|w| *w == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x2A, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[0][..4], &words(&data[..32], 0)[..4]);
            assert!(x86.xmm[0][4..].iter().all(|w| *w == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = sentinel;
        }
        execute_lifted_x86(&[0x62, 0xE2, 0x7D, 0x48, 0x2A, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[16][..8], &words(&data, 0)[..8]);
            assert!(x86.xmm[16][8..].iter().all(|w| *w == 0));
        }

        for (addr, insn, reg) in [
            (0x101, &[0x66, 0x0F, 0x38, 0x2A, 0x00][..], 0usize),
            (0x110, &[0xC4, 0xE2, 0x7D, 0x2A, 0x00][..], 0),
            (0x120, &[0x62, 0xE2, 0x7D, 0x48, 0x2A, 0x00][..], 16),
        ] {
            ctx.write_vreg(rax, addr);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[reg] = sentinel;
            }
            let exit = execute_lifted_x86(insn, &mut ctx, &mut memory);
            assert!(matches!(
                exit,
                BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
            ));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[reg], sentinel);
            }
        }
        ctx.write_vreg(rax, 0x400);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let fault = execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x2A, 0x00], &mut ctx, &mut memory);
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
    fn lifted_crc32c_executes_widths_high_bytes_aliases_memory_faults_and_flags() {
        fn reference(mut crc: u32, data: u64, bytes: u32) -> u64 {
            for byte in 0..bytes {
                crc ^= ((data >> (byte * 8)) & 0xFF) as u32;
                for _ in 0..8 {
                    crc = (crc >> 1) ^ (0x82F6_3B78 & 0u32.wrapping_sub(crc & 1));
                }
            }
            u64::from(crc)
        }

        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for (insn, initial, data, bytes, expected) in [
            (
                &[0xF2, 0x45, 0x0F, 0x38, 0xF0, 0xC1][..],
                u64::MAX,
                0x31,
                1,
                0x6F0A_661C,
            ),
            (
                &[0x66, 0xF2, 0x45, 0x0F, 0x38, 0xF1, 0xC1][..],
                0x1234_5678,
                0xABCD,
                2,
                0xAAE3_2043,
            ),
            (
                &[0xF2, 0x45, 0x0F, 0x38, 0xF1, 0xC1][..],
                0x89AB_CDEF,
                0x0123_4567,
                4,
                0x796A_B9A9,
            ),
            (
                &[0xF2, 0x4D, 0x0F, 0x38, 0xF1, 0xC1][..],
                0xFFFF_FFFF_DEAD_BEEF,
                0x0123_4567_89AB_CDEF,
                8,
                0x3AB0_1437,
            ),
        ] {
            ctx.write_vreg(r8, initial);
            ctx.write_vreg(r9, data);
            execute_lifted_x86(insn, &mut ctx, &mut memory);
            assert_eq!(ctx.read_vreg(r8), expected);
            assert_eq!(expected, reference(initial as u32, data, bytes));
        }

        // Same-register source/destination reads both operands from the old
        // value, then zero-extends the 32-bit result.
        let alias_value = 0xA5A5_5A5A_DEAD_BEEF;
        ctx.write_vreg(r8, alias_value);
        execute_lifted_x86(&[0xF2, 0x45, 0x0F, 0x38, 0xF0, 0xC0], &mut ctx, &mut memory);
        assert_eq!(
            ctx.read_vreg(r8),
            reference(alias_value as u32, alias_value, 1)
        );
        ctx.write_vreg(r8, alias_value);
        execute_lifted_x86(&[0xF2, 0x4D, 0x0F, 0x38, 0xF1, 0xC0], &mut ctx, &mut memory);
        assert_eq!(
            ctx.read_vreg(r8),
            reference(alias_value as u32, alias_value, 8)
        );

        // Without REX, byte code 5 is CH rather than BPL.
        ctx.write_vreg(rcx, 0x1122_3344_5566_AB88);
        ctx.write_vreg(rdx, 0xFFFF_FFFF_1234_5678);
        execute_lifted_x86(&[0xF2, 0x0F, 0x38, 0xF0, 0xD5], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rdx), reference(0x1234_5678, 0xAB, 1));

        // Qword memory accesses are unaligned-capable.
        let memory_value = 0x8877_6655_4433_2211u64;
        memory.write(0x109, &memory_value.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(r8, 0xFFFF_FFFF_89AB_CDEF);
        execute_lifted_x86(
            &[0xF2, 0x4C, 0x0F, 0x38, 0xF1, 0x40, 0x09],
            &mut ctx,
            &mut memory,
        );
        assert_eq!(ctx.read_vreg(r8), reference(0x89AB_CDEF, memory_value, 8));

        // A memory fault occurs before the sole architectural destination
        // write and preserves both destination and RFLAGS.
        ctx.write_vreg(rax, 0x3FC);
        ctx.write_vreg(r8, alias_value);
        let fault =
            execute_lifted_x86(&[0xF2, 0x4C, 0x0F, 0x38, 0xF1, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        assert_eq!(ctx.read_vreg(r8), alias_value);
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_0f3a_extracts_execute_lanes_widths_tuples_faults_and_flags() {
        fn vector(bytes: &[u8], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (word, chunk) in bytes.chunks_exact(8).enumerate() {
                out[word] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            out
        }

        let source = (0..16)
            .map(|lane| (lane * 13 + 7) as u8)
            .collect::<Vec<_>>();
        let lane32 =
            |lane: usize| u32::from_le_bytes(source[lane * 4..lane * 4 + 4].try_into().unwrap());
        let lane64 =
            |lane: usize| u64::from_le_bytes(source[lane * 8..lane * 8 + 8].try_into().unwrap());
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector(&source, upper);
            x86.xmm[17] = vector(&source, upper);
        }

        // Immediate bits above the lane selector are ignored, and each GPR
        // form zero-extends its scalar result.
        ctx.write_vreg(r8, u64::MAX);
        execute_lifted_x86(
            &[0x66, 0x45, 0x0F, 0x3A, 0x14, 0xC8, 0x1F],
            &mut ctx,
            &mut memory,
        );
        assert_eq!(ctx.read_vreg(r8), u64::from(source[15]));

        ctx.write_vreg(r8, u64::MAX);
        execute_lifted_x86(&[0x66, 0x45, 0x0F, 0xC5, 0xC1, 0x0F], &mut ctx, &mut memory);
        assert_eq!(
            ctx.read_vreg(r8),
            u64::from(u16::from_le_bytes(source[14..16].try_into().unwrap()))
        );

        ctx.write_vreg(r8, u64::MAX);
        execute_lifted_x86(
            &[0x62, 0x31, 0x7D, 0x08, 0xC5, 0xC1, 0x0F],
            &mut ctx,
            &mut memory,
        );
        assert_eq!(
            ctx.read_vreg(r8),
            u64::from(u16::from_le_bytes(source[14..16].try_into().unwrap()))
        );

        ctx.write_vreg(r8, u64::MAX);
        execute_lifted_x86(&[0xC4, 0x43, 0x79, 0x16, 0xC8, 0x07], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(r8), u64::from(lane32(3)));

        ctx.write_vreg(r8, 0);
        execute_lifted_x86(&[0xC4, 0x43, 0xF9, 0x16, 0xC8, 0x03], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(r8), lane64(1));

        ctx.write_vreg(r8, u64::MAX);
        execute_lifted_x86(
            &[0x62, 0xC3, 0x7D, 0x08, 0x17, 0xC8, 0x07],
            &mut ctx,
            &mut memory,
        );
        assert_eq!(ctx.read_vreg(r8), u64::from(lane32(3)));

        // Scalar memory destinations are unaligned-capable and touch exactly
        // their architectural byte count. This EVEX word form also exercises
        // Tuple1 Scalar disp8*N (17*2 = 34 bytes).
        memory.write(0x121, &[0xCC; 8]).unwrap();
        ctx.write_vreg(rax, 0x100);
        execute_lifted_x86(
            &[0x62, 0xE3, 0x7D, 0x08, 0x15, 0x48, 0x11, 0x0F],
            &mut ctx,
            &mut memory,
        );
        let mut around = [0u8; 8];
        memory.read(0x121, &mut around).unwrap();
        assert_eq!(around[0], 0xCC);
        assert_eq!(&around[1..3], &source[14..16]);
        assert!(around[3..].iter().all(|byte| *byte == 0xCC));

        // A destination store fault leaves the source vector and RFLAGS
        // unchanged. There is no alignment precondition for this scalar store.
        let source_before = if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            x86.xmm[9]
        } else {
            unreachable!()
        };
        ctx.write_vreg(rax, 0x400);
        let fault = execute_lifted_x86(
            &[0x66, 0x44, 0x0F, 0x3A, 0x16, 0x08, 0x03],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[9], source_before);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_duplicate_moves_execute_patterns_masks_memory_and_faults() {
        fn vector(values: &[u32], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            let bytes = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            for (index, chunk) in bytes.chunks(8).enumerate() {
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
            x86.xmm[10] = vector(&(0..8).collect::<Vec<_>>(), 0);
        }
        for (insn, expected) in [
            (
                &[0xC4, 0x41, 0x7E, 0x12, 0xCA][..],
                vec![0, 0, 2, 2, 4, 4, 6, 6],
            ),
            (
                &[0xC4, 0x41, 0x7E, 0x16, 0xCA][..],
                vec![1, 1, 3, 3, 5, 5, 7, 7],
            ),
        ] {
            execute_lifted_x86(insn, &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(lanes(&x86.xmm[9], 8), expected);
            }
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = sentinel;
            x86.xmm[18] = vector(&(0..16).collect::<Vec<_>>(), 0);
            x86.k[3] = 0x5555;
        }
        execute_lifted_x86(&[0x62, 0xA1, 0x7E, 0x4B, 0x12, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let actual = lanes(&x86.xmm[17], 16);
            for lane in 0..16 {
                assert_eq!(
                    actual[lane],
                    if lane % 2 == 0 {
                        (lane / 2 * 2) as u32
                    } else {
                        0xCCCC_CCCC
                    }
                );
            }
        }
        memory
            .write(0x101, &0x1122_3344_5566_7788u64.to_le_bytes())
            .unwrap();
        ctx.write_vreg(rax, 0x100);
        execute_lifted_x86(&[0xC5, 0x7B, 0x12, 0x48, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[9][..2], &[0x1122_3344_5566_7788; 2]);
        }
        ctx.write_vreg(rax, 0x1FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
        }
        let fault = execute_lifted_x86(&[0xC5, 0x7B, 0x12, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[9], sentinel);
        }
    }
    #[test]
    fn lifted_movnti_executes_sizes_unaligned_addresses_and_faults_atomically() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        ctx.write_vreg(rax, 0x21);
        ctx.write_vreg(rcx, 0xFFFF_FFFF_89AB_CDEF);
        assert!(matches!(
            execute_lifted_x86(&[0x0F, 0xC3, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let mut dword = [0u8; 4];
        memory.read(0x21, &mut dword).unwrap();
        assert_eq!(u32::from_le_bytes(dword), 0x89AB_CDEF);

        ctx.write_vreg(r8, 0x38);
        ctx.write_vreg(r9, 0x0123_4567_89AB_CDEF);
        assert!(matches!(
            execute_lifted_x86(&[0x4D, 0x0F, 0xC3, 0x48, 0x08], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let mut qword = [0u8; 8];
        memory.read(0x40, &mut qword).unwrap();
        assert_eq!(u64::from_le_bytes(qword), 0x0123_4567_89AB_CDEF);

        let mut fault_memory = StoreFaultMemory {
            inner: FlatMemory::new(0x100),
            stores_before_fault: 0,
        };
        ctx.write_vreg(rax, 0x20);
        let fault = execute_lifted_x86(&[0x0F, 0xC3, 0x08], &mut ctx, &mut fault_memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        fault_memory.inner.read(0x20, &mut dword).unwrap();
        assert_eq!(dword, [0; 4]);
        assert_eq!(ctx.read_vreg(rcx), 0xFFFF_FFFF_89AB_CDEF);
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_non_temporal_vector_stores_execute_widths_alignment_and_faults() {
        fn bytes(words: &[u64]) -> Vec<u8> {
            words.iter().flat_map(|word| word.to_le_bytes()).collect()
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let flags_before = 0xCD7;
        let legacy = [0x1111_0000_0000_0001, 0x1111_0000_0000_0002];
        let vex = [
            0x2222_0000_0000_0001,
            0x2222_0000_0000_0002,
            0x2222_0000_0000_0003,
            0x2222_0000_0000_0004,
        ];
        let evex = [
            0x3333_0000_0000_0001,
            0x3333_0000_0000_0002,
            0x3333_0000_0000_0003,
            0x3333_0000_0000_0004,
            0x3333_0000_0000_0005,
            0x3333_0000_0000_0006,
            0x3333_0000_0000_0007,
            0x3333_0000_0000_0008,
        ];
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x500);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][..legacy.len()].copy_from_slice(&legacy);
            x86.xmm[2][..vex.len()].copy_from_slice(&vex);
            x86.xmm[19][..evex.len()].copy_from_slice(&evex);
        }

        // MOVNTQ accepts an unaligned m64 destination. Its successful store
        // enters MMX state while preserving TOP.
        let mmx_value = 0x0123_4567_89AB_CDEFu64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[1] = mmx_value;
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 5 << 11;
        }
        ctx.write_vreg(rax, 0x83);
        assert!(matches!(
            execute_lifted_x86(&[0x0F, 0xE7, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let mut mmx_stored = [0u8; 8];
        memory.read(0x83, &mut mmx_stored).unwrap();
        assert_eq!(u64::from_le_bytes(mmx_stored), mmx_value);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[1], mmx_value);
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 5 << 11);
        }

        ctx.write_vreg(rax, 0x100);
        assert!(matches!(
            execute_lifted_x86(&[0x0F, 0x2B, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let mut actual = [0u8; 64];
        memory.read(0x100, &mut actual[..16]).unwrap();
        assert_eq!(&actual[..16], bytes(&legacy));

        assert!(matches!(
            execute_lifted_x86(&[0xC5, 0xFC, 0x2B, 0x50, 0x20], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        memory.read(0x120, &mut actual[..32]).unwrap();
        assert_eq!(&actual[..32], bytes(&vex));

        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xE1, 0x7C, 0x48, 0x2B, 0x58, 0x01],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        memory.read(0x140, &mut actual).unwrap();
        assert_eq!(actual.as_slice(), bytes(&evex));

        memory.read(0x180, &mut actual[..16]).unwrap();
        let before_misaligned = actual[..16].to_vec();
        ctx.write_vreg(rax, 0x180);
        let misaligned = execute_lifted_x86(&[0x0F, 0x2B, 0x48, 0x01], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        memory.read(0x180, &mut actual[..16]).unwrap();
        assert_eq!(&actual[..16], before_misaligned);

        let mut mmx_fault_memory = StoreFaultMemory {
            inner: FlatMemory::new(0x100),
            stores_before_fault: 0,
        };
        ctx.write_vreg(rax, 0x40);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[1] = mmx_value;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0xE7, 0x08], &mut ctx, &mut mmx_fault_memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        mmx_fault_memory.inner.read(0x40, &mut mmx_stored).unwrap();
        assert_eq!(mmx_stored, [0; 8]);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[1], mmx_value);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        let mut fault_memory = StoreFaultMemory {
            inner: FlatMemory::new(0x100),
            stores_before_fault: 0,
        };
        ctx.write_vreg(rax, 0x40);
        let fault = execute_lifted_x86(
            &[0x62, 0xE1, 0x7C, 0x48, 0x2B, 0x18],
            &mut ctx,
            &mut fault_memory,
        );
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        let mut fault_bytes = [0u8; 64];
        fault_memory.inner.read(0x40, &mut fault_bytes).unwrap();
        assert_eq!(fault_bytes, [0; 64]);

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_half_vector_moves_execute_merges_stores_upper_rules_and_faults() {
        let upper = 0xCCCC_CCCC_CCCC_CCCC;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        memory
            .write(0x100, &0x1122_3344_5566_7788u64.to_le_bytes())
            .unwrap();
        ctx.write_vreg(rax, 0x100);

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [upper; 16];
            x86.xmm[1][..2].copy_from_slice(&[1, 2]);
        }
        execute_lifted_x86(&[0x0F, 0x12, 0x08], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[1][..2], &[0x1122_3344_5566_7788, 2]);
            assert!(x86.xmm[1][2..].iter().all(|word| *word == upper));
        }
        execute_lifted_x86(&[0x0F, 0x16, 0x08], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                &x86.xmm[1][..2],
                &[0x1122_3344_5566_7788, 0x1122_3344_5566_7788]
            );
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[5][..2].copy_from_slice(&[10, 11]);
            x86.xmm[6][..2].copy_from_slice(&[20, 21]);
            x86.xmm[7] = [upper; 16];
            x86.xmm[8][..2].copy_from_slice(&[30, 31]);
        }
        execute_lifted_x86(&[0x0F, 0x12, 0xEE], &mut ctx, &mut memory);
        execute_lifted_x86(&[0x41, 0x0F, 0x16, 0xF8], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[5][..2], &[21, 11]);
            assert_eq!(&x86.xmm[7][..2], &[upper, 30]);
            assert!(x86.xmm[7][2..].iter().all(|word| *word == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [upper; 16];
            x86.xmm[2][..2].copy_from_slice(&[40, 41]);
        }
        execute_lifted_x86(&[0xC5, 0xE8, 0x12, 0x08], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[1][..2], &[0x1122_3344_5566_7788, 41]);
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = [upper; 16];
            x86.xmm[18][..2].copy_from_slice(&[50, 51]);
            x86.xmm[19][..2].copy_from_slice(&[60, 61]);
        }
        execute_lifted_x86(&[0x62, 0xA1, 0x6C, 0x00, 0x12, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[17][..2], &[61, 51]);
            assert!(x86.xmm[17][2..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[10][..2].copy_from_slice(&[70, 71]);
            x86.xmm[25][..2].copy_from_slice(&[80, 81]);
        }
        execute_lifted_x86(&[0xC5, 0x79, 0x13, 0x50, 0x08], &mut ctx, &mut memory);
        let mut stored = [0u8; 8];
        memory.read(0x108, &mut stored).unwrap();
        assert_eq!(u64::from_le_bytes(stored), 70);
        execute_lifted_x86(
            &[0x62, 0x61, 0xFD, 0x08, 0x17, 0x48, 0x08],
            &mut ctx,
            &mut memory,
        );
        memory.read(0x140, &mut stored).unwrap();
        assert_eq!(u64::from_le_bytes(stored), 81);

        ctx.write_vreg(rax, 0x300);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [upper; 16];
        }
        let fault = execute_lifted_x86(&[0x0F, 0x12, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], [upper; 16]);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn issue_23_xadd_mem_faulting_store_preserves_flags_and_source() {
        // Seed EVERY flag set: a premature add (the pre-fix bug) would compute
        // 0xFFFF_FFFF + 1 == 0, clearing SF/OF and forcing ZF/CF, so any leaked
        // flag commit is observable as a deviation from the all-ones sentinel.
        const SENTINEL: u64 = 0x0000_0CD7; // CF|PF|AF|ZF|SF|DF|OF (+ reserved bit 1)
        let mut inner = FlatMemory::new(0x1000);
        inner.write(0x800, &0xFFFF_FFFFu32.to_le_bytes()).unwrap();
        let mut memory = StoreFaultMemory {
            inner,
            stores_before_fault: 0,
        };

        let (rcx, exit, rflags) = run_xadd_mem32(0x800, 0x0000_0001, SENTINEL, &mut memory);

        // The store must fault on the read-only page...
        assert!(
            matches!(
                exit,
                BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
            ),
            "read-only store must raise a write page fault, got {exit:?}",
        );
        // ...and a faulting XADD must leave the source register untouched...
        assert_eq!(
            rcx, 0x0000_0001,
            "source register must survive a faulting XADD store",
        );
        // ...and must NOT have committed any arithmetic flags.
        assert_eq!(
            rflags, SENTINEL,
            "a faulting memory XADD must not update RFLAGS",
        );
        // The read-only page itself is unchanged.
        let mut buf = [0u8; 4];
        memory.read(0x800, &mut buf).unwrap();
        assert_eq!(
            u32::from_le_bytes(buf),
            0xFFFF_FFFF,
            "faulting store must not mutate memory",
        );
    }
    #[test]
    fn issue_23_xadd_mem_successful_store_commits_flags_and_source() {
        // Positive control: with writable memory the same XADD commits. With
        // [mem] = 0xFFFF_FFFF and ecx = 1, the 32-bit sum wraps to 0 (ZF=1, CF=1),
        // memory takes the sum, and ECX takes the old memory value.
        let mut memory = FlatMemory::new(0x1000);
        memory.write(0x800, &0xFFFF_FFFFu32.to_le_bytes()).unwrap();
        // Seed flags cleared (only reserved bit 1) so committed add flags are
        // unambiguous.
        let (rcx, exit, rflags) = run_xadd_mem32(0x800, 0x0000_0001, 0x0000_0002, &mut memory);

        assert!(
            matches!(exit, BlockResult::Exit(ExitReason::Halt)),
            "writable XADD must run to completion, got {exit:?}",
        );
        // ECX receives the old destination (zero-extended into RCX).
        assert_eq!(rcx, 0x0000_0000_FFFF_FFFF, "ECX takes the old memory value");
        // Memory receives the sum.
        let mut buf = [0u8; 4];
        memory.read(0x800, &mut buf).unwrap();
        assert_eq!(u32::from_le_bytes(buf), 0x0000_0000, "memory takes the sum");
        // Flags ARE committed on the success path: ZF and CF set, SF and OF clear.
        assert_ne!(rflags & (1 << 6), 0, "ZF must be set (sum is zero)");
        assert_ne!(rflags & (1 << 0), 0, "CF must be set (carry out)");
        assert_eq!(rflags & (1 << 7), 0, "SF must be clear");
        assert_eq!(rflags & (1 << 11), 0, "OF must be clear");
    }
