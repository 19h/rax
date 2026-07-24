//! Precise x86 COMI/UCOMI floating-point exception and flag tests.

use super::*;
use crate::isa::x86_64::flags;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

const STATUS_FLAGS: u64 = flags::bits::CF
    | flags::bits::PF
    | flags::bits::AF
    | flags::bits::ZF
    | flags::bits::SF
    | flags::bits::OF;
const INITIAL_FLAGS: u64 = 0x2 | STATUS_FLAGS | flags::bits::DF;

fn execute_compare(bytes: &[u8], first: u64, second: u64, mxcsr: u32) -> (BlockResult, u64, u32) {
    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1][0] = first;
        x86.xmm[2][0] = second;
        x86.mxcsr = mxcsr;
    }
    ctx.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
    ctx.flags.lazy = None;
    let mut memory = FlatMemory::new(1);
    let result = execute_lifted_x86(bytes, &mut ctx, &mut memory);
    ctx.flags.materialize_all();
    let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
        unreachable!()
    };
    (result, ctx.flags.materialized.to_rflags(), x86.mxcsr)
}

#[test]
fn lifted_x86_fp_compare_commits_every_truth_table_result_and_masked_invalid_status() {
    let finite = [
        (0x3C00, 0x3C00, flags::bits::ZF),
        (0x3C00, 0x4000, flags::bits::CF),
        (0x4000, 0x3C00, 0),
        (
            0x7E01,
            0x3C00,
            flags::bits::ZF | flags::bits::PF | flags::bits::CF,
        ),
    ];
    for (first, second, expected) in finite {
        let (result, rflags, _) =
            execute_compare(&[0x62, 0xF5, 0x7C, 0x08, 0x2E, 0xCA], first, second, 0x1F80);
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(rflags & STATUS_FLAGS, expected);
        assert_ne!(rflags & flags::bits::DF, 0);
    }

    let formats: &[(&[u8], u64, u64)] = &[
        (&[0x62, 0xF5, 0x7C, 0x08, 0x2F, 0xCA], 0x7E01, 0x7C01),
        (
            &[0x62, 0xF1, 0x7C, 0x08, 0x2F, 0xCA],
            0x7FC0_0001,
            0x7F80_0001,
        ),
        (
            &[0x62, 0xF1, 0xFD, 0x08, 0x2F, 0xCA],
            0x7FF8_0000_0000_0001,
            0x7FF0_0000_0000_0001,
        ),
    ];
    for (ordered, qnan, snan) in formats {
        let mut unordered = ordered.to_vec();
        unordered[4] = 0x2E;
        for (bytes, value, expect_invalid) in [
            (&ordered[..], *qnan, true),
            (&unordered[..], *qnan, false),
            (&ordered[..], *snan, true),
            (&unordered[..], *snan, true),
        ] {
            let (result, rflags, mxcsr) = execute_compare(bytes, value, 0, 0x1F80);
            assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
            assert_eq!(
                rflags & STATUS_FLAGS,
                flags::bits::ZF | flags::bits::PF | flags::bits::CF,
                "{bytes:02X?}"
            );
            assert_eq!(mxcsr & 1 != 0, expect_invalid, "{bytes:02X?}");
        }
    }
}

#[test]
fn lifted_x86_fp_compare_unmasked_exceptions_are_precise_and_sae_is_non_accruing() {
    for (bytes, qnan) in [
        (&[0x62, 0xF5, 0x7C, 0x08, 0x2F, 0xCA][..], 0x7E01),
        (&[0x62, 0xF1, 0x7C, 0x08, 0x2F, 0xCA][..], 0x7FC0_0001),
        (
            &[0x62, 0xF1, 0xFD, 0x08, 0x2F, 0xCA][..],
            0x7FF8_0000_0000_0001,
        ),
    ] {
        let (result, rflags, mxcsr) = execute_compare(bytes, qnan, 0, 0x1F80 & !(1 << 7));
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        assert_eq!(rflags & STATUS_FLAGS, INITIAL_FLAGS & STATUS_FLAGS);
        assert_ne!(mxcsr & 1, 0);

        let mut sae = bytes.to_vec();
        sae[3] |= 0x10;
        let (result, rflags, mxcsr) = execute_compare(&sae, qnan, 0, 0x1F80 & !(1 << 7));
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(
            rflags & STATUS_FLAGS,
            flags::bits::ZF | flags::bits::PF | flags::bits::CF
        );
        assert_eq!(mxcsr & 0x3F, 0);
    }
}

#[test]
fn lifted_x86_fp_compare_handles_fp16_denormals_and_fp32_daz_precisely() {
    let vcomish = &[0x62, 0xF5, 0x7C, 0x08, 0x2F, 0xCA];
    for daz in [false, true] {
        let mxcsr = 0x1F80 | if daz { 1 << 6 } else { 0 };
        let (result, rflags, mxcsr) = execute_compare(vcomish, 1, 0, mxcsr);
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(rflags & STATUS_FLAGS, 0);
        assert_ne!(mxcsr & (1 << 1), 0, "FP16 DAZ={daz}");
    }

    let vcomiss = &[0x62, 0xF1, 0x7C, 0x08, 0x2F, 0xCA];
    for (daz, expected_flags, expect_denormal) in [(false, 0, true), (true, flags::bits::ZF, false)]
    {
        let mxcsr = 0x1F80 | if daz { 1 << 6 } else { 0 };
        let (result, rflags, mxcsr) = execute_compare(vcomiss, 1, 0, mxcsr);
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(rflags & STATUS_FLAGS, expected_flags);
        assert_eq!(mxcsr & (1 << 1) != 0, expect_denormal);
    }

    let (result, rflags, mxcsr) = execute_compare(vcomish, 1, 0, 0x1F80 & !(1 << 8));
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    assert_eq!(rflags & STATUS_FLAGS, INITIAL_FLAGS & STATUS_FLAGS);
    assert_ne!(mxcsr & (1 << 1), 0);

    let mut sae = *vcomish;
    sae[3] |= 0x10;
    let (result, rflags, mxcsr) = execute_compare(&sae, 1, 0, 0x1F80 & !(1 << 8));
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(rflags & STATUS_FLAGS, 0);
    assert_eq!(mxcsr & 0x3F, 0);
}
