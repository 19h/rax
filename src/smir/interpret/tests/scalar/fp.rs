//! scalar::fp tests

use super::*;
use crate::smir::interpret::*;
use crate::smir::interpret::tests::*;

    #[test]
    fn lifted_loop_family_executes_conditions_counter_width_and_preserves_flags() {
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();

        ctx.write_vreg(rcx, 2);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;
        assert!(execute_lifted_x86_condition(
            &[0xE2, 0],
            &mut ctx,
            &mut memory
        ));
        assert_eq!(ctx.read_vreg(rcx), 1);
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);

        assert!(!execute_lifted_x86_condition(
            &[0xE2, 0],
            &mut ctx,
            &mut memory
        ));
        assert_eq!(ctx.read_vreg(rcx), 0);

        ctx.write_vreg(rcx, 0xFFFF_FFFF_0000_0001);
        assert!(!execute_lifted_x86_condition(
            &[0x67, 0xE2, 0],
            &mut ctx,
            &mut memory
        ));
        assert_eq!(ctx.read_vreg(rcx), 0, "67h LOOP must decrement ECX");

        ctx.write_vreg(rcx, 0);
        assert!(execute_lifted_x86_condition(
            &[0xE3, 0],
            &mut ctx,
            &mut memory
        ));
        assert_eq!(ctx.read_vreg(rcx), 0, "JRCXZ must not decrement");

        ctx.write_vreg(rcx, 2);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0x42); // ZF=1
        ctx.flags.lazy = None;
        assert!(execute_lifted_x86_condition(
            &[0xE1, 0],
            &mut ctx,
            &mut memory
        ));
        ctx.write_vreg(rcx, 2);
        assert!(!execute_lifted_x86_condition(
            &[0xE0, 0],
            &mut ctx,
            &mut memory
        ));
    }
    #[test]
    fn lifted_sqrt_memory_faults_preserve_destinations_and_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        for (name, bytes, dst) in [
            ("legacy SQRTSD", &[0xF2, 0x0F, 0x51, 0x00][..], 0usize),
            ("VEX VSQRTSS", &[0xC5, 0xF2, 0x51, 0x00][..], 0usize),
            (
                "EVEX VSQRTSS k1",
                &[0x62, 0xF1, 0x7E, 0x09, 0x51, 0x10][..],
                2usize,
            ),
        ] {
            let original = [0x0123_4567_89AB_CDEFu64; 16];
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
    fn lifted_comi_ucomi_set_exact_x86_flags_and_preserve_registers() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();
        let original0 = [0x0123_4567_89AB_CDEFu64; 16];
        let original1 = [0xFEDC_BA98_7654_3210u64; 16];

        for (name, a, b, expected_rflags) in [
            ("greater", 2.0f32.to_bits(), 1.0f32.to_bits(), 0x402u64),
            ("less", 1.0f32.to_bits(), 2.0f32.to_bits(), 0x403u64),
            ("equal", 1.0f32.to_bits(), 1.0f32.to_bits(), 0x442u64),
            ("unordered", 0x7FC1_2345u32, 1.0f32.to_bits(), 0x447u64),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = original0;
                x86.xmm[1] = original1;
                x86.xmm[0][0] = (x86.xmm[0][0] & !0xFFFF_FFFF) | u64::from(a);
                x86.xmm[1][0] = (x86.xmm[1][0] & !0xFFFF_FFFF) | u64::from(b);
            }
            ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
            ctx.flags.lazy = None;
            execute_lifted_x86(&[0x0F, 0x2E, 0xC1], &mut ctx, &mut memory); // UCOMISS
            ctx.flags.materialize_all();
            assert_eq!(
                ctx.flags.materialized.to_rflags(),
                expected_rflags,
                "{name}"
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[0][1..], original0[1..]);
                assert_eq!(x86.xmm[1][1..], original1[1..]);
            }
        }

        // VEX COMISD has the same architectural integer-flag truth table.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0][0] = (-4.0f64).to_bits();
            x86.xmm[1][0] = 7.0f64.to_bits();
        }
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;
        execute_lifted_x86(&[0xC5, 0xF9, 0x2F, 0xC1], &mut ctx, &mut memory);
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0x403);

        // EVEX scalar disp8 is compressed by 8 bytes for a double operand.
        ctx.write_vreg(rax, 0x200);
        memory
            .write(0x240, &(-4.0f64).to_bits().to_le_bytes())
            .unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[18][0] = (-4.0f64).to_bits();
        }
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;
        execute_lifted_x86(
            &[0x62, 0xE1, 0xFD, 0x08, 0x2F, 0x50, 0x08],
            &mut ctx,
            &mut memory,
        );
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0x442);

        for (name, opcode, a, b, expected_rflags) in [
            ("FP16 greater", 0x2E, 0x4000u16, 0x3C00u16, 0x402u64),
            ("FP16 less", 0x2F, 0x3C00, 0x4000, 0x403),
            ("FP16 equal", 0x2E, 0xBC00, 0xBC00, 0x442),
            ("FP16 unordered", 0x2F, 0x7E01, 0x3C00, 0x447),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, u64::from(a));
                SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 16, u64::from(b));
            }
            ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
            ctx.flags.lazy = None;
            execute_lifted_x86(
                &[0x62, 0xF5, 0x7C, 0x08, opcode, 0xD3],
                &mut ctx,
                &mut memory,
            );
            ctx.flags.materialize_all();
            assert_eq!(
                ctx.flags.materialized.to_rflags(),
                expected_rflags,
                "{name}"
            );
        }

        // The scalar tuple compresses disp8 by 2 bytes for an FP16 memory
        // operand, and a successful load commits the comparison flags.
        ctx.write_vreg(rax, 0x200);
        memory.write(0x2FE, &0xBC00u16.to_le_bytes()).unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, 0xBC00);
        }
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;
        execute_lifted_x86(
            &[0x62, 0xF5, 0x7C, 0x08, 0x2F, 0x50, 0x7F],
            &mut ctx,
            &mut memory,
        );
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0x442);
    }
    #[test]
    fn lifted_comi_ucomi_memory_faults_preserve_all_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        for (name, bytes) in [
            ("legacy UCOMISS", &[0x0F, 0x2E, 0x00][..]),
            ("VEX COMISD", &[0xC5, 0xF9, 0x2F, 0x00][..]),
            ("EVEX VCOMISD", &[0x62, 0xF1, 0xFD, 0x08, 0x2F, 0x00][..]),
            (
                "EVEX VCOMISH",
                &[0x62, 0xF5, 0x7C, 0x08, 0x2F, 0x40, 0x01][..],
            ),
        ] {
            let mut ctx = SmirContext::new_x86_64();
            ctx.write_vreg(rax, 0x200);
            ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
            ctx.flags.lazy = None;
            let mut short_memory = FlatMemory::new(0x202);
            let exit = execute_lifted_x86(bytes, &mut ctx, &mut short_memory);
            assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ),
                "{name}: {exit:?}"
            );
            ctx.flags.materialize_all();
            assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7, "{name}");
        }
    }
    #[test]
    fn lifted_x86_fp_to_int_honors_mxcsr_width_truncation_and_indefinite() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let r10 = VReg::Arch(ArchReg::X86(X86Reg::R10));
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();

        for (name, rc, input, expected) in [
            ("nearest-even", 0u32, 2.5f32, 2i32),
            ("down", 1, -2.1f32, -3),
            ("up", 2, 2.1f32, 3),
            ("toward-zero", 3, -2.9f32, -2),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1][0] = input.to_bits() as u64;
                x86.mxcsr = (0x1F80 & !(3 << 13)) | (rc << 13);
            }
            ctx.write_vreg(rax, u64::MAX);
            ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
            ctx.flags.lazy = None;
            execute_lifted_x86(&[0xF3, 0x0F, 0x2D, 0xC1], &mut ctx, &mut memory);
            assert_eq!(ctx.read_vreg(rax), expected as u32 as u64, "{name}");
            ctx.flags.materialize_all();
            assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7, "{name}: flags");
        }

        // Truncating forms ignore MXCSR.RC.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = (-2.9f32).to_bits() as u64;
            x86.mxcsr = (0x1F80 & !(3 << 13)) | (2 << 13);
        }
        execute_lifted_x86(&[0xF3, 0x0F, 0x2C, 0xC1], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), (-2i32) as u32 as u64);

        // REX.W selects a signed 64-bit result and retains nearest-even ties.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = (-2.5f64).to_bits();
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(&[0xF2, 0x48, 0x0F, 0x2D, 0xC1], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), (-2i64) as u64);

        // Masked invalid conversion produces the width-specific integer
        // indefinite value; a 32-bit destination also clears the upper half.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 0x7FF8_1234_5678_9ABC;
        }
        ctx.write_vreg(rax, u64::MAX);
        execute_lifted_x86(&[0xF2, 0x0F, 0x2D, 0xC1], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), 0x8000_0000);

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 9_223_372_036_854_775_808.0f64.to_bits();
        }
        execute_lifted_x86(&[0xF2, 0x48, 0x0F, 0x2C, 0xC1], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), 0x8000_0000_0000_0000);

        // EVEX high-XMM source decoding and extended GPR destination.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17][0] = 42.0f32.to_bits() as u64;
        }
        ctx.write_vreg(r10, 0);
        execute_lifted_x86(&[0x62, 0x31, 0xFE, 0x08, 0x2D, 0xD1], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(r10), 42);

        // FP16 non-truncating conversion uses MXCSR.RC when EVEX.b is clear.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 16, 0x4100); // +2.5
            x86.mxcsr = (0x1F80 & !(3 << 13)) | (2 << 13); // toward +infinity
        }
        ctx.write_vreg(rax, u64::MAX);
        execute_lifted_x86(&[0x62, 0xF5, 0x7E, 0x08, 0x2D, 0xC3], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), 3);

        // EVEX embedded rounding overrides MXCSR for register sources.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 16, 0xC100); // -2.5
            x86.mxcsr = (0x1F80 & !(3 << 13)) | (2 << 13); // toward +infinity
        }
        execute_lifted_x86(
            &[0x62, 0xF5, 0x7E, 0x38, 0x2D, 0xC3], // {rd-sae}
            &mut ctx,
            &mut memory,
        );
        assert_eq!(ctx.read_vreg(rax), (-3i32) as u32 as u64);

        // Truncating conversion is round-toward-zero regardless of MXCSR.RC.
        execute_lifted_x86(
            &[0x62, 0xF5, 0x7E, 0x18, 0x2C, 0xC3], // {sae}
            &mut ctx,
            &mut memory,
        );
        assert_eq!(ctx.read_vreg(rax), (-2i32) as u32 as u64);

        // W=1 selects a 64-bit destination and EVEX.X' selects XMM19.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[19], 0, 16, 0x5140); // +42
        }
        ctx.write_vreg(r8, 0);
        execute_lifted_x86(&[0x62, 0x35, 0xFE, 0x08, 0x2D, 0xC3], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(r8), 42);

        // Masked-invalid FP16 NaN conversion returns signed integer indefinite
        // and the 32-bit destination write clears the upper GPR half.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 16, 0x7E01);
        }
        ctx.write_vreg(rax, u64::MAX);
        execute_lifted_x86(&[0x62, 0xF5, 0x7E, 0x08, 0x2D, 0xC3], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), 0x8000_0000);

        // The scalar memory tuple compresses disp8 by 2 bytes.
        ctx.write_vreg(rbx, 0x200);
        memory.write(0x2FE, &0x4300u16.to_le_bytes()).unwrap(); // +3.5
        execute_lifted_x86(
            &[0x62, 0xF5, 0x7E, 0x08, 0x2C, 0x43, 0x7F],
            &mut ctx,
            &mut memory,
        );
        assert_eq!(ctx.read_vreg(rax), 3);

        // Unsigned FP16 embedded rounding uses EVEX.RC and writes a 32-bit
        // zero-extended result.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 16, 0x4300); // +3.5
            x86.mxcsr = (0x1F80 & !(3 << 13)) | (1 << 13); // toward -infinity
        }
        ctx.write_vreg(rax, u64::MAX);
        execute_lifted_x86(
            &[0x62, 0xF5, 0x7E, 0x58, 0x79, 0xC3], // {ru-sae}
            &mut ctx,
            &mut memory,
        );
        assert_eq!(ctx.read_vreg(rax), 4);

        // Negative and NaN inputs return the all-ones unsigned integer
        // indefinite value for the selected destination width.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 16, 0xBC00); // -1
        }
        execute_lifted_x86(&[0x62, 0xF5, 0x7E, 0x08, 0x78, 0xC3], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), u32::MAX as u64);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 16, 0x7E01);
        }
        execute_lifted_x86(&[0x62, 0xF5, 0xFE, 0x08, 0x79, 0xC3], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), u64::MAX);

        // FP32 W=0 covers the highest representable value below 2^32; FP64
        // W=1 accepts values above i64::MAX and rejects 2^64.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3][0] = 4_294_967_040.0f32.to_bits() as u64;
        }
        execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x08, 0x78, 0xC3], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), 4_294_967_040);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3][0] = 9_223_372_036_854_775_808.0f64.to_bits();
        }
        execute_lifted_x86(&[0x62, 0xF1, 0xFF, 0x08, 0x78, 0xC3], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), 0x8000_0000_0000_0000);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3][0] = 18_446_744_073_709_551_616.0f64.to_bits();
        }
        execute_lifted_x86(&[0x62, 0xF1, 0xFF, 0x08, 0x78, 0xC3], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), u64::MAX);

        // Successful memory conversion reads the scalar Load result from the
        // virtual scalar register file.
        ctx.write_vreg(rbx, 0x200);
        memory
            .write(0x200, &3.75f64.to_bits().to_le_bytes())
            .unwrap();
        execute_lifted_x86(&[0xF2, 0x48, 0x0F, 0x2C, 0x03], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), 3);
    }
    #[test]
    fn lifted_x86_fp_to_int_memory_fault_preserves_destination_and_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        for (name, bytes) in [
            ("legacy CVTSD2SI", &[0xF2, 0x48, 0x0F, 0x2D, 0x00][..]),
            ("VEX VCVTTSS2SI", &[0xC5, 0xFA, 0x2C, 0x00][..]),
            (
                "EVEX VCVTSH2SI",
                &[0x62, 0xF5, 0x7E, 0x08, 0x2D, 0x40, 0x01][..],
            ),
            (
                "EVEX VCVTSH2USI",
                &[0x62, 0xF5, 0x7E, 0x08, 0x79, 0x40, 0x01][..],
            ),
        ] {
            let mut ctx = SmirContext::new_x86_64();
            ctx.write_vreg(rax, 0x200);
            ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
            ctx.flags.lazy = None;
            let mut short_memory = FlatMemory::new(0x202);
            let exit = execute_lifted_x86(bytes, &mut ctx, &mut short_memory);
            assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ),
                "{name}: {exit:?}"
            );
            assert_eq!(ctx.read_vreg(rax), 0x200, "{name}: destination");
            ctx.flags.materialize_all();
            assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7, "{name}: flags");
        }
    }
    #[test]
    fn lifted_x86_int_to_fp_honors_mxcsr_merge_zeroing_and_source_width() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
        let r10 = VReg::Arch(ArchReg::X86(X86Reg::R10));
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;

        let legacy = [0xABCD_EF01_2345_6789u64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = legacy;
            x86.mxcsr = 0x1F80;
        }
        ctx.write_vreg(rax, 0xFFFF_FFFE);
        execute_lifted_x86(&[0xF3, 0x0F, 0x2A, 0xC8], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let mut expected = legacy;
            expected[0] = (legacy[0] & 0xFFFF_FFFF_0000_0000) | (-2.0f32).to_bits() as u64;
            assert_eq!(
                x86.xmm[1], expected,
                "legacy merge and AVX-upper preservation"
            );
        }

        // 2^24+1 is exactly between adjacent f32 values. MXCSR.RC selects the
        // lower or upper representable result for directed modes.
        for (name, rc, input, expected) in [
            ("nearest", 0u32, 16_777_217i64, 16_777_216f32),
            ("down", 1, 16_777_217, 16_777_216f32),
            ("up", 2, 16_777_217, 16_777_218f32),
            ("zero-negative", 3, -16_777_217, -16_777_216f32),
            ("down-negative", 1, -16_777_217, -16_777_218f32),
        ] {
            let merge = [0x1111_2222_3333_4444u64; 16];
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1] = [u64::MAX; 16];
                x86.xmm[2] = merge;
                x86.mxcsr = (0x1F80 & !(3 << 13)) | (rc << 13);
            }
            ctx.write_vreg(r9, input as u64);
            execute_lifted_x86(&[0xC4, 0xC1, 0xEA, 0x2A, 0xC9], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[1][0] as u32, expected.to_bits(), "{name}");
                assert_eq!(x86.xmm[1][0] >> 32, merge[0] >> 32, "{name}: merge");
                assert_eq!(x86.xmm[1][1], merge[1], "{name}: merge");
                assert!(
                    x86.xmm[1][2..].iter().all(|word| *word == 0),
                    "{name}: VEX upper"
                );
            }
        }

        // 2^53+1 similarly distinguishes directed from nearest rounding for f64.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3] = [0x7777_8888_9999_AAAAu64; 16];
            x86.mxcsr = (0x1F80 & !(3 << 13)) | (2 << 13); // toward +infinity
        }
        ctx.write_vreg(r10, 9_007_199_254_740_993);
        execute_lifted_x86(&[0xC4, 0xC1, 0xE3, 0x2A, 0xD2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.xmm[2][0],
                9_007_199_254_740_994.0f64.to_bits(),
                "directed i64-to-f64 rounding"
            );
            assert_eq!(x86.xmm[2][1], 0x7777_8888_9999_AAAA);
            assert!(x86.xmm[2][2..].iter().all(|word| *word == 0));
        }

        // EVEX high vector registers and 64-bit GPR source.
        let merge = [0x5555_AAAA_1234_5678u64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = merge;
            x86.xmm[17] = [u64::MAX; 16];
            x86.mxcsr = 0x1F80;
        }
        ctx.write_vreg(r10, 42);
        execute_lifted_x86(&[0x62, 0xC1, 0xFE, 0x00, 0x2A, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[17][0] as u32, 42.0f32.to_bits());
            assert_eq!(x86.xmm[17][0] >> 32, merge[0] >> 32);
            assert_eq!(x86.xmm[17][1], merge[1]);
            assert!(x86.xmm[17][2..].iter().all(|word| *word == 0));
        }

        // EVEX W=1 compressed disp8 scales by the qword integer source width.
        ctx.write_vreg(rax, 0x200);
        memory.write(0x240, &(-7i64).to_le_bytes()).unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = merge;
        }
        execute_lifted_x86(
            &[0x62, 0xE1, 0xFE, 0x00, 0x2A, 0x48, 0x08],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[17][0] as u32, (-7.0f32).to_bits());
        }

        // Signed FP16 embedded round-down avoids positive overflow to infinity.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = merge;
            x86.mxcsr = (0x1F80 & !(3 << 13)) | (2 << 13); // toward +infinity
        }
        ctx.write_vreg(rax, 65_520);
        execute_lifted_x86(
            &[0x62, 0xF5, 0xEE, 0x38, 0x2A, 0xC8], // {rd-sae}
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] as u16, 0x7BFF);
            assert_eq!(x86.xmm[1][0] & !0xFFFF, merge[0] & !0xFFFF);
            assert_eq!(x86.xmm[1][1], merge[1]);
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        }

        // Unsigned W=1 conversion preserves the full u64 source domain and
        // honors embedded directed rounding for FP16/FP32/FP64 destinations.
        ctx.write_vreg(rax, u64::MAX);
        execute_lifted_x86(
            &[0x62, 0xF5, 0xEE, 0x78, 0x7B, 0xC8], // FP16 {rz-sae}
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] as u16, 0x7BFF);
        }
        execute_lifted_x86(
            &[0x62, 0xF1, 0xEE, 0x78, 0x7B, 0xC8], // FP32 {rz-sae}
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] as u32, 0x5F7F_FFFF);
        }
        execute_lifted_x86(
            &[0x62, 0xF1, 0xEF, 0x38, 0x7B, 0xC8], // FP64 {rd-sae}
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x43EF_FFFF_FFFF_FFFF);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn lifted_x86_int_to_fp_memory_fault_preserves_destination_and_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let original = [0xCAFE_BABE_DEAD_BEEFu64; 16];
        for (name, bytes, dst) in [
            (
                "legacy CVTSI2SD",
                &[0xF2, 0x48, 0x0F, 0x2A, 0x08][..],
                1usize,
            ),
            (
                "EVEX VCVTSI2SS",
                &[0x62, 0xE1, 0xFE, 0x00, 0x2A, 0x08][..],
                17usize,
            ),
            (
                "EVEX VCVTUSI2SH",
                &[0x62, 0xE5, 0xFE, 0x00, 0x7B, 0x08][..],
                17usize,
            ),
        ] {
            let mut ctx = SmirContext::new_x86_64();
            ctx.write_vreg(rax, 0x200);
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
                assert_eq!(x86.xmm[dst], original, "{name}: destination");
            }
            ctx.flags.materialize_all();
            assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7, "{name}: flags");
        }
    }
    #[test]
    fn lifted_x86_scalar_fp_convert_honors_rounding_merge_and_upper_state() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut memory = FlatMemory::new(0x1000);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;

        let legacy = [0xCAFE_BABE_DEAD_BEEFu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = legacy;
            x86.xmm[1][0] = 1.5f32.to_bits() as u64;
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(&[0xF3, 0x0F, 0x5A, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let mut expected = legacy;
            expected[0] = 1.5f64.to_bits();
            assert_eq!(
                x86.xmm[0], expected,
                "legacy widening preserves upper state"
            );
        }

        let midpoint = 1.0f64 + 2.0f64.powi(-24);
        for (name, rc, expected) in [
            ("nearest-even", 0u32, 1.0f32.to_bits()),
            ("down", 1, 1.0f32.to_bits()),
            ("up", 2, 1.0f32.to_bits() + 1),
            ("toward-zero", 3, 1.0f32.to_bits()),
        ] {
            let merge = [0x0123_4567_89AB_CDEFu64; 16];
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [u64::MAX; 16];
                x86.xmm[1] = merge;
                x86.xmm[2][0] = midpoint.to_bits();
                x86.mxcsr = (0x1F80 & !(3 << 13)) | (rc << 13);
            }
            execute_lifted_x86(&[0xC5, 0xF3, 0x5A, 0xC2], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[0][0] as u32, expected, "{name}");
                assert_eq!(x86.xmm[0][0] >> 32, merge[0] >> 32, "{name}: merge");
                assert_eq!(x86.xmm[0][1], merge[1], "{name}: merge");
                assert!(
                    x86.xmm[0][2..].iter().all(|word| *word == 0),
                    "{name}: upper"
                );
            }
        }

        // Negative directed rounding selects the more-negative neighbour.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0; 16];
            x86.xmm[2][0] = (-midpoint).to_bits();
            x86.mxcsr = (0x1F80 & !(3 << 13)) | (1 << 13);
        }
        execute_lifted_x86(&[0xC5, 0xF3, 0x5A, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0] as u32, (-1.0f32).to_bits() + 1);
        }

        // EVEX high destination/merge plus compressed f64 memory source.
        let merge = [0x7777_8888_9999_AAAAu64; 16];
        ctx.write_vreg(rax, 0x200);
        memory
            .write(0x240, &2.25f64.to_bits().to_le_bytes())
            .unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = merge;
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(
            &[0x62, 0xE1, 0xFF, 0x00, 0x5A, 0x50, 0x08],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[18][0] as u32, 2.25f32.to_bits());
            assert_eq!(x86.xmm[18][0] >> 32, merge[0] >> 32);
            assert_eq!(x86.xmm[18][1], merge[1]);
            assert!(x86.xmm[18][2..].iter().all(|word| *word == 0));
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn x86_scalar_precision_softfloat_core_is_exact_across_f16_f32_f64() {
        for raw in 0u16..=u16::MAX {
            if raw & 0x7C00 == 0x7C00 && raw & 0x03FF != 0 {
                continue;
            }
            let converted = SmirInterpreter::x86_simd_fp_convert_precision(
                u64::from(raw),
                X86_SIMD_F16,
                X86_SIMD_F32,
                FpRoundMode::RoundNearest,
                0x1F80,
                true,
            );
            assert_eq!(
                converted.bits as u32,
                SmirInterpreter::x86_fp16_to_f32(raw).to_bits(),
                "f16 0x{raw:04X}"
            );
        }

        let mut state = 0xD1B5_4A32_D192_ED03u64;
        for _ in 0..20_000 {
            state = state
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(0xBF58_476D_1CE4_E5B9);
            let f32_bits = state as u32;
            if f32_bits & 0x7F80_0000 != 0x7F80_0000 {
                let widened = SmirInterpreter::x86_simd_fp_convert_precision(
                    u64::from(f32_bits),
                    X86_SIMD_F32,
                    X86_SIMD_F64,
                    FpRoundMode::RoundNearest,
                    0x1F80,
                    true,
                );
                assert_eq!(widened.bits, (f32::from_bits(f32_bits) as f64).to_bits());
                let narrowed = SmirInterpreter::x86_simd_fp_convert_precision(
                    u64::from(f32_bits),
                    X86_SIMD_F32,
                    X86_SIMD_F16,
                    FpRoundMode::RoundNearest,
                    0x1F80,
                    true,
                );
                assert_eq!(
                    narrowed.bits as u16,
                    SmirInterpreter::x86_f32_to_fp16(f32::from_bits(f32_bits), 0)
                );
            }

            let f64_bits = state.rotate_left(29);
            if f64_bits & 0x7FF0_0000_0000_0000 != 0x7FF0_0000_0000_0000 {
                let narrowed = SmirInterpreter::x86_simd_fp_convert_precision(
                    f64_bits,
                    X86_SIMD_F64,
                    X86_SIMD_F32,
                    FpRoundMode::RoundNearest,
                    0x1F80,
                    true,
                );
                assert_eq!(
                    narrowed.bits as u32,
                    (f64::from_bits(f64_bits) as f32).to_bits()
                );
            }
        }

        // This value is just above an FP16 midpoint but rounds to the midpoint
        // in FP32. Direct FP64->FP16 must round upward, avoiding double rounding.
        let double_rounding_probe = 1.0f64 + 2.0f64.powi(-11) + 2.0f64.powi(-30);
        let direct = SmirInterpreter::x86_simd_fp_convert_precision(
            double_rounding_probe.to_bits(),
            X86_SIMD_F64,
            X86_SIMD_F16,
            FpRoundMode::RoundNearest,
            0x1F80,
            true,
        );
        assert_eq!(direct.bits, 0x3C01);
        assert_eq!(
            SmirInterpreter::x86_f32_to_fp16(double_rounding_probe as f32, 0),
            0x3C00,
            "probe must distinguish direct conversion from an FP32 intermediate"
        );

        for (source, mode, expected) in [
            (1.0f64 + 2.0f64.powi(-11), FpRoundMode::RoundNearest, 0x3C00),
            (1.0f64 + 2.0f64.powi(-11), FpRoundMode::RoundDown, 0x3C00),
            (1.0f64 + 2.0f64.powi(-11), FpRoundMode::RoundUp, 0x3C01),
            (
                1.0f64 + 2.0f64.powi(-11),
                FpRoundMode::RoundTowardZero,
                0x3C00,
            ),
            (
                -1.0f64 - 2.0f64.powi(-11),
                FpRoundMode::RoundNearest,
                0xBC00,
            ),
            (-1.0f64 - 2.0f64.powi(-11), FpRoundMode::RoundDown, 0xBC01),
            (-1.0f64 - 2.0f64.powi(-11), FpRoundMode::RoundUp, 0xBC00),
            (
                -1.0f64 - 2.0f64.powi(-11),
                FpRoundMode::RoundTowardZero,
                0xBC00,
            ),
        ] {
            let converted = SmirInterpreter::x86_simd_fp_convert_precision(
                source.to_bits(),
                X86_SIMD_F64,
                X86_SIMD_F16,
                mode,
                0x1F80,
                true,
            );
            assert_eq!(converted.bits as u16, expected, "{source} {mode:?}");
        }

        for (bits, from, to, expected) in [
            (0x8000_0000_0000_0000, X86_SIMD_F64, X86_SIMD_F16, 0x8000),
            (0xFFF0_0000_0000_0000, X86_SIMD_F64, X86_SIMD_F16, 0xFC00),
            (0x8000, X86_SIMD_F16, X86_SIMD_F64, 0x8000_0000_0000_0000),
            (0xFC00, X86_SIMD_F16, X86_SIMD_F64, 0xFFF0_0000_0000_0000),
        ] {
            let converted = SmirInterpreter::x86_simd_fp_convert_precision(
                bits,
                from,
                to,
                FpRoundMode::RoundNearest,
                0x1F80,
                true,
            );
            assert_eq!(converted.bits, expected);
            assert_eq!(converted.status, 0);
        }

        let snan = SmirInterpreter::x86_simd_fp_convert_precision(
            0x7FF0_0123_4567_89AB,
            X86_SIMD_F64,
            X86_SIMD_F16,
            FpRoundMode::RoundNearest,
            0x1F80,
            true,
        );
        let expected_payload = ((0x0000_0123_4567_89ABu64 >> 42) as u16) & 0x03FF;
        assert_eq!(snan.bits as u16, 0x7C00 | expected_payload | 0x0200);
        assert_eq!(snan.status & 1, 1);
        let qnan = SmirInterpreter::x86_simd_fp_convert_precision(
            0xFFF8_0123_4567_89AB,
            X86_SIMD_F64,
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1F80,
            true,
        );
        assert_eq!(qnan.bits, 0xFFC0_091A);
        assert_eq!(qnan.status & 1, 0);

        let denormal = SmirInterpreter::x86_simd_fp_convert_precision(
            1,
            X86_SIMD_F16,
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1F80,
            true,
        );
        assert_eq!(denormal.bits, 0x3380_0000);
        assert_eq!(denormal.status & (1 << 1), 1 << 1);
        let daz = SmirInterpreter::x86_simd_fp_convert_precision(
            1,
            X86_SIMD_F16,
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1F80 | (1 << 6),
            true,
        );
        assert_eq!(daz.bits, 0x3380_0000);
        assert_eq!(daz.status & (1 << 1), 1 << 1);
        let fp16_no_denormal_status = SmirInterpreter::x86_simd_fp_convert_precision(
            1,
            X86_SIMD_F16,
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1F80 | (1 << 6),
            false,
        );
        assert_eq!(fp16_no_denormal_status.bits, 0x3380_0000);
        assert_eq!(fp16_no_denormal_status.status, 0);

        let overflow = SmirInterpreter::x86_simd_fp_convert_precision(
            u64::from(f32::MAX.to_bits()),
            X86_SIMD_F32,
            X86_SIMD_F16,
            FpRoundMode::RoundNearest,
            0x1F80,
            true,
        );
        assert_eq!(overflow.bits, 0x7C00);
        assert_eq!(overflow.status & ((1 << 3) | (1 << 5)), (1 << 3) | (1 << 5));
        let underflow = SmirInterpreter::x86_simd_fp_convert_precision(
            u64::from(f32::MIN_POSITIVE.to_bits()),
            X86_SIMD_F32,
            X86_SIMD_F16,
            FpRoundMode::RoundNearest,
            0x1F80,
            true,
        );
        assert_eq!(underflow.bits, 0);
        assert_eq!(
            underflow.status & ((1 << 4) | (1 << 5)),
            (1 << 4) | (1 << 5)
        );
    }
    #[test]
    fn lifted_x86_scalar_fp_convert_memory_fault_preserves_destination_and_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let original = [0xA5A5_5A5A_0123_4567u64; 16];
        for (name, bytes, dst) in [
            ("legacy CVTSS2SD", &[0xF3, 0x0F, 0x5A, 0x00][..], 0usize),
            ("VEX VCVTSD2SS", &[0xC5, 0xF3, 0x5A, 0x00][..], 0usize),
        ] {
            let mut ctx = SmirContext::new_x86_64();
            ctx.write_vreg(rax, 0x200);
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
                assert_eq!(x86.xmm[dst], original, "{name}: destination");
            }
            ctx.flags.materialize_all();
            assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7, "{name}: flags");
        }
    }
    #[test]
    fn lifted_vcvtps2ph_register_memory_mask_rounding_sae_and_fault_atomicity() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let midpoint = 1.0f32 + 2.0f32.powi(-11);
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;

        // ModRM:r/m is the destination and ModRM:reg is the source. The
        // immediate overrides an oppositely directed MXCSR rounding mode.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [u64::MAX; 16];
            for lane in 0..4 {
                SmirInterpreter::set_lane(&mut x86.xmm[2], lane, 32, u64::from(midpoint.to_bits()));
            }
            x86.mxcsr = (0x1F80 & !(3 << 13)) | (1 << 13); // Round down.
        }
        let exit = execute_lifted_x86(&[0xC4, 0xE3, 0x79, 0x1D, 0xD1, 0x02], &mut ctx, &mut memory);
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..4 {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 16), 0x3C01);
            }
            assert!(x86.xmm[1][1..].iter().all(|word| *word == 0));
        }

        // A masked memory destination writes only active 2-byte lanes.
        ctx.write_vreg(rax, 0x200);
        ctx.write_vreg(k1, 0b0101);
        memory.write(0x200, &[0xA5; 8]).unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for (lane, value) in [1.0f32, 2.0, 3.0, 4.0].into_iter().enumerate() {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[2],
                    lane as u8,
                    32,
                    u64::from(value.to_bits()),
                );
            }
            x86.mxcsr = 0x1F80;
        }
        let exit = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x09, 0x1D, 0x10, 0x00],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        let mut stored = [0u8; 8];
        memory.read(0x200, &mut stored).unwrap();
        assert_eq!(&stored[0..2], &0x3C00u16.to_le_bytes());
        assert_eq!(&stored[2..4], &[0xA5; 2]);
        assert_eq!(&stored[4..6], &0x4200u16.to_le_bytes());
        assert_eq!(&stored[6..8], &[0xA5; 2]);

        // DAZ zeroes an FP32 denormal input; FTZ never zeroes an FP16
        // denormal output.
        ctx.write_vreg(k1, 0b11);
        memory.write(0x200, &[0xA5; 8]).unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 32, 1);
            SmirInterpreter::set_lane(
                &mut x86.xmm[2],
                1,
                32,
                u64::from(2.0f32.powi(-24).to_bits()),
            );
            x86.mxcsr = 0x1F80 | (1 << 6) | (1 << 15);
        }
        let exit = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x09, 0x1D, 0x10, 0x04],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        memory.read(0x200, &mut stored).unwrap();
        assert_eq!(u16::from_le_bytes(stored[0..2].try_into().unwrap()), 0);
        assert_eq!(u16::from_le_bytes(stored[2..4].try_into().unwrap()), 1);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr & (1 << 1), 0, "DAZ must suppress DE");
        }

        // Preflight covers every active lane before the first write.
        let mut short_memory = FlatMemory::new(0x204);
        short_memory.write(0x200, &[0xA5; 4]).unwrap();
        ctx.write_vreg(rax, 0x200);
        ctx.write_vreg(k1, 0b0101);
        let exit = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x09, 0x1D, 0x10, 0x00],
            &mut ctx,
            &mut short_memory,
        );
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        let mut untouched = [0u8; 4];
        short_memory.read(0x200, &mut untouched).unwrap();
        assert_eq!(untouched, [0xA5; 4]);

        // An all-zero mask suppresses every destination memory fault.
        ctx.write_vreg(rax, 0x2000);
        ctx.write_vreg(k1, 0);
        let exit = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x09, 0x1D, 0x10, 0x00],
            &mut ctx,
            &mut short_memory,
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));

        // A backend write fault after successful preflight must not commit a
        // masked precision status update.
        let mut fault_inner = FlatMemory::new(0x400);
        fault_inner.write(0x200, &[0xA5; 4]).unwrap();
        let mut write_fault_memory = StoreFaultMemory {
            inner: fault_inner,
            stores_before_fault: 0,
        };
        ctx.write_vreg(rax, 0x200);
        ctx.write_vreg(k1, 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 32, u64::from(midpoint.to_bits()));
            x86.mxcsr = 0x1F80;
        }
        let exit = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x09, 0x1D, 0x10, 0x00],
            &mut ctx,
            &mut write_fault_memory,
        );
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr & (1 << 5), 0);
        }

        // An unmasked conversion exception updates sticky status but commits
        // no memory write.
        ctx.write_vreg(rax, 0x200);
        ctx.write_vreg(k1, 1);
        short_memory.write(0x200, &[0xA5; 4]).unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 32, u64::from(f32::MAX.to_bits()));
            x86.mxcsr = 0x1F80 & !(1 << 10);
        }
        let exit = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x09, 0x1D, 0x10, 0x00],
            &mut ctx,
            &mut short_memory,
        );
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        short_memory.read(0x200, &mut untouched).unwrap();
        assert_eq!(untouched, [0xA5; 4]);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr & (1 << 3), 1 << 3);
        }

        // EVEX.b supplies SAE, not rounding: imm8 still selects truncation,
        // and a signaling NaN cannot update MXCSR or trap.
        ctx.write_vreg(k1, 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [u64::MAX; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 32, 0x7F80_0001);
            x86.mxcsr = 0x1F80 & !(1 << 7);
        }
        let mxcsr_before = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.mxcsr,
            _ => unreachable!(),
        };
        let exit = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x99, 0x1D, 0xD1, 0x03],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr, mxcsr_before);
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], 0, 16) & 0x7C00,
                0x7C00
            );
            assert_ne!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16) & 0x0200, 0);
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
    #[test]
    fn x87_u128_integer_sqrt_satisfies_floor_invariant() {
        for value in 0u128..10_000 {
            let root = SmirInterpreter::x86_x87_integer_sqrt(value);
            assert!(root * root <= value, "{value}: lower bound");
            assert!((root + 1) * (root + 1) > value, "{value}: upper bound");
        }
        for value in [
            1u128 << 126,
            (1u128 << 127) - 1,
            1u128 << 127,
            u128::MAX - (1u128 << 64),
            u128::MAX,
        ] {
            let root = SmirInterpreter::x86_x87_integer_sqrt(value);
            assert!(root * root <= value, "{value}: lower bound");
            if root < u64::MAX as u128 {
                assert!((root + 1) * (root + 1) > value, "{value}: upper bound");
            } else {
                assert_eq!(root, u64::MAX as u128);
            }
        }
    }
    #[test]
    fn x87_narrow_ieee_conversion_covers_rounding_boundaries_and_special_values() {
        fn raw(significand: u64, exponent_sign: u16) -> [u8; 10] {
            let mut value = [0u8; 10];
            value[..8].copy_from_slice(&significand.to_le_bytes());
            value[8..].copy_from_slice(&exponent_sign.to_le_bytes());
            value
        }
        let convert32 = |value: [u8; 10], rc| SmirInterpreter::x86_x87_to_ieee(&value, 8, 23, rc);

        for (name, value, expected_bits, invalid) in [
            (
                "exact normal",
                raw(0xC000_0000_0000_0000, 0x3FFF),
                0x3FC0_0000u64,
                false,
            ),
            ("negative zero", raw(0, 0x8000), 0x8000_0000, false),
            (
                "negative infinity",
                raw(0x8000_0000_0000_0000, 0xFFFF),
                0xFF80_0000,
                false,
            ),
            (
                "quiet NaN payload",
                raw(0xC123_4567_89AB_CDEF, 0x7FFF),
                0x7FC1_2345,
                false,
            ),
            (
                "signaling NaN payload",
                raw(0x8123_4567_89AB_CDEF, 0x7FFF),
                0x7FC1_2345,
                true,
            ),
            (
                "unsupported encoding",
                raw(0x4123_4567_89AB_CDEF, 0x4000),
                0xFFC0_0000,
                true,
            ),
        ] {
            let conversion = convert32(value, 0);
            assert_eq!(conversion.bits, expected_bits, "{name}");
            assert_eq!(conversion.invalid, invalid, "{name}: IE");
            assert!(!conversion.overflow, "{name}: OE");
            assert!(!conversion.underflow, "{name}: UE");
            assert!(!conversion.inexact, "{name}: PE");
            assert!(!conversion.rounded_up, "{name}: C1");
        }

        let half_ulp_above_one = raw(0x8000_0080_0000_0000, 0x3FFF);
        for (rc, bits, rounded_up) in [
            (0u16, 0x3F80_0000u64, false),
            (1, 0x3F80_0000, false),
            (2, 0x3F80_0001, true),
            (3, 0x3F80_0000, false),
        ] {
            let conversion = convert32(half_ulp_above_one, rc);
            assert_eq!(conversion.bits, bits, "RC={rc}");
            assert!(conversion.inexact, "RC={rc}: PE");
            assert_eq!(conversion.rounded_up, rounded_up, "RC={rc}: C1");
            assert!(!conversion.invalid, "RC={rc}: IE");
            assert!(!conversion.overflow, "RC={rc}: OE");
            assert!(!conversion.underflow, "RC={rc}: UE");
        }

        for (name, value, rc, bits, underflow, rounded_up) in [
            (
                "minimum subnormal exact",
                raw(0x8000_0000_0000_0000, 0x3F6A),
                0u16,
                0x0000_0001u64,
                false,
                false,
            ),
            (
                "half minimum subnormal nearest",
                raw(0x8000_0000_0000_0000, 0x3F69),
                0,
                0,
                true,
                false,
            ),
            (
                "half minimum subnormal upward",
                raw(0x8000_0000_0000_0000, 0x3F69),
                2,
                1,
                true,
                true,
            ),
            (
                "below minimum normal rounds normal",
                raw(0xFFFF_FF00_0000_0000, 0x3F80),
                0,
                0x0080_0000,
                true,
                true,
            ),
        ] {
            let conversion = convert32(value, rc);
            assert_eq!(conversion.bits, bits, "{name}");
            assert_eq!(conversion.underflow, underflow, "{name}: UE");
            assert_eq!(conversion.inexact, underflow, "{name}: PE");
            assert_eq!(conversion.rounded_up, rounded_up, "{name}: C1");
        }

        let overflow_threshold = raw(0xFFFF_FF80_0000_0000, 0x407E);
        let nearest = convert32(overflow_threshold, 0);
        assert_eq!(nearest.bits, 0x7F80_0000);
        assert!(nearest.overflow && nearest.inexact && nearest.rounded_up);
        let downward = convert32(overflow_threshold, 1);
        assert_eq!(downward.bits, 0x7F7F_FFFF);
        assert!(!downward.overflow && downward.inexact && !downward.rounded_up);

        let two_pow_128 = convert32(raw(0x8000_0000_0000_0000, 0x407F), 1);
        assert_eq!(two_pow_128.bits, 0x7F7F_FFFF);
        assert!(two_pow_128.overflow && two_pow_128.inexact && !two_pow_128.rounded_up);

        for (rc, bits, rounded_up) in [
            (0u16, 0xFF80_0000u64, true),
            (1, 0xFF80_0000, true),
            (2, 0xFF7F_FFFF, false),
            (3, 0xFF7F_FFFF, false),
        ] {
            let conversion = convert32(raw(0x8000_0000_0000_0000, 0xC07F), rc);
            assert_eq!(conversion.bits, bits, "negative overflow RC={rc}");
            assert!(conversion.overflow && conversion.inexact, "RC={rc}");
            assert_eq!(conversion.rounded_up, rounded_up, "RC={rc}: C1");
        }

        for (rc, bits, rounded_up) in [
            (0u16, 0x8000_0000u64, false),
            (1, 0x8000_0001, true),
            (2, 0x8000_0000, false),
            (3, 0x8000_0000, false),
        ] {
            let conversion = convert32(raw(0x8000_0000_0000_0000, 0xBF69), rc);
            assert_eq!(conversion.bits, bits, "negative underflow RC={rc}");
            assert!(conversion.underflow && conversion.inexact, "RC={rc}");
            assert_eq!(conversion.rounded_up, rounded_up, "RC={rc}: C1");
        }

        // Exercise the generic binary64 path at precision and range edges.
        let f64_half_ulp =
            SmirInterpreter::x86_x87_to_ieee(&raw(0x8000_0000_0000_0400, 0x3FFF), 11, 52, 2);
        assert_eq!(f64_half_ulp.bits, 0x3FF0_0000_0000_0001);
        assert!(f64_half_ulp.inexact && f64_half_ulp.rounded_up);
        let f64_min_sub =
            SmirInterpreter::x86_x87_to_ieee(&raw(0x8000_0000_0000_0000, 0x3BCD), 11, 52, 0);
        assert_eq!(f64_min_sub.bits, 1);
        assert!(!f64_min_sub.underflow && !f64_min_sub.inexact);
        let f64_max =
            SmirInterpreter::x86_x87_to_ieee(&raw(0xFFFF_FFFF_FFFF_F800, 0x43FE), 11, 52, 0);
        assert_eq!(f64_max.bits, 0x7FEF_FFFF_FFFF_FFFF);
        assert!(!f64_max.overflow && !f64_max.inexact);
    }
    #[test]
    fn lifted_xgetbv_xsetbv_roundtrip_dependencies_and_faults() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));

        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(1);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xcr0 = 0x0008_00E7;
            x86.xgetbv1 = 0x0000_0025;
        }
        ctx.write_vreg(rcx, 0);
        ctx.write_vreg(rax, u64::MAX);
        ctx.write_vreg(rdx, u64::MAX);
        execute_lifted_x86(&[0x0F, 0x01, 0xD0], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), 0x0008_00E7);
        assert_eq!(ctx.read_vreg(rdx), 0);
        ctx.write_vreg(rcx, 1);
        execute_lifted_x86(&[0x0F, 0x01, 0xD0], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), 0x25);
        assert_eq!(ctx.read_vreg(rdx), 0);
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);

        for value in [1u64, 3, 7, 0xE7, 0x0008_0001, 0x0008_00E7] {
            let mut ctx = SmirContext::new_x86_64();
            ctx.write_vreg(rcx, 0);
            ctx.write_vreg(rax, value | 0xFFFF_FFFF_0000_0000);
            ctx.write_vreg(rdx, value >> 32);
            execute_lifted_x86(&[0x0F, 0x01, 0xD1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xcr0, value, "valid XCR0={value:#x}");
            }
        }

        for (name, selector, value) in [
            ("x87 disabled", 0u64, 0u64),
            ("AVX without SSE", 0, 5),
            ("partial AVX-512", 0, 0x27),
            ("unknown component", 0, 0x101),
            ("nonzero selector", 1, 1),
        ] {
            let mut ctx = SmirContext::new_x86_64();
            ctx.write_vreg(rcx, selector);
            ctx.write_vreg(rax, value as u32 as u64);
            ctx.write_vreg(rdx, value >> 32);
            let exit = execute_lifted_x86(&[0x0F, 0x01, 0xD1], &mut ctx, &mut memory);
            assert!(
                matches!(
                    exit,
                    BlockResult::Exit(ExitReason::GeneralProtection {
                        addr: 0x1000,
                        error_code: 0
                    })
                ),
                "{name}: {exit:?}"
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xcr0, 1, "{name}: XCR0 unchanged");
            }
        }

        let mut ctx = SmirContext::new_x86_64();
        ctx.write_vreg(rcx, 2);
        ctx.write_vreg(rax, 0xAAAA_AAAA);
        ctx.write_vreg(rdx, 0xBBBB_BBBB);
        let exit = execute_lifted_x86(&[0x0F, 0x01, 0xD0], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::GeneralProtection {
                addr: 0x1000,
                error_code: 0
            })
        ));
        assert_eq!(ctx.read_vreg(rax), 0xAAAA_AAAA);
        assert_eq!(ctx.read_vreg(rdx), 0xBBBB_BBBB);
    }
    #[test]
    fn lifted_xsave_xsaveopt_xrstor_roundtrip_masks_initialization_and_faults() {
        fn read_u64(memory: &mut FlatMemory, addr: u64) -> u64 {
            let mut bytes = [0u8; 8];
            memory.read(addr, &mut bytes).unwrap();
            u64::from_le_bytes(bytes)
        }

        const ALL: u64 = 0x0008_00E7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x2000);
        ctx.write_vreg(rbx, 0x100);
        ctx.write_vreg(rax, ALL);
        ctx.write_vreg(rdx, 0);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;
        memory.write(0x100, &[0u8; 2688]).unwrap();
        memory
            .write(0x100 + 520, &0x0123_4567_89AB_CDEFu64.to_le_bytes())
            .unwrap();

        let x87_raw = [1, 2, 3, 4, 5, 6, 7, 0x80, 0xFF, 0x3F];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xcr0 = ALL;
            x86.x87.control_word = 0x027F;
            x86.x87.status_word = 3 << 11;
            x86.x87.instr_ptr = 0x1122_3344_5566_7788;
            x86.x87.data_ptr = 0x8877_6655_4433_2211;
            x86.x87.last_opcode = 0x345;
            x86.x87.set_logical_raw(0, x87_raw);
            x86.mxcsr = 0x1FC0;
            x86.xmm[0][0..8].copy_from_slice(&[0x10, 0x11, 0x20, 0x21, 0x30, 0x31, 0x32, 0x33]);
            x86.xmm[16][0..8].copy_from_slice(&[0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47]);
            x86.k[3] = 0x5152_5354_5556_5758;
            x86.gpr[16] = 0x6162_6364_6566_6768;
        }

        execute_lifted_x86(&[0x48, 0x0F, 0xAE, 0x23], &mut ctx, &mut memory);
        assert_eq!(read_u64(&mut memory, 0x100 + 512), ALL);
        assert_eq!(
            read_u64(&mut memory, 0x100 + 520),
            0x0123_4567_89AB_CDEF,
            "standard XSAVE must preserve XCOMP_BV"
        );
        assert_eq!(read_u64(&mut memory, 0x100 + 576), 0x20);
        assert_eq!(read_u64(&mut memory, 0x100 + 960), 0x6162_6364_6566_6768);
        assert_eq!(
            read_u64(&mut memory, 0x100 + 1088 + 3 * 8),
            0x5152_5354_5556_5758
        );
        assert_eq!(read_u64(&mut memory, 0x100 + 1152), 0x30);
        assert_eq!(read_u64(&mut memory, 0x100 + 1664), 0x40);
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);

        // A standard-format image requires XCOMP_BV and the remaining header
        // bytes to be zero before XRSTOR.
        memory.write(0x100 + 520, &[0u8; 56]).unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.x87 = Default::default();
            x86.mxcsr = 0x1F80;
            x86.xmm = [[0; 16]; 32];
            x86.k = [0; 8];
            x86.gpr[16..32].fill(0);
        }
        execute_lifted_x86(&[0x48, 0x0F, 0xAE, 0x2B], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.control_word, 0x027F);
            assert_eq!(x86.x87.instr_ptr, 0x1122_3344_5566_7788);
            assert_eq!(x86.x87.data_ptr, 0x8877_6655_4433_2211);
            assert_eq!(x86.x87.regs[x86.x87.physical_index(0)], x87_raw);
            assert_eq!(x86.mxcsr, 0x1FC0);
            assert_eq!(
                &x86.xmm[0][0..8],
                &[0x10, 0x11, 0x20, 0x21, 0x30, 0x31, 0x32, 0x33]
            );
            assert_eq!(
                &x86.xmm[16][0..8],
                &[0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47]
            );
            assert_eq!(x86.k[3], 0x5152_5354_5556_5758);
            assert_eq!(x86.gpr[16], 0x6162_6364_6566_6768);
        }

        // Clearing XSTATE_BV requests architectural initial state for every
        // component, while MXCSR is still loaded when SSE or AVX is requested.
        memory.write(0x100 + 512, &[0u8; 64]).unwrap();
        memory
            .write(0x100 + 24, &0x0000_1F80u32.to_le_bytes())
            .unwrap();
        execute_lifted_x86(&[0x0F, 0xAE, 0x2B], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.control_word, 0x037F);
            assert_eq!(x86.x87.status_word, 0);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
            assert_eq!(x86.mxcsr, 0x1F80);
            assert!(
                x86.xmm
                    .iter()
                    .all(|register| register[..8].iter().all(|lane| *lane == 0))
            );
            assert_eq!(x86.k, [0; 8]);
            assert!(x86.gpr[16..32].iter().all(|register| *register == 0));
        }

        // A partial AVX-only XSAVEOPT transfers MXCSR and YMM_Hi128, preserves
        // unrelated legacy bytes, and preserves unrequested XSTATE_BV bits.
        memory.write(0x500, &[0xA5; 2688]).unwrap();
        memory
            .write(0x500 + 512, &(1u64 | (1 << 5)).to_le_bytes())
            .unwrap();
        memory
            .write(0x500 + 520, &0xCAFE_BABE_DEAD_BEEFu64.to_le_bytes())
            .unwrap();
        ctx.write_vreg(rbx, 0x500);
        ctx.write_vreg(rax, 1 << 2);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0][2] = 0xDEAD_BEEF;
            x86.mxcsr = 0x1FA0;
        }
        execute_lifted_x86(&[0x0F, 0xAE, 0x33], &mut ctx, &mut memory);
        let mut byte = [0u8; 1];
        memory.read(0x500, &mut byte).unwrap();
        assert_eq!(byte[0], 0xA5);
        assert_eq!(read_u64(&mut memory, 0x500 + 160), 0xA5A5_A5A5_A5A5_A5A5);
        assert_eq!(read_u64(&mut memory, 0x500 + 576), 0xDEAD_BEEF);
        assert_eq!(read_u64(&mut memory, 0x500 + 512), 1 | (1 << 2) | (1 << 5));
        assert_eq!(read_u64(&mut memory, 0x500 + 520), 0xCAFE_BABE_DEAD_BEEF);

        // Malformed headers, invalid MXCSR, and misalignment produce #GP(0)
        // and do not commit restored component state.
        ctx.write_vreg(rax, 1 << 1);
        memory
            .write(0x500 + 512, &(1u64 << 1).to_le_bytes())
            .unwrap();
        memory.write(0x500 + 520, &[0u8; 56]).unwrap();
        memory
            .write(0x500 + 24, &0x0001_0000u32.to_le_bytes())
            .unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F80;
            x86.xmm[0][0] = 0x7777;
        }
        let exit = execute_lifted_x86(&[0x0F, 0xAE, 0x2B], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::GeneralProtection {
                addr: 0x1000,
                error_code: 0
            })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr, 0x1F80);
            assert_eq!(x86.xmm[0][0], 0x7777);
        }

        memory.write(0x500 + 24, &0x1F80u32.to_le_bytes()).unwrap();
        memory.write(0x500 + 528, &[1]).unwrap();
        let exit = execute_lifted_x86(&[0x0F, 0xAE, 0x2B], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::GeneralProtection { .. })
        ));
        memory.write(0x500 + 528, &[0]).unwrap();
        memory.write(0x500 + 536, &[1]).unwrap();
        let exit = execute_lifted_x86(&[0x0F, 0xAE, 0x2B], &mut ctx, &mut memory);
        assert!(
            !matches!(
                exit,
                BlockResult::Exit(ExitReason::GeneralProtection { .. })
            ),
            "standard XRSTOR ignores XSAVE-header bytes 63:24"
        );
        ctx.write_vreg(rbx, 0x508);
        let exit = execute_lifted_x86(&[0x0F, 0xAE, 0x23], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::GeneralProtection { .. })
        ));

        // An x87-only request accesses no extended-state tail.
        let mut narrow = FlatMemory::new(576);
        let mut narrow_ctx = SmirContext::new_x86_64();
        narrow_ctx.write_vreg(rbx, 0);
        narrow_ctx.write_vreg(rax, 1);
        execute_lifted_x86(&[0x0F, 0xAE, 0x23], &mut narrow_ctx, &mut narrow);
        narrow.write(520, &[0u8; 56]).unwrap();
        let exit = execute_lifted_x86(&[0x0F, 0xAE, 0x2B], &mut narrow_ctx, &mut narrow);
        assert!(!matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));

        // XSAVEOPT applies the init optimization to component payloads while
        // always transferring MXCSR/MXCSR_MASK for an SSE-or-AVX request.
        let mut opt_ctx = SmirContext::new_x86_64();
        let mut opt_memory = FlatMemory::new(0x400);
        opt_ctx.write_vreg(rbx, 0);
        opt_ctx.write_vreg(rax, 0x6);
        if let ArchRegState::X86_64(x86) = &mut opt_ctx.arch_regs {
            x86.xcr0 = 0x7;
        }
        opt_memory.write(0, &[0xA5; 0x400]).unwrap();
        opt_memory.write(512, &[0u8; 64]).unwrap();
        execute_lifted_x86(&[0x0F, 0xAE, 0x33], &mut opt_ctx, &mut opt_memory);
        assert_eq!(read_u64(&mut opt_memory, 160), 0xA5A5_A5A5_A5A5_A5A5);
        assert_eq!(read_u64(&mut opt_memory, 576), 0xA5A5_A5A5_A5A5_A5A5);
        assert_eq!(read_u64(&mut opt_memory, 512), 0);
        let mut mxcsr = [0u8; 4];
        opt_memory.read(24, &mut mxcsr).unwrap();
        assert_eq!(u32::from_le_bytes(mxcsr), 0x1F80);
    }
    #[test]
    fn lifted_compacted_xsave_family_layout_restore_and_validation() {
        fn read_u64(memory: &mut FlatMemory, addr: u64) -> u64 {
            let mut bytes = [0u8; 8];
            memory.read(addr, &mut bytes).unwrap();
            u64::from_le_bytes(bytes)
        }

        const ALL: u64 = 0x0008_00E7;
        const COMPACTED: u64 = 1 << 63;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x2000);
        ctx.write_vreg(rbx, 0x100);
        ctx.write_vreg(rax, ALL);
        ctx.write_vreg(rdx, 0);
        memory.write(0x100, &[0xA5; 2688]).unwrap();
        memory.write(0x100 + 512, &[0u8; 64]).unwrap();

        let x87_raw = [9, 8, 7, 6, 5, 4, 3, 0x80, 0xFF, 0x3F];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xcr0 = ALL;
            x86.x87.set_logical_raw(0, x87_raw);
            // XINUSE[1] excludes MXCSR, but compacted saves have a specified
            // non-default-MXCSR exception that sets XSTATE_BV[1].
            x86.mxcsr = 0x1FC0;
            x86.k[0] = 0x1111_2222_3333_4444;
            x86.xmm[0][4] = 0x5555_6666_7777_8888;
            x86.xmm[16][0] = 0x9999_AAAA_BBBB_CCCC;
            x86.gpr[16] = 0xDDDD_EEEE_FFFF_0001;
            // AVX component 2 remains in its initial configuration.
            x86.xmm[0][2] = 0;
            x86.xmm[0][3] = 0;
        }

        execute_lifted_x86(&[0x48, 0x0F, 0xC7, 0x23], &mut ctx, &mut memory);
        let expected_state = ALL & !(1 << 2);
        assert_eq!(read_u64(&mut memory, 0x100 + 512), expected_state);
        assert_eq!(read_u64(&mut memory, 0x100 + 520), COMPACTED | ALL);
        assert_eq!(
            read_u64(&mut memory, 0x100 + 576),
            0xA5A5_A5A5_A5A5_A5A5,
            "init AVX payload must not be written"
        );
        assert_eq!(
            read_u64(&mut memory, 0x100 + 832),
            0x1111_2222_3333_4444,
            "opmask follows the reserved AVX compacted slot"
        );
        assert_eq!(read_u64(&mut memory, 0x100 + 896), 0x5555_6666_7777_8888);
        assert_eq!(read_u64(&mut memory, 0x100 + 1408), 0x9999_AAAA_BBBB_CCCC);
        assert_eq!(read_u64(&mut memory, 0x100 + 2432), 0xDDDD_EEEE_FFFF_0001);

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.x87 = Default::default();
            x86.mxcsr = 0x1F80;
            x86.xmm = [[0xDEAD; 16]; 32];
            x86.k = [0; 8];
            x86.gpr[16..32].fill(0);
        }
        execute_lifted_x86(&[0x48, 0x0F, 0xAE, 0x2B], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.regs[x86.x87.physical_index(0)], x87_raw);
            assert_eq!(x86.mxcsr, 0x1FC0);
            assert!(
                x86.xmm[..16]
                    .iter()
                    .all(|register| register[0..4].iter().all(|lane| *lane == 0))
            );
            assert_eq!(x86.k[0], 0x1111_2222_3333_4444);
            assert_eq!(x86.xmm[0][4], 0x5555_6666_7777_8888);
            assert_eq!(x86.xmm[16][0], 0x9999_AAAA_BBBB_CCCC);
            assert_eq!(x86.gpr[16], 0xDDDD_EEEE_FFFF_0001);
        }

        // XSAVES is distinct in SMIR but has the same represented RFBM while
        // this CPUID profile advertises no IA32_XSS-managed components.
        ctx.write_vreg(rbx, 0xB00);
        ctx.write_vreg(rax, 1 << 5);
        memory.write(0xB00, &[0xA5; 640]).unwrap();
        memory.write(0xB00 + 512, &[0u8; 64]).unwrap();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[0] = 0xABCD_EF01_2345_6789;
        }
        execute_lifted_x86(&[0x0F, 0xC7, 0x2B], &mut ctx, &mut memory);
        assert_eq!(read_u64(&mut memory, 0xB00 + 512), 1 << 5);
        assert_eq!(read_u64(&mut memory, 0xB00 + 520), COMPACTED | (1 << 5));
        assert_eq!(read_u64(&mut memory, 0xB00 + 576), 0xABCD_EF01_2345_6789);

        // A requested component absent from FORMAT is initialized. Compact
        // AVX-only restoration does not apply the standard-form MXCSR exception.
        ctx.write_vreg(rax, 1 << 2);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1FA0;
            for register in &mut x86.xmm[..16] {
                register[2..4].fill(0xCAFE);
            }
        }
        execute_lifted_x86(&[0x0F, 0xAE, 0x2B], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr, 0x1FA0);
            assert!(
                x86.xmm[..16]
                    .iter()
                    .all(|register| register[2..4].iter().all(|lane| *lane == 0))
            );
        }

        // XRSTORS accepts compacted format and rejects standard format.
        ctx.write_vreg(rax, 1 << 5);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[0] = 0;
        }
        execute_lifted_x86(&[0x0F, 0xC7, 0x1B], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[0], 0xABCD_EF01_2345_6789);
        }
        memory.write(0xB00 + 520, &[0u8; 8]).unwrap();
        let exit = execute_lifted_x86(&[0x0F, 0xC7, 0x1B], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));

        // Compact-format header invariants fault before committing state.
        memory
            .write(0xB00 + 520, &(COMPACTED | (1 << 5)).to_le_bytes())
            .unwrap();
        memory
            .write(0xB00 + 512, &(1u64 << 2).to_le_bytes())
            .unwrap();
        let exit = execute_lifted_x86(&[0x0F, 0xAE, 0x2B], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::GeneralProtection { .. })
        ));
        memory.write(0xB00 + 512, &[0u8; 8]).unwrap();
        memory
            .write(0xB00 + 520, &(COMPACTED | (1 << 8)).to_le_bytes())
            .unwrap();
        let exit = execute_lifted_x86(&[0x0F, 0xAE, 0x2B], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::GeneralProtection { .. })
        ));
        memory
            .write(0xB00 + 520, &(COMPACTED | (1 << 5)).to_le_bytes())
            .unwrap();
        memory.write(0xB00 + 528, &[1]).unwrap();
        let exit = execute_lifted_x86(&[0x0F, 0xAE, 0x2B], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::GeneralProtection { .. })
        ));

        // A k-only compacted image ends at byte 640 and performs no access to
        // the standard-format k offset (1088).
        let mut narrow_ctx = SmirContext::new_x86_64();
        let mut narrow = FlatMemory::new(640);
        narrow_ctx.write_vreg(rbx, 0);
        narrow_ctx.write_vreg(rax, 1 << 5);
        if let ArchRegState::X86_64(x86) = &mut narrow_ctx.arch_regs {
            x86.xcr0 = 1 | (1 << 5);
            x86.k[0] = 0x1234;
        }
        let exit = execute_lifted_x86(&[0x0F, 0xC7, 0x23], &mut narrow_ctx, &mut narrow);
        assert!(!matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { .. })
        ));
        assert_eq!(read_u64(&mut narrow, 576), 0x1234);
    }
    #[test]
    fn lea_scaled_index_address_wraps() {
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
        let rsi = VReg::Arch(ArchReg::X86(X86Reg::Rsi));
        let mut ctx = SmirContext::new_x86_64();
        ctx.write_vreg(rsi, i64::MIN as u64);

        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        interp
            .execute_op(
                &mut ctx,
                &mut memory,
                &SmirOp::new(
                    OpId(0),
                    0x1000,
                    OpKind::Lea {
                        dst: rdx,
                        addr: Address::BaseIndexScale {
                            base: None,
                            index: rsi,
                            scale: 8,
                            disp: 0,
                            disp_size: DispSize::Auto,
                        },
                    },
                ),
            )
            .unwrap();

        assert_eq!(ctx.read_vreg(rdx), (i64::MIN as u64).wrapping_mul(8));
    }
    #[test]
    fn fp_to_int_honors_rounding_mode() {
        let interp = SmirInterpreter::new();
        let src = VReg::Arch(ArchReg::Arm(ArmReg::V(1)));
        let dst = VReg::Arch(ArchReg::Arm(ArmReg::X(0)));
        let mut memory = FlatMemory::new(0x1000);

        let cases: [(FpRoundMode, f64, u64); 7] = [
            (FpRoundMode::RoundNearest, 2.5, 2_u64),
            (FpRoundMode::RoundNearestTiesAway, 2.5, 3),
            (FpRoundMode::RoundNearestTiesAway, -2.5, (-3_i64) as u64),
            (FpRoundMode::RoundUp, 2.1, 3),
            (FpRoundMode::RoundDown, -2.1, (-3_i64) as u64),
            (FpRoundMode::RoundTowardZero, -2.9, (-2_i64) as u64),
            (FpRoundMode::Dynamic, 2.1, 3),
        ];

        for (mode, input, expected) in cases {
            let mut ctx = SmirContext::new_aarch64();
            ctx.write_vreg(src, input.to_bits());
            if mode == FpRoundMode::Dynamic {
                ctx.write_vreg(VReg::Arch(ArchReg::Arm(ArmReg::Fpcr)), 0b01 << 22);
            }

            interp
                .execute_op(
                    &mut ctx,
                    &mut memory,
                    &SmirOp::new(
                        OpId(0),
                        0x1000,
                        OpKind::FpToInt {
                            dst,
                            src,
                            fp_precision: FpPrecision::F64,
                            int_width: OpWidth::W64,
                            signed: true,
                            round: mode,
                        },
                    ),
                )
                .unwrap();

            assert_eq!(ctx.read_vreg(dst), expected, "{mode:?} input {input}");
        }
    }
    #[test]
    fn executes_bf16_converts_dot_masks_rounding_aliases_and_fault_classes() {
        fn vector_u32(values: &[u32], fill: u64) -> VecValue {
            let mut result = [fill; 16];
            for (lane, value) in values.iter().enumerate() {
                SmirInterpreter::set_lane(&mut result, lane as u8, 32, u64::from(*value));
            }
            result
        }
        fn vector_bf16(values: &[u16], fill: u64) -> VecValue {
            let mut result = [fill; 16];
            for (lane, value) in values.iter().enumerate() {
                SmirInterpreter::set_lane(&mut result, lane as u8, 16, u64::from(*value));
            }
            result
        }

        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        let conversion_inputs = [
            0x0000_0001,
            0x8000_0001,
            0x7F80_0000,
            0x7F80_0001,
            0x3F80_0000,
            0x3F80_8000,
            0x3F81_8000,
            0xBF80_0001,
        ];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[4] = [0xDEAD_BEEF_DEAD_BEEF; 16];
            x86.xmm[6] = vector_u32(&conversion_inputs, 0x1111_1111_1111_1111);
        }
        ctx.flags.materialized = MaterializedFlags::from_rflags(0x8D5);
        ctx.flags.lazy = None;
        execute_lifted_x86(&[0xC4, 0xE2, 0x7E, 0x72, 0xE6], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for (lane, expected) in [
                0x0000u16, 0x8000, 0x7F80, 0x7FC0, 0x3F80, 0x3F80, 0x3F82, 0xBF80,
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[4], lane as u8, 16),
                    u64::from(expected)
                );
            }
            assert_eq!(&x86.xmm[4][2..], &[0; 14]);
        }
        ctx.flags.materialize_all();
        assert_eq!(
            ctx.flags.materialized.to_rflags(),
            MaterializedFlags::from_rflags(0x8D5).to_rflags()
        );

        // The one-source EVEX form masks only the converted low half; all
        // higher destination bits are unconditionally zero.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = vector_bf16(&[0xA55A; 8], 0xA55A_A55A_A55A_A55A);
            x86.xmm[2] = vector_u32(&[0x3F80_0000, 0x4000_0000, 0x4040_0000, 0x4080_0000], 0);
            x86.k[1] = 0b0101;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x7E, 0x09, 0x72, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                (0..8)
                    .map(|lane| SmirInterpreter::get_lane(&x86.xmm[1], lane, 16) as u16)
                    .collect::<Vec<_>>(),
                vec![0x3F80, 0xA55A, 0x4040, 0xA55A, 0, 0, 0, 0]
            );
            assert_eq!(&x86.xmm[1][2..], &[0; 14]);
        }

        // Two-source conversion stores src2 in the low half and src1 in the
        // high half, with a mask spanning the complete BF16 result.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = vector_bf16(&[0xA55A; 8], 0xA55A_A55A_A55A_A55A);
            x86.xmm[2] = vector_u32(
                &[
                    10.0f32.to_bits(),
                    20.0f32.to_bits(),
                    30.0f32.to_bits(),
                    40.0f32.to_bits(),
                ],
                0,
            );
            x86.xmm[3] = vector_u32(
                &[
                    1.0f32.to_bits(),
                    2.0f32.to_bits(),
                    3.0f32.to_bits(),
                    4.0f32.to_bits(),
                ],
                0,
            );
            x86.k[1] = 0b1010_0101;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6F, 0x09, 0x72, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let raw = [
                0x3F80u16, 0x4000, 0x4040, 0x4080, 0x4120, 0x41A0, 0x41F0, 0x4220,
            ];
            for (lane, raw) in raw.into_iter().enumerate() {
                let expected = if 0b1010_0101 & (1 << lane) != 0 {
                    raw
                } else {
                    0xA55A
                };
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane as u8, 16),
                    u64::from(expected)
                );
            }
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = vector_u32(&[1.0f32.to_bits(), 0, 0, f32::INFINITY.to_bits()], 0);
            x86.xmm[2] = vector_bf16(
                &[
                    0x4000, 0x4040, 0x0080, 0x0001, 0x7F81, 0x7FC3, 0x0000, 0x0000,
                ],
                0,
            );
            x86.xmm[3] = vector_bf16(
                &[
                    0x4080, 0x40A0, 0x3F00, 0x7F7F, 0x7FC2, 0x7FC4, 0x7F80, 0x0000,
                ],
                0,
            );
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6E, 0x08, 0x52, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], 0, 32),
                24.0f32.to_bits() as u64
            );
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 1, 32), 0);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 2, 32), 0x7FC1_0000);
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 3, 32), 0xFFC0_0000);
            assert_eq!(&x86.xmm[1][2..], &[0; 14]);
        }

        // All three operands may alias; every lane must use the original bits.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = vector_u32(&[1.0f32.to_bits(); 4], 0xFFFF_FFFF_FFFF_FFFF);
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x76, 0x08, 0x52, 0xC9], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..4 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                    2.0f32.to_bits() as u64
                );
            }
        }

        // VCVTNEPS2BF16 and VDPBF16PS are E4 fault-suppressing; the two-source
        // conversion explicitly is not.
        let sentinel = [0x4242_4242_4242_4242; 16];
        ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x400);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 0;
        }
        let no_fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x7E, 0x09, 0x72, 0x08], &mut ctx, &mut memory);
        assert!(matches!(no_fault, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[1][..1], &sentinel[..1]);
            assert_eq!(&x86.xmm[1][1..], &[0; 15]);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 1;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x7E, 0x09, 0x72, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 0;
        }
        let pair_fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x6F, 0x09, 0x72, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            pair_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 0;
        }
        let dot_no_fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x6E, 0x09, 0x52, 0x08], &mut ctx, &mut memory);
        assert!(matches!(dot_no_fault, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[1][..2], &sentinel[..2]);
            assert_eq!(&x86.xmm[1][2..], &[0; 14]);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[1] = 1;
        }
        let dot_fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x6E, 0x09, 0x52, 0x08], &mut ctx, &mut memory);
        assert!(matches!(
            dot_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }
    }
    #[test]
    fn smir_x86_scalar_shifts_apply_the_operand_count_mask() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let initial_flags = 0x2 | 0x8D5;

        for (op, initial) in [
            (
                OpKind::Shl {
                    dst: rax,
                    src: rax,
                    amount: SrcOperand::Imm(32),
                    width: OpWidth::W8,
                    flags: FlagUpdate::All,
                },
                0xA5A5_A5A5_A5A5_A581,
            ),
            (
                OpKind::Shr {
                    dst: rax,
                    src: rax,
                    amount: SrcOperand::Reg(VReg::Arch(ArchReg::X86(X86Reg::Rcx))),
                    width: OpWidth::W16,
                    flags: FlagUpdate::All,
                },
                0xA5A5_A5A5_A5A5_8001,
            ),
            (
                OpKind::Sar {
                    dst: rax,
                    src: rax,
                    amount: SrcOperand::Imm(32),
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
                0x8000_0000,
            ),
        ] {
            let (value, flags) = exec_x86_rax_op(op, initial, 32, initial_flags);
            assert_eq!(
                value, initial,
                "masked count zero must preserve the operand"
            );
            assert_eq!(
                flags & 0x8D5,
                initial_flags & 0x8D5,
                "masked count zero must preserve every status flag"
            );
        }
    }
    /// Pins a few known (input -> Rd, Pe) pairs for the reciprocal / inverse-sqrt
    /// seed + fixup family. The expected values were derived directly from the
    /// reference sem (`src/isa/hexagon/semantics/float_ext.rs`:
    /// `sf_recipa`/`sf_invsqrta`/`sf_recip_common`/`sf_invsqrt_common`), which is
    /// what the full diff harness (`tests/suites/smir/lift/hexagon.rs`) compares against.
    #[test]
    fn smir_hex_fp_recip_eval_matches_sem() {
        use HexFpRecipKind::*;

        // ---- sfrecipa normal seed path (no scalbn adjust, Pe = 0) ----
        // recipa(_, 2.0): idx=0, mant=(0xfe<<15)|1, exp=125 -> 0x3eff0001.
        assert_eq!(
            hex_fp_recip_eval(SfRecipa, 0x3f80_0000, 0x4000_0000),
            (0x3eff_0001, 0x00)
        );
        // recipa(_, 4.0): idx=0, exp=124 -> 0x3e7f0001.
        assert_eq!(
            hex_fp_recip_eval(SfRecipa, 0x3f80_0000, 0x4080_0000),
            (0x3e7f_0001, 0x00)
        );
        // ---- sfrecipa special cases (Pe = 0) ----
        // Rt == 0 (divide-by-zero) -> the common sets RdV = float32_one (the seed
        // result for the special cases; the actual inf/zero lands in RsV/RtV for
        // the fixup ops). So sfrecipa's Rd = 1.0, Pe = 0.
        assert_eq!(
            hex_fp_recip_eval(SfRecipa, 0x4040_0000 /*3.0*/, 0x0000_0000),
            (0x3f80_0000, 0x00)
        );
        // Either NaN -> default all-ones NaN.
        assert_eq!(
            hex_fp_recip_eval(SfRecipa, 0x7fc0_0000, 0x3f80_0000),
            (0xffff_ffff, 0x00)
        );

        // ---- sfinvsqrta normal seed path (Rt ignored, Pe = 0) ----
        // invsqrta(4.0): idx=64, mant=0xfe<<15, exp=125 -> 0x3eff0000.
        assert_eq!(
            hex_fp_recip_eval(SfInvSqrtA, 0x4080_0000, 0),
            (0x3eff_0000, 0x00)
        );
        // invsqrta(1.0): idx=64, exp=126 -> 0x3f7f0000.
        assert_eq!(
            hex_fp_recip_eval(SfInvSqrtA, 0x3f80_0000, 0),
            (0x3f7f_0000, 0x00)
        );
        // ---- sfinvsqrta extreme-exponent path: Rs=2^-110 (raw exp 17 <= 24) ----
        // scalbn(+64) -> 0x28800000; idx=64, exp=149 -> 0x4aff0000, Pe = 0xe0.
        assert_eq!(
            hex_fp_recip_eval(SfInvSqrtA, 0x0880_0000, 0),
            (0x4aff_0000, 0xe0)
        );
        // invsqrta(-1.0): negative non-zero -> default NaN, Pe = 0.
        assert_eq!(
            hex_fp_recip_eval(SfInvSqrtA, 0xbf80_0000, 0),
            (0xffff_ffff, 0x00)
        );

        // ---- fixup ops return the (possibly adjusted) operand, no Pe ----
        // sffixupn/d on a no-adjust normal pair returns the operands unchanged.
        assert_eq!(
            hex_fp_recip_eval(SfFixupN, 0x3f80_0000, 0x4000_0000),
            (0x3f80_0000, 0x00)
        );
        assert_eq!(
            hex_fp_recip_eval(SfFixupD, 0x3f80_0000, 0x4000_0000),
            (0x4000_0000, 0x00)
        );
        // sffixupr on Rs=2^-110 returns the scalbn(+64)-adjusted radicand.
        assert_eq!(
            hex_fp_recip_eval(SfFixupR, 0x0880_0000, 0),
            (0x2880_0000, 0x00)
        );
    }
    #[test]
    fn smir_hex_fp_eval_matches_sem() {
        use HexFpOp::*;
        let f32b = |x: f32| x.to_bits() as u64;
        let f64b = |x: f64| x.to_bits();

        // ---- compares -> predicate byte ----
        assert_eq!(hex_fp_eval(SfCmpEq, f32b(1.0), f32b(1.0)), 0xff);
        assert_eq!(hex_fp_eval(SfCmpEq, f32b(1.0), f32b(2.0)), 0x00);
        assert_eq!(hex_fp_eval(SfCmpGt, f32b(2.0), f32b(1.0)), 0xff);
        assert_eq!(hex_fp_eval(SfCmpGe, f32b(1.0), f32b(1.0)), 0xff);
        // NaN -> unordered: eq/gt/ge false, uo true.
        let snan32 = 0x7f80_0001u64; // signaling NaN
        assert_eq!(hex_fp_eval(SfCmpEq, snan32, f32b(1.0)), 0x00);
        assert_eq!(hex_fp_eval(SfCmpUo, snan32, f32b(1.0)), 0xff);
        assert_eq!(hex_fp_eval(DfCmpGt, f64b(3.0), f64b(2.0)), 0xff);
        assert_eq!(hex_fp_eval(DfCmpUo, f64::NAN.to_bits(), f64b(0.0)), 0xff);

        // ---- classify: mask bit by category (0=zero,1=normal,2=sub,3=inf,4=nan) ----
        assert_eq!(hex_fp_eval(SfClass, f32b(0.0), 1 << 0), 0xff); // zero
        assert_eq!(hex_fp_eval(SfClass, f32b(1.5), 1 << 1), 0xff); // normal
        assert_eq!(
            hex_fp_eval(SfClass, f32::INFINITY.to_bits() as u64, 1 << 3),
            0xff
        );
        assert_eq!(hex_fp_eval(SfClass, snan32, 1 << 4), 0xff); // nan
        assert_eq!(hex_fp_eval(SfClass, f32b(1.5), 1 << 0), 0x00); // normal !zero
        assert_eq!(hex_fp_eval(DfClass, f64b(0.0), 1 << 0), 0xff);

        // ---- min / max with signed-zero tie + NaN ----
        assert_eq!(hex_fp_eval(SfMax, f32b(1.0), f32b(2.0)), f32b(2.0));
        assert_eq!(hex_fp_eval(SfMin, f32b(1.0), f32b(2.0)), f32b(1.0));
        // max(+0,-0) = +0 ; min(+0,-0) = -0
        assert_eq!(hex_fp_eval(SfMax, f32b(0.0), f32b(-0.0)), f32b(0.0));
        assert_eq!(hex_fp_eval(SfMin, f32b(0.0), f32b(-0.0)), f32b(-0.0));
        // one quiet NaN -> the number (no canonicalisation).
        let qnan32 = 0x7fc0_0000u64;
        assert_eq!(hex_fp_eval(SfMax, qnan32, f32b(3.0)), f32b(3.0));
        // both NaN -> default all-ones.
        assert_eq!(hex_fp_eval(SfMax, qnan32, qnan32), 0xFFFF_FFFF);
        assert_eq!(hex_fp_eval(DfMax, f64b(1.0), f64b(2.0)), f64b(2.0));

        // ---- arithmetic, native round + default-NaN ----
        assert_eq!(hex_fp_eval(SfAdd, f32b(1.0), f32b(2.0)), f32b(3.0));
        assert_eq!(hex_fp_eval(SfSub, f32b(5.0), f32b(2.0)), f32b(3.0));
        assert_eq!(hex_fp_eval(SfMpy, f32b(3.0), f32b(4.0)), f32b(12.0));
        assert_eq!(hex_fp_eval(DfAdd, f64b(1.0), f64b(2.0)), f64b(3.0));
        assert_eq!(hex_fp_eval(DfSub, f64b(5.0), f64b(2.0)), f64b(3.0));
        // inf - inf -> default NaN
        assert_eq!(
            hex_fp_eval(
                SfSub,
                f32::INFINITY.to_bits() as u64,
                f32::INFINITY.to_bits() as u64
            ),
            0xFFFF_FFFF
        );

        // ---- conversions ----
        assert_eq!(hex_fp_eval(ConvSf2Df, f32b(2.5), 0), f64b(2.5));
        assert_eq!(hex_fp_eval(ConvDf2Sf, f64b(2.5), 0), f32b(2.5));
        assert_eq!(hex_fp_eval(ConvW2Sf, (-3i32) as u32 as u64, 0), f32b(-3.0));
        assert_eq!(hex_fp_eval(ConvUw2Sf, 3u64, 0), f32b(3.0));
        assert_eq!(hex_fp_eval(ConvW2Df, (-3i32) as u32 as u64, 0), f64b(-3.0));
        // sf -> signed int (round-to-nearest-even): 2.5 -> 2 ; 3.5 -> 4
        assert_eq!(hex_fp_eval(ConvSf2W, f32b(2.5), 0), 2);
        assert_eq!(hex_fp_eval(ConvSf2W, f32b(3.5), 0), 4);
        assert_eq!(hex_fp_eval(ConvSf2WChop, f32b(2.9), 0), 2);
        // NaN -> -1 (signed) ; saturate max (unsigned)
        assert_eq!(hex_fp_eval(ConvSf2W, snan32, 0), 0xFFFF_FFFF);
        assert_eq!(hex_fp_eval(ConvSf2Uw, snan32, 0), 0xFFFF_FFFF);
        // negative -> unsigned saturates to 0
        assert_eq!(hex_fp_eval(ConvSf2Uw, f32b(-1.0), 0), 0);
        // out-of-range signed saturates to i32::MAX
        assert_eq!(hex_fp_eval(ConvSf2W, f32b(1e30), 0), i32::MAX as u32 as u64);
        assert_eq!(hex_fp_eval(ConvDf2D, f64b(123.0), 0), 123);

        // ---- fused multiply-add (single rounding) ----
        // 2*3 + 4 = 10 ; 4 - 2*3 = -2
        assert_eq!(
            hex_sf_fma(f32b(2.0) as u32, f32b(3.0) as u32, f32b(4.0) as u32, false),
            f32b(10.0) as u32
        );
        assert_eq!(
            hex_sf_fma(f32b(2.0) as u32, f32b(3.0) as u32, f32b(4.0) as u32, true),
            f32b(-2.0) as u32
        );
        // NaN accumulator -> canonical all-ones.
        assert_eq!(
            hex_sf_fma(f32b(2.0) as u32, f32b(3.0) as u32, snan32 as u32, false),
            0xFFFF_FFFF
        );
        // 0 * inf -> NaN -> canonical.
        assert_eq!(
            hex_sf_fma(
                f32b(0.0) as u32,
                f32::INFINITY.to_bits(),
                f32b(1.0) as u32,
                false
            ),
            0xFFFF_FFFF
        );
    }
    #[test]
    fn lifted_scalar_vector_movq_executes_aliasing_upper_state_memory_and_faults_exactly() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let flags_before = 0xCD7;
        let mut memory = FlatMemory::new(0x400);
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Legacy load clears bits 127:64 and preserves backing state above bit
        // 127. A same-register source must be captured before that clear.
        let legacy_dst = [0xAAAA_AAAA_AAAA_AAAAu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = legacy_dst;
            x86.xmm[1][0] = 0x0123_4567_89AB_CDEF;
        }
        execute_lifted_x86(&[0xF3, 0x0F, 0x7E, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0x0123_4567_89AB_CDEF);
            assert_eq!(x86.xmm[0][1], 0);
            assert_eq!(&x86.xmm[0][2..], &legacy_dst[2..]);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = legacy_dst;
            x86.xmm[0][0] = 0x8877_6655_4433_2211;
        }
        execute_lifted_x86(&[0xF3, 0x0F, 0x7E, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0x8877_6655_4433_2211);
            assert_eq!(x86.xmm[0][1], 0);
            assert_eq!(&x86.xmm[0][2..], &legacy_dst[2..]);
        }

        // Legacy store-to-register has the same destination upper-state rule.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0][0] = 0xDEAD_BEEF_CAFE_BABE;
            x86.xmm[1] = legacy_dst;
        }
        execute_lifted_x86(&[0x66, 0x0F, 0xD6, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(x86.xmm[1][1], 0);
            assert_eq!(&x86.xmm[1][2..], &legacy_dst[2..]);
        }

        // VEX load and store-to-register clear all state above the low qword.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1][0] = 0x1111_2222_3333_4444;
        }
        execute_lifted_x86(&[0xC5, 0xFA, 0x7E, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0x1111_2222_3333_4444);
            assert!(x86.xmm[0][1..].iter().all(|word| *word == 0));
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0][0] = 0x5555_6666_7777_8888;
            x86.xmm[1] = [u64::MAX; 16];
        }
        execute_lifted_x86(&[0xC5, 0xF9, 0xD6, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x5555_6666_7777_8888);
            assert!(x86.xmm[1][1..].iter().all(|word| *word == 0));
        }

        // EVEX high-register load and compressed disp8*N store use N=8 bytes.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[17] = [u64::MAX; 16];
            x86.xmm[18][0] = 0x9999_AAAA_BBBB_CCCC;
        }
        execute_lifted_x86(&[0x62, 0xA1, 0xFE, 0x08, 0x7E, 0xCA], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[17][0], 0x9999_AAAA_BBBB_CCCC);
            assert!(x86.xmm[17][1..].iter().all(|word| *word == 0));
        }
        memory.write(0x180, &[0xA5; 16]).unwrap();
        ctx.write_vreg(rax, 0x100);
        execute_lifted_x86(
            &[0x62, 0xF1, 0xFD, 0x08, 0xD6, 0x40, 0x10],
            &mut ctx,
            &mut memory,
        );
        let mut stored = [0u8; 16];
        memory.read(0x180, &mut stored).unwrap();
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&stored[..8], &x86.xmm[0][0].to_le_bytes());
        }
        assert_eq!(&stored[8..], &[0xA5; 8]);

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

        // A faulting load must not perform any part of its destination write.
        let fault_sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = fault_sentinel;
        }
        ctx.write_vreg(rax, 0x1000);
        let exit = execute_lifted_x86(&[0xC5, 0xFA, 0x7E, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], fault_sentinel);
        }
    }
    #[test]
    fn lifted_horizontal_integer_family_executes_ordering_wrapping_saturation_and_faults() {
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

        fn reference(
            first: &[u8],
            second: &[u8],
            elem_bytes: usize,
            subtract: bool,
            saturating: bool,
        ) -> Vec<u8> {
            let bits = elem_bytes * 8;
            let mask = (1u64 << bits) - 1;
            let block_lanes = usize::min(16, first.len()) / elem_bytes;
            let lanes = first.len() / elem_bytes;
            let read = |source: &[u8], lane: usize| -> u64 {
                let at = lane * elem_bytes;
                match elem_bytes {
                    2 => u64::from(u16::from_le_bytes(source[at..at + 2].try_into().unwrap())),
                    4 => u64::from(u32::from_le_bytes(source[at..at + 4].try_into().unwrap())),
                    _ => unreachable!(),
                }
            };
            let calculate = |a: u64, b: u64| -> u64 {
                if saturating {
                    let shift = 64 - bits;
                    let lhs = ((a << shift) as i64) >> shift;
                    let rhs = ((b << shift) as i64) >> shift;
                    let value = if subtract { lhs - rhs } else { lhs + rhs };
                    value.clamp(-(1i64 << (bits - 1)), (1i64 << (bits - 1)) - 1) as u64 & mask
                } else if subtract {
                    a.wrapping_sub(b) & mask
                } else {
                    a.wrapping_add(b) & mask
                }
            };
            let mut result = vec![0; first.len()];
            let mut write = |lane: usize, value: u64| {
                let at = lane * elem_bytes;
                result[at..at + elem_bytes].copy_from_slice(&value.to_le_bytes()[..elem_bytes]);
            };
            for block_base in (0..lanes).step_by(block_lanes) {
                let half = block_lanes / 2;
                for pair in 0..half {
                    let lhs = block_base + pair * 2;
                    write(
                        block_base + pair,
                        calculate(read(first, lhs), read(first, lhs + 1)),
                    );
                    write(
                        block_base + half + pair,
                        calculate(read(second, lhs), read(second, lhs + 1)),
                    );
                }
            }
            result
        }

        let words1 = [
            30_000i16,
            10_000,
            i16::MAX,
            1,
            i16::MIN,
            -1,
            200,
            -300,
            12_000,
            -15_000,
            20_000,
            20_000,
            -20_000,
            -20_000,
            1234,
            4321,
        ];
        let words2 = [
            -30_000i16,
            -10_000,
            i16::MIN,
            1,
            i16::MAX,
            -1,
            -200,
            300,
            -12_000,
            15_000,
            25_000,
            25_000,
            -25_000,
            -25_000,
            -1234,
            -4321,
        ];
        let dwords1 = [
            2_000_000_000i32,
            1_000_000_000,
            i32::MAX,
            1,
            i32::MIN,
            -1,
            123_456,
            -654_321,
        ];
        let dwords2 = [
            -2_000_000_000i32,
            -1_000_000_000,
            i32::MIN,
            1,
            i32::MAX,
            -1,
            -123_456,
            654_321,
        ];
        let words1_bytes = words1
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let words2_bytes = words2
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let dwords1_bytes = dwords1
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let dwords2_bytes = dwords2
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for (opcode, first, second, elem_bytes, subtract, saturating) in [
            (
                0x01,
                words1_bytes.as_slice(),
                words2_bytes.as_slice(),
                2,
                false,
                false,
            ),
            (
                0x02,
                dwords1_bytes.as_slice(),
                dwords2_bytes.as_slice(),
                4,
                false,
                false,
            ),
            (
                0x03,
                words1_bytes.as_slice(),
                words2_bytes.as_slice(),
                2,
                false,
                true,
            ),
            (
                0x05,
                words1_bytes.as_slice(),
                words2_bytes.as_slice(),
                2,
                true,
                false,
            ),
            (
                0x06,
                dwords1_bytes.as_slice(),
                dwords2_bytes.as_slice(),
                4,
                true,
                false,
            ),
            (
                0x07,
                words1_bytes.as_slice(),
                words2_bytes.as_slice(),
                2,
                true,
                true,
            ),
        ] {
            let mmx_first = u64::from_le_bytes(first[..8].try_into().unwrap());
            let mmx_second = u64::from_le_bytes(second[..8].try_into().unwrap());
            let mmx_expected = u64::from_le_bytes(
                reference(&first[..8], &second[..8], elem_bytes, subtract, saturating)
                    .try_into()
                    .unwrap(),
            );
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mm[0] = mmx_first;
                x86.mm[1] = mmx_second;
                x86.x87.tag_word = 0xFFFF;
                x86.x87.status_word = 3 << 11;
            }
            execute_lifted_x86(&[0x0F, 0x38, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    x86.mm[0], mmx_expected,
                    "MMX horizontal opcode {opcode:02X}"
                );
                assert_eq!(x86.x87.tag_word, 0);
                assert_eq!(x86.x87.status_word & 0x3800, 3 << 11);
            }

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = seeded(&first[..16], upper);
                x86.xmm[1] = seeded(&second[..16], 0);
            }
            execute_lifted_x86(&[0x66, 0x0F, 0x38, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[0], 16),
                    reference(
                        &first[..16],
                        &second[..16],
                        elem_bytes,
                        subtract,
                        saturating,
                    ),
                    "legacy horizontal opcode {opcode:02X}",
                );
                assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
            }

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [u64::MAX; 16];
                x86.xmm[1] = seeded(first, 0);
                x86.xmm[2] = seeded(second, 0);
            }
            execute_lifted_x86(&[0xC4, 0xE2, 0x75, opcode, 0xC2], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[0], 32),
                    reference(first, second, elem_bytes, subtract, saturating),
                    "VEX horizontal opcode {opcode:02X}",
                );
                assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
            }
        }

        // Destructive same-register legacy operands are read before any result
        // lane is merged back into the destination.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&words1_bytes[..16], upper);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x01, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                reference(&words1_bytes[..16], &words1_bytes[..16], 2, false, false,)
            );
        }

        // The destructive MMX alias reads both source operands before the
        // packed result replaces MM0.
        let mmx_alias = u64::from_le_bytes(words1_bytes[..8].try_into().unwrap());
        let mmx_alias_expected = u64::from_le_bytes(
            reference(&words1_bytes[..8], &words1_bytes[..8], 2, false, false)
                .try_into()
                .unwrap(),
        );
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = mmx_alias;
        }
        execute_lifted_x86(&[0x0F, 0x38, 0x01, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], mmx_alias_expected);
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        memory.write(0x101, &words2_bytes).unwrap();
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x03, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        // The MMX source is an unaligned m64. Its complete load precedes the
        // destructive destination write and the x87-to-MMX state transition.
        memory.write(0x181, &words2_bytes[..8]).unwrap();
        ctx.write_vreg(rax, 0x180);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(words1_bytes[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x38, 0x07, 0x40, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                u64::from_le_bytes(
                    reference(&words1_bytes[..8], &words2_bytes[..8], 2, true, true)
                        .try_into()
                        .unwrap()
                )
            );
            assert_eq!(x86.x87.tag_word, 0);
        }

        ctx.write_vreg(rax, 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0x38, 0x01, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        // Type-4 alignment applies only to legacy SSE; VEX.256 accepts the
        // same unaligned address and consumes the complete 32-byte operand.
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&words1_bytes, 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x01, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                reference(&words1_bytes, &words2_bytes, 2, false, false)
            );
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        ctx.write_vreg(rax, 0x3F0);
        let fault = execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x06, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }
    }
    #[test]
    fn lifted_psign_family_executes_wrapping_control_aliases_and_faults() {
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

        fn reference(value: &[u8], control: &[u8], elem_bytes: usize) -> Vec<u8> {
            let bits = elem_bytes * 8;
            let mask = (1u64 << bits) - 1;
            value
                .chunks_exact(elem_bytes)
                .zip(control.chunks_exact(elem_bytes))
                .flat_map(|(value, control)| {
                    let read = |bytes: &[u8]| -> u64 {
                        match elem_bytes {
                            1 => u64::from(bytes[0]),
                            2 => u64::from(u16::from_le_bytes(bytes.try_into().unwrap())),
                            4 => u64::from(u32::from_le_bytes(bytes.try_into().unwrap())),
                            _ => unreachable!(),
                        }
                    };
                    let value = read(value);
                    let control = read(control);
                    let result = if control == 0 {
                        0
                    } else if control & (1u64 << (bits - 1)) != 0 {
                        0u64.wrapping_sub(value) & mask
                    } else {
                        value
                    };
                    result.to_le_bytes()[..elem_bytes].to_vec()
                })
                .collect()
        }

        let byte_values = [
            0x80u8, 0x7F, 0x01, 0xFF, 0x55, 0xAA, 0x00, 0x40, 0x81, 0x11, 0x22, 0x33, 0x44, 0x66,
            0x77, 0x88, 0x80, 0x7F, 0x01, 0xFF, 0x55, 0xAA, 0x00, 0x40, 0x81, 0x11, 0x22, 0x33,
            0x44, 0x66, 0x77, 0x88,
        ];
        let byte_controls = [
            0xFFu8, 0x80, 0x00, 0x01, 0x7F, 0xFE, 0x00, 0x02, 0x81, 0x00, 0x01, 0xFF, 0x7F, 0x80,
            0x00, 0x01, 0xFF, 0x80, 0x00, 0x01, 0x7F, 0xFE, 0x00, 0x02, 0x81, 0x00, 0x01, 0xFF,
            0x7F, 0x80, 0x00, 0x01,
        ];
        let word_values = [
            i16::MIN,
            i16::MAX,
            1,
            -1,
            0x1234,
            -0x2345,
            0,
            17,
            i16::MIN,
            i16::MAX,
            1,
            -1,
            0x3456,
            -0x4567,
            0,
            29,
        ];
        let word_controls = [
            -1i16,
            i16::MIN,
            0,
            1,
            i16::MAX,
            -2,
            0,
            2,
            -1,
            i16::MIN,
            0,
            1,
            i16::MAX,
            -2,
            0,
            2,
        ];
        let dword_values = [i32::MIN, i32::MAX, 1, -1, 0x1234_5678, -0x2345_678, 0, 37];
        let dword_controls = [-1i32, i32::MIN, 0, 1, i32::MAX, -2, 0, 2];
        let word_value_bytes = word_values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let word_control_bytes = word_controls
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let dword_value_bytes = dword_values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let dword_control_bytes = dword_controls
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let cases = [
            (
                0x08,
                1usize,
                byte_values.as_slice(),
                byte_controls.as_slice(),
            ),
            (
                0x09,
                2,
                word_value_bytes.as_slice(),
                word_control_bytes.as_slice(),
            ),
            (
                0x0A,
                4,
                dword_value_bytes.as_slice(),
                dword_control_bytes.as_slice(),
            ),
        ];

        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        for &(opcode, elem_bytes, value, control) in &cases {
            let value = &value[..8];
            let control = &control[..8];
            let expected =
                u64::from_le_bytes(reference(value, control, elem_bytes).try_into().unwrap());
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mm[0] = u64::from_le_bytes(value.try_into().unwrap());
                x86.mm[1] = u64::from_le_bytes(control.try_into().unwrap());
                x86.x87.tag_word = 0xFFFF;
                x86.x87.status_word = 3 << 11;
            }
            execute_lifted_x86(&[0x0F, 0x38, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.mm[0], expected, "MMX opcode={opcode:02X}");
                assert_eq!(x86.x87.tag_word, 0);
                assert_eq!(x86.x87.status_word & 0x3800, 3 << 11);
            }
        }

        // The m64 control operand is unaligned and must be read completely
        // before either the destructive destination or x87/MMX state changes.
        let mmx_value = &byte_values[..8];
        let mmx_control = &byte_controls[..8];
        let mmx_expected =
            u64::from_le_bytes(reference(mmx_value, mmx_control, 1).try_into().unwrap());
        memory.write(0x81, mmx_control).unwrap();
        ctx.write_vreg(rax, 0x80);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(mmx_value.try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x38, 0x08, 0x40, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], mmx_expected);
            assert_eq!(x86.x87.tag_word, 0);
        }

        ctx.write_vreg(rax, 0x3FC);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
            x86.x87.tag_word = 0xFFFF;
        }
        let mmx_fault = execute_lifted_x86(&[0x0F, 0x38, 0x08, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            mmx_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }

        for (opcode, elem_bytes, value, control) in cases {
            let expected = reference(value, control, elem_bytes);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = seeded(&value[..16], upper);
                x86.xmm[1] = seeded(&control[..16], 0);
            }
            execute_lifted_x86(&[0x66, 0x0F, 0x38, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 16), expected[..16]);
                assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
            }

            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = sentinel;
                x86.xmm[1] = seeded(value, 0);
                x86.xmm[2] = seeded(control, 0);
            }
            execute_lifted_x86(&[0xC4, 0xE2, 0x75, opcode, 0xC2], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 32), expected);
                assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
            }
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&byte_values[..16], 0);
            x86.xmm[2] = seeded(&byte_controls[..16], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x71, 0x08, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                reference(&byte_values[..16], &byte_controls[..16], 1)
            );
            assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        }

        // Wrapping negation leaves each signed minimum unchanged.
        assert_eq!(
            reference(&dword_value_bytes, &dword_control_bytes, 4)[..4],
            i32::MIN.to_le_bytes()
        );

        // Legacy value/control alias: both roles must be captured before the
        // first result lane is merged into the architectural destination.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&byte_values[..16], upper);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x08, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                reference(&byte_values[..16], &byte_values[..16], 1)
            );
        }

        // VEX destination aliases src1, then src2. Both inputs are reduced to
        // temporaries before the final architectural VAndNot write.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&byte_values, 0);
            x86.xmm[2] = seeded(&byte_controls, 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x08, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                reference(&byte_values, &byte_controls, 1)
            );
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&byte_controls, 0);
            x86.xmm[1] = seeded(&byte_values, 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x08, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                reference(&byte_values, &byte_controls, 1)
            );
        }

        memory.write(0x101, &byte_controls).unwrap();
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x08, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&byte_values, 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x08, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                reference(&byte_values, &byte_controls, 1)
            );
        }

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        ctx.write_vreg(rax, 0x3F0);
        let fault = execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x08, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }
    }
    #[test]
    fn lifted_pavgb_pavgw_execute_rounded_unsigned_masks_alignment_and_faults() {
        fn packed_bytes(values: &[u8], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (index, byte) in values.iter().copied().enumerate() {
                let shift = (index % 8) * 8;
                out[index / 8] =
                    (out[index / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
            }
            out
        }

        fn bytes(value: &VecValue, count: usize) -> Vec<u8> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count)
                .collect()
        }

        fn packed_words(values: &[u16], fill: u64) -> VecValue {
            let raw = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            packed_bytes(&raw, fill)
        }

        fn words(value: &VecValue, count: usize) -> Vec<u16> {
            bytes(value, count * 2)
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }

        let a8 = (0..64)
            .map(|lane| (lane * 37 + 0x7F) as u8)
            .collect::<Vec<_>>();
        let b8 = (0..64)
            .map(|lane| 0xFFu8.wrapping_sub((lane * 29) as u8))
            .collect::<Vec<_>>();
        let a16 = (0..32)
            .map(|lane| 0x8001u16.wrapping_add((lane as u16).wrapping_mul(0x1111)))
            .collect::<Vec<_>>();
        let b16 = (0..32)
            .map(|lane| 0xFFF1u16.wrapping_sub((lane as u16).wrapping_mul(0x0101)))
            .collect::<Vec<_>>();
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = packed_bytes(&a8[..16], upper);
            x86.xmm[1] = packed_bytes(&b8[..16], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0xE0, 0xD1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[2], 16),
                a8[..16]
                    .iter()
                    .zip(&b8[..16])
                    .map(|(a, b)| ((u16::from(*a) + u16::from(*b) + 1) >> 1) as u8)
                    .collect::<Vec<_>>(),
            );
            assert!(x86.xmm[2][2..].iter().all(|word| *word == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[4] = packed_words(&a16[..8], upper);
            x86.xmm[3] = packed_words(&b16[..8], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0xE3, 0xE3], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                words(&x86.xmm[4], 8),
                a16[..8]
                    .iter()
                    .zip(&b16[..8])
                    .map(|(a, b)| ((u32::from(*a) + u32::from(*b) + 1) >> 1) as u16)
                    .collect::<Vec<_>>(),
            );
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = [upper; 16];
            x86.xmm[9] = packed_bytes(&a8[..32], 0);
            x86.xmm[10] = packed_bytes(&b8[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0x41, 0x35, 0xE0, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[8], 32),
                a8[..32]
                    .iter()
                    .zip(&b8[..32])
                    .map(|(a, b)| ((u16::from(*a) + u16::from(*b) + 1) >> 1) as u8)
                    .collect::<Vec<_>>(),
            );
            assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = [upper; 16];
            x86.xmm[17] = packed_words(&a16, 0);
            x86.xmm[18] = packed_words(&b16, 0);
            x86.k[1] = 0xA5A5_5A5A;
        }
        execute_lifted_x86(&[0x62, 0xA1, 0x75, 0x41, 0xE3, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let actual = words(&x86.xmm[16], 32);
            for lane in 0..32 {
                assert_eq!(
                    actual[lane],
                    if (0xA5A5_5A5Au64 >> lane) & 1 != 0 {
                        ((u32::from(a16[lane]) + u32::from(b16[lane]) + 1) >> 1) as u16
                    } else {
                        0xA5A5
                    },
                );
            }
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        memory.write(0xF0, &b8[..16]).unwrap();
        ctx.write_vreg(rax, 0xF0);
        ctx.write_vreg(k1, 0xFFFF);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0; 16];
            x86.xmm[1] = packed_bytes(&a8, 0);
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xE0, 0x00], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.write_vreg(k1, 1 << 16);
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        let fault =
            execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xE0, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }
        ctx.write_vreg(rax, 0xF1);
        let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0xE3, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_dot_products_execute_masks_rounding_mxcsr_atomicity_and_faults() {
        fn vector_f32(values: &[u32], fill: u64) -> VecValue {
            let mut bytes = Vec::with_capacity(values.len() * 4);
            for value in values {
                bytes.extend(value.to_le_bytes());
            }
            let mut out = [fill; 16];
            for (word, chunk) in bytes.chunks_exact(8).enumerate() {
                out[word] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            out
        }
        fn vector_f64(values: &[u64], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            for (lane, value) in values.iter().copied().enumerate() {
                out[lane] = value;
            }
            out
        }
        fn f32_lanes(value: &VecValue, count: usize) -> Vec<u32> {
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
        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // DPPS performs the documented pairwise tree and broadcasts only to
        // low-mask-selected lanes. Legacy state above bit 127 is preserved.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector_f32(
                &[
                    1.0f32.to_bits(),
                    2.0f32.to_bits(),
                    3.0f32.to_bits(),
                    4.0f32.to_bits(),
                ],
                upper,
            );
            x86.xmm[10] = vector_f32(
                &[
                    10.0f32.to_bits(),
                    20.0f32.to_bits(),
                    30.0f32.to_bits(),
                    40.0f32.to_bits(),
                ],
                0,
            );
        }
        execute_lifted_x86(
            &[0x66, 0x45, 0x0F, 0x3A, 0x40, 0xCA, 0xF1],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(f32_lanes(&x86.xmm[9], 4), vec![300.0f32.to_bits(), 0, 0, 0]);
            assert!(x86.xmm[9][2..].iter().all(|word| *word == upper));
        }

        // DPPD uses imm[5:4] for input selection and imm[1:0] for output.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector_f64(&[1.5f64.to_bits(), 2.0f64.to_bits()], upper);
            x86.xmm[10] = vector_f64(&[2.0f64.to_bits(), 3.0f64.to_bits()], 0);
        }
        execute_lifted_x86(
            &[0x66, 0x45, 0x0F, 0x3A, 0x41, 0xCA, 0x33],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[9][..2], &[9.0f64.to_bits(), 9.0f64.to_bits()]);
            assert!(x86.xmm[9][2..].iter().all(|word| *word == upper));
        }

        // VDPPS.256 repeats the same primitive independently in each 128-bit
        // half and clears all state above bit 255.
        let first = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0].map(f32::to_bits);
        let second = [2.0f32, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0].map(f32::to_bits);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[10] = vector_f32(&second, 0);
            x86.xmm[11] = vector_f32(&first, 0);
        }
        execute_lifted_x86(&[0xC4, 0x43, 0x25, 0x40, 0xCA, 0xFF], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                f32_lanes(&x86.xmm[9], 8),
                [40.0f32; 4]
                    .into_iter()
                    .chain([200.0f32; 4])
                    .map(f32::to_bits)
                    .collect::<Vec<_>>()
            );
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
        }

        // Each multiply is rounded before horizontal addition. This product is
        // just above an exact representable value: RN selects +2 ULP, RU +3 ULP.
        for (rc, expected) in [(0u32, 0x3F80_0002u32), (2, 0x3F80_0003)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mxcsr = (0x1F80 & !(3 << 13)) | (rc << 13);
                x86.xmm[9] = sentinel;
                x86.xmm[10] = vector_f32(&[0x3F80_0001, 0, 0, 0], 0);
                x86.xmm[11] = vector_f32(&[0x3F80_0001, 0, 0, 0], 0);
            }
            execute_lifted_x86(&[0xC4, 0x43, 0x21, 0x40, 0xCA, 0x11], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(f32_lanes(&x86.xmm[9], 1), vec![expected]);
                assert_ne!(x86.mxcsr & (1 << 5), 0, "inexact multiplication");
                assert!(x86.xmm[9][2..].iter().all(|word| *word == 0));
            }
        }

        // Input selection occurs before arithmetic and suppresses SNaN and
        // denormal-input exceptions for deselected lanes.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F80;
            x86.xmm[9] = vector_f32(&[0x7F80_0001, 1, 0, 0], upper);
            x86.xmm[10] = vector_f32(&[1.0f32.to_bits(), 1.0f32.to_bits(), 0, 0], 0);
        }
        execute_lifted_x86(
            &[0x66, 0x45, 0x0F, 0x3A, 0x40, 0xCA, 0x0F],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(f32_lanes(&x86.xmm[9], 4), vec![0; 4]);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }

        // With invalid masked, a selected SNaN is quieted with its payload and
        // sign preserved. A zero output mask does not suppress computation.
        for (imm, expected) in [(0x11u8, 0x7FC0_0123u32), (0x10, 0)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mxcsr = 0x1F80;
                x86.xmm[9] = sentinel;
                x86.xmm[10] = vector_f32(&[1.0f32.to_bits(), 0, 0, 0], 0);
                x86.xmm[11] = vector_f32(&[0x7F80_0123, 0, 0, 0], 0);
            }
            execute_lifted_x86(&[0xC4, 0x43, 0x21, 0x40, 0xCA, imm], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(f32_lanes(&x86.xmm[9], 1), vec![expected]);
                assert_ne!(x86.mxcsr & 1, 0);
            }
        }

        // DAZ converts selected denormals to signed zero without DE. Without
        // DAZ, exact denormal operands/results survive and DE becomes sticky.
        for (daz, expected, expect_de) in [(false, 1u32, true), (true, 0x0000_0000u32, false)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mxcsr = 0x1F80 | if daz { 1 << 6 } else { 0 };
                x86.xmm[9] = sentinel;
                x86.xmm[10] = vector_f32(&[1.0f32.to_bits(), 0, 0, 0], 0);
                x86.xmm[11] = vector_f32(&[1, 0, 0, 0], 0);
            }
            execute_lifted_x86(&[0xC4, 0x43, 0x21, 0x40, 0xCA, 0x11], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(f32_lanes(&x86.xmm[9], 1), vec![expected]);
                assert_eq!(x86.mxcsr & (1 << 1) != 0, expect_de);
            }
        }

        // An exact tiny product is retained with masked underflow and FTZ=0;
        // FTZ flushes it and sets UE+PE even though the pre-flush result is exact.
        for (ftz, expected, expected_status) in [
            (false, 0x0040_0000u32, 0u32),
            (true, 0u32, (1 << 4) | (1 << 5)),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mxcsr = 0x1F80 | if ftz { 1 << 15 } else { 0 };
                x86.xmm[9] = sentinel;
                x86.xmm[10] = vector_f32(&[0.5f32.to_bits(), 0, 0, 0], 0);
                x86.xmm[11] = vector_f32(&[0x0080_0000, 0, 0, 0], 0);
            }
            execute_lifted_x86(&[0xC4, 0x43, 0x21, 0x40, 0xCA, 0x11], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(f32_lanes(&x86.xmm[9], 1), vec![expected]);
                assert_eq!(x86.mxcsr & ((1 << 4) | (1 << 5)), expected_status);
            }
        }

        // Selected SNaN, overflow, and exact tiny results all trap before any
        // architectural write when their corresponding exception is unmasked.
        for (mxcsr, first_lane, second_lane, expected_status) in [
            (0x1F80 & !(1 << 7), 0x7F80_0001, 1.0f32.to_bits(), 1),
            (0x1F80 & !(1 << 10), 0x7F7F_FFFF, 2.0f32.to_bits(), 1 << 3),
            (0x1F80 & !(1 << 11), 0x0080_0000, 0.5f32.to_bits(), 1 << 4),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mxcsr = mxcsr;
                x86.xmm[9] = sentinel;
                x86.xmm[10] = vector_f32(&[second_lane, 0, 0, 0], 0);
                x86.xmm[11] = vector_f32(&[first_lane, 0, 0, 0], 0);
            }
            let exit =
                execute_lifted_x86(&[0xC4, 0x43, 0x21, 0x40, 0xCA, 0x11], &mut ctx, &mut memory);
            assert!(matches!(
                exit,
                BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
            ));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[9], sentinel);
                assert_ne!(x86.mxcsr & expected_status, 0);
            }
        }

        // VEX memory is unaligned-capable. Legacy alignment and VEX load faults
        // occur before dot-product status or destination writes.
        let memory_operand = [2.0f32, 3.0, 4.0, 5.0]
            .into_iter()
            .flat_map(|value| value.to_bits().to_le_bytes())
            .collect::<Vec<_>>();
        memory.write(0x101, &memory_operand).unwrap();
        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F80;
            x86.xmm[9] = sentinel;
            x86.xmm[11] = vector_f32(&first[..4], 0);
        }
        execute_lifted_x86(
            &[0xC4, 0x63, 0x21, 0x40, 0x48, 0x01, 0xF1],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(f32_lanes(&x86.xmm[9], 1), vec![40.0f32.to_bits()]);
        }

        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
        }
        let misaligned = execute_lifted_x86(
            &[0x66, 0x44, 0x0F, 0x3A, 0x40, 0x08, 0xF1],
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

        ctx.write_vreg(rax, 0x3F8);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.mxcsr = 0x1F80;
        }
        let fault =
            execute_lifted_x86(&[0xC4, 0x63, 0x21, 0x40, 0x08, 0xF1], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[9], sentinel);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_fp_compare_family_executes_all_predicates_masks_mxcsr_sae_and_faults() {
        fn vector_f32(values: &[u32], fill: u64) -> VecValue {
            let mut bytes = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            bytes.resize(bytes.len().next_multiple_of(8), 0);
            let mut out = [fill; 16];
            for (word, chunk) in bytes.chunks_exact(8).enumerate() {
                out[word] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            out
        }
        fn f32_lanes(value: &VecValue, count: usize) -> Vec<u32> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 4)
                .collect::<Vec<_>>()
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }
        fn vector_f16(values: &[u16], fill: u64) -> VecValue {
            let mut bytes = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            bytes.resize(bytes.len().next_multiple_of(8), 0);
            let mut out = [fill; 16];
            for (word, chunk) in bytes.chunks_exact(8).enumerate() {
                out[word] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            out
        }

        const TRUTH_TABLES: [u8; 16] = [
            0b0100, 0b0010, 0b0110, 0b1000, 0b1011, 0b1101, 0b1001, 0b0111, 0b1100, 0b1010, 0b1110,
            0b0000, 0b0011, 0b0101, 0b0001, 0b1111,
        ];
        const SIGNALING: [u8; 16] = [1, 2, 5, 6, 9, 10, 13, 14, 16, 19, 20, 23, 24, 27, 28, 31];
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let qnan = 0x7FC0_1234u32;
        let snan = 0x7F80_1234u32;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        // Lanes encode the four mutually exclusive relations in table order:
        // greater, less, equal, unordered. Both AVX predicate halves share the
        // same truth table; they differ only in QNaN signaling policy.
        for predicate in 0u8..32 {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1] = sentinel;
                x86.xmm[2] = vector_f32(
                    &[2.0f32.to_bits(), 1.0f32.to_bits(), 1.0f32.to_bits(), qnan],
                    0,
                );
                x86.xmm[3] = vector_f32(
                    &[
                        1.0f32.to_bits(),
                        2.0f32.to_bits(),
                        1.0f32.to_bits(),
                        0.0f32.to_bits(),
                    ],
                    0,
                );
                x86.mxcsr = 0x1F80;
            }
            execute_lifted_x86(&[0xC5, 0xE8, 0xC2, 0xCB, predicate], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                let table = TRUTH_TABLES[usize::from(predicate & 15)];
                let expected = (0..4)
                    .map(|relation| {
                        if table & (1 << relation) != 0 {
                            u32::MAX
                        } else {
                            0
                        }
                    })
                    .collect::<Vec<_>>();
                assert_eq!(f32_lanes(&x86.xmm[1], 4), expected, "predicate {predicate}");
                assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
                assert_eq!(
                    x86.mxcsr & 1 != 0,
                    SIGNALING.contains(&predicate),
                    "predicate {predicate} QNaN invalid status"
                );
            }
        }

        // FP16 comparisons share the complete 32-predicate truth table and
        // additionally use FP16 DAZ, denormal, NaN, opmask, and destination
        // width rules. Lanes encode greater, less, equal, unordered,
        // denormal, signed-zero equality, infinity equality, and SNaN.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = vector_f16(
                &[
                    0x4000, 0x3C00, 0x3C00, 0x7E00, 0x0001, 0x8000, 0x7C00, 0x7D00,
                ],
                0,
            );
            x86.xmm[0] = vector_f16(&[0x3C00, 0x4000, 0x3C00, 0, 0, 0, 0x7C00, 0], 0);
            x86.k[2] = 0xFF;
            x86.k[3] = u64::MAX;
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x6C, 0x0A, 0xC2, 0xD8, 0],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[3], 0x64);
            assert_eq!(x86.mxcsr & 3, 3);
        }

        for (daz, expected, denormal_status) in [(false, 0u64, true), (true, 1, false)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[2] = vector_f16(&[1], 0);
                x86.xmm[0] = vector_f16(&[0], 0);
                x86.k[2] = 1;
                x86.k[3] = u64::MAX;
                x86.mxcsr = 0x1F80 | if daz { 1 << 6 } else { 0 };
            }
            execute_lifted_x86(
                &[0x62, 0xF3, 0x6C, 0x0A, 0xC2, 0xD8, 0],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.k[3], expected);
                assert_eq!(x86.mxcsr & (1 << 1) != 0, denormal_status);
            }
        }

        // Packed FP16 broadcast compares the same m16 value against every
        // active source lane and zeros all inactive destination mask bits.
        memory.write(0x100, &0x3C00u16.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = vector_f16(
                &[
                    0x3C00, 0x4000, 0x3C00, 0x4000, 0x3C00, 0x4000, 0x3C00, 0x4000,
                ],
                0,
            );
            x86.k[2] = 0x55;
            x86.k[3] = u64::MAX;
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x6C, 0x1A, 0xC2, 0x18, 0],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[3], 0x55);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }

        // Scalar FP16 SAE suppresses a signaling-predicate QNaN exception.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = vector_f16(&[0x7E00], 0);
            x86.xmm[0] = vector_f16(&[0], 0);
            x86.k[2] = 1;
            x86.k[3] = u64::MAX;
            x86.mxcsr = 0x1F80 & !(1 << 7);
        }
        let fp16_sae = execute_lifted_x86(
            &[0x62, 0xF3, 0x6E, 0x1A, 0xC2, 0xD8, 5],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(fp16_sae, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[3], 1);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }

        // An inactive scalar opmask suppresses the m16 load; an active mask
        // exposes the fault without committing the destination opmask.
        ctx.write_vreg(rax, 0x300);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[2] = 0;
            x86.k[3] = u64::MAX;
        }
        let fp16_suppressed_fault = execute_lifted_x86(
            &[0x62, 0xF3, 0x6E, 0x0A, 0xC2, 0x18, 1],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            fp16_suppressed_fault,
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[3], 0);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[2] = 1;
            x86.k[3] = 0xAA;
        }
        let fp16_fault = execute_lifted_x86(
            &[0x62, 0xF3, 0x6E, 0x0A, 0xC2, 0x18, 1],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            fp16_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[3], 0xAA);
        }

        // Legacy scalar preserves every bit above its result lane. VEX scalar
        // copies lanes 1..3 from vvvv and clears all state above bit 127.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = vector_f32(
                &[
                    1.0f32.to_bits(),
                    11.0f32.to_bits(),
                    12.0f32.to_bits(),
                    13.0f32.to_bits(),
                ],
                sentinel[0],
            );
            x86.xmm[3] = vector_f32(&[1.0f32.to_bits(); 4], 0);
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(&[0xF3, 0x0F, 0xC2, 0xCB, 0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                f32_lanes(&x86.xmm[1], 4),
                [
                    u32::MAX,
                    11.0f32.to_bits(),
                    12.0f32.to_bits(),
                    13.0f32.to_bits()
                ]
            );
            assert!(x86.xmm[1][2..].iter().all(|word| *word == sentinel[0]));
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[2] = vector_f32(
                &[
                    1.0f32.to_bits(),
                    21.0f32.to_bits(),
                    22.0f32.to_bits(),
                    23.0f32.to_bits(),
                ],
                sentinel[0],
            );
            x86.xmm[3] = vector_f32(&[2.0f32.to_bits(); 4], 0);
        }
        execute_lifted_x86(&[0xC5, 0xEA, 0xC2, 0xCB, 4], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                f32_lanes(&x86.xmm[1], 4),
                [
                    u32::MAX,
                    21.0f32.to_bits(),
                    22.0f32.to_bits(),
                    23.0f32.to_bits()
                ]
            );
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        }

        // DAZ converts a denormal operand to signed zero without DE; without
        // DAZ the denormal remains unequal to zero and records DE.
        for (daz, expected, denormal_status) in [(false, 0u32, true), (true, u32::MAX, false)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[2] = vector_f32(&[1, 0, 0, 0], 0);
                x86.xmm[3] = vector_f32(&[0; 4], 0);
                x86.mxcsr = 0x1F80 | if daz { 1 << 6 } else { 0 };
            }
            execute_lifted_x86(&[0xC5, 0xE8, 0xC2, 0xCB, 0], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(f32_lanes(&x86.xmm[1], 1), [expected]);
                assert_eq!(x86.mxcsr & (1 << 1) != 0, denormal_status);
            }
        }

        // Every predicate invalidates SNaN. An unmasked invalid exception sets
        // MXCSR.IE but leaves the architectural destination fully unchanged.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[2] = vector_f32(&[snan, 0, 0, 0], 0);
            x86.xmm[3] = vector_f32(&[0; 4], 0);
            x86.mxcsr = 0x1F80 & !(1 << 7);
        }
        let invalid = execute_lifted_x86(&[0xC5, 0xE8, 0xC2, 0xCB, 0], &mut ctx, &mut memory);
        assert!(matches!(
            invalid,
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
            assert_ne!(x86.mxcsr & 1, 0);
        }

        // Scalar EVEX SAE suppresses MXCSR status and traps while retaining the
        // signaling predicate's unordered truth value in the destination bit.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[18] = vector_f32(&[qnan, 0, 0, 0], 0);
            x86.xmm[19] = vector_f32(&[0; 4], 0);
            x86.k[2] = 1;
            x86.k[3] = u64::MAX;
            x86.mxcsr = 0x1F80 & !(1 << 7);
        }
        let sae = execute_lifted_x86(
            &[0x62, 0xB1, 0x6E, 0x12, 0xC2, 0xDB, 0x05],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(sae, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[3], 1);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }

        // EVEX broadcast compares one memory scalar against all active lanes;
        // the write mask both zeroes inactive results and suppresses their
        // memory accesses and floating-point exceptions.
        memory.write(0x100, &qnan.to_le_bytes()).unwrap();
        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = vector_f32(&[0; 16], 0);
            x86.k[2] = 0x5555;
            x86.k[3] = u64::MAX;
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(
            &[0x62, 0xF1, 0x6C, 0x5A, 0xC2, 0x18, 0x03],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[3], 0x5555);
            assert_eq!(x86.mxcsr & 1, 0);
        }

        ctx.write_vreg(rax, 0x300);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[2] = 0;
            x86.k[3] = u64::MAX;
            x86.mxcsr = 0x1F80;
        }
        let suppressed_fault = execute_lifted_x86(
            &[0x62, 0xF1, 0x6E, 0x0A, 0xC2, 0x18, 0x01],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            suppressed_fault,
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[3], 0);
            assert_eq!(x86.mxcsr, 0x1F80);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[2] = 1;
            x86.k[3] = 0xAA;
        }
        let fault = execute_lifted_x86(
            &[0x62, 0xF1, 0x6E, 0x0A, 0xC2, 0x18, 0x01],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[3], 0xAA);
        }

        // Legacy packed operands use Type 2 alignment rules.
        ctx.write_vreg(rax, 0x101);
        let misaligned = execute_lifted_x86(&[0x0F, 0xC2, 0x08, 0], &mut ctx, &mut memory);
        assert!(matches!(
            misaligned,
            BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
        ));
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    }
    #[test]
    fn lifted_round_scale_executes_grids_mxcsr_masks_sae_and_faults() {
        fn vector_u32(values: &[u32], fill: u64) -> VecValue {
            let mut bytes = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            bytes.resize(bytes.len().next_multiple_of(8), 0);
            let mut result = [fill; 16];
            for (index, chunk) in bytes.chunks_exact(8).enumerate() {
                result[index] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            result
        }
        fn lanes_u32(value: &VecValue, count: usize) -> Vec<u32> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 4)
                .collect::<Vec<_>>()
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }
        fn vector_u16(values: &[u16], fill: u64) -> VecValue {
            let mut bytes = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            bytes.resize(bytes.len().next_multiple_of(8), 0);
            let mut result = [fill; 16];
            for (index, chunk) in bytes.chunks_exact(8).enumerate() {
                result[index] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            result
        }
        fn lanes_u16(value: &VecValue, count: usize) -> Vec<u16> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 2)
                .collect::<Vec<_>>()
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }

        const IE: u32 = 1;
        const UE: u32 = 1 << 4;
        const PE: u32 = 1 << 5;
        const DAZ: u32 = 1 << 6;
        const IM: u32 = 1 << 7;
        const UM: u32 = 1 << 11;
        const PM: u32 = 1 << 12;
        const FTZ: u32 = 1 << 15;

        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);

        // M=0 selects the integer grid. The low two immediate bits select all
        // four IEEE rounding directions when imm[2] is clear.
        let source = [1.5f32, 2.5, -1.5, -2.5].map(f32::to_bits);
        for (imm, expected) in [
            (0x00, [2.0f32, 2.0, -2.0, -2.0]),
            (0x01, [1.0f32, 2.0, -2.0, -3.0]),
            (0x02, [2.0f32, 3.0, -1.0, -2.0]),
            (0x03, [1.0f32, 2.0, -1.0, -2.0]),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1] = sentinel;
                x86.xmm[3] = vector_u32(&source, 0);
                x86.mxcsr = 0x1F80;
            }
            let result = execute_lifted_x86(
                &[0x62, 0xF3, 0x7D, 0x08, 0x08, 0xCB, imm],
                &mut ctx,
                &mut memory,
            );
            assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(lanes_u32(&x86.xmm[1], 4), expected.map(f32::to_bits));
                assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
                assert_ne!(x86.mxcsr & PE, 0);
            }
        }

        // M=1 rounds to a 2^-1 grid. imm[2] delegates the rounding direction
        // to MXCSR.RC and ignores the immediate RC bits.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3] = vector_u32(&[1.25f32, 1.75, -1.25, -1.75].map(f32::to_bits), 0);
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x08, 0x08, 0xCB, 0x10],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                lanes_u32(&x86.xmm[1], 4),
                [1.0f32, 2.0, -1.0, -2.0].map(f32::to_bits)
            );
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3] = vector_u32(&[1.25f32.to_bits(); 4], 0);
            x86.mxcsr = 0x1F80 | (2 << 13); // round toward +infinity
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x08, 0x08, 0xCB, 0x07],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(lanes_u32(&x86.xmm[1], 4), [2.0f32.to_bits(); 4]);
        }

        // imm[3] suppresses only precision. An unmasked precision exception
        // commits MXCSR.PE but leaves the destination atomic; SPE avoids both.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[3] = vector_u32(&[1.25f32.to_bits(); 4], 0);
            x86.mxcsr = 0x1F80 & !PM;
        }
        let precision = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x08, 0x08, 0xCB, 0x00],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            precision,
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
            assert_ne!(x86.mxcsr & PE, 0);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.mxcsr = 0x1F80 & !PM;
        }
        let precision_suppressed = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x08, 0x08, 0xCB, 0x08],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            precision_suppressed,
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(lanes_u32(&x86.xmm[1], 4), [1.0f32.to_bits(); 4]);
            assert_eq!(x86.mxcsr & PE, 0);
        }

        // Zeros and infinities are unchanged. QNaN sign/payload survive while
        // SNaN is quieted and raises IE unless an inactive mask or SAE applies.
        let qnan = 0xFFC0_1234u32;
        let snan = 0xFF80_5678u32;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3] = vector_u32(
                &[
                    0.0f32.to_bits(),
                    (-0.0f32).to_bits(),
                    f32::INFINITY.to_bits(),
                    f32::NEG_INFINITY.to_bits(),
                ],
                0,
            );
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x08, 0x08, 0xCB, 0xF3],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                lanes_u32(&x86.xmm[1], 4),
                [
                    0.0f32.to_bits(),
                    (-0.0f32).to_bits(),
                    f32::INFINITY.to_bits(),
                    f32::NEG_INFINITY.to_bits(),
                ]
            );
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[3] = vector_u32(&[qnan, snan, qnan, snan], 0);
            x86.mxcsr = 0x1F80 & !IM;
        }
        let invalid = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x08, 0x08, 0xCB, 0x00],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            invalid,
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
            assert_ne!(x86.mxcsr & IE, 0);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[3] = vector_u32(&[snan; 16], 0);
            x86.mxcsr = 0x1F80 & !IM;
        }
        let sae = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x18, 0x08, 0xCB, 0x00],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(sae, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(lanes_u32(&x86.xmm[1], 1), [snan | 0x0040_0000]);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }

        // DAZ affects FP32/FP64 only. FP16 ignores DAZ and FTZ; M=15 with RU
        // maps the smallest subnormal to 2^-15, reports UE and optionally PE.
        for (mxcsr, expected_status) in [(0x1F80, PE), (0x1F80 | DAZ, 0)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[3] = vector_u32(&[0x8000_0001; 4], 0);
                x86.mxcsr = mxcsr;
            }
            execute_lifted_x86(
                &[0x62, 0xF3, 0x7D, 0x08, 0x08, 0xCB, 0x00],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(lanes_u32(&x86.xmm[1], 4), [0x8000_0000; 4]);
                assert_eq!(x86.mxcsr & (IE | UE | PE), expected_status);
            }
        }
        for (imm, expected_status) in [(0xF2, UE | PE), (0xFA, UE)] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[3] = vector_u16(&[1; 8], 0);
                x86.mxcsr = 0x1F80 | DAZ | FTZ;
            }
            execute_lifted_x86(
                &[0x62, 0xF3, 0x7C, 0x08, 0x08, 0xCB, imm],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(lanes_u16(&x86.xmm[1], 8), [0x0200; 8]);
                assert_eq!(x86.mxcsr & (UE | PE), expected_status);
            }
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[3] = vector_u16(&[1; 32], 0);
            x86.mxcsr = (0x1F80 | DAZ | FTZ) & !UM;
        }
        let fp16_underflow_sae = execute_lifted_x86(
            &[0x62, 0xF3, 0x7C, 0x18, 0x08, 0xCB, 0xF2],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            fp16_underflow_sae,
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(lanes_u16(&x86.xmm[1], 32), [0x0200; 32]);
            assert_eq!(x86.mxcsr & (UE | PE), 0);
        }

        // F64 uses the same immediate grid without host floating-point state.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3] = [
                1.25f64.to_bits(),
                (-1.75f64).to_bits(),
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
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0xFD, 0x08, 0x09, 0xCB, 0x10],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][..2], [1.0f64.to_bits(), (-2.0f64).to_bits()]);
        }

        // Scalar writemasking applies to the low element only. Inactive merge
        // preserves old dst[31:0], copies upper XMM bits from vvvv, and clears
        // architectural state above bit 127; {z} replaces only the low lane.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = vector_u32(&[7.0f32.to_bits(); 4], sentinel[0]);
            x86.xmm[2] = vector_u32(
                &[
                    99.0f32.to_bits(),
                    11.0f32.to_bits(),
                    12.0f32.to_bits(),
                    13.0f32.to_bits(),
                ],
                sentinel[0],
            );
            x86.xmm[3] = vector_u32(&[snan, 0, 0, 0], 0);
            x86.k[2] = 0;
            x86.mxcsr = 0x1F80 & !IM;
        }
        let masked_snan = execute_lifted_x86(
            &[0x62, 0xF3, 0x6D, 0x0A, 0x0A, 0xCB, 0x00],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(masked_snan, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                lanes_u32(&x86.xmm[1], 4),
                [7.0f32, 11.0, 12.0, 13.0].map(f32::to_bits)
            );
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
            assert_eq!(x86.mxcsr & IE, 0);
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x6D, 0x8A, 0x0A, 0xCB, 0x00],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(lanes_u32(&x86.xmm[1], 1), [0]);
        }

        // Inactive scalar and packed-broadcast masks suppress invalid memory.
        // Any applicable active bit performs exactly one scalar broadcast read.
        ctx.write_vreg(rax, 0x300);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[2] = 0;
            x86.mxcsr = 0x1F80;
        }
        let scalar_suppressed = execute_lifted_x86(
            &[0x62, 0xF3, 0x6D, 0x0A, 0x0A, 0x08, 0x00],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            scalar_suppressed,
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.k[2] = 1;
        }
        let scalar_fault = execute_lifted_x86(
            &[0x62, 0xF3, 0x6D, 0x0A, 0x0A, 0x08, 0x00],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            scalar_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
        }
        let mut broadcast_preserved = sentinel;
        broadcast_preserved[8..].fill(0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.k[2] = 1 << 63;
        }
        let broadcast_suppressed = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x5A, 0x08, 0x00, 0x00],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            broadcast_suppressed,
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], broadcast_preserved);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[2] = 1;
        }
        let broadcast_fault = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x5A, 0x08, 0x00, 0x00],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(
            broadcast_fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], broadcast_preserved);
        }
    }
    #[test]
    fn lifted_round_family_executes_mxcsr_daz_exceptions_merges_and_faults() {
        fn vector_f32(values: &[u32], fill: u64) -> VecValue {
            let mut bytes = Vec::with_capacity(values.len() * 4);
            for value in values {
                bytes.extend(value.to_le_bytes());
            }
            let mut out = [fill; 16];
            for (word, chunk) in bytes.chunks_exact(8).enumerate() {
                out[word] = u64::from_le_bytes(chunk.try_into().unwrap());
            }
            out
        }
        fn f32_lanes(value: &VecValue, count: usize) -> Vec<u32> {
            value
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(count * 4)
                .collect::<Vec<_>>()
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }
        fn vector_f64(values: &[u64], fill: u64) -> VecValue {
            let mut out = [fill; 16];
            out[..values.len()].copy_from_slice(values);
            out
        }
        fn f64_lanes(value: &VecValue, count: usize) -> Vec<u64> {
            value[..count].to_vec()
        }

        let upper = 0xA5A5_A5A5_A5A5_A5A5;
        let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        let flags_before = 0xCD7;
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;

        let inputs = [2.5f32, -2.5, 2.1, -2.1].map(f32::to_bits);
        for (mode, expected) in [
            (0u8, [2.0f32, -2.0, 2.0, -2.0]),
            (1, [2.0f32, -3.0, 2.0, -3.0]),
            (2, [3.0f32, -2.0, 3.0, -2.0]),
            (3, [2.0f32, -2.0, 2.0, -2.0]),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = [upper; 16];
                x86.xmm[10] = vector_f32(&inputs, 0);
                x86.mxcsr = 0x1F80;
            }
            execute_lifted_x86(
                &[0x66, 0x45, 0x0F, 0x3A, 0x08, 0xCA, mode],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    f32_lanes(&x86.xmm[9], 4),
                    expected.map(f32::to_bits),
                    "mode {mode}"
                );
                assert!(x86.xmm[9][2..].iter().all(|word| *word == upper));
                assert_ne!(x86.mxcsr & (1 << 5), 0, "mode {mode}: precision");
            }
        }

        // VEX.256 rounds all eight lanes and clears state above bit 255.
        let packed256 = [2.9f32, -2.1, 3.0, -3.0, 4.7, -4.2, 0.5, -0.5];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[10] = vector_f32(&packed256.map(f32::to_bits), upper);
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(&[0xC4, 0x43, 0x7D, 0x08, 0xCA, 0x01], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                f32_lanes(&x86.xmm[9], 8),
                packed256.map(|value| value.floor().to_bits())
            );
            assert!(x86.xmm[9][4..].iter().all(|word| *word == 0));
        }

        // Double-precision packed and scalar forms use the same control fields.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = [upper; 16];
            x86.xmm[10] = vector_f64(&[2.1f64.to_bits(), (-2.9f64).to_bits()], 0);
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(
            &[0x66, 0x45, 0x0F, 0x3A, 0x09, 0xCA, 0x02],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                f64_lanes(&x86.xmm[9], 2),
                [3.0f64.to_bits(), (-2.0f64).to_bits()]
            );
            assert!(x86.xmm[9][2..].iter().all(|word| *word == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[10] = vector_f64(&[(-2.9f64).to_bits(), 99.0f64.to_bits()], 0);
            x86.xmm[11] = vector_f64(&[88.0f64.to_bits(), 17.0f64.to_bits()], upper);
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(&[0xC4, 0x43, 0x21, 0x0B, 0xCA, 0x03], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                f64_lanes(&x86.xmm[9], 2),
                [(-2.0f64).to_bits(), 17.0f64.to_bits()]
            );
            assert!(x86.xmm[9][2..].iter().all(|word| *word == 0));
        }

        // VEX scalar form obtains untouched lanes from vvvv and clears all
        // state above bit 127; its rounding mode is selected dynamically.
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[10] = vector_f32(&[2.1f32.to_bits(); 4], 0);
            x86.xmm[11] = vector_f32(
                &[
                    99.0f32.to_bits(),
                    11.0f32.to_bits(),
                    12.0f32.to_bits(),
                    13.0f32.to_bits(),
                ],
                upper,
            );
            x86.mxcsr = (0x1F80 & !(3 << 13)) | (2 << 13);
        }
        execute_lifted_x86(&[0xC4, 0x43, 0x21, 0x0A, 0xCA, 0x04], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                f32_lanes(&x86.xmm[9], 4),
                [3.0f32, 11.0, 12.0, 13.0].map(f32::to_bits)
            );
            assert!(x86.xmm[9][2..].iter().all(|word| *word == 0));
        }

        // DAZ changes a positive subnormal rounded toward +infinity from 1.0
        // to +0.0, and the DAZ conversion itself does not signal precision.
        for (daz, expected, precision) in [
            (false, 1.0f32.to_bits(), true),
            (true, 0.0f32.to_bits(), false),
        ] {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = sentinel;
                x86.xmm[10] = vector_f32(&[1, 0, 0, 0], 0);
                x86.mxcsr = 0x1F80 | if daz { 1 << 6 } else { 0 };
            }
            execute_lifted_x86(
                &[0x66, 0x45, 0x0F, 0x3A, 0x0A, 0xCA, 0x02],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(f32_lanes(&x86.xmm[9], 1), [expected]);
                assert_eq!(x86.mxcsr & (1 << 5) != 0, precision);
            }
        }

        // Masked invalid quiets SNaN while preserving its sign/payload. Bit 3
        // suppresses precision only; invalid status is still recorded.
        let snan = 0x7F80_1234u32;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.xmm[10] = vector_f32(&[snan, 0, 0, 0], 0);
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(
            &[0x66, 0x45, 0x0F, 0x3A, 0x0A, 0xCA, 0x08],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(f32_lanes(&x86.xmm[9], 1), [snan | 0x0040_0000]);
            assert_ne!(x86.mxcsr & 1, 0);
            assert_eq!(x86.mxcsr & (1 << 5), 0);
        }

        // Unmasked precision and invalid exceptions update MXCSR status but
        // fault before any architectural vector write.
        for (input, imm, mask_bit, status_bit) in
            [(1.5f32.to_bits(), 0x00, 12u32, 5u32), (snan, 0x08, 7, 0)]
        {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[9] = sentinel;
                x86.xmm[10] = vector_f32(&[input, 0, 0, 0], 0);
                x86.mxcsr = 0x1F80 & !(1 << mask_bit);
            }
            let exit = execute_lifted_x86(
                &[0x66, 0x45, 0x0F, 0x3A, 0x0A, 0xCA, imm],
                &mut ctx,
                &mut memory,
            );
            assert!(matches!(
                exit,
                BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
            ));
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(x86.xmm[9], sentinel);
                assert_ne!(x86.mxcsr & (1 << status_bit), 0);
            }
        }

        // Legacy packed memory requires 16-byte alignment; VEX packed memory
        // is unaligned-capable but still faults atomically on a short operand.
        ctx.write_vreg(rax, 0x101);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = sentinel;
            x86.mxcsr = 0x1F80;
        }
        let misaligned = execute_lifted_x86(
            &[0x66, 0x44, 0x0F, 0x3A, 0x08, 0x08, 0x08],
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
            x86.xmm[9] = sentinel;
            x86.mxcsr = 0x1F80;
        }
        let fault =
            execute_lifted_x86(&[0xC4, 0x63, 0x7D, 0x09, 0x08, 0x08], &mut ctx, &mut memory);
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
    fn x86_exp2_semantics_cover_error_bound_exact_specials_denormals_and_overflow() {
        let one32 = u64::from(1.0f32.to_bits());
        let one64 = 1.0f64.to_bits();
        for (bits, format, expected) in [
            (0u64, X86_SIMD_F32, one32),
            (0x8000_0000, X86_SIMD_F32, one32),
            (1, X86_SIMD_F32, one32),
            (0x8000_0001, X86_SIMD_F32, one32),
            (0, X86_SIMD_F64, one64),
            (0x8000_0000_0000_0000, X86_SIMD_F64, one64),
            (1, X86_SIMD_F64, one64),
            (0x8000_0000_0000_0001, X86_SIMD_F64, one64),
        ] {
            assert_eq!(
                SmirInterpreter::x86_simd_exp2(bits, format),
                X86SimdFpResult {
                    bits: expected,
                    status: 0
                }
            );
        }

        for (input, expected) in [
            (-126.0f32, 0x0080_0000u64),
            (-1.0, u64::from(0.5f32.to_bits())),
            (0.0, one32),
            (1.0, u64::from(2.0f32.to_bits())),
            (127.0, 0x7F00_0000),
        ] {
            let result = SmirInterpreter::x86_simd_exp2(u64::from(input.to_bits()), X86_SIMD_F32);
            assert_eq!(result.bits, expected, "FP32 integral {input}");
            assert_eq!(result.status, 0);
        }
        assert_eq!(
            SmirInterpreter::x86_simd_exp2(u64::from((-127.0f32).to_bits()), X86_SIMD_F32,),
            X86SimdFpResult { bits: 0, status: 0 },
            "exact subnormal result is architecturally flushed",
        );
        assert_eq!(
            SmirInterpreter::x86_simd_exp2(u64::from(128.0f32.to_bits()), X86_SIMD_F32,),
            X86SimdFpResult {
                bits: 0x7F80_0000,
                status: 1 << 3,
            }
        );

        for input in [-100.25f32, -1.5, -0.125, 0.1, 17.75, 127.25] {
            let result = SmirInterpreter::x86_simd_exp2(u64::from(input.to_bits()), X86_SIMD_F32);
            if result.status == 0 && result.bits != 0 {
                let actual = f64::from(f32::from_bits(result.bits as u32));
                let reference = f64::from(input).exp2();
                let relative_error = ((actual - reference) / reference).abs();
                assert!(
                    relative_error < 2.0f64.powi(-23),
                    "FP32 {input}: relative error {relative_error:e}"
                );
            }
        }
        for input in [-1000.25f64, -1.5, -0.125, 0.1, 17.75, 1000.25] {
            let result = SmirInterpreter::x86_simd_exp2(input.to_bits(), X86_SIMD_F64);
            if result.status == 0 && result.bits != 0 {
                let actual = f64::from_bits(result.bits);
                let reference = input.exp2();
                let relative_error = ((actual - reference) / reference).abs();
                assert!(
                    relative_error < 2.0f64.powi(-23),
                    "FP64 {input}: relative error {relative_error:e}"
                );
            }
        }

        for (input, expected) in [
            (f32::INFINITY.to_bits(), 0x7F80_0000u64),
            (f32::NEG_INFINITY.to_bits(), 0),
        ] {
            assert_eq!(
                SmirInterpreter::x86_simd_exp2(u64::from(input), X86_SIMD_F32),
                X86SimdFpResult {
                    bits: expected,
                    status: 0,
                }
            );
        }
        let qnan = SmirInterpreter::x86_simd_exp2(0xFFC1_2345, X86_SIMD_F32);
        assert_eq!(qnan.bits, 0xFFC1_2345);
        assert_eq!(qnan.status, 0);
        let snan = SmirInterpreter::x86_simd_exp2(0xFF81_2345, X86_SIMD_F32);
        assert_eq!(snan.bits, 0xFFC1_2345);
        assert_eq!(snan.status, 1);
    }
    #[test]
    fn x86_exp2_matches_intel_reference_polynomial_and_segment_error_bound() {
        for (input, expected) in [
            (-100.25f32, 0x0D57_44FDu64),
            (-1.5, 0x3EB5_04F3),
            (-0.125, 0x3F6A_C0C7),
            (0.1, 0x3F89_2FDF),
            (17.75, 0x4857_44FD),
            (127.25, 0x7F18_37F0),
        ] {
            assert_eq!(
                SmirInterpreter::x86_simd_exp2(u64::from(input.to_bits()), X86_SIMD_F32),
                X86SimdFpResult {
                    bits: expected,
                    status: 0,
                },
                "Intel EXP2S reference vector {input}",
            );
        }
        for (input, expected) in [
            (-1000.25f64, 0x016A_E89F_A000_0000u64),
            (-1.5, 0x3FD6_A09E_6000_0000),
            (-0.125, 0x3FED_5818_E000_0000),
            (0.1, 0x3FF1_25FB_E000_0000),
            (17.75, 0x410A_E89F_A000_0000),
            (1000.25, 0x7E73_06FE_0000_0000),
        ] {
            assert_eq!(
                SmirInterpreter::x86_simd_exp2(input.to_bits(), X86_SIMD_F64),
                X86SimdFpResult {
                    bits: expected,
                    status: 0,
                },
                "Intel EXP2D reference vector {input}",
            );
        }

        // Exercise both sides and the interior of every one of the 64 table
        // segments at representative output scales, including the finite
        // boundaries. The exact ISA requirement is relative error < 2^-23.
        let limit = 2.0f64.powi(-23);
        for scale in [-126, -100, -1, 0, 17, 126] {
            for segment in 0..64 {
                for offset in [1, 0x1F_FFFF, 0x20_0000, 0x3F_FFFF] {
                    let fraction = ((segment << 22) + offset) as f64 / 268_435_456.0;
                    let input = scale as f64 + fraction;
                    let input32 = input as f32;
                    let result =
                        SmirInterpreter::x86_simd_exp2(u64::from(input32.to_bits()), X86_SIMD_F32);
                    let actual = f64::from(f32::from_bits(result.bits as u32));
                    let reference = f64::from(input32).exp2();
                    let relative_error = ((actual - reference) / reference).abs();
                    assert!(
                        relative_error < limit,
                        "EXP2S {input32}: relative error {relative_error:e}",
                    );
                }
            }
        }
        for scale in [-1022, -1000, -1, 0, 17, 1000, 1022] {
            for segment in 0..64 {
                for offset in [1, 0x1F_FFFF, 0x20_0000, 0x3F_FFFF] {
                    let fraction = ((segment << 22) + offset) as f64 / 268_435_456.0;
                    let input = scale as f64 + fraction;
                    let result = SmirInterpreter::x86_simd_exp2(input.to_bits(), X86_SIMD_F64);
                    let actual = f64::from_bits(result.bits);
                    let reference = input.exp2();
                    let relative_error = ((actual - reference) / reference).abs();
                    assert!(
                        relative_error < limit,
                        "EXP2D {input}: relative error {relative_error:e}",
                    );
                }
            }
        }
    }
    #[test]
    fn x86_recip14_matches_intel_reference_all_segments_mxcsr_and_special_values() {
        // FNV-1a accumulation over outputs generated by Intel's RECIP14.c
        // RCP14S/RCP14D implementation. The corpus covers every polynomial
        // segment, both signs, four exponent scales, and four segment offsets.
        const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
        let mut hash32 = FNV_OFFSET;
        let mut count32 = 0usize;
        for sign in [0u32, 1] {
            for exponent in [1u32, 127, 253, 254] {
                for segment in 0u32..64 {
                    for tail in [0u32, 1, 0xFFFF, 0x1_FFFF] {
                        let bits = (sign << 31) | (exponent << 23) | (segment << 17) | tail;
                        let result =
                            SmirInterpreter::x86_simd_recip14(u64::from(bits), X86_SIMD_F32, 0);
                        hash32 = (hash32 ^ result.bits).wrapping_mul(FNV_PRIME);
                        assert_eq!(result.status, 0);
                        count32 += 1;
                    }
                }
            }
        }
        assert_eq!(count32, 2_048);
        assert_eq!(hash32, 0x3458_3FF8_E41E_DD25);

        let mut hash64 = FNV_OFFSET;
        let mut count64 = 0usize;
        for sign in [0u64, 1] {
            for exponent in [1u64, 1023, 2045, 2046] {
                for segment in 0u64..64 {
                    for tail in [0u64, 1, (1 << 45) - 1, (1 << 46) - 1] {
                        let bits = (sign << 63) | (exponent << 52) | (segment << 46) | tail;
                        let result = SmirInterpreter::x86_simd_recip14(bits, X86_SIMD_F64, 0);
                        hash64 = (hash64 ^ result.bits).wrapping_mul(FNV_PRIME);
                        assert_eq!(result.status, 0);
                        count64 += 1;
                    }
                }
            }
        }
        assert_eq!(count64, 2_048);
        assert_eq!(hash64, 0xD3E9_7608_DF2E_C325);

        for (bits, format, mxcsr, expected) in [
            (0, X86_SIMD_F32, 0, 0x7F80_0000),
            (0x8000_0000, X86_SIMD_F32, 0, 0xFF80_0000),
            (0x7F80_0000, X86_SIMD_F32, 0, 0),
            (0xFF80_0000, X86_SIMD_F32, 0, 0x8000_0000),
            (0x7FC1_2345, X86_SIMD_F32, 0, 0x7FC1_2345),
            (0x7F81_2345, X86_SIMD_F32, 0, 0x7FC1_2345),
            (0x0020_0000, X86_SIMD_F32, 0, 0x7F80_0000),
            (0x0020_0001, X86_SIMD_F32, 0, 0x7F7F_FE00),
            (0x0020_0001, X86_SIMD_F32, 1 << 6, 0x7F80_0000),
            (0x0040_0000, X86_SIMD_F32, 0, 0x7F00_0000),
            (0x7E80_0001, X86_SIMD_F32, 0, 0x007F_FF00),
            (0x7E80_0001, X86_SIMD_F32, 1 << 15, 0),
            (0x7F00_0001, X86_SIMD_F32, 0, 0x003F_FF80),
            (0, X86_SIMD_F64, 0, 0x7FF0_0000_0000_0000),
            (
                0x8000_0000_0000_0000,
                X86_SIMD_F64,
                0,
                0xFFF0_0000_0000_0000,
            ),
            (0x7FF0_0000_0000_0000, X86_SIMD_F64, 0, 0),
            (
                0xFFF0_0000_0000_0000,
                X86_SIMD_F64,
                0,
                0x8000_0000_0000_0000,
            ),
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
                0x0004_0000_0000_0000,
                X86_SIMD_F64,
                0,
                0x7FF0_0000_0000_0000,
            ),
            (
                0x0004_0000_0000_0001,
                X86_SIMD_F64,
                0,
                0x7FEF_FFC0_0000_0000,
            ),
            (
                0x0004_0000_0000_0001,
                X86_SIMD_F64,
                1 << 6,
                0x7FF0_0000_0000_0000,
            ),
            (
                0x7FD0_0000_0000_0001,
                X86_SIMD_F64,
                0,
                0x000F_FFE0_0000_0000,
            ),
            (0x7FD0_0000_0000_0001, X86_SIMD_F64, 1 << 15, 0),
        ] {
            assert_eq!(
                SmirInterpreter::x86_simd_recip14(bits, format, mxcsr),
                X86SimdFpResult {
                    bits: expected,
                    status: 0,
                }
            );
        }

        let limit = 2.0f64.powi(-14);
        for exponent in [1u64, 256, 1023, 1792, 2046] {
            for segment in 0u64..64 {
                for tail in [1u64, (1 << 45) - 1, (1 << 46) - 1] {
                    let bits = (exponent << 52) | (segment << 46) | tail;
                    let input = f64::from_bits(bits);
                    let actual = f64::from_bits(
                        SmirInterpreter::x86_simd_recip14(bits, X86_SIMD_F64, 0).bits,
                    );
                    let reference = input.recip();
                    let relative_error = ((actual - reference) / reference).abs();
                    assert!(
                        relative_error < limit,
                        "VRCP14D {input:e}: relative error {relative_error:e}"
                    );
                }
            }
        }
    }
    #[test]
    fn x86_recip28_matches_intel_reference_all_segments_and_special_values() {
        // FNV-1a-style accumulation over outputs and status flags generated by
        // Intel's RECIP28EXP2.c RCP28S/RCP28D implementation. The corpus
        // exercises all 256 polynomial segments, both signs, four in-segment
        // positions, and minimum/central/maximum normal exponent fields.
        const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
        let mut hash32 = FNV_OFFSET;
        let mut count32 = 0usize;
        for sign in [0u32, 1] {
            for exponent in [1u32, 127, 254] {
                for segment in 0u32..256 {
                    for tail in [0u32, 1, 0x3FFF, 0x7FFF] {
                        let fraction = (segment << 15) | tail;
                        if fraction == 0 {
                            continue;
                        }
                        let bits = (sign << 31) | (exponent << 23) | fraction;
                        let result =
                            SmirInterpreter::x86_simd_recip28(u64::from(bits), X86_SIMD_F32);
                        hash32 = (hash32 ^ result.bits).wrapping_mul(FNV_PRIME);
                        hash32 = (hash32 ^ u64::from(result.status)).wrapping_mul(FNV_PRIME);
                        count32 += 1;
                    }
                }
            }
        }
        assert_eq!(count32, 6_138);
        assert_eq!(hash32, 0x033E_3A71_C458_F825);

        let mut hash64 = FNV_OFFSET;
        let mut count64 = 0usize;
        for sign in [0u64, 1] {
            for exponent in [1u64, 1023, 2046] {
                for segment in 0u64..256 {
                    for tail in [0u64, 1, 0x1F_FFFF, 0x3F_FFFF] {
                        let fraction = (segment << 44) | (tail << 22) | (tail & 0x3F_FFFF);
                        if fraction == 0 {
                            continue;
                        }
                        let bits = (sign << 63) | (exponent << 52) | fraction;
                        let result = SmirInterpreter::x86_simd_recip28(bits, X86_SIMD_F64);
                        hash64 = (hash64 ^ result.bits).wrapping_mul(FNV_PRIME);
                        hash64 = (hash64 ^ u64::from(result.status)).wrapping_mul(FNV_PRIME);
                        count64 += 1;
                    }
                }
            }
        }
        assert_eq!(count64, 6_138);
        assert_eq!(hash64, 0xC358_27E9_E21A_2AD5);

        for (bits, format, expected, status) in [
            (0, X86_SIMD_F32, 0x7F80_0000, 1 << 2),
            (0x8000_0000, X86_SIMD_F32, 0xFF80_0000, 1 << 2),
            (1, X86_SIMD_F32, 0x7F80_0000, 1 << 2),
            (0x8000_0001, X86_SIMD_F32, 0xFF80_0000, 1 << 2),
            (0x7F80_0000, X86_SIMD_F32, 0, 0),
            (0xFF80_0000, X86_SIMD_F32, 0x8000_0000, 0),
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
                0x8000_0000_0000_0000,
                0,
            ),
        ] {
            assert_eq!(
                SmirInterpreter::x86_simd_recip28(bits, format),
                X86SimdFpResult {
                    bits: expected,
                    status,
                }
            );
        }

        for (input, expected) in [(0.5f64, 2.0f64), (1.0, 1.0), (2.0, 0.5), (-4.0, -0.25)] {
            assert_eq!(
                SmirInterpreter::x86_simd_recip28(input.to_bits(), X86_SIMD_F64),
                X86SimdFpResult {
                    bits: expected.to_bits(),
                    status: 0,
                }
            );
        }

        let qnan = SmirInterpreter::x86_simd_recip28(0xFFC1_2345, X86_SIMD_F32);
        assert_eq!(
            qnan,
            X86SimdFpResult {
                bits: 0xFFC1_2345,
                status: 0
            }
        );
        let snan = SmirInterpreter::x86_simd_recip28(0xFF81_2345, X86_SIMD_F32);
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
                    let result = SmirInterpreter::x86_simd_recip28(bits, X86_SIMD_F64);
                    if result.bits == 0 || result.bits == 0x7FF0_0000_0000_0000 {
                        continue;
                    }
                    let actual = f64::from_bits(result.bits);
                    let reference = input.recip();
                    let relative_error = ((actual - reference) / reference).abs();
                    assert!(
                        relative_error < limit,
                        "VRCP28D {input:e}: relative error {relative_error:e}"
                    );
                }
            }
        }
    }
    #[test]
    fn lifted_x86_recip14_preserves_widths_scalar_merge_masks_mxcsr_and_fault_suppression() {
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
            x86.xmm[3][0] = u64::from(3.0f32.to_bits());
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0x4D, 0xCB], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_3EAA_AA80);
            assert_eq!(x86.xmm[1][1], 0x0FED_CBA9_8765_4321);
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 0xAAAA_AAAA_DEAD_BEEF;
            x86.k[1] = 0;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0x4D, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_DEAD_BEEF);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 0xAAAA_AAAA_DEAD_BEEF;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x89, 0x4D, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_0000_0000);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3][0] = 0x0020_0001;
            x86.mxcsr = 0;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0x4D, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] as u32, 0x7F7F_FE00);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 1 << 6;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0x4D, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] as u32, 0x7F80_0000);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3][0] = 0x7E80_0001;
            x86.mxcsr = 0;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0x4D, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] as u32, 0x007F_FF00);
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 1 << 15;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0x4D, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] as u32, 0);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0xDEAD_BEEF_CAFE_BABE; 16];
            x86.xmm[3][0] = (u64::from(4.0f32.to_bits()) << 32) | u64::from(2.0f32.to_bits());
            x86.xmm[3][1] = (u64::from(16.0f32.to_bits()) << 32) | u64::from(8.0f32.to_bits());
            x86.mxcsr = 0;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x08, 0x4C, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x3E80_0000_3F00_0000);
            assert_eq!(x86.xmm[1][1], 0x3D80_0000_3E00_0000);
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k1, 0);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0x4D, 0x08], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.write_vreg(k1, 1);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0x4D, 0x08], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
    }
    #[test]
    fn lifted_x86_recip28_preserves_scalar_merge_masks_sae_and_fault_atomicity() {
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
            x86.xmm[3][0] = u64::from(2.0f32.to_bits());
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0xCB, 0xCB], &mut ctx, &mut memory,),
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
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0xCB, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_DEAD_BEEF);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1][0] = 0xAAAA_AAAA_DEAD_BEEF;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x89, 0xCB, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_0000_0000);
        }

        let sentinel = [0xDEAD_BEEF_CAFE_BABEu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[3][0] = 0x7F80_1234;
            x86.k[1] = 1;
            x86.mxcsr = 0;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0xCB, 0xCB], &mut ctx, &mut memory,),
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
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x19, 0xCB, 0xCB], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0x0123_4567_7FC0_1234);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k1, 0);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0xCB, 0x08], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.write_vreg(k1, 1);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0xCB, 0x08], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
    }
    #[test]
    fn lifted_x86_exp2_preserves_masks_fault_suppression_sae_and_exception_atomicity() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0xAAAA_AAAA_DEAD_BEEF; 16];
            x86.xmm[3][0] = (u64::from(2.0f32.to_bits()) << 32) | u64::from(1.0f32.to_bits());
            x86.k[1] = 1;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x49, 0xC8, 0xCB], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0xAAAA_AAAA_4000_0000);
            assert!(
                x86.xmm[1][1..8]
                    .iter()
                    .all(|word| *word == 0xAAAA_AAAA_DEAD_BEEF)
            );
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [u64::MAX; 16];
            x86.k[1] = 0;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0xC9, 0xC8, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert!(x86.xmm[1][..8].iter().all(|word| *word == 0));
        }

        let sentinel = [0xDEAD_BEEF_CAFE_BABEu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[3][0] = 0x7F80_1234;
            x86.k[1] = 1;
            x86.mxcsr = 0;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x49, 0xC8, 0xCB], &mut ctx, &mut memory,),
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
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x19, 0xC8, 0xCB], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] & 0xFFFF_FFFF, 0x7FC0_1234);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k1, 0);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x49, 0xC8, 0x08], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.write_vreg(k1, 1);
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x49, 0xC8, 0x08], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
    }
    #[test]
    fn lifted_x86_range_preserves_scalar_merge_fault_suppression_and_exception_atomicity() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0xCCCC_CCCC_4120_0000; 16];
            x86.xmm[2] = [
                0xA5A5_A5A5_C000_0000,
                0x0123_4567_89AB_CDEF,
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
            x86.xmm[3][0] = u64::from(3.0f32.to_bits());
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xF3, 0x6D, 0x08, 0x51, 0xCB, 0x05],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0xA5A5_A5A5_4040_0000);
            assert_eq!(x86.xmm[1][1], 0x0123_4567_89AB_CDEF);
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[1] = 0;
            x86.xmm[1][0] = 0xCCCC_CCCC_4120_0000;
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x6D, 0x09, 0x51, 0xCB, 0x05],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0xA5A5_A5A5_4120_0000);
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x6D, 0x89, 0x51, 0xCB, 0x05],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0xA5A5_A5A5_0000_0000);
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k1, 0);
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xF3, 0x6D, 0x09, 0x51, 0x08, 0x05],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.write_vreg(k1, 1);
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xF3, 0x6D, 0x09, 0x51, 0x08, 0x05],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));

        let sentinel = [0xDEAD_BEEF_CAFE_BABEu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F00;
            x86.xmm[1] = sentinel;
            x86.xmm[2][0] = 0x7F80_1234;
            x86.xmm[3][0] = u64::from(1.0f32.to_bits());
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xF3, 0x6D, 0x08, 0x51, 0xCB, 0x0C],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
            assert_ne!(x86.mxcsr & 1, 0);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F00;
            x86.xmm[1] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(
                &[0x62, 0xF3, 0x6D, 0x18, 0x51, 0xCB, 0x0C],
                &mut ctx,
                &mut memory,
            ),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] & 0xFFFF_FFFF, 0x7FC0_1234);
            assert_eq!(x86.mxcsr, 0x1F00);
        }
    }
    #[test]
    fn x86_scale_f_exact_semantics_cover_floor_specials_daz_ftz_and_rounding() {
        let run32 = |first: f32, second: f32, mxcsr: u32| {
            SmirInterpreter::x86_simd_scale_f(
                u64::from(first.to_bits()),
                u64::from(second.to_bits()),
                X86_SIMD_F32,
                FpRoundMode::RoundNearest,
                mxcsr,
                false,
            )
        };
        assert_eq!(run32(1.5, 2.75, 0x1F80).bits, u64::from(6.0f32.to_bits()));
        assert_eq!(
            run32(1.5, -1.25, 0x1F80).bits,
            u64::from(0.375f32.to_bits())
        );

        let first_qnan = 0x7FC1_2345u64;
        let second_snan = 0x7F81_5678u64;
        let nan = SmirInterpreter::x86_simd_scale_f(
            first_qnan,
            second_snan,
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1F80,
            false,
        );
        assert_eq!(nan.bits, first_qnan);
        assert_eq!(nan.status, 1);

        let denormal_then_nan = SmirInterpreter::x86_simd_scale_f(
            1,
            second_snan,
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1F80,
            false,
        );
        assert_eq!(denormal_then_nan.status, 1, "src2 NaN suppresses src1 DE");

        for (first, second, expected, status) in [
            (f32::INFINITY, f32::NEG_INFINITY, 0xFFC0_0000u64, 1u32),
            (0.0, f32::INFINITY, 0xFFC0_0000, 1),
            (
                -1.0,
                f32::INFINITY,
                u64::from(f32::NEG_INFINITY.to_bits()),
                0,
            ),
            (-1.0, f32::NEG_INFINITY, u64::from((-0.0f32).to_bits()), 0),
        ] {
            let result = run32(first, second, 0x1F80);
            assert_eq!(result.bits, expected);
            assert_eq!(result.status, status);
        }

        let denormal = SmirInterpreter::x86_simd_scale_f(
            1,
            u64::from(0.0f32.to_bits()),
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1F80,
            false,
        );
        assert_eq!(denormal.bits, 1);
        assert_eq!(denormal.status, 1 << 1);
        let daz = SmirInterpreter::x86_simd_scale_f(
            1,
            u64::from(0.0f32.to_bits()),
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1FC0,
            false,
        );
        assert_eq!(daz.bits, 0);
        assert_eq!(daz.status, 0);

        let negative_denormal_scale = 0x8000_0001u64;
        let no_daz = SmirInterpreter::x86_simd_scale_f(
            u64::from(1.0f32.to_bits()),
            negative_denormal_scale,
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1F80,
            false,
        );
        assert_eq!(no_daz.bits, u64::from(0.5f32.to_bits()));
        assert_eq!(no_daz.status, 0, "src2 denormal never raises DE");
        let with_daz = SmirInterpreter::x86_simd_scale_f(
            u64::from(1.0f32.to_bits()),
            negative_denormal_scale,
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1FC0,
            false,
        );
        assert_eq!(with_daz.bits, u64::from(1.0f32.to_bits()));

        let fp16_gradual = SmirInterpreter::x86_simd_scale_f(
            1,
            0,
            X86_SIMD_F16,
            FpRoundMode::RoundNearest,
            0x9FC0,
            false,
        );
        assert_eq!(fp16_gradual.bits, 1, "packed FP16 ignores DAZ and FTZ");
        assert_eq!(fp16_gradual.status, 1 << 1);
        let fp16_scalar_ftz = SmirInterpreter::x86_simd_scale_f(
            1,
            0,
            X86_SIMD_F16,
            FpRoundMode::RoundNearest,
            0x9FC0,
            true,
        );
        assert_eq!(fp16_scalar_ftz.bits, 0, "scalar FP16 honors FTZ");
        assert_eq!(fp16_scalar_ftz.status, (1 << 1) | (1 << 4) | (1 << 5));

        let max_f32 = 0x7F7F_FFFFu64;
        let overflow_nearest = SmirInterpreter::x86_simd_scale_f(
            max_f32,
            u64::from(1.0f32.to_bits()),
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1F80,
            false,
        );
        assert_eq!(overflow_nearest.bits, u64::from(f32::INFINITY.to_bits()));
        assert_eq!(overflow_nearest.status, (1 << 3) | (1 << 5));
        let overflow_zero = SmirInterpreter::x86_simd_scale_f(
            max_f32,
            u64::from(1.0f32.to_bits()),
            X86_SIMD_F32,
            FpRoundMode::RoundTowardZero,
            0x1F80,
            false,
        );
        assert_eq!(overflow_zero.bits, max_f32);
        assert_eq!(overflow_zero.status, (1 << 3) | (1 << 5));
    }
    #[test]
    fn lifted_x86_scale_f_preserves_scalar_merge_and_exception_atomicity() {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0xCCCC_CCCC_4120_0000; 16];
            x86.xmm[2] = [
                0xA5A5_A5A5_3FC0_0000,
                0x0123_4567_89AB_CDEF,
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
            x86.xmm[3][0] = u64::from(2.75f32.to_bits());
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0x2D, 0xCB], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0xA5A5_A5A5_40C0_0000);
            assert_eq!(x86.xmm[1][1], 0x0123_4567_89AB_CDEF);
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.k[1] = 0;
            x86.xmm[1][0] = 0xCCCC_CCCC_4120_0000;
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0x2D, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0xA5A5_A5A5_4120_0000);
        }
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x89, 0x2D, 0xCB], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0], 0xA5A5_A5A5_0000_0000);
        }

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        ctx.write_vreg(rax, 0x100);
        ctx.write_vreg(k1, 0);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0x2D, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let masked_memory_result = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.xmm[1],
            _ => unreachable!(),
        };
        ctx.write_vreg(k1, 1);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x09, 0x2D, 0x08], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], [0xCCCC_CCCC_CCCC_CCCCu64; 16]);
            assert_ne!(masked_memory_result, x86.xmm[1]);
        }

        let sentinel = [0xDEAD_BEEF_CAFE_BABEu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F00;
            x86.xmm[1] = sentinel;
            x86.xmm[2][0] = u64::from(f32::INFINITY.to_bits());
            x86.xmm[3][0] = u64::from(f32::NEG_INFINITY.to_bits());
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x08, 0x2D, 0xCB], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1], sentinel);
            assert_ne!(x86.mxcsr & 1, 0);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F00;
            x86.xmm[1] = sentinel;
        }
        assert!(matches!(
            execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x18, 0x2D, 0xCB], &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] & 0xFFFF_FFFF, 0xFFC0_0000);
            assert_eq!(x86.mxcsr, 0x1F00);
        }
    }
