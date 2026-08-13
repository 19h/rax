//! Exhaustive strict-lift and canonical-interpreter coverage for register
//! `XCHG r/m8,r8` (`86 /r`).

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

const LEGACY_PREFIXES: [&[u8]; 7] = [&[], &[0x66], &[0xF2], &[0xF3], &[0x67], &[0x64], &[0x65]];

fn assert_low_byte_xchg(bytes: &[u8], reg1: u8, reg2: u8, requires_apx: bool) {
    let result =
        lift_single(bytes).unwrap_or_else(|error| panic!("low-byte XCHG {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(
        matches!(result.control_flow, ControlFlow::Fallthrough),
        "{bytes:02X?}"
    );
    assert!(result.branch_targets.is_empty(), "{bytes:02X?}");

    let expected_len = usize::from(requires_apx) + 1;
    assert_eq!(
        result.ops.len(),
        expected_len,
        "{bytes:02X?}: {:?}",
        result.ops
    );
    if requires_apx {
        assert!(
            matches!(result.ops[0].kind, OpKind::X86RequireApx),
            "{bytes:02X?}: {:?}",
            result.ops
        );
    }
    assert!(
        matches!(
            result.ops[expected_len - 1].kind,
            OpKind::Xchg {
                reg1: VReg::Arch(ArchReg::X86(reg1_actual)),
                reg2: VReg::Arch(ArchReg::X86(reg2_actual)),
                width: OpWidth::W8,
            } if reg1_actual == X86Reg::gpr(reg1) && reg2_actual == X86Reg::gpr(reg2)
        ),
        "{bytes:02X?}: {:?}",
        result.ops
    );
}

#[test]
fn legacy_low_bytes_use_one_canonical_xchg_and_high_bytes_keep_the_merge_graph() {
    let mut low = 0usize;
    let mut high = 0usize;
    for prefix in LEGACY_PREFIXES {
        for modrm in 0xC0_u8..=0xFF {
            let rm = modrm & 7;
            let reg = (modrm >> 3) & 7;
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(&[0x86, modrm]);
            if rm < 4 && reg < 4 {
                assert_low_byte_xchg(&bytes, rm, reg, false);
                low += 1;
            } else {
                let result = lift_single(&bytes)
                    .unwrap_or_else(|error| panic!("high-byte XCHG {bytes:02X?}: {error:?}"));
                assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
                assert!(
                    result
                        .ops
                        .iter()
                        .all(|op| !matches!(op.kind, OpKind::Xchg { .. })),
                    "high-byte form must retain explicit extraction/merge: {bytes:02X?}: {:?}",
                    result.ops
                );
                assert!(result.ops.len() >= 4, "{bytes:02X?}: {:?}", result.ops);
                high += 1;
            }
        }
    }
    assert_eq!(low, 7 * 16);
    assert_eq!(high, 7 * 48);
}

#[test]
fn every_rex_payload_and_modrm_register_cell_uses_the_low_byte_namespace() {
    let mut checks = 0usize;
    for rex in 0x40_u8..=0x4F {
        let reg_ext = (rex & 0x04) << 1;
        let rm_ext = (rex & 0x01) << 3;
        for modrm in 0xC0_u8..=0xFF {
            let bytes = [rex, 0x86, modrm];
            assert_low_byte_xchg(
                &bytes,
                (modrm & 7) | rm_ext,
                ((modrm >> 3) & 7) | reg_ext,
                false,
            );
            checks += 1;
        }
    }
    assert_eq!(checks, 16 * 64);
}

#[test]
fn every_map_zero_rex2_payload_and_register_cell_selects_all_32_low_bytes() {
    let mut checks = 0usize;
    for payload in 0x00_u8..=0x7F {
        let reg_ext = u8::from(payload & 0x40 != 0) * 16 | u8::from(payload & 0x04 != 0) * 8;
        let rm_ext = u8::from(payload & 0x10 != 0) * 16 | u8::from(payload & 0x01 != 0) * 8;
        for modrm in 0xC0_u8..=0xFF {
            let bytes = [0xD5, payload, 0x86, modrm];
            assert_low_byte_xchg(
                &bytes,
                (modrm & 7) | rm_ext,
                ((modrm >> 3) & 7) | reg_ext,
                true,
            );
            checks += 1;
        }
    }
    assert_eq!(checks, 128 * 64);
}

#[test]
fn byte_xchg_memory_forms_remain_sequentially_consistent_atomic_swaps() {
    for bytes in [
        &[0x86, 0x00][..],
        &[0xF0, 0x86, 0x00],
        &[0x40, 0x86, 0x00],
        &[0xD5, 0x00, 0x86, 0x00],
        &[0xF0, 0xD5, 0x00, 0x86, 0x00],
    ] {
        let result = lift_single(bytes)
            .unwrap_or_else(|error| panic!("memory byte XCHG {bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::AtomicRmw {
                op: AtomicOp::Swap,
                width: MemWidth::B1,
                order: MemoryOrder::SeqCst,
                ..
            }
        )));
        assert!(
            result
                .ops
                .iter()
                .all(|op| !matches!(op.kind, OpKind::Xchg { .. })),
            "memory form must not become a register Xchg: {bytes:02X?}"
        );
    }
}

#[test]
fn lock_register_forms_remain_invalid_before_semantic_lifting() {
    for bytes in [&[0xF0, 0x86, 0xC0][..], &[0xF0, 0xD5, 0x00, 0x86, 0xC0]] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn canonical_interpreter_preserves_upper_bits_flags_and_aliases_at_o0_and_o2() {
    for level in [OptLevel::O0, OptLevel::O2] {
        for (modrm, reg1, reg2) in [(0xC8, 0u8, 1u8), (0xC0, 0, 0), (0xEC, 4, 5)] {
            let bytes = [0x40, 0x86, modrm];
            let lifted = lift_single(&bytes).expect("REX low-byte XCHG");
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.set_terminator(Terminator::Trap {
                kind: TrapKind::Halt,
            });
            let mut function = builder.finish();
            function.blocks[0].ops = lifted.ops;
            optimize_function(&mut function, level);

            let mut context = SmirContext::new_x86_64();
            context.flags.materialized =
                crate::smir::ir::flags::MaterializedFlags::from_rflags(0x2 | 0x08D5);
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!();
            };
            for index in 0u8..32 {
                x86.set_gpr(index, 0xA0A1_0000_0000_0010 + u64::from(index) * 0x0101);
            }
            let before1 = x86.get_gpr(reg1);
            let before2 = x86.get_gpr(reg2);

            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut FlatMemory::new(1),
                &function.blocks[0],
            );
            assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                unreachable!();
            };
            let expected1 = (before1 & !0xFF) | (before2 & 0xFF);
            let expected2 = (before2 & !0xFF) | (before1 & 0xFF);
            assert_eq!(x86.get_gpr(reg1), expected1, "{bytes:02X?} {level:?}");
            assert_eq!(x86.get_gpr(reg2), expected2, "{bytes:02X?} {level:?}");
            assert_eq!(context.flags.materialized.to_rflags(), 0x2 | 0x08D5);
        }
    }
}
