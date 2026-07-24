//! Precise scalar x86 floating-point-to-integer exception-state tests.

use super::*;

fn mxcsr(ctx: &SmirContext) -> u32 {
    match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.mxcsr,
        _ => unreachable!(),
    }
}

fn set_xmm_lane(ctx: &mut SmirContext, register: usize, bits: u64, width: u32) {
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!()
    };
    SmirInterpreter::set_lane(&mut x86.xmm[register], 0, width, bits);
}

#[test]
fn lifted_scalar_fp_to_int_commits_masked_invalid_and_precision_status() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(1);

    set_xmm_lane(&mut ctx, 1, 2.5f32.to_bits().into(), 32);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mxcsr = 0x1F80;
    }
    ctx.write_vreg(rax, u64::MAX);
    let result = execute_lifted_x86(&[0xF3, 0x0F, 0x2D, 0xC1], &mut ctx, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(ctx.read_vreg(rax), 2);
    assert_eq!(mxcsr(&ctx) & 0x3F, 1 << 5, "precision status");

    set_xmm_lane(&mut ctx, 1, 0x7FF8_1234_5678_9ABC, 64);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mxcsr = 0x1F80 | (1 << 5);
    }
    ctx.write_vreg(rax, u64::MAX);
    let result = execute_lifted_x86(&[0xF2, 0x48, 0x0F, 0x2D, 0xC1], &mut ctx, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(ctx.read_vreg(rax), 0x8000_0000_0000_0000);
    assert_eq!(mxcsr(&ctx) & 0x3F, (1 << 5) | 1, "sticky status");
}

#[test]
fn lifted_scalar_fp_to_int_unmasked_exception_is_precise_and_noncommitting() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut memory = FlatMemory::new(1);

    for (name, input, mxcsr_value, expected_status) in [
        ("invalid", 0x7FC0_1234u32, 0x1F80 & !(1 << 7), 1u32),
        (
            "precision",
            2.5f32.to_bits(),
            0x1F80 & !(1 << 12),
            1u32 << 5,
        ),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        set_xmm_lane(&mut ctx, 1, input.into(), 32);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = mxcsr_value;
        }
        ctx.write_vreg(rax, 0x0123_4567_89AB_CDEF);
        let result = execute_lifted_x86(&[0xF3, 0x0F, 0x2D, 0xC1], &mut ctx, &mut memory);
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
            ),
            "{name}: {result:?}"
        );
        assert_eq!(
            ctx.read_vreg(rax),
            0x0123_4567_89AB_CDEF,
            "{name}: destination"
        );
        assert_eq!(mxcsr(&ctx) & expected_status, expected_status, "{name}");
    }
}

#[test]
fn lifted_scalar_fp_to_int_sae_suppresses_status_and_unmasked_traps() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(1);

    set_xmm_lane(&mut ctx, 3, 0x7E01, 16);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mxcsr = (0x1F80 & !(1 << 7)) | (1 << 5);
    }
    ctx.write_vreg(rax, 0);
    let result = execute_lifted_x86(&[0x62, 0xF5, 0x7E, 0x18, 0x2D, 0xC3], &mut ctx, &mut memory);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Halt)),
        "{result:?}"
    );
    assert_eq!(ctx.read_vreg(rax), 0x8000_0000);
    assert_eq!(mxcsr(&ctx) & 0x3F, 1 << 5, "SAE preserves prior status");
}

#[test]
fn lifted_scalar_fp_to_int_applies_daz_only_to_binary32_and_binary64() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut memory = FlatMemory::new(1);

    for daz in [false, true] {
        let mut ctx = SmirContext::new_x86_64();
        set_xmm_lane(&mut ctx, 1, 1, 32);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mxcsr = 0x1F80 | if daz { 1 << 6 } else { 0 };
        }
        execute_lifted_x86(&[0xF3, 0x0F, 0x2D, 0xC1], &mut ctx, &mut memory);
        assert_eq!(ctx.read_vreg(rax), 0);
        assert_eq!(mxcsr(&ctx) & (1 << 5), if daz { 0 } else { 1 << 5 });
    }

    let mut ctx = SmirContext::new_x86_64();
    set_xmm_lane(&mut ctx, 3, 1, 16);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mxcsr = 0x1F80 | (1 << 6);
    }
    execute_lifted_x86(&[0x62, 0xF5, 0x7E, 0x08, 0x2D, 0xC3], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0);
    assert_eq!(mxcsr(&ctx) & (1 << 5), 1 << 5, "FP16 ignores DAZ");
}
