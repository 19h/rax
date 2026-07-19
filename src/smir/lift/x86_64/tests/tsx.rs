//! RTM fixed-encoding lifting tests.

use super::*;
use crate::smir::lift::x86_64::*;

fn lift_at(pc: u64, bytes: &[u8]) -> Result<LiftResult, LiftError> {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    lifter.lift_insn(pc, bytes, &mut ctx)
}

fn assert_forced_abort(result: &LiftResult, expected_len: usize, expected_target: u64) {
    assert_eq!(result.bytes_consumed, expected_len);
    assert!(matches!(
        result.control_flow,
        ControlFlow::Branch { target } if target == expected_target
    ));
    assert_eq!(result.branch_targets, vec![expected_target]);
    assert!(matches!(
        result.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src: SrcOperand::Imm(0),
                width: OpWidth::W32,
            },
            ..
        }]
    ));
}

fn assert_gp0_trap(result: &LiftResult, expected_len: usize) {
    assert_eq!(result.bytes_consumed, expected_len);
    assert!(result.ops.is_empty());
    assert!(result.branch_targets.is_empty());
    assert!(matches!(
        result.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::GeneralProtection,
        }
    ));
}

#[test]
fn xabort_consumes_immediate_and_accepts_ignored_non_lock_prefixes() {
    for bytes in [
        &[0xC6, 0xF8, 0x42][..],
        &[0x66, 0xC6, 0xF8, 0x42],
        &[0x67, 0xC6, 0xF8, 0x42],
        &[0xF2, 0xC6, 0xF8, 0x42],
        &[0xF3, 0xC6, 0xF8, 0x42],
        &[0x64, 0xC6, 0xF8, 0x42],
        &[0x48, 0xC6, 0xF8, 0x42],
        &[0xD5, 0x00, 0xC6, 0xF8, 0x42],
    ] {
        let result = lift_at(0x1000, bytes)
            .unwrap_or_else(|error| panic!("XABORT {bytes:02X?} must lift completely: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(result.ops.is_empty(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    }
}

#[test]
fn xbegin_rel16_rel32_and_prefix_order_compute_exact_fallbacks() {
    assert_forced_abort(
        &lift_at(0x1000, &[0xC7, 0xF8, 0x05, 0x00, 0x00, 0x00]).unwrap(),
        6,
        0x100B,
    );
    assert_forced_abort(
        &lift_at(0x1000, &[0x66, 0xC7, 0xF8, 0xFB, 0xFF]).unwrap(),
        5,
        0x1000,
    );

    // A legacy 66 after REX.W invalidates the REX and selects rel16.
    assert_forced_abort(
        &lift_at(0x1000, &[0x48, 0x66, 0xC7, 0xF8, 0xFB, 0xFF]).unwrap(),
        6,
        0x1001,
    );
    // A later REX.W remains effective; XBEGIN still uses rel32 rather than
    // widening its displacement to 64 bits.
    assert_forced_abort(
        &lift_at(0x1000, &[0x66, 0x48, 0xC7, 0xF8, 0xFA, 0xFF, 0xFF, 0xFF]).unwrap(),
        8,
        0x1002,
    );
}

#[test]
fn xbegin_noncanonical_fallback_lifts_to_precise_gp0_trap() {
    let pc = 0x0000_7FFF_FFFF_FFFAu64;
    let result = lift_at(pc, &[0xC7, 0xF8, 0, 0, 0, 0]).unwrap();
    assert_gp0_trap(&result, 6);
}

#[test]
fn xtest_lifts_exact_flag_operation_and_xend_lifts_to_gp0() {
    for bytes in [
        &[0x0F, 0x01, 0xD6][..],
        &[0x66, 0xF2, 0x67, 0x64, 0x48, 0x0F, 0x01, 0xD6],
        &[0xD5, 0x00, 0x0F, 0x01, 0xD6],
    ] {
        let result = lift_at(0x1000, bytes).unwrap_or_else(|error| {
            panic!("XTEST {bytes:02X?} must accept ignored prefixes: {error:?}")
        });
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86XTest,
                ..
            }]
        ));
    }

    for bytes in [
        &[0x0F, 0x01, 0xD5][..],
        &[0x66, 0xF2, 0x67, 0x64, 0x48, 0x0F, 0x01, 0xD5],
        &[0xD5, 0x00, 0x0F, 0x01, 0xD5],
    ] {
        let result = lift_at(0x1000, bytes)
            .unwrap_or_else(|error| panic!("XEND {bytes:02X?} must lift to #GP(0): {error:?}"));
        assert_gp0_trap(&result, bytes.len());
    }
}

#[test]
fn rtm_lock_prefixes_are_invalid_and_immediates_are_required() {
    for bytes in [
        &[0xF0, 0xC6, 0xF8, 0x42][..],
        &[0xF0, 0xC7, 0xF8, 0, 0, 0, 0],
        &[0xF0, 0x0F, 0x01, 0xD6],
        &[0xF0, 0x0F, 0x01, 0xD5],
    ] {
        assert!(
            matches!(
                lift_at(0x1000, bytes),
                Err(LiftError::InvalidEncoding { .. })
            ),
            "{bytes:02X?}"
        );
    }

    assert!(matches!(
        lift_at(0x1000, &[0xC6, 0xF8]),
        Err(LiftError::Incomplete {
            have: 2,
            need: 3,
            ..
        })
    ));
    assert!(matches!(
        lift_at(0x1000, &[0x66, 0xC7, 0xF8, 0x00]),
        Err(LiftError::Incomplete {
            have: 4,
            need: 5,
            ..
        })
    ));
}

#[test]
fn xgetbv_xsetbv_semantics_survive_0f01_extraction() {
    assert!(matches!(
        lift_at(0x1000, &[0x0F, 0x01, 0xD0]).unwrap().ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86XGetBv { .. },
            ..
        }]
    ));
    assert!(matches!(
        lift_at(0x1000, &[0x0F, 0x01, 0xD1]).unwrap().ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86XSetBv { .. },
            ..
        }]
    ));
    assert!(matches!(
        lift_at(0x1000, &[0x66, 0x0F, 0x01, 0xD0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}
