//! Strict lift, canonical interpretation, and optimizer coverage for INVPCID.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::SmirMemory;
use crate::smir::ir::ops::X86InvpcidOp;
use crate::smir::optimize::{OptLevel, optimize_function};

const DESCRIPTOR: u64 = 0x2000;

fn exact_invpcid(result: &LiftResult) -> &X86InvpcidOp {
    assert_eq!(result.ops.len(), 1, "unexpected lift: {:#?}", result.ops);
    match &result.ops[0].kind {
        OpKind::X86Invpcid(invpcid) => invpcid,
        other => panic!("expected one exact X86Invpcid op, got {other:?}"),
    }
}

fn invpcid_block(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict INVPCID lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

struct DescriptorMemory {
    base: u64,
    payload: [u8; 16],
    fail_read: bool,
    reads: Vec<(u64, usize)>,
    invalidated: Vec<(u8, u16, u64)>,
}

impl DescriptorMemory {
    fn new(low: u64, linear: u64) -> Self {
        let mut payload = [0_u8; 16];
        payload[..8].copy_from_slice(&low.to_le_bytes());
        payload[8..].copy_from_slice(&linear.to_le_bytes());
        Self {
            base: DESCRIPTOR,
            payload,
            fail_read: false,
            reads: Vec::new(),
            invalidated: Vec::new(),
        }
    }

    fn faulting() -> Self {
        Self {
            fail_read: true,
            ..Self::new(0, 0)
        }
    }
}

impl SmirMemory for DescriptorMemory {
    fn read(&mut self, addr: u64, buf: &mut [u8]) -> Result<(), MemoryError> {
        self.reads.push((addr, buf.len()));
        if self.fail_read || addr != self.base || buf.len() != self.payload.len() {
            return Err(MemoryError::PageFault {
                addr,
                write: false,
                user: false,
            });
        }
        buf.copy_from_slice(&self.payload);
        Ok(())
    }

    fn write(&mut self, _addr: u64, _data: &[u8]) -> Result<(), MemoryError> {
        panic!("INVPCID must not write memory")
    }

    fn atomic_load(
        &mut self,
        _addr: u64,
        _size: MemWidth,
        _order: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        panic!("INVPCID must use one complete 16-byte read")
    }

    fn atomic_store(
        &mut self,
        _addr: u64,
        _value: u64,
        _size: MemWidth,
        _order: MemoryOrder,
    ) -> Result<(), MemoryError> {
        panic!("INVPCID must not atomically store memory")
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
        panic!("INVPCID must not compare-and-swap memory")
    }

    fn atomic_rmw(
        &mut self,
        _addr: u64,
        _op: AtomicOp,
        _operand: u64,
        _size: MemWidth,
        _order: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        panic!("INVPCID must not read-modify-write memory")
    }

    fn load_exclusive(&mut self, _addr: u64, _size: MemWidth) -> Result<u64, MemoryError> {
        panic!("INVPCID must not load-exclusive memory")
    }

    fn store_exclusive(
        &mut self,
        _addr: u64,
        _value: u64,
        _size: MemWidth,
    ) -> Result<bool, MemoryError> {
        panic!("INVPCID must not store-exclusive memory")
    }

    fn clear_exclusive(&mut self) {}

    fn fence(&mut self, _kind: FenceKind) {}

    fn invalidate_process_context(&mut self, invpcid_type: u8, pcid: u16, linear: u64) {
        self.invalidated.push((invpcid_type, pcid, linear));
    }

    fn probe(&self, _addr: u64, _size: usize, _write: bool) -> Result<(), MemoryError> {
        panic!("INVPCID must perform the descriptor read, not a separate probe")
    }
}

fn context(invpcid_type: u64, address: u64) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    context.write_vreg(x86_gpr(0), invpcid_type);
    context.write_vreg(x86_gpr(3), address);
    context
}

#[test]
fn invpcid_strictly_lifts_legacy_apx_wig_addr32_segments_and_egprs() {
    let legacy = lift_single(&[0x66, 0x0F, 0x38, 0x82, 0x03]).unwrap();
    assert_eq!(legacy.bytes_consumed, 5);
    assert!(matches!(
        exact_invpcid(&legacy),
        X86InvpcidOp {
            invpcid_type,
            addr: Address::Direct(base),
            requires_apx: false,
            stack_segment: false,
            next_pc: 0x1005,
        } if *invpcid_type == x86_gpr(0) && *base == x86_gpr(3)
    ));

    // LLVM 23: `invpcid r15, [r12 + 4*r13 + 64]`.
    let sib = lift_single(&[0x66, 0x47, 0x0F, 0x38, 0x82, 0x7C, 0xAC, 0x40]).unwrap();
    assert!(matches!(
        exact_invpcid(&sib),
        X86InvpcidOp {
            invpcid_type,
            addr: Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x40,
                ..
            },
            requires_apx: false,
            stack_segment: true,
            next_pc: 0x1008,
        } if *invpcid_type == x86_gpr(15)
            && *base == x86_gpr(12)
            && *index == x86_gpr(13)
    ));

    let addr32 = lift_single(&[0x64, 0x67, 0x66, 0x0F, 0x38, 0x82, 0x44, 0x8D, 0x40]).unwrap();
    assert!(matches!(
        &exact_invpcid(&addr32).addr,
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
    assert!(!exact_invpcid(&addr32).stack_segment);

    let ss = lift_single(&[0x36, 0x66, 0x0F, 0x38, 0x82, 0x03]).unwrap();
    assert!(exact_invpcid(&ss).stack_segment);

    for code in [
        &[0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01][..],
        &[0x62, 0xEC, 0xFE, 0x08, 0xF2, 0x01],
    ] {
        let apx = lift_single(code).unwrap();
        assert!(matches!(
            exact_invpcid(&apx),
            X86InvpcidOp {
                invpcid_type,
                addr: Address::Direct(base),
                requires_apx: true,
                stack_segment: false,
                next_pc: 0x1006,
            } if *invpcid_type == x86_gpr(16) && *base == x86_gpr(17)
        ));
    }

    let apx_sib = lift_single(&[0x62, 0x2C, 0x7A, 0x08, 0xF2, 0x7C, 0xEC, 0x40]).unwrap();
    assert!(matches!(
        exact_invpcid(&apx_sib),
        X86InvpcidOp {
            invpcid_type,
            addr: Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 8,
                disp: 0x40,
                ..
            },
            requires_apx: true,
            next_pc: 0x1008,
            ..
        } if *invpcid_type == x86_gpr(31)
            && *base == x86_gpr(20)
            && *index == x86_gpr(29)
    ));

    let apx_addr32 =
        lift_single(&[0x64, 0x67, 0x62, 0x2C, 0x7A, 0x08, 0xF2, 0x7C, 0xEC, 0x40]).unwrap();
    assert!(matches!(
        &exact_invpcid(&apx_addr32).addr,
        Address::X86Addr32(inner) if matches!(
            inner.as_ref(),
            Address::SegmentRel {
                segment,
                base: Some(base),
                index: Some(index),
                scale: 8,
                disp: 0x40,
            } if *segment == VReg::Arch(ArchReg::X86(X86Reg::FsBase))
                && *base == x86_gpr(20)
                && *index == x86_gpr(29)
        )
    ));
    assert!(!exact_invpcid(&apx_addr32).stack_segment);
}

#[test]
fn invpcid_rejects_missing_mandatory_prefix_reserved_apx_fields_and_register_forms() {
    for bytes in [
        &[0x0F, 0x38, 0x82, 0x03][..],
        &[0xF0, 0x66, 0x0F, 0x38, 0x82, 0x03],
        &[0x66, 0x0F, 0x38, 0x82, 0xC3],
        &[0x62, 0xEC, 0x7C, 0x08, 0xF2, 0x01],
        &[0x62, 0xEC, 0x7E, 0x18, 0xF2, 0x01],
        &[0x62, 0xEC, 0x7E, 0x0C, 0xF2, 0x01],
        &[0x62, 0xEC, 0x7E, 0x88, 0xF2, 0x01],
        &[0x62, 0xEC, 0x7E, 0x28, 0xF2, 0x01],
        &[0x62, 0xEC, 0x7E, 0x09, 0xF2, 0x01],
        &[0x62, 0xEC, 0x76, 0x08, 0xF2, 0x01],
        &[0x62, 0xEC, 0x7E, 0x00, 0xF2, 0x01],
        &[0x62, 0xEC, 0x7E, 0x08, 0xF2, 0xC1],
        &[0x66, 0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01],
        &[0xF2, 0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01],
        &[0xF3, 0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01],
        &[0x40, 0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01],
        &[0xF0, 0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "unexpected lift for {bytes:02X?}"
        );
    }
}

#[test]
fn invpcid_metadata_tracks_type_then_address_and_one_atomic_memory_effect() {
    let op = &lift_single(&[0x66, 0x0F, 0x38, 0x82, 0x7C, 0x48, 0x08])
        .unwrap()
        .ops[0];
    assert_eq!(
        op.kind.source_vregs(),
        vec![x86_gpr(7), x86_gpr(1), x86_gpr(0)]
    );
    assert!(op.kind.dests().is_empty());
    assert!(op.kind.flags_read().is_empty());
    assert!(op.kind.flags_written().is_empty());
    assert!(op.kind.has_side_effects());
    assert!(op.kind.reads_memory());
    assert!(!op.kind.writes_memory());
    assert!(op.is_jit_safe());
}

#[test]
fn invpcid_interpreter_reads_one_complete_descriptor_and_accepts_all_types() {
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
    for (invpcid_type, low, linear) in [
        (0, 0x123, 0x0000_7FFF_FFFF_F000),
        (1, 0xABC, 0x0000_8000_0000_0000),
        (2, 0xFFF, 0x0000_8000_0000_0000),
        (3, 0xFFF, 0xFFFF_7FFF_FFFF_FFFF),
    ] {
        let mut context = context(invpcid_type, DESCRIPTOR);
        context.flags.materialized = flags;
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr4 = 1 << 17;
        let mut memory = DescriptorMemory::new(low, linear);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            &invpcid_block(&[0x66, 0x0F, 0x38, 0x82, 0x03]),
        );
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(memory.reads, vec![(DESCRIPTOR, 16)]);
        assert_eq!(
            memory.invalidated,
            vec![(invpcid_type as u8, low as u16, linear)]
        );
        assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
        assert!(context.flags.lazy.is_none());
    }
}

#[test]
fn invpcid_interpreter_fault_order_is_apx_cpl_canonical_memory_then_descriptor() {
    let apx_block = invpcid_block(&[0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01]);
    for (apx, cpl, expected_undefined) in [(false, 3, true), (true, 3, false)] {
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(x86_gpr(16), 4);
        context.write_vreg(x86_gpr(17), DESCRIPTOR);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.apx_enabled = apx;
        x86.cpl = cpl;
        let mut memory = DescriptorMemory::faulting();
        let result = SmirInterpreter::new().execute_block(&mut context, &mut memory, &apx_block);
        assert_eq!(
            matches!(result, BlockResult::Exit(ExitReason::Undefined { .. })),
            expected_undefined
        );
        assert_eq!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::GeneralProtection { .. })
            ),
            !expected_undefined
        );
        assert!(memory.reads.is_empty());
        assert!(memory.invalidated.is_empty());
    }

    for (bytes, address, expected_ss) in [
        (
            &[0x36, 0x66, 0x0F, 0x38, 0x82, 0x03][..],
            0x0000_8000_0000_0000,
            true,
        ),
        (
            &[0x66, 0x0F, 0x38, 0x82, 0x03][..],
            0x0000_7FFF_FFFF_FFF8,
            false,
        ),
    ] {
        let mut context = context(4, address);
        let mut memory = DescriptorMemory::faulting();
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &invpcid_block(bytes));
        assert_eq!(
            matches!(result, BlockResult::Exit(ExitReason::StackSegment { .. })),
            expected_ss
        );
        assert_eq!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::GeneralProtection { .. })
            ),
            !expected_ss
        );
        assert!(memory.reads.is_empty());
    }

    let mut context = context(4, DESCRIPTOR);
    let mut memory = DescriptorMemory::faulting();
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        &invpcid_block(&[0x66, 0x0F, 0x38, 0x82, 0x03]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: DESCRIPTOR,
            write: false
        })
    ));
    assert_eq!(memory.reads, vec![(DESCRIPTOR, 16)]);
    assert!(memory.invalidated.is_empty());
}

#[test]
fn invpcid_interpreter_rejects_each_descriptor_constraint_after_the_read() {
    for (name, invpcid_type, low, linear, pcide) in [
        ("type", 4, 0, 0x4000, true),
        ("reserved", 0, 1 << 12, 0x4000, true),
        ("PCIDE", 1, 1, 0x4000, false),
        ("linear", 0, 0, 0x0000_8000_0000_0000, true),
    ] {
        let mut context = context(invpcid_type, DESCRIPTOR);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr4 = if pcide { 1 << 17 } else { 0 };
        let mut memory = DescriptorMemory::new(low, linear);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            &invpcid_block(&[0x66, 0x0F, 0x38, 0x82, 0x03]),
        );
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::GeneralProtection {
                    addr: 0x1000,
                    error_code: 0
                })
            ),
            "{name}: {result:?}"
        );
        assert_eq!(memory.reads, vec![(DESCRIPTOR, 16)], "{name}");
        assert!(memory.invalidated.is_empty(), "{name}");
    }
}

#[test]
fn invpcid_interpreter_rejects_malformed_ir_without_read_or_invalidation() {
    for malformed in [
        X86InvpcidOp {
            invpcid_type: VReg::virt(0),
            addr: Address::Direct(x86_gpr(3)),
            requires_apx: false,
            stack_segment: false,
            next_pc: 0x1005,
        },
        X86InvpcidOp {
            invpcid_type: x86_gpr(16),
            addr: Address::Direct(x86_gpr(3)),
            requires_apx: false,
            stack_segment: false,
            next_pc: 0x1005,
        },
        X86InvpcidOp {
            invpcid_type: x86_gpr(0),
            addr: Address::Direct(VReg::virt(0)),
            requires_apx: false,
            stack_segment: false,
            next_pc: 0x1005,
        },
        X86InvpcidOp {
            invpcid_type: x86_gpr(0),
            addr: Address::Direct(x86_gpr(3)),
            requires_apx: false,
            stack_segment: false,
            next_pc: 0x1004,
        },
        X86InvpcidOp {
            invpcid_type: x86_gpr(0),
            addr: Address::Direct(x86_gpr(3)),
            requires_apx: true,
            stack_segment: false,
            next_pc: 0x1005,
        },
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, OpKind::X86Invpcid(malformed));
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let function = builder.finish();
        let mut context = context(0, DESCRIPTOR);
        let mut memory = DescriptorMemory::new(0, 0x4000);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            function.entry_block().unwrap(),
        );
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
        ));
        assert!(memory.reads.is_empty());
        assert!(memory.invalidated.is_empty());
    }
}

#[test]
fn invpcid_survives_o2_with_sources_and_semantics_intact() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86Invpcid(X86InvpcidOp {
            invpcid_type: x86_gpr(2),
            addr: Address::BaseIndexScale {
                base: Some(x86_gpr(0)),
                index: x86_gpr(1),
                scale: 4,
                disp: 0x20,
                disp_size: DispSize::Disp8,
            },
            requires_apx: false,
            stack_segment: false,
            next_pc: 0x1007,
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
        OpKind::X86Invpcid(..)
    ));

    let execute = |function: &crate::smir::ir::SmirFunction| {
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(x86_gpr(0), 0x1FC0);
        context.write_vreg(x86_gpr(1), 8);
        context.write_vreg(x86_gpr(2), 2);
        let mut memory = DescriptorMemory::new(0, 0x4567);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            function.entry_block().unwrap(),
        );
        (format!("{result:?}"), memory.reads, memory.invalidated)
    };
    assert_eq!(execute(&optimized), execute(&unoptimized));
}
