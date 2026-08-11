//! Lift, admission, lowering, and native-state tests for x86 flag controls.

use super::*;
use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::{BlockId, OpId};
use crate::smir::lift::ControlFlow;
#[cfg(feature = "smir-jit")]
use crate::smir::lower::runtime::is_native_clobber_safe;
use crate::smir::optimize::{OptLevel, optimize_function};

const STATUS_FLAGS: u64 = 0x08D5;
const DF: u64 = 1 << 10;

fn expected_kind(opcode: u8) -> OpKind {
    match opcode {
        0xF5 => OpKind::CmcCF,
        0xF8 => OpKind::SetCF { value: false },
        0xF9 => OpKind::SetCF { value: true },
        0xFC => OpKind::SetDF { value: false },
        0xFD => OpKind::SetDF { value: true },
        _ => unreachable!("not a flag-control opcode"),
    }
}

fn kind_matches(actual: &OpKind, expected: &OpKind) -> bool {
    matches!(
        (actual, expected),
        (OpKind::CmcCF, OpKind::CmcCF)
            | (
                OpKind::SetCF { value: false },
                OpKind::SetCF { value: false }
            )
            | (OpKind::SetCF { value: true }, OpKind::SetCF { value: true })
            | (
                OpKind::SetDF { value: false },
                OpKind::SetDF { value: false }
            )
            | (OpKind::SetDF { value: true }, OpKind::SetDF { value: true })
    )
}

fn function_from_ops(ops: Vec<SmirOp>) -> SmirFunction {
    let mut block = crate::smir::ir::SmirBlock::new(BlockId(0), 0x1000);
    block.ops = ops;
    block.set_terminator(Terminator::Return { values: vec![] });
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    function
}

fn lift_exact(bytes: &[u8]) -> crate::smir::lift::LiftResult {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, bytes, &mut context)
        .unwrap_or_else(|error| panic!("lift {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(
        matches!(
            result.control_flow,
            ControlFlow::Fallthrough | ControlFlow::NextInsn
        ),
        "{bytes:02X?}: {:?}",
        result.control_flow
    );
    result
}

fn assert_post_opt_native(ops: Vec<SmirOp>, bytes: &[u8]) {
    let mut function = function_from_ops(ops);
    optimize_function(&mut function, OptLevel::O2);
    #[cfg(feature = "smir-jit")]
    assert!(
        is_native_clobber_safe(&function),
        "post-O2 gate rejected {bytes:02X?}: {:?}",
        function.blocks[0].ops
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("lower {bytes:02X?}: {error:?}"));
    lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("finalize {bytes:02X?}: {error:?}"));
}

#[test]
fn legacy_flag_controls_cover_every_scanned_prefix_and_reject_lock() {
    const PREFIXES: &[&[u8]] = &[
        &[],
        &[0x66],
        &[0xF2],
        &[0xF3],
        &[0x67],
        &[0x64],
        &[0x65],
        &[0x48],
        &[0x44],
        &[0x41],
        &[0x4D],
        &[0x66, 0x48],
        &[0xF2, 0x48],
        &[0xF3, 0x48],
    ];

    for opcode in [0xF5, 0xF8, 0xF9, 0xFC, 0xFD] {
        let expected = expected_kind(opcode);
        for prefix in PREFIXES {
            let mut bytes = prefix.to_vec();
            bytes.push(opcode);
            let result = lift_exact(&bytes);
            assert_eq!(result.ops.len(), 1, "{bytes:02X?}");
            assert!(kind_matches(&result.ops[0].kind, &expected), "{bytes:02X?}");
            assert_post_opt_native(result.ops, &bytes);
        }

        for bytes in [vec![0xF0, opcode], vec![0xF0, 0x48, opcode]] {
            let mut lifter = X86_64Lifter::strict();
            let mut context = LiftContext::new(SourceArch::X86_64);
            assert!(
                matches!(
                    lifter.lift_insn(0x1000, &bytes, &mut context),
                    Err(crate::smir::lift::LiftError::InvalidEncoding { .. })
                ),
                "accepted illegal LOCK form {bytes:02X?}"
            );
        }
    }
}

#[test]
fn rex2_flag_controls_are_guarded_for_every_map_zero_payload() {
    for opcode in [0xF5, 0xF8, 0xF9, 0xFC, 0xFD] {
        let expected = expected_kind(opcode);
        for payload in 0x00..=0x7F {
            let bytes = [0xD5, payload, opcode];
            let result = lift_exact(&bytes);
            assert!(
                matches!(
                    result.ops.as_slice(),
                    [
                        SmirOp {
                            kind: OpKind::X86RequireApx,
                            ..
                        },
                        control
                    ] if kind_matches(&control.kind, &expected)
                ),
                "{bytes:02X?}: {:?}",
                result.ops
            );
            assert_post_opt_native(result.ops, &bytes);
        }

        for bytes in [
            vec![0xF0, 0xD5, 0x00, opcode],
            vec![0x48, 0xD5, 0x00, opcode],
        ] {
            let mut lifter = X86_64Lifter::strict();
            let mut context = LiftContext::new(SourceArch::X86_64);
            assert!(
                matches!(
                    lifter.lift_insn(0x1000, &bytes, &mut context),
                    Err(crate::smir::lift::LiftError::InvalidEncoding { .. })
                ),
                "accepted illegal REX2 prefix combination {bytes:02X?}"
            );
        }
    }
}

#[test]
fn df_lowering_preserves_standalone_shape_and_commits_jit_shadow() {
    for (value, opcode) in [(false, 0xFC), (true, 0xFD)] {
        let standalone = lower_single_op(OpKind::SetDF { value });
        assert!(standalone.contains(&opcode), "{standalone:02X?}");

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, OpKind::SetDF { value });
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_jit_fault_deopt_guards(true);
        lowerer
            .lower_function(&builder.finish())
            .expect("lower state-backed DF control");
        let code = lowerer
            .finalize()
            .expect("finalize state-backed DF control");
        assert!(
            code.windows(4).any(|window| {
                window == (crate::smir::lower::X86_GUEST_RFLAGS_OFFSET as u32).to_le_bytes()
            }),
            "missing GuestRegs.rflags shadow access: {code:02X?}"
        );
        assert!(
            code.windows(3).any(|window| window == [0x9D, 0x58, opcode]),
            "DF must change only after status/GPR restoration: {code:02X?}"
        );
    }
}

#[test]
fn every_helper_publish_clears_host_df_after_guest_state_is_saved() {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.emit_helper_call_state(PhysReg::Rax, true, false);
    assert_eq!(lowerer.code.data(), &[0xFC]);

    lowerer.code.clear();
    lowerer.emit_helper_call_state(PhysReg::Rax, false, false);
    assert!(
        lowerer.code.data().is_empty(),
        "restoring helper state must not overwrite the guest DF before POPFQ"
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native(
    ops: Vec<SmirOp>,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let function = function_from_ops(ops);
    assert!(is_native_clobber_safe(&function));
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower native flag-control sequence");
    let code = lowerer.finalize().expect("finalize native flag controls");
    let executable = ExecMem::new(&code).expect("map native flag controls");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0x0123_4567_89AB_CDEFu64.rotate_left(index as u32 * 5);
    }
    regs.rflags = 0x2;
    regs.exit_pc = 0xDEAD_BEEF;
    configure(&mut regs);
    let expected_gprs = regs.gpr;
    executable.run(lowered.entry_offset, &mut regs);
    assert_eq!(
        regs.gpr, expected_gprs,
        "flag controls must preserve every GPR"
    );
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_flag_controls_match_all_status_and_direction_inputs() {
    for kind in [
        OpKind::SetCF { value: false },
        OpKind::SetCF { value: true },
        OpKind::CmcCF,
        OpKind::SetDF { value: false },
        OpKind::SetDF { value: true },
    ] {
        for status_pattern in 0u64..64 {
            let status = [0, 2, 4, 6, 7, 11]
                .into_iter()
                .enumerate()
                .fold(0u64, |flags, (index, bit)| {
                    flags | (((status_pattern >> index) & 1) << bit)
                });
            for initial_df in [false, true] {
                let initial = 0x2 | status | (u64::from(initial_df) * DF);
                let before =
                    execute_native(vec![SmirOp::new(OpId(0), 0x1000, kind.clone())], |regs| {
                        regs.rflags = initial
                    });
                let expected = match kind {
                    OpKind::SetCF { value: false } => initial & !1,
                    OpKind::SetCF { value: true } => initial | 1,
                    OpKind::CmcCF => initial ^ 1,
                    OpKind::SetDF { value: false } => initial & !DF,
                    OpKind::SetDF { value: true } => initial | DF,
                    _ => unreachable!(),
                };
                assert_eq!(
                    before.rflags & (STATUS_FLAGS | DF),
                    expected & (STATUS_FLAGS | DF),
                    "{kind:?}, status={status:#05X}, initial_df={initial_df}"
                );
                assert_eq!(before.exit_pc, 0xDEAD_BEEF);
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn rex2_guard_precedes_every_flag_control_commit() {
    for opcode in [0xF5, 0xF8, 0xF9, 0xFC, 0xFD] {
        let ops = lift_exact(&[0xD5, 0x00, opcode]).ops;
        let initial = 0x2 | STATUS_FLAGS;

        let disabled = execute_native(ops.clone(), |regs| {
            regs.rflags = initial;
            regs.apx_enabled = 0;
        });
        assert_eq!(
            disabled.rflags & (STATUS_FLAGS | DF),
            initial & (STATUS_FLAGS | DF)
        );
        assert_eq!(disabled.exit_pc, 0x1000);

        let enabled = execute_native(ops, |regs| {
            regs.rflags = initial;
            regs.apx_enabled = 1;
        });
        let expected = match opcode {
            0xF5 | 0xF8 => initial & !1,
            0xF9 => initial | 1,
            0xFC => initial & !DF,
            0xFD => initial | DF,
            _ => unreachable!(),
        };
        assert_eq!(
            enabled.rflags & (STATUS_FLAGS | DF),
            expected & (STATUS_FLAGS | DF)
        );
        assert_eq!(enabled.exit_pc, 0xDEAD_BEEF);
    }
}
