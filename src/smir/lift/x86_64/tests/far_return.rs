//! Strict lift and canonical interpretation coverage for far RET (`CA`/`CB`).

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::ops::X86FarReturnOp;
use crate::smir::optimize::{OptLevel, optimize_function};

const MEMORY_BASE: u64 = 0x2000;
const GDT: u64 = 0x2100;
const RSP: u64 = 0x2800;

fn exact_far_return(result: &LiftResult) -> &X86FarReturnOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86FarReturn(ret) => ret,
        other => panic!("expected one exact X86FarReturn op, got {other:?}"),
    }
}

fn far_return_block(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict far-RET lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn code_descriptor(
    dpl: u8,
    conforming: bool,
    l: bool,
    db: bool,
    present: bool,
    accessed: bool,
) -> [u8; 8] {
    let raw = 0xFFFF_u64
        | ((0xA_u64 | (u64::from(conforming) << 2) | u64::from(accessed)) << 40)
        | (1 << 44)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (u64::from(l) << 53)
        | (u64::from(db) << 54);
    raw.to_le_bytes()
}

fn stack_descriptor(dpl: u8, db: bool, present: bool, accessed: bool) -> [u8; 8] {
    let raw = 0xFFFF_u64
        | ((0x2_u64 | u64::from(accessed)) << 40)
        | (1 << 44)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (u64::from(db) << 54);
    raw.to_le_bytes()
}

fn context_for_far_return(cpl: u8) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.efer = 1 << 10;
    x86.cs_l = true;
    x86.cpl = cpl;
    x86.cs_selector = 0x8 | u16::from(cpl);
    x86.cs_cache.l = true;
    x86.cs_cache.present = true;
    x86.cs_cache.s = true;
    x86.ss_selector = 0x10 | u16::from(cpl);
    x86.ss_cache.type_ = 0x3;
    x86.ss_cache.present = true;
    x86.ss_cache.s = true;
    x86.ss_cache.dpl = cpl;
    x86.ss_cache.db = true;
    x86.gdtr_base = GDT;
    x86.gdtr_limit = 0x7F;
    x86.gpr[4] = RSP;
    context
}

fn write_slot(memory: &mut FlatMemory, address: u64, width: OpWidth, value: u64) {
    let bytes = value.to_le_bytes();
    memory
        .write(address, &bytes[..width.to_mem_width().bytes() as usize])
        .unwrap();
}

fn read_u64(memory: &mut dyn SmirMemory, address: u64) -> u64 {
    let mut bytes = [0_u8; 8];
    memory.read(address, &mut bytes).unwrap();
    u64::from_le_bytes(bytes)
}

fn run_far_return(
    bytes: &[u8],
    context: &mut SmirContext,
    memory: &mut dyn SmirMemory,
) -> BlockResult {
    SmirInterpreter::new().execute_block(context, memory, &far_return_block(bytes))
}

#[test]
fn far_return_strictly_lifts_widths_immediates_apx_and_dynamic_target() {
    for (bytes, width, pop_bytes) in [
        (&[0xCB][..], OpWidth::W32, 0),
        (&[0x66, 0xCB], OpWidth::W16, 0),
        (&[0x48, 0xCB], OpWidth::W64, 0),
        (&[0xCA, 0x34, 0x12], OpWidth::W32, 0x1234),
        (&[0x48, 0xCA, 0xFE, 0xCA], OpWidth::W64, 0xCAFE),
    ] {
        let result = lift_single(bytes).expect("strict far-RET lift");
        assert_eq!(result.bytes_consumed, bytes.len());
        let ret = exact_far_return(&result);
        assert_eq!(ret.target, VReg::Arch(ArchReg::X86(X86Reg::Rip)));
        assert_eq!(ret.offset_width, width);
        assert_eq!(ret.pop_bytes, pop_bytes);
        assert!(!ret.requires_apx);
        assert_eq!(ret.next_pc, 0x1000 + bytes.len() as u64);
        assert!(matches!(
            result.control_flow,
            ControlFlow::IndirectBranch { target }
                if target == VReg::Arch(ArchReg::X86(X86Reg::Rip))
        ));
    }

    let apx = lift_single(&[0xD5, 0x18, 0xCB]).expect("REX2.W far RET");
    assert!(matches!(
        exact_far_return(&apx),
        X86FarReturnOp {
            offset_width: OpWidth::W64,
            requires_apx: true,
            ..
        }
    ));
    assert!(matches!(
        lift_single(&[0xF0, 0xCB]),
        Err(LiftError::InvalidEncoding { .. })
    ));
    assert!(matches!(
        lift_single(&[0xCA, 0x10]),
        Err(LiftError::Incomplete {
            have: 1,
            need: 2,
            ..
        })
    ));
}

#[test]
fn interpreter_frontiers_and_optimizer_preserve_typed_far_return_ownership() {
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut context = LiftContext::new(SourceArch::X86_64);
    let function = lifter
        .lift_function(
            0x1800,
            &TestMemory::new(0x1800, vec![0x48, 0xCA, 0x10, 0x00]),
            &mut context,
        )
        .expect("typed far RET must remain a native candidate");
    assert_eq!(function.blocks.len(), 1);
    assert!(matches!(
        function.blocks[0].ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86FarReturn(_),
            ..
        }]
    ));
    assert!(matches!(
        function.blocks[0].terminator,
        Terminator::IndirectBranch { target, .. }
            if target == VReg::Arch(ArchReg::X86(X86Reg::Rip))
    ));

    let lifted = lift_single(&[0x48, 0xCB]).unwrap();
    let op = &lifted.ops[0];
    assert!(op.kind.source_vregs().is_empty());
    assert_eq!(op.kind.dests(), vec![VReg::Arch(ArchReg::X86(X86Reg::Rip))]);
    assert!(op.kind.flags_read().is_empty());
    assert!(op.kind.flags_written().is_empty());
    assert!(op.kind.has_side_effects());
    assert!(op.kind.reads_memory());
    assert!(op.kind.writes_memory());
    assert!(op.is_jit_safe());

    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::IndirectBranch {
        target: VReg::Arch(ArchReg::X86(X86Reg::Rip)),
        possible_targets: vec![],
    });
    let mut function = SmirFunction::new(FunctionId(0), BlockId(0), 0x1000);
    function.add_block(block);
    optimize_function(&mut function, OptLevel::O2);
    assert!(matches!(
        function.blocks[0].ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86FarReturn(_),
            ..
        }]
    ));
}

#[test]
fn far_return_interpreter_same_privilege_commits_width_frame_and_accessed_bit_last() {
    for (bytes, width, pop_bytes, target) in [
        (&[0x66, 0xCB][..], OpWidth::W16, 0_u16, 0x1234_u64),
        (&[0xCA, 0x10, 0x00], OpWidth::W32, 0x10, 0x3456),
        (&[0x48, 0xCB], OpWidth::W64, 0, 0xFFFF_8000_1234_5678),
    ] {
        let selector: u16 = 0x18;
        let mut context = context_for_far_return(0);
        let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x1800);
        memory.load(
            (GDT + 0x18 - MEMORY_BASE) as usize,
            &code_descriptor(0, false, true, false, true, false),
        );
        write_slot(&mut memory, RSP, width, target);
        write_slot(
            &mut memory,
            RSP + u64::from(width.bytes()),
            width,
            u64::from(selector),
        );

        let result = run_far_return(bytes, &mut context, &mut memory);
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.rip, target);
        assert_eq!(x86.cs_selector, selector);
        assert_eq!(
            x86.gpr[4],
            RSP + 2 * u64::from(width.bytes()) + u64::from(pop_bytes)
        );
        assert_eq!(read_u64(&mut memory, GDT + 0x18) & (1 << 40), 1 << 40);
    }
}

#[test]
fn far_return_interpreter_outer_privilege_restores_both_stacks_and_invalidates_segments() {
    let code_selector: u16 = 0x1B;
    let stack_selector: u16 = 0x23;
    let pop_bytes = 0x10_u16;
    let target = 0xFFFF_8000_2468_ACE0;
    let loaded_rsp = 0x3400;
    let mut context = context_for_far_return(0);
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.es_selector = 0x3;
    x86.es_cache.s = true;
    x86.es_cache.type_ = 0x3;
    x86.es_cache.dpl = 0;
    x86.ds_selector = 0x33;
    x86.ds_cache.s = true;
    x86.ds_cache.type_ = 0x3;
    x86.ds_cache.dpl = 3;
    x86.fs_selector = 0x38;
    x86.fs_cache.s = true;
    x86.fs_cache.type_ = 0xE;
    x86.fs_cache.dpl = 0;
    x86.fs_base = 0xAAAA_BBBB_CCCC_DDDD;
    x86.gs_selector = 0x40;
    x86.gs_cache.s = true;
    x86.gs_cache.type_ = 0xA;
    x86.gs_cache.dpl = 0;

    let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x1800);
    memory.load(
        (GDT + 0x18 - MEMORY_BASE) as usize,
        &code_descriptor(3, false, true, false, true, false),
    );
    memory.load(
        (GDT + 0x20 - MEMORY_BASE) as usize,
        &stack_descriptor(3, true, true, false),
    );
    write_slot(&mut memory, RSP, OpWidth::W64, target);
    write_slot(&mut memory, RSP + 8, OpWidth::W64, u64::from(code_selector));
    write_slot(
        &mut memory,
        RSP + 16 + u64::from(pop_bytes),
        OpWidth::W64,
        loaded_rsp,
    );
    write_slot(
        &mut memory,
        RSP + 24 + u64::from(pop_bytes),
        OpWidth::W64,
        u64::from(stack_selector),
    );

    let result = run_far_return(&[0x48, 0xCA, pop_bytes as u8, 0], &mut context, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rip, target);
    assert_eq!(x86.cpl, 3);
    assert_eq!(x86.cs_selector, code_selector);
    assert_eq!(x86.ss_selector, stack_selector);
    assert_eq!(x86.gpr[4], loaded_rsp + u64::from(pop_bytes));
    assert_eq!(x86.es_selector, 0);
    assert!(x86.es_cache.unusable);
    assert_eq!(x86.ds_selector, 0x33);
    assert_eq!(x86.fs_selector, 0x38, "conforming code remains accessible");
    assert_eq!(x86.fs_base, 0xAAAA_BBBB_CCCC_DDDD);
    assert_eq!(x86.gs_selector, 0);
    assert_eq!(read_u64(&mut memory, GDT + 0x18) & (1 << 40), 1 << 40);
    assert_eq!(read_u64(&mut memory, GDT + 0x20) & (1 << 40), 1 << 40);
}

#[test]
fn far_return_outer_ss_fault_precedes_bad_target_and_commits_nothing() {
    let code_selector: u16 = 0x1B;
    let stack_selector: u16 = 0x23;
    let bad_target = 0x0000_8000_0000_0000;
    let mut context = context_for_far_return(0);
    let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x1800);
    memory.load(
        (GDT + 0x18 - MEMORY_BASE) as usize,
        &code_descriptor(3, false, true, false, true, false),
    );
    memory.load(
        (GDT + 0x20 - MEMORY_BASE) as usize,
        &stack_descriptor(3, true, false, false),
    );
    write_slot(&mut memory, RSP, OpWidth::W64, bad_target);
    write_slot(&mut memory, RSP + 8, OpWidth::W64, u64::from(code_selector));
    write_slot(&mut memory, RSP + 16, OpWidth::W64, 0x3400);
    write_slot(
        &mut memory,
        RSP + 24,
        OpWidth::W64,
        u64::from(stack_selector),
    );

    let result = run_far_return(&[0x48, 0xCB], &mut context, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::StackSegment {
            addr: 0x1000,
            error_code: 0x20,
        })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rip, 0);
    assert_eq!(x86.cs_selector, 0x8);
    assert_eq!(x86.ss_selector, 0x10);
    assert_eq!(x86.gpr[4], RSP);
    assert_eq!(read_u64(&mut memory, GDT + 0x18) & (1 << 40), 0);
    assert_eq!(read_u64(&mut memory, GDT + 0x20) & (1 << 40), 0);
}

#[test]
fn far_return_null_ss_and_compatibility_stack_width_rules_are_exact() {
    let mut null_context = context_for_far_return(0);
    let mut null_memory = FlatMemory::with_base(MEMORY_BASE, 0x1800);
    null_memory.load(
        (GDT + 0x18 - MEMORY_BASE) as usize,
        &code_descriptor(1, false, true, false, true, true),
    );
    write_slot(&mut null_memory, RSP, OpWidth::W64, 0xFFFF_8000_1234_5678);
    write_slot(&mut null_memory, RSP + 8, OpWidth::W64, 0x19);
    write_slot(&mut null_memory, RSP + 16, OpWidth::W64, 0x3500);
    write_slot(&mut null_memory, RSP + 24, OpWidth::W64, 0x1);
    let result = run_far_return(&[0x48, 0xCB], &mut null_context, &mut null_memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &null_context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.cpl, 1);
    assert_eq!(x86.ss_selector, 1);
    assert_eq!(x86.gpr[4], 0x3500);

    let mut compat_context = context_for_far_return(0);
    let mut compat_memory = FlatMemory::with_base(MEMORY_BASE, 0x1800);
    compat_memory.load(
        (GDT + 0x18 - MEMORY_BASE) as usize,
        &code_descriptor(3, false, false, true, true, true),
    );
    compat_memory.load(
        (GDT + 0x20 - MEMORY_BASE) as usize,
        &stack_descriptor(3, true, true, true),
    );
    write_slot(&mut compat_memory, RSP, OpWidth::W64, 0x1234);
    write_slot(&mut compat_memory, RSP + 8, OpWidth::W64, 0x1B);
    write_slot(
        &mut compat_memory,
        RSP + 16 + 0x30,
        OpWidth::W64,
        0xFFFF_FFFF_FFFF_FFF0,
    );
    write_slot(&mut compat_memory, RSP + 24 + 0x30, OpWidth::W64, 0x23);
    let result = run_far_return(
        &[0x48, 0xCA, 0x30, 0],
        &mut compat_context,
        &mut compat_memory,
    );
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &compat_context.arch_regs else {
        unreachable!()
    };
    assert!(!x86.cs_l);
    assert_eq!(
        x86.gpr[4], 0x20,
        "SS.B selects 32-bit outer stack arithmetic"
    );
}

#[test]
fn far_return_rejects_bad_runtime_shape_mode_and_apx_without_stack_reads() {
    for (bytes, mutate) in [(&[0x48, 0xCB][..], 0_u8), (&[0xD5, 0x18, 0xCB], 1)] {
        let mut context = context_for_far_return(0);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        if mutate == 0 {
            x86.cs_l = false;
        } else {
            x86.apx_enabled = false;
        }
        let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x1800);
        let result = run_far_return(bytes, &mut context, &mut memory);
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
        ));
    }
}

struct ReadOnlyMemory {
    inner: FlatMemory,
}

impl SmirMemory for ReadOnlyMemory {
    fn read(&mut self, addr: GuestAddr, buf: &mut [u8]) -> Result<(), MemoryError> {
        self.inner.read(addr, buf)
    }

    fn write(&mut self, addr: GuestAddr, _data: &[u8]) -> Result<(), MemoryError> {
        Err(MemoryError::PageFault {
            addr,
            write: true,
            user: false,
        })
    }

    fn atomic_load(
        &mut self,
        addr: GuestAddr,
        size: MemWidth,
        order: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        self.inner.atomic_load(addr, size, order)
    }

    fn atomic_store(
        &mut self,
        addr: GuestAddr,
        _value: u64,
        _size: MemWidth,
        _order: MemoryOrder,
    ) -> Result<(), MemoryError> {
        self.write(addr, &[])
    }

    fn compare_and_swap(
        &mut self,
        addr: GuestAddr,
        _expected: u64,
        _new: u64,
        _size: MemWidth,
        _success_order: MemoryOrder,
        _failure_order: MemoryOrder,
    ) -> Result<(u64, bool), MemoryError> {
        Err(MemoryError::PageFault {
            addr,
            write: true,
            user: false,
        })
    }

    fn atomic_rmw(
        &mut self,
        addr: GuestAddr,
        _op: AtomicOp,
        _operand: u64,
        _size: MemWidth,
        _order: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        Err(MemoryError::PageFault {
            addr,
            write: true,
            user: false,
        })
    }

    fn load_exclusive(&mut self, addr: GuestAddr, size: MemWidth) -> Result<u64, MemoryError> {
        self.inner.load_exclusive(addr, size)
    }

    fn store_exclusive(
        &mut self,
        addr: GuestAddr,
        _value: u64,
        _size: MemWidth,
    ) -> Result<bool, MemoryError> {
        Err(MemoryError::PageFault {
            addr,
            write: true,
            user: false,
        })
    }

    fn clear_exclusive(&mut self) {
        self.inner.clear_exclusive();
    }

    fn fence(&mut self, kind: FenceKind) {
        self.inner.fence(kind);
    }

    fn probe(&self, addr: GuestAddr, size: usize, write: bool) -> Result<(), MemoryError> {
        if write {
            Err(MemoryError::PageFault {
                addr,
                write: true,
                user: false,
            })
        } else {
            self.inner.probe(addr, size, false)
        }
    }
}

#[test]
fn far_return_accessed_write_fault_is_precise_and_noncommitting() {
    let mut inner = FlatMemory::with_base(MEMORY_BASE, 0x1800);
    inner.load(
        (GDT + 0x18 - MEMORY_BASE) as usize,
        &code_descriptor(0, false, true, false, true, false),
    );
    write_slot(&mut inner, RSP, OpWidth::W64, 0xFFFF_8000_1234_5678);
    write_slot(&mut inner, RSP + 8, OpWidth::W64, 0x18);
    let mut memory = ReadOnlyMemory { inner };
    let mut context = context_for_far_return(0);
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.rip = 0xCAFE;

    let result = run_far_return(&[0x48, 0xCB], &mut context, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: 0x2118,
            write: true,
        })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rip, 0xCAFE);
    assert_eq!(x86.cs_selector, 0x8);
    assert_eq!(x86.gpr[4], RSP);
    assert_eq!(read_u64(&mut memory, GDT + 0x18) & (1 << 40), 0);
}
