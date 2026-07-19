//! RTM deterministic fallback interpretation tests.

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn lifted_xtest_overwrites_all_status_flags_and_preserves_other_state() {
    const STATUS: u64 = 0x08D5;
    const DF: u64 = 1 << 10;

    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x1000);
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let r15 = VReg::Arch(ArchReg::X86(X86Reg::R15));
    ctx.write_vreg(rax, 0x0123_4567_89AB_CDEF);
    ctx.write_vreg(r15, 0xFEDC_BA98_7654_3210);
    ctx.flags.materialized = MaterializedFlags::from_rflags(STATUS | DF);
    // Exercise the lazy-input path: XTEST overwrites every status flag but
    // must retain the separately materialized direction flag.
    ctx.flags.set_lazy_sub(0, 1, u64::MAX, OpWidth::W64);

    assert!(matches!(
        execute_lifted_x86(&[0x0F, 0x01, 0xD6], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    ctx.flags.materialize_all();

    assert_eq!(ctx.flags.materialized.to_rflags() & STATUS, 1 << 6);
    assert!(ctx.flags.materialized.df, "XTEST must preserve DF");
    assert_eq!(ctx.read_vreg(rax), 0x0123_4567_89AB_CDEF);
    assert_eq!(ctx.read_vreg(r15), 0xFEDC_BA98_7654_3210);
}

#[test]
fn xtest_ir_metadata_is_exact_and_not_cross_host_whitelisted() {
    let kind = OpKind::X86XTest;
    assert!(kind.dests().is_empty());
    assert!(kind.source_vregs().is_empty());
    assert!(kind.has_side_effects());
    assert_eq!(kind.flags_written(), FlagSet::ALL_X86);
    assert_eq!(kind.flags_must_write(), FlagSet::ALL_X86);
    assert_eq!(kind.flags_read(), FlagSet::EMPTY);
    assert!(
        !kind.is_jit_safe(),
        "XTEST admission must remain x86-host-specific"
    );
}

#[test]
fn xtest_interpretation_matches_at_every_optimization_level() {
    use crate::smir::optimize::{OptLevel, optimize_function};

    const STATUS: u64 = 0x08D5;
    const DF: u64 = 1 << 10;

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Sub {
            dst: rax,
            src1: rax,
            src2: SrcOperand::Imm(2),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    );
    builder.push_op(0x1001, OpKind::X86XTest);
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let original = builder.finish();

    let mut baseline = None;
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut function = original.clone();
        optimize_function(&mut function, level);
        let block = &function.blocks[0];
        assert_eq!(
            block
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::X86XTest))
                .count(),
            1,
            "{level:?} must retain XTEST"
        );
        let expected_sub_flags = if level == OptLevel::O0 {
            FlagUpdate::All
        } else {
            FlagUpdate::None
        };
        let OpKind::Sub { flags, .. } = &block.ops[0].kind else {
            panic!("{level:?} must retain the architectural subtraction");
        };
        assert_eq!(*flags, expected_sub_flags, "{level:?}");

        let mut ctx = SmirContext::new_x86_64();
        ctx.write_vreg(rax, 1);
        ctx.flags.materialized = MaterializedFlags::from_rflags(STATUS | DF);
        let mut memory = FlatMemory::new(0x1000);
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut ctx, &mut memory, block),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.flags.materialize_all();

        let result = (
            ctx.read_vreg(rax),
            ctx.flags.materialized.to_rflags() & (STATUS | DF),
        );
        assert_eq!(result, (u64::MAX, (1 << 6) | DF), "{level:?}");
        if let Some(expected) = baseline {
            assert_eq!(result, expected, "{level:?} differs from O0");
        } else {
            baseline = Some(result);
        }
    }
}
