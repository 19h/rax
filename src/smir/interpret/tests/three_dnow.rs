//! tests::three_dnow tests

use super::*;
use crate::smir::interpret::*;
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::flags::{FlagSet, FlagUpdate, MaterializedFlags};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::types::ShiftOp;

#[test]
fn lifted_3dnow_pavgusb_and_pswapd_execute_and_order_memory_faults() {
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x100);
    let flags_before = 0x8D7;
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes([0, 1, 2, 3, 254, 255, 100, 101]);
        x86.mm[1] = u64::from_le_bytes([1, 2, 3, 4, 255, 0, 101, 104]);
        x86.mm[2] = 0;
        x86.x87.tag_word = 0xFFFF;
        x86.x87.status_word = 6 << 11 | 0x45;
    }
    execute_lifted_x86(&[0x0F, 0x0F, 0xC1, 0xBF], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0].to_le_bytes(), [1, 2, 3, 4, 255, 128, 101, 103]);
        assert_eq!(x86.x87.tag_word, 0);
        assert_eq!(x86.x87.status_word, 6 << 11 | 0x45);
    }

    execute_lifted_x86(&[0x0F, 0x0F, 0xD0, 0xBB], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[2], 0x0403_0201_6765_80FF);
        assert_eq!(x86.x87.tag_word, 0);
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    ctx.write_vreg(rax, 0x100);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
        x86.x87.tag_word = 0xFFFF;
    }
    let fault = execute_lifted_x86(&[0x0F, 0x0F, 0x00, 0xBF], &mut ctx, &mut memory);
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
fn three_d_now_atomic_family_covers_all_defined_suffix_semantics_and_numeric_bounds() {
    let fp =
        |low: f32, high: f32| SmirInterpreter::x86_three_d_now_pack(low.to_bits(), high.to_bits());
    let lanes = |value| {
        (
            SmirInterpreter::x86_three_d_now_lane(value, 0),
            SmirInterpreter::x86_three_d_now_lane(value, 1),
        )
    };
    let assert_fp = |kind, first, second, low: f32, high: f32| {
        assert_eq!(
            lanes(SmirInterpreter::x86_three_d_now_eval(kind, first, second)),
            (low.to_bits(), high.to_bits()),
            "{kind:?}"
        );
    };

    assert_eq!(
        lanes(SmirInterpreter::x86_three_d_now_eval(
            X86ThreeDNowKind::Pf2Iw,
            0,
            fp(40_000.75, -1.75),
        )),
        (0x0000_7FFF, 0xFFFF_FFFF)
    );
    assert_eq!(
        lanes(SmirInterpreter::x86_three_d_now_eval(
            X86ThreeDNowKind::Pf2Id,
            0,
            fp(2_147_483_648.0, -2_147_483_648.0),
        )),
        (0x7FFF_FFFF, 0x8000_0000)
    );

    let integer_words = u64::from((-32_768i16) as u16)
        | (0xA55Au64 << 16)
        | (u64::from(32_767u16) << 32)
        | (0x5AA5u64 << 48);
    assert_fp(
        X86ThreeDNowKind::Pi2Fw,
        0,
        integer_words,
        -32_768.0,
        32_767.0,
    );

    let integer_dwords = u64::from((-16_777_217i32) as u32) | (u64::from(16_777_217u32) << 32);
    assert_fp(
        X86ThreeDNowKind::Pi2Fd,
        0,
        integer_dwords,
        -16_777_217i32 as f32,
        16_777_217u32 as f32,
    );

    let horizontal_first = fp(1.5, 2.5);
    let horizontal_second = fp(-3.0, 5.0);
    assert_fp(
        X86ThreeDNowKind::PfAcc,
        horizontal_first,
        horizontal_second,
        4.0,
        2.0,
    );
    assert_fp(
        X86ThreeDNowKind::PfNAcc,
        horizontal_first,
        horizontal_second,
        -1.0,
        -8.0,
    );
    assert_fp(
        X86ThreeDNowKind::PfPNAcc,
        horizontal_first,
        horizontal_second,
        -1.0,
        2.0,
    );

    let first = fp(6.0, -4.0);
    let second = fp(2.0, 0.5);
    assert_fp(X86ThreeDNowKind::PfAdd, first, second, 8.0, -3.5);
    assert_fp(X86ThreeDNowKind::PfSub, first, second, 4.0, -4.5);
    assert_fp(X86ThreeDNowKind::PfSubR, first, second, -4.0, 4.5);
    assert_fp(X86ThreeDNowKind::PfMul, first, second, 12.0, -2.0);

    let comparison_first = fp(2.0, -1.0);
    let comparison_second = fp(2.0, -2.0);
    assert_eq!(
        lanes(SmirInterpreter::x86_three_d_now_eval(
            X86ThreeDNowKind::PfCmpEq,
            comparison_first,
            comparison_second,
        )),
        (u32::MAX, 0)
    );
    assert_eq!(
        lanes(SmirInterpreter::x86_three_d_now_eval(
            X86ThreeDNowKind::PfCmpGe,
            comparison_first,
            comparison_second,
        )),
        (u32::MAX, u32::MAX)
    );
    assert_eq!(
        lanes(SmirInterpreter::x86_three_d_now_eval(
            X86ThreeDNowKind::PfCmpGt,
            comparison_first,
            comparison_second,
        )),
        (0, u32::MAX)
    );

    let signed_zero_mix = fp(-0.0, -2.0);
    let signed_zero_other = fp(-3.0, 0.0);
    assert_eq!(
        lanes(SmirInterpreter::x86_three_d_now_eval(
            X86ThreeDNowKind::PfMax,
            signed_zero_mix,
            signed_zero_other,
        )),
        (0, 0)
    );
    assert_fp(
        X86ThreeDNowKind::PfMin,
        signed_zero_mix,
        signed_zero_other,
        -3.0,
        -2.0,
    );

    assert_fp(X86ThreeDNowKind::PfRcp, 0, fp(4.0, 123.0), 0.25, 0.25);
    assert_fp(X86ThreeDNowKind::PfRsqrt, 0, fp(-4.0, 123.0), -0.5, -0.5);

    let iteration_first = fp(4.0, 5.0);
    let iteration_second = fp(0.2, 0.1);
    assert_fp(
        X86ThreeDNowKind::PfRcpIt1,
        iteration_first,
        iteration_second,
        2.0f32 - 4.0f32 * 0.2f32,
        2.0f32 - 5.0f32 * 0.1f32,
    );
    assert_fp(
        X86ThreeDNowKind::PfRcpIt2,
        iteration_first,
        iteration_second,
        4.0f32 * 0.2f32,
        5.0f32 * 0.1f32,
    );
    assert_fp(
        X86ThreeDNowKind::PfRsqIt1,
        iteration_first,
        iteration_second,
        (3.0f32 - 4.0f32 * 0.2f32) * 0.5f32,
        (3.0f32 - 5.0f32 * 0.1f32) * 0.5f32,
    );

    let words = |values: [i16; 4]| {
        values
            .into_iter()
            .enumerate()
            .fold(0u64, |packed, (lane, value)| {
                packed | (u64::from(value as u16) << (lane * 16))
            })
    };
    assert_eq!(
        SmirInterpreter::x86_three_d_now_eval(
            X86ThreeDNowKind::PmulHrw,
            words([0x4000, -0x4000, 0x7FFF, i16::MIN]),
            words([0x4000, 0x4000, 0x7FFF, i16::MIN]),
        ),
        words([0x1000, -0x1000, 0x3FFF, 0x4000])
    );

    assert_eq!(
        lanes(SmirInterpreter::x86_three_d_now_eval(
            X86ThreeDNowKind::PfMul,
            fp(f32::MIN_POSITIVE, -f32::MAX),
            fp(0.5, 2.0),
        )),
        (0, 0xFF7F_FFFF),
        "underflow must flush and overflow must saturate"
    );
    assert_eq!(
        lanes(SmirInterpreter::x86_three_d_now_eval(
            X86ThreeDNowKind::PfAdd,
            fp(f32::from_bits(1), f32::from_bits(0x8000_0001)),
            fp(1.0, -0.0),
        )),
        (1.0f32.to_bits(), (-0.0f32).to_bits()),
        "subnormal inputs must decode as signed zero"
    );
    assert_eq!(
        lanes(SmirInterpreter::x86_three_d_now_eval(
            X86ThreeDNowKind::PfRcp,
            0,
            fp(-0.0, 1.0),
        )),
        (0xFF7F_FFFF, 0xFF7F_FFFF)
    );
    assert_eq!(
        lanes(SmirInterpreter::x86_three_d_now_eval(
            X86ThreeDNowKind::PfRcpIt1,
            fp(-0.0, 1.0),
            fp(2.0, -0.0),
        )),
        (0x8000_0000, 0x8000_0000)
    );
    assert_eq!(
        lanes(SmirInterpreter::x86_three_d_now_eval(
            X86ThreeDNowKind::PfAdd,
            fp(f32::NAN, 1.0),
            fp(1.0, 2.0),
        )),
        (0x7FC0_0000, 3.0f32.to_bits()),
        "unsupported exponent-255 inputs use the documented deterministic result"
    );
}
#[test]
fn lifted_3dnow_atomic_operation_preserves_flags_top_and_fault_atomicity() {
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x100);
    let flags_before = 0x8D7;
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = SmirInterpreter::x86_three_d_now_pack(6.0f32.to_bits(), (-4.0f32).to_bits());
        x86.mm[1] = SmirInterpreter::x86_three_d_now_pack(2.0f32.to_bits(), 0.5f32.to_bits());
        x86.x87.tag_word = 0xFFFF;
        x86.x87.status_word = 6 << 11 | 0x45;
    }

    execute_lifted_x86(&[0x0F, 0x0F, 0xC1, 0x9E], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0],
            SmirInterpreter::x86_three_d_now_pack(8.0f32.to_bits(), (-3.5f32).to_bits(),)
        );
        assert_eq!(x86.x87.tag_word, 0);
        assert_eq!(x86.x87.status_word, 6 << 11 | 0x45);
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    ctx.write_vreg(rax, 0x100);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
        x86.x87.tag_word = 0xFFFF;
    }
    let fault = execute_lifted_x86(&[0x0F, 0x0F, 0x00, 0x9E], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
        assert_eq!(x86.x87.status_word, 6 << 11 | 0x45);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
