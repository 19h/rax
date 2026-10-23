//! Cache-maintenance fault preservation through lifting and optimization.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::optimize::{OptLevel, optimize_function};

const ENCODINGS: &[&[u8]] = &[&[0x0f, 0x08], &[0x0f, 0x09], &[0xf3, 0x0f, 0x09]];

#[test]
fn cache_maintenance_privilege_fault_precedes_a_following_store_at_every_opt_level() {
    for &bytes in ENCODINGS {
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let lifted = lift_single(bytes).expect("strict cache-maintenance lift");
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x0ff8);
            builder.push_op(
                0x0ff8,
                OpKind::Store {
                    src: VReg::imm(0x11),
                    addr: Address::Absolute(0),
                    width: MemWidth::B8,
                },
            );
            for op in lifted.ops {
                builder.push_op(op.guest_pc, op.kind);
            }
            builder.push_op(
                0x1000 + bytes.len() as u64,
                OpKind::Store {
                    src: VReg::imm(0x5a),
                    addr: Address::Absolute(0),
                    width: MemWidth::B8,
                },
            );
            builder.set_terminator(Terminator::Trap {
                kind: TrapKind::Halt,
            });
            let mut function = builder.finish();
            optimize_function(&mut function, level);
            let mut ctx = SmirContext::new_x86_64();
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!()
            };
            x86.cr0 = 0x8005003b;
            x86.cpl = 3;
            x86.rflags = 0xc43;
            x86.gpr[0] = 0x1122334455667788;
            let mut memory = FlatMemory::new(8);
            let result = SmirInterpreter::new().execute_block(
                &mut ctx,
                &mut memory,
                function.entry_block().unwrap(),
            );
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::GeneralProtection {
                        addr: 0x1000,
                        error_code: 0
                    })
                ),
                "{bytes:02x?} {level:?}: {result:?}"
            );
            let mut data = [0; 8];
            SmirMemory::read(&mut memory, 0, &mut data).unwrap();
            assert_eq!(data, [0x11, 0, 0, 0, 0, 0, 0, 0]);
            let ArchRegState::X86_64(x86) = ctx.arch_regs else {
                unreachable!()
            };
            assert_eq!(x86.cr0, 0x8005003b);
            assert_eq!(x86.rflags, 0xc43);
            assert_eq!(x86.gpr[0], 0x1122334455667788);
        }
    }
}

#[test]
fn cache_maintenance_interpreter_preserves_state_when_privilege_allows_it() {
    for &bytes in ENCODINGS {
        for (cr0, cpl) in [(0x8005003b, 0), (0x8005003a, 3)] {
            let lifted = lift_single(bytes).unwrap();
            let mut block = SmirBlock::new(BlockId(0), 0x1000);
            block.ops = lifted.ops;
            block.set_terminator(Terminator::Trap {
                kind: TrapKind::Halt,
            });
            let mut ctx = SmirContext::new_x86_64();
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!()
            };
            x86.cr0 = cr0;
            x86.cpl = cpl;
            x86.rflags = 0xc43;
            let result =
                SmirInterpreter::new().execute_block(&mut ctx, &mut FlatMemory::new(1), &block);
            assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
            let ArchRegState::X86_64(x86) = ctx.arch_regs else {
                unreachable!()
            };
            assert_eq!(x86.cr0, cr0);
            assert_eq!(x86.rflags, 0xc43);
        }
    }
}

#[test]
fn cache_maintenance_kinds_and_metadata_preserve_the_exact_instruction_boundary() {
    use crate::smir::ir::ops::X86CacheMaintenanceKind;
    for (bytes, expected) in ENCODINGS.iter().zip([
        X86CacheMaintenanceKind::Invd,
        X86CacheMaintenanceKind::Wbinvd,
        X86CacheMaintenanceKind::Wbnoinvd,
    ]) {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert_eq!(lifted.ops.len(), 1);
        let op = &lifted.ops[0];
        assert!(matches!(
            op.kind,
            OpKind::X86CacheMaintenance { kind, next_pc }
                if kind == expected && next_pc == 0x1000 + bytes.len() as u64
        ));
        assert!(op.kind.source_vregs().is_empty());
        assert!(op.kind.dests().is_empty());
        assert!(op.kind.flags_read().is_empty());
        assert!(op.kind.flags_written().is_empty());
        assert!(op.kind.has_side_effects());
        assert!(!op.kind.reads_memory());
        assert!(!op.kind.writes_memory());
        assert!(op.is_jit_safe());
    }
}

#[test]
fn cache_maintenance_lifting_rejects_lock_before_privilege_and_ignores_other_prefixes() {
    use crate::smir::ir::ops::X86CacheMaintenanceKind;
    for opcode in [8, 9] {
        assert!(matches!(
            lift_single(&[0xf0, 0x0f, opcode]),
            Err(LiftError::InvalidEncoding { .. })
        ));
        for prefix in [
            0x26, 0x2e, 0x36, 0x3e, 0x64, 0x65, 0x66, 0x67, 0x40, 0x41, 0x42, 0x44, 0x48, 0x4f,
            0xf2, 0xf3,
        ] {
            let lifted = lift_single(&[prefix, 0x0f, opcode]).unwrap();
            let expected = if opcode == 8 {
                X86CacheMaintenanceKind::Invd
            } else if prefix == 0xf3 {
                X86CacheMaintenanceKind::Wbnoinvd
            } else {
                X86CacheMaintenanceKind::Wbinvd
            };
            assert_eq!(lifted.bytes_consumed, 3);
            assert!(matches!(
                lifted.ops.last().unwrap().kind,
                OpKind::X86CacheMaintenance { kind, next_pc: 0x1003 } if kind == expected
            ));
        }
    }
}

#[test]
fn rex2_cache_maintenance_keeps_one_apx_guard_before_the_privilege_operation() {
    use crate::smir::ir::ops::X86CacheMaintenanceKind;
    for payload in 0x80..=0xff {
        for (opcode, prefix, expected) in [
            (8, None, X86CacheMaintenanceKind::Invd),
            (9, None, X86CacheMaintenanceKind::Wbinvd),
            (9, Some(0xf3), X86CacheMaintenanceKind::Wbnoinvd),
        ] {
            let mut bytes = Vec::new();
            bytes.extend(prefix);
            bytes.extend_from_slice(&[0xd5, payload, opcode]);
            let lifted = lift_single(&bytes).unwrap();
            assert_eq!(lifted.bytes_consumed, bytes.len());
            assert!(matches!(
                lifted.ops.as_slice(),
                [
                    SmirOp { id: OpId(0), guest_pc: 0x1000, kind: OpKind::X86RequireApx, .. },
                    SmirOp { id: OpId(1), guest_pc: 0x1000,
                        kind: OpKind::X86CacheMaintenance { kind, next_pc }, .. }
                ] if *kind == expected && *next_pc == 0x1000 + bytes.len() as u64
            ));
        }
    }
}
