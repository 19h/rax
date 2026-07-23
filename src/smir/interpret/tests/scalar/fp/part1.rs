//! fp part 1 tests

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

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
            SmirInterpreter::set_lane(&mut x86.xmm[2], lane as u8, 32, u64::from(value.to_bits()));
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
    let f64_max = SmirInterpreter::x86_x87_to_ieee(&raw(0xFFFF_FFFF_FFFF_F800, 0x43FE), 11, 52, 0);
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
        x86.xcr0 = 0x0008_02E7;
        x86.xgetbv1 = 0x0000_0025;
    }
    ctx.write_vreg(rcx, 0);
    ctx.write_vreg(rax, u64::MAX);
    ctx.write_vreg(rdx, u64::MAX);
    execute_lifted_x86(&[0x0F, 0x01, 0xD0], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0x0008_02E7);
    assert_eq!(ctx.read_vreg(rdx), 0);
    ctx.write_vreg(rcx, 1);
    execute_lifted_x86(&[0x0F, 0x01, 0xD0], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0x25);
    assert_eq!(ctx.read_vreg(rdx), 0);
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);

    for value in [1u64, 3, 7, 0xE7, 0x2E7, 0x0008_0001, 0x0008_02E7] {
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

    const ALL: u64 = 0x0008_02E7;
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
    memory.write(0x100, &[0u8; 2696]).unwrap();
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
        x86.pkru = 0xA1B2_C3D4;
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
    assert_eq!(read_u64(&mut memory, 0x100 + 2688), 0xA1B2_C3D4);
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
        x86.pkru = 0;
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
        assert_eq!(x86.pkru, 0xA1B2_C3D4);
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
        assert_eq!(x86.pkru, 0);
        assert!(x86.gpr[16..32].iter().all(|register| *register == 0));
    }

    // A partial AVX-only XSAVEOPT transfers MXCSR and YMM_Hi128, preserves
    // unrelated legacy bytes, and preserves unrequested XSTATE_BV bits.
    memory.write(0x500, &[0xA5; 2696]).unwrap();
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

    const ALL: u64 = 0x0008_02E7;
    const COMPACTED: u64 = 1 << 63;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x2000);
    ctx.write_vreg(rbx, 0x100);
    ctx.write_vreg(rax, ALL);
    ctx.write_vreg(rdx, 0);
    memory.write(0x100, &[0xA5; 2696]).unwrap();
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
        x86.pkru = 0xA1B2_C3D4;
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
    assert_eq!(read_u64(&mut memory, 0x100 + 2432), 0xA1B2_C3D4);
    assert_eq!(read_u64(&mut memory, 0x100 + 2440), 0xDDDD_EEEE_FFFF_0001);

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87 = Default::default();
        x86.mxcsr = 0x1F80;
        x86.xmm = [[0xDEAD; 16]; 32];
        x86.k = [0; 8];
        x86.pkru = 0;
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
        assert_eq!(x86.pkru, 0xA1B2_C3D4);
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
    let no_fault = execute_lifted_x86(&[0x62, 0xF2, 0x7E, 0x09, 0x72, 0x08], &mut ctx, &mut memory);
    assert!(matches!(no_fault, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[1][..1], &sentinel[..1]);
        assert_eq!(&x86.xmm[1][1..], &[0; 15]);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.k[1] = 1;
    }
    let fault = execute_lifted_x86(&[0x62, 0xF2, 0x7E, 0x09, 0x72, 0x08], &mut ctx, &mut memory);
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
