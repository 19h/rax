//! Strict lift, canonical interpretation, and optimizer coverage for INVLPG.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::SmirMemory;
use crate::smir::ir::ops::X86InvlpgOp;
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_invlpg(result: &LiftResult) -> &X86InvlpgOp {
    assert_eq!(result.ops.len(), 1, "unexpected lift: {:#?}", result.ops);
    match &result.ops[0].kind {
        OpKind::X86Invlpg(invlpg) => invlpg,
        other => panic!("expected one exact X86Invlpg op, got {other:?}"),
    }
}

fn invlpg_block(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict INVLPG lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

#[derive(Default)]
struct TranslationLog {
    invalidated: Vec<u64>,
}

impl SmirMemory for TranslationLog {
    fn read(&mut self, _addr: u64, _buf: &mut [u8]) -> Result<(), MemoryError> {
        panic!("INVLPG must not read memory")
    }

    fn write(&mut self, _addr: u64, _data: &[u8]) -> Result<(), MemoryError> {
        panic!("INVLPG must not write memory")
    }

    fn atomic_load(
        &mut self,
        _addr: u64,
        _size: MemWidth,
        _order: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        panic!("INVLPG must not atomically load memory")
    }

    fn atomic_store(
        &mut self,
        _addr: u64,
        _value: u64,
        _size: MemWidth,
        _order: MemoryOrder,
    ) -> Result<(), MemoryError> {
        panic!("INVLPG must not atomically store memory")
    }

    fn compare_and_swap(
        &mut self,
        _addr: u64,
        _expected: u64,
        _new: u64,
        _size: MemWidth,
        _success_order: MemoryOrder,
        _failure_order: MemoryOrder,
    ) -> Result<(u64, bool), MemoryError> {
        panic!("INVLPG must not compare-and-swap memory")
    }

    fn atomic_rmw(
        &mut self,
        _addr: u64,
        _op: AtomicOp,
        _operand: u64,
        _size: MemWidth,
        _order: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        panic!("INVLPG must not read-modify-write memory")
    }

    fn load_exclusive(&mut self, _addr: u64, _size: MemWidth) -> Result<u64, MemoryError> {
        panic!("INVLPG must not load-exclusive memory")
    }

    fn store_exclusive(
        &mut self,
        _addr: u64,
        _value: u64,
        _size: MemWidth,
    ) -> Result<bool, MemoryError> {
        panic!("INVLPG must not store-exclusive memory")
    }

    fn clear_exclusive(&mut self) {}

    fn fence(&mut self, _kind: FenceKind) {}

    fn invalidate_translation(&mut self, addr: u64) {
        self.invalidated.push(addr);
    }

    fn probe(&self, _addr: u64, _size: usize, _write: bool) -> Result<(), MemoryError> {
        panic!("INVLPG must not probe memory")
    }
}

#[test]
fn invlpg_strictly_lifts_exact_legacy_addr32_rip_relative_and_rex2_addresses() {
    let direct = lift_single(&[0x0F, 0x01, 0x38]).expect("strict INVLPG lift");
    assert_eq!(direct.bytes_consumed, 3);
    assert!(matches!(
        exact_invlpg(&direct),
        X86InvlpgOp {
            addr: Address::Direct(base),
            requires_apx: false,
            next_pc: 0x1003,
        } if *base == x86_gpr(0)
    ));

    let rip = lift_single(&[0x0F, 0x01, 0x3D, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert!(matches!(
        exact_invlpg(&rip).addr,
        Address::PcRel {
            offset: 0x1234_5678,
            base: Some(0x1007),
            disp_size: DispSize::Disp32,
        }
    ));

    let addr32 = lift_single(&[0x64, 0x67, 0x0F, 0x01, 0x7C, 0x8D, 0x40]).unwrap();
    assert!(matches!(
        &exact_invlpg(&addr32).addr,
        Address::X86Addr32(inner) if matches!(
            inner.as_ref(),
            Address::SegmentRel {
                segment,
                base: Some(base),
                index: Some(index),
                scale: 4,
                disp: 0x40,
            } if *segment == VReg::Arch(ArchReg::X86(X86Reg::FsBase))
                && *base == x86_gpr(5)
                && *index == x86_gpr(1)
        )
    ));

    let apx = lift_single(&[0xD5, 0xB3, 0x01, 0x3C, 0xD1]).unwrap();
    assert!(matches!(
        exact_invlpg(&apx),
        X86InvlpgOp {
            addr: Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 8,
                disp: 0,
                ..
            },
            requires_apx: true,
            next_pc: 0x1005,
        } if *base == x86_gpr(25) && *index == x86_gpr(26)
    ));
}

#[test]
fn invlpg_ignores_non_lock_legacy_prefixes_and_rejects_lock() {
    for bytes in [
        &[0x66, 0x0F, 0x01, 0x38][..],
        &[0x48, 0x0F, 0x01, 0x38],
        &[0xF2, 0x0F, 0x01, 0x38],
        &[0xF3, 0x0F, 0x01, 0x38],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(exact_invlpg(&result).addr, Address::Direct(base) if base == x86_gpr(0)));
    }
    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x01, 0x38]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}

#[test]
fn invlpg_metadata_tracks_address_without_claiming_a_memory_access() {
    let op = &lift_single(&[0x0F, 0x01, 0x7C, 0x48, 0x08]).unwrap().ops[0];
    assert_eq!(op.kind.source_vregs(), vec![x86_gpr(1), x86_gpr(0)]);
    assert!(op.kind.dests().is_empty());
    assert!(op.kind.flags_read().is_empty());
    assert!(op.kind.flags_written().is_empty());
    assert!(op.kind.has_side_effects());
    assert!(!op.kind.reads_memory());
    assert!(!op.kind.writes_memory());
    assert!(op.is_jit_safe());
}

#[test]
fn invlpg_interpreter_invalidates_only_canonical_cpl0_addresses_and_preserves_flags() {
    let flags = MaterializedFlags {
        cf: true,
        zf: false,
        sf: true,
        of: true,
        pf: false,
        af: true,
        df: true,
        ac: true,
    };
    for (address, expected) in [
        (0x0000_7FFF_FFFF_F123, vec![0x0000_7FFF_FFFF_F123]),
        (0xFFFF_8000_0000_0456, vec![0xFFFF_8000_0000_0456]),
        (0x0000_8000_0000_0000, vec![]),
    ] {
        let mut context = SmirContext::new_x86_64();
        context.flags.materialized = flags;
        context.write_vreg(x86_gpr(0), address);
        let mut memory = TranslationLog::default();
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            &invlpg_block(&[0x0F, 0x01, 0x38]),
        );
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(memory.invalidated, expected, "address={address:#018x}");
        assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
        assert!(context.flags.lazy.is_none());
    }
}

#[test]
fn invlpg_interpreter_orders_apx_before_cpl_and_never_touches_memory_on_fault() {
    let block = invlpg_block(&[0xD5, 0x91, 0x01, 0x3F]);
    for (apx, expected) in [
        (
            false,
            ExitReason::Undefined {
                addr: 0x1000,
                opcode: 0,
            },
        ),
        (
            true,
            ExitReason::GeneralProtection {
                addr: 0x1000,
                error_code: 0,
            },
        ),
    ] {
        let mut context = SmirContext::new_x86_64();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.apx_enabled = apx;
        x86.cpl = 3;
        context.write_vreg(x86_gpr(31), 0x4000);
        let mut memory = TranslationLog::default();
        let result = SmirInterpreter::new().execute_block(&mut context, &mut memory, &block);
        assert_eq!(
            format!("{result:?}"),
            format!("{:?}", BlockResult::Exit(expected))
        );
        assert!(memory.invalidated.is_empty());
    }
}

#[test]
fn invlpg_interpreter_rejects_malformed_ir_without_invalidating() {
    for malformed in [
        X86InvlpgOp {
            addr: Address::Direct(VReg::virt(0)),
            requires_apx: false,
            next_pc: 0x1003,
        },
        X86InvlpgOp {
            addr: Address::Direct(x86_gpr(16)),
            requires_apx: false,
            next_pc: 0x1004,
        },
        X86InvlpgOp {
            addr: Address::Direct(x86_gpr(0)),
            requires_apx: false,
            next_pc: 0x1002,
        },
        X86InvlpgOp {
            addr: Address::Direct(x86_gpr(0)),
            requires_apx: true,
            next_pc: 0x1003,
        },
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, OpKind::X86Invlpg(malformed));
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let function = builder.finish();
        let mut context = SmirContext::new_x86_64();
        let mut memory = TranslationLog::default();
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            function.entry_block().unwrap(),
        );
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
        ));
        assert!(memory.invalidated.is_empty());
    }
}

#[test]
fn invlpg_survives_o2_with_address_sources_and_semantics_intact() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86Invlpg(X86InvlpgOp {
            addr: Address::BaseIndexScale {
                base: Some(x86_gpr(0)),
                index: x86_gpr(1),
                scale: 4,
                disp: 0x20,
                disp_size: DispSize::Disp8,
            },
            requires_apx: false,
            next_pc: 0x1004,
        }),
    );
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut optimized = builder.finish();
    let unoptimized = optimized.clone();
    optimize_function(&mut optimized, OptLevel::O2);
    assert_eq!(optimized.blocks[0].ops.len(), 1);
    assert!(matches!(
        optimized.blocks[0].ops[0].kind,
        OpKind::X86Invlpg(..)
    ));

    let execute = |function: &crate::smir::ir::SmirFunction| {
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(x86_gpr(0), 0x2000);
        context.write_vreg(x86_gpr(1), 3);
        let mut memory = TranslationLog::default();
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            function.entry_block().unwrap(),
        );
        (format!("{result:?}"), memory.invalidated)
    };
    assert_eq!(execute(&optimized), execute(&unoptimized));
}
