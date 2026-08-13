//! Interpreter semantics for the legacy Group-2 `/6` SAL alias.

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::{OptLevel, optimize_function};

const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const AF: u64 = 1 << 4;

fn execute(
    bytes: &[u8],
    level: OptLevel,
    rax_value: u64,
    rcx_value: u64,
    rflags: u64,
) -> (u64, u64, u64) {
    let mut lift_context = LiftContext::new(SourceArch::X86_64);
    let result = X86_64Lifter::strict()
        .lift_insn(0x1000, bytes, &mut lift_context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = builder.finish();
    function.blocks[0].ops = result.ops;
    optimize_function(&mut function, level);

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let mut context = SmirContext::new_x86_64();
    context.write_vreg(rax, rax_value);
    context.write_vreg(rcx, rcx_value);
    context.flags.materialized = MaterializedFlags::from_rflags(rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x1000);
    let exit = SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    context.flags.materialize_all();
    (
        context.read_vreg(rax),
        context.read_vreg(rcx),
        context.flags.materialized.to_rflags(),
    )
}

#[test]
fn lifted_sal_group6_matches_shl_except_for_its_defined_emulator_af_policy() {
    let cases: &[(&str, &[u8], &[u8], u64, u64, bool)] = &[
        (
            "AL immediate zero",
            &[0xC0, 0xF0, 0],
            &[0xC0, 0xE0, 0],
            0xA5,
            0,
            false,
        ),
        (
            "AL immediate masked zero",
            &[0xC0, 0xF0, 32],
            &[0xC0, 0xE0, 32],
            0xA5,
            0,
            false,
        ),
        (
            "AX count one",
            &[0x66, 0xD1, 0xF0],
            &[0x66, 0xD1, 0xE0],
            0x1122_3344_5566_80A5,
            0,
            true,
        ),
        (
            "EAX count eight",
            &[0xC1, 0xF0, 8],
            &[0xC1, 0xE0, 8],
            0xFFFF_FFFF_8000_00A5,
            0,
            true,
        ),
        (
            "RAX count 63",
            &[0x48, 0xC1, 0xF0, 63],
            &[0x48, 0xC1, 0xE0, 63],
            0x8000_0000_0000_0001,
            0,
            true,
        ),
        (
            "AL by CL zero",
            &[0xD2, 0xF0],
            &[0xD2, 0xE0],
            0xA5,
            0,
            false,
        ),
        (
            "AL by CL masked zero",
            &[0xD2, 0xF0],
            &[0xD2, 0xE0],
            0xA5,
            32,
            false,
        ),
        ("AL by CL one", &[0xD2, 0xF0], &[0xD2, 0xE0], 0xA5, 1, true),
        (
            "AL by CL eight",
            &[0xD2, 0xF0],
            &[0xD2, 0xE0],
            0xA5,
            8,
            true,
        ),
        (
            "AL by CL oversized",
            &[0xD2, 0xF0],
            &[0xD2, 0xE0],
            0xA5,
            31,
            true,
        ),
        (
            "RCX by aliased CL",
            &[0x48, 0xD3, 0xF1],
            &[0x48, 0xD3, 0xE1],
            0x0123_4567_89AB_CDEF,
            0x8000_0000_0000_0001,
            true,
        ),
        (
            "RCX by aliased masked-zero CL",
            &[0x48, 0xD3, 0xF1],
            &[0x48, 0xD3, 0xE1],
            0x0123_4567_89AB_CDEF,
            0x8000_0000_0000_0040,
            false,
        ),
    ];

    let mut profiles = 0usize;
    for (name, sal, shl, rax, rcx, masked_nonzero) in cases {
        for initial in [0x2, 0x4_0CD7] {
            for level in LEVELS {
                let sal_state = execute(sal, level, *rax, *rcx, initial);
                let shl_state = execute(shl, level, *rax, *rcx, initial);
                assert_eq!(sal_state.0, shl_state.0, "{name} {level:?}: RAX");
                assert_eq!(sal_state.1, shl_state.1, "{name} {level:?}: RCX");
                if *masked_nonzero {
                    assert_eq!(sal_state.2 & AF, 0, "{name} {level:?}: SAL AF");
                    assert_eq!(shl_state.2 & AF, initial & AF, "{name} {level:?}: SHL AF");
                    assert_eq!(sal_state.2 & !AF, shl_state.2 & !AF, "{name} {level:?}");
                } else {
                    assert_eq!(sal_state.2, initial, "{name} {level:?}: zero count flags");
                    assert_eq!(sal_state, shl_state, "{name} {level:?}");
                }
                profiles += 1;
            }
        }
    }
    assert_eq!(profiles, cases.len() * 2 * LEVELS.len());
}

#[test]
fn sal_group6_zero_count_preserves_a_pending_lazy_flag_producer() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut context = SmirContext::new_x86_64();
    context.write_vreg(rax, 0xA5);
    context.flags.materialized = MaterializedFlags::from_rflags(0x402);
    context.flags.set_lazy_sub(0, 1, u64::MAX, OpWidth::W64);
    let mut expected = context.flags.clone();
    expected.materialize_all();

    let mut memory = FlatMemory::new(0x1000);
    execute_lifted_x86(&[0xC0, 0xF0, 0], &mut context, &mut memory);
    context.flags.materialize_all();
    assert_eq!(
        context.flags.materialized.to_rflags(),
        expected.materialized.to_rflags()
    );

    context.flags.materialized = MaterializedFlags::from_rflags(0x412);
    context.flags.set_lazy_sub(0, 1, u64::MAX, OpWidth::W64);
    execute_lifted_x86(&[0xC0, 0xF0, 1], &mut context, &mut memory);
    context.flags.materialize_all();
    assert!(
        !context.flags.materialized.af,
        "nonzero `/6` count clears AF"
    );
}
