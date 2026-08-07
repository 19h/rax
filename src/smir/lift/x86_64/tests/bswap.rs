//! Strict lifting coverage for `BSWAP` (`0F C8+rd`).

use super::*;

fn assert_empty_undefined_result(result: &LiftResult, bytes: &[u8], requires_apx: bool) {
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(result.branch_targets.is_empty(), "{bytes:02X?}");
    assert!(
        matches!(result.control_flow, ControlFlow::Fallthrough),
        "{bytes:02X?}"
    );
    if requires_apx {
        assert_eq!(result.ops.len(), 1, "{bytes:02X?}");
        assert!(
            matches!(result.ops[0].kind, OpKind::X86RequireApx),
            "{bytes:02X?}: {:?}",
            result.ops
        );
    } else {
        assert!(result.ops.is_empty(), "{bytes:02X?}: {:?}", result.ops);
    }
}

#[test]
fn lift_bswap_registers_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    let cases: &[(&[u8], &str, usize, VReg, OpWidth)] = &[
        (&[0x0F, 0xC8], "bswap_eax", 2, x86_gpr(0), OpWidth::W32),
        (
            &[0x48, 0x0F, 0xC8],
            "bswap_rax",
            3,
            x86_gpr(0),
            OpWidth::W64,
        ),
        (
            &[0xD5, 0x90, 0xC8],
            "bswap_r16d",
            3,
            x86_gpr(16),
            OpWidth::W32,
        ),
        (
            &[0xD5, 0x98, 0xC8],
            "bswap_r16",
            3,
            x86_gpr(16),
            OpWidth::W64,
        ),
        (
            &[0xD5, 0x99, 0xC8],
            "bswap_r24",
            3,
            x86_gpr(24),
            OpWidth::W64,
        ),
        (
            &[0xD5, 0x91, 0xCF],
            "bswap_r31d",
            3,
            x86_gpr(31),
            OpWidth::W32,
        ),
        (
            &[0xD5, 0x99, 0xCF],
            "bswap_r31",
            3,
            x86_gpr(31),
            OpWidth::W64,
        ),
    ];

    for (bytes, name, bytes_consumed, reg, width) in cases {
        // LLVM 23 examples:
        //   `bswap eax`   => 0f c8
        //   `bswap rax`   => 48 0f c8
        //   `bswap r16d`  => d5 90 c8
        //   `bswap r16`   => d5 98 c8
        //   `bswap r24`   => d5 99 c8
        //   `bswap r31d`  => d5 91 cf
        //   `bswap r31`   => d5 99 cf
        let result = lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap();
        assert_eq!(result.bytes_consumed, *bytes_consumed, "{name}");
        assert_bswap_op(&result, name, *reg, *width);
    }
}

#[test]
fn lift_undefined_bswap_r16_covers_all_2048_scanner_images() {
    let mut checks = 0usize;
    for opcode in 0xC8_u8..=0xCF {
        for trailing_byte in u8::MIN..=u8::MAX {
            let bytes = [0x66, 0x0F, opcode, trailing_byte];
            let result = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("BSWAP r16 {bytes:02X?}: {error:?}"));
            assert_eq!(result.bytes_consumed, 3, "{bytes:02X?}");
            assert_empty_undefined_result(&result, &bytes[..3], false);
            checks += 1;
        }
    }
    assert_eq!(checks, 8 * 256);
}

#[test]
fn lift_undefined_bswap_r16_preserves_prefix_order_and_all_register_classes() {
    for bytes in [
        &[0x66, 0x41, 0x0F, 0xC8][..],
        &[0x48, 0x66, 0x0F, 0xC9],
        &[0x66, 0x66, 0x0F, 0xCA],
        &[0xF3, 0x2E, 0x66, 0x0F, 0xCF],
    ] {
        let result =
            lift_single(bytes).unwrap_or_else(|error| panic!("BSWAP r16 {bytes:02X?}: {error:?}"));
        assert_empty_undefined_result(&result, bytes, false);
    }

    let rex_w = lift_single(&[0x66, 0x48, 0x0F, 0xC8]).expect("66 then REX.W BSWAP");
    assert_bswap_op(&rex_w, "66 then REX.W", x86_gpr(0), OpWidth::W64);

    for (bytes, reg) in [
        (&[0x66, 0xD5, 0x98, 0xC8][..], x86_gpr(16)),
        (&[0x66, 0xD5, 0x99, 0xCF][..], x86_gpr(31)),
    ] {
        let result = lift_single(bytes).expect("66 then REX2.W BSWAP");
        assert_bswap_op(&result, "66 then REX2.W", reg, OpWidth::W64);
    }
}

#[test]
fn lift_undefined_bswap_r16_exhaustively_guards_rex2_payloads() {
    let mut checks = 0usize;
    for payload in 0x80_u8..=0xFF {
        if payload & 0x08 != 0 {
            continue;
        }
        for opcode in 0xC8_u8..=0xCF {
            let bytes = [0x66, 0xD5, payload, opcode];
            let result = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("REX2 BSWAP r16 {bytes:02X?}: {error:?}"));
            assert_empty_undefined_result(&result, &bytes, true);
            checks += 1;
        }
    }
    assert_eq!(checks, 64 * 8);
}

#[test]
fn lift_lock_bswap_rejected_like_spec() {
    // LOCK is only valid on selected read-modify-write memory forms; BSWAP
    // is a register-only instruction and must #UD with LOCK at every width.
    for bytes in [
        &[0xF0, 0x0F, 0xC8][..],
        &[0xF0, 0x48, 0x0F, 0xC8],
        &[0xF0, 0x66, 0x0F, 0xC8],
        &[0xF0, 0xD5, 0x90, 0xC8],
        &[0xF0, 0x66, 0xD5, 0x90, 0xC8],
    ] {
        let error = lift_single(bytes).expect_err("LOCK BSWAP must be invalid");
        assert!(
            matches!(error, LiftError::InvalidEncoding { .. }),
            "{error:?}"
        );
    }
}

#[test]
fn undefined_bswap_r16_keeps_the_following_instruction_in_the_strict_function() {
    let code = vec![0x66, 0x0F, 0xC8, 0x48, 0x83, 0xC0, 0x01, 0xC3];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let function = lifter
        .lift_function(0x1600, &TestMemory::new(0x1600, code), &mut context)
        .expect("undefined BSWAP r16 must not create an interpreter frontier");

    assert_eq!(function.blocks.len(), 1);
    let block = &function.blocks[0];
    assert!(block.ops.iter().all(|op| op.guest_pc != 0x1600));
    assert!(block.ops.iter().any(|op| op.guest_pc == 0x1603));
    assert!(matches!(block.terminator, Terminator::Return { .. }));
}
