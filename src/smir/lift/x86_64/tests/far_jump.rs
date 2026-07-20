//! Strict lift, metadata, optimizer, and interpreter coverage for indirect
//! far JMP (`FF /5`).

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, MemoryError, SmirMemory};
use crate::smir::ir::ops::X86FarJumpOp;

const MEMORY_BASE: u64 = 0x2000;
const POINTER: u64 = 0x2020;
const GDT: u64 = 0x2100;

fn exact_far_jump(result: &LiftResult) -> &X86FarJumpOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86FarJump(jump) => jump,
        other => panic!("expected one exact X86FarJump op, got {other:?}"),
    }
}

fn far_jump_block(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict far-JMP lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn code_descriptor(
    dpl: u8,
    present: bool,
    conforming: bool,
    l: bool,
    db: bool,
    limit: u32,
) -> [u8; 8] {
    assert!(limit <= 0xF_FFFF);
    let raw = u64::from(limit & 0xFFFF)
        | ((0xA_u64 | (u64::from(conforming) << 2)) << 40)
        | (1 << 44)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (u64::from((limit >> 16) & 0xF) << 48)
        | (u64::from(l) << 53)
        | (u64::from(db) << 54);
    raw.to_le_bytes()
}

fn call_gate(target_selector: u16, target_offset: u64, dpl: u8, present: bool) -> [u8; 16] {
    let low = (target_offset & 0xFFFF)
        | (u64::from(target_selector) << 16)
        | (0xC << 40)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (((target_offset >> 16) & 0xFFFF) << 48);
    let high = (target_offset >> 32) & 0xFFFF_FFFF;
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&low.to_le_bytes());
    bytes[8..].copy_from_slice(&high.to_le_bytes());
    bytes
}

fn far_pointer(offset: u64, selector: u16, width: OpWidth) -> Vec<u8> {
    let mut bytes = match width {
        OpWidth::W16 => (offset as u16).to_le_bytes().to_vec(),
        OpWidth::W32 => (offset as u32).to_le_bytes().to_vec(),
        OpWidth::W64 => offset.to_le_bytes().to_vec(),
        _ => unreachable!(),
    };
    bytes.extend_from_slice(&selector.to_le_bytes());
    bytes
}

fn context_for_far_jump(pointer: u64, cpl: u8) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.efer = 1 << 10;
    x86.cs_l = true;
    x86.cpl = cpl;
    x86.cs_selector = 0x8 | u16::from(cpl);
    x86.cs_cache.base = 0xDEAD_BEEF;
    x86.gdtr_base = GDT;
    x86.gdtr_limit = 0x7F;
    context.write_vreg(x86_gpr(0), pointer);
    context
}

fn run_far_jump(
    bytes: &[u8],
    context: &mut SmirContext,
    memory: &mut dyn SmirMemory,
) -> BlockResult {
    SmirInterpreter::new().execute_block(context, memory, &far_jump_block(bytes))
}

#[test]
fn far_jump_strictly_lifts_widths_addresses_segments_and_dynamic_target() {
    for (bytes, width) in [
        (&[0xFF, 0x28][..], OpWidth::W32),
        (&[0x66, 0xFF, 0x28], OpWidth::W16),
        (&[0x48, 0xFF, 0x28], OpWidth::W64),
    ] {
        let result = lift_single(bytes).expect("strict FF /5 lift");
        assert_eq!(result.bytes_consumed, bytes.len());
        let jump = exact_far_jump(&result);
        assert_eq!(jump.addr, Address::Direct(x86_gpr(0)));
        assert_eq!(jump.target, VReg::Arch(ArchReg::X86(X86Reg::Rip)));
        assert_eq!(jump.offset_width, width);
        assert!(!jump.requires_apx);
        assert!(!jump.stack_segment);
        assert_eq!(jump.next_pc, 0x1000 + bytes.len() as u64);
        assert!(matches!(
            result.control_flow,
            ControlFlow::IndirectBranch { target }
                if target == VReg::Arch(ArchReg::X86(X86Reg::Rip))
        ));
    }

    let rip_relative = lift_single(&[0x48, 0xFF, 0x2D, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert!(matches!(
        &exact_far_jump(&rip_relative).addr,
        Address::PcRel {
            offset: 0x1234_5678,
            disp_size: DispSize::Disp32,
            base: Some(0x1007),
        }
    ));

    let addr32 = lift_single(&[0x67, 0xFF, 0x2C, 0x88]).unwrap();
    assert!(matches!(
        &exact_far_jump(&addr32).addr,
        Address::X86Addr32(inner)
            if matches!(
                inner.as_ref(),
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    ..
                } if *base == x86_gpr(0) && *index == x86_gpr(1)
            )
    ));

    assert!(exact_far_jump(&lift_single(&[0xFF, 0x2C, 0x24]).unwrap()).stack_segment);
    assert!(!exact_far_jump(&lift_single(&[0x3E, 0xFF, 0x2C, 0x24]).unwrap()).stack_segment);
    assert!(exact_far_jump(&lift_single(&[0x36, 0xFF, 0x28]).unwrap()).stack_segment);

    let apx = lift_single(&[0xD5, 0x18, 0xFF, 0x28]).unwrap();
    assert!(matches!(
        exact_far_jump(&apx),
        X86FarJumpOp {
            addr: Address::Direct(base),
            offset_width: OpWidth::W64,
            requires_apx: true,
            ..
        } if *base == x86_gpr(16)
    ));
}

#[test]
fn far_jump_invalid_group5_encodings_trap_without_reintroducing_far_call_support() {
    for bytes in [
        &[0xFF, 0xE8][..], // register FF /5
        &[0xFF, 0xD8],     // register FF /3
        &[0xFF, 0x38],     // memory FF /7
        &[0xFF, 0xF8],     // register FF /7
    ] {
        let result = lift_single(bytes).expect("architectural #UD must lift");
        assert!(result.ops.is_empty(), "{bytes:02X?}");
        assert!(matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
    assert!(matches!(
        lift_single(&[0xFF, 0x18]),
        Err(LiftError::Unsupported { mnemonic, .. }) if mnemonic == "group5 far call"
    ));
    assert!(matches!(
        lift_single(&[0xF0, 0xFF, 0x28]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}

#[test]
fn interpreter_frontiers_preserve_exact_far_jump_but_strip_ordinary_indirect_jump() {
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);

    let mut far_context = LiftContext::new(SourceArch::X86_64);
    let far = lifter
        .lift_function(
            0x1800,
            &TestMemory::new(0x1800, vec![0x48, 0xFF, 0x28]),
            &mut far_context,
        )
        .expect("typed far JMP must remain in the native candidate block");
    assert_eq!(far.blocks.len(), 1);
    assert!(matches!(
        far.blocks[0].ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86FarJump(_),
            ..
        }]
    ));
    assert!(matches!(
        far.blocks[0].terminator,
        Terminator::IndirectBranch { target, .. }
            if target == VReg::Arch(ArchReg::X86(X86Reg::Rip))
    ));

    let mut near_context = LiftContext::new(SourceArch::X86_64);
    let near = lifter
        .lift_function(
            0x1900,
            &TestMemory::new(0x1900, vec![0xFF, 0xE0]),
            &mut near_context,
        )
        .expect("ordinary indirect JMP must form an interpreter frontier");
    assert_eq!(near.blocks.len(), 1);
    assert!(near.blocks[0].ops.is_empty());
    assert!(matches!(
        near.blocks[0].terminator,
        Terminator::Return { .. }
    ));
}

#[test]
fn far_jump_metadata_records_address_dynamic_rip_and_descriptor_effects() {
    let result = lift_single(&[0x48, 0xFF, 0x6C, 0x88, 0x08]).unwrap();
    let op = &result.ops[0];
    assert_eq!(op.kind.source_vregs(), vec![x86_gpr(1), x86_gpr(0)]);
    assert_eq!(op.kind.dests(), vec![VReg::Arch(ArchReg::X86(X86Reg::Rip))]);
    assert!(op.kind.flags_read().is_empty());
    assert!(op.kind.flags_written().is_empty());
    assert!(op.kind.has_side_effects());
    assert!(op.kind.reads_memory());
    assert!(op.kind.writes_memory());
    assert!(op.is_jit_safe());
}

#[test]
fn far_jump_interpreter_commits_code_descriptor_accessed_bit_cs_and_rip_last() {
    let target = 0xFFFF_8000_1234_5678;
    let selector = 0x10;
    let descriptor = code_descriptor(0, true, false, true, false, 0);
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
    let mut context = context_for_far_jump(POINTER, 0);
    context.flags.materialized = flags;
    let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x400);
    memory.load(
        (POINTER - MEMORY_BASE) as usize,
        &far_pointer(target, selector, OpWidth::W64),
    );
    memory.load(
        (GDT + u64::from(selector) - MEMORY_BASE) as usize,
        &descriptor,
    );

    let result = run_far_jump(&[0x48, 0xFF, 0x28], &mut context, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rip, target);
    assert_eq!(x86.cs_selector, selector);
    assert_eq!(x86.cs_cache.type_ & 1, 1);
    assert!(x86.cs_cache.l);
    assert!(!x86.cs_cache.db);
    assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
    let mut committed = [0_u8; 8];
    memory
        .read(GDT + u64::from(selector), &mut committed)
        .unwrap();
    assert_eq!(
        u64::from_le_bytes(committed),
        u64::from_le_bytes(descriptor) | (1 << 40)
    );
}

#[test]
fn far_jump_interpreter_uses_ia32e_call_gate_target_and_ignores_pointer_offset() {
    let gate_selector = 0x18;
    let target_selector = 0x30;
    let target = 0xFFFF_8000_2468_ACE0;
    let mut context = context_for_far_jump(POINTER, 3);
    let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x400);
    memory.load(
        (POINTER - MEMORY_BASE) as usize,
        &far_pointer(u64::MAX, gate_selector | 3, OpWidth::W64),
    );
    memory.load(
        (GDT + u64::from(gate_selector) - MEMORY_BASE) as usize,
        &call_gate(target_selector, target, 3, true),
    );
    let target_descriptor = code_descriptor(3, true, false, true, false, 0);
    memory.load(
        (GDT + u64::from(target_selector) - MEMORY_BASE) as usize,
        &target_descriptor,
    );

    let result = run_far_jump(&[0x48, 0xFF, 0x28], &mut context, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rip, target);
    assert_eq!(x86.cs_selector, target_selector | 3);
    let mut gate_after = [0_u8; 16];
    memory
        .read(GDT + u64::from(gate_selector), &mut gate_after)
        .unwrap();
    assert_eq!(gate_after, call_gate(target_selector, target, 3, true));
    let mut target_after = [0_u8; 8];
    memory
        .read(GDT + u64::from(target_selector), &mut target_after)
        .unwrap();
    assert_eq!(
        u64::from_le_bytes(target_after),
        u64::from_le_bytes(target_descriptor) | (1 << 40)
    );
}

#[test]
fn far_jump_interpreter_resolves_code_descriptors_through_the_ldt_cache() {
    const LDT: u64 = 0x2200;
    const SELECTOR: u16 = 0x0C;
    let target = 0x1234_5678;
    let descriptor = code_descriptor(0, true, false, true, false, 0);
    let mut context = context_for_far_jump(POINTER, 0);
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.ldtr_selector = 0x20;
    x86.ldtr_cache.base = LDT;
    x86.ldtr_cache.limit = 0x1F;
    x86.ldtr_cache.unusable = false;
    let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x400);
    memory.load(
        (POINTER - MEMORY_BASE) as usize,
        &far_pointer(target, SELECTOR, OpWidth::W64),
    );
    memory.load((LDT + 8 - MEMORY_BASE) as usize, &descriptor);

    let result = run_far_jump(&[0x48, 0xFF, 0x28], &mut context, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rip, target);
    assert_eq!(x86.cs_selector, SELECTOR);
    let mut descriptor_after = [0_u8; 8];
    memory.read(LDT + 8, &mut descriptor_after).unwrap();
    assert_eq!(descriptor_after[5] & 1, 1);
}

#[test]
fn far_jump_interpreter_faults_are_precise_and_noncommitting() {
    for (name, selector, limit, descriptor, target, expected) in [
        (
            "null selector",
            3,
            0x7F,
            code_descriptor(0, true, false, true, false, 0),
            0x1234,
            ExitReason::GeneralProtection {
                addr: 0x1000,
                error_code: 0,
            },
        ),
        (
            "table limit",
            0x10,
            0x16,
            code_descriptor(0, true, false, true, false, 0),
            0x1234,
            ExitReason::GeneralProtection {
                addr: 0x1000,
                error_code: 0x10,
            },
        ),
        (
            "not present",
            0x10,
            0x7F,
            code_descriptor(0, false, false, true, false, 0),
            0x1234,
            ExitReason::SegmentNotPresent {
                addr: 0x1000,
                error_code: 0x10,
            },
        ),
        (
            "noncanonical target",
            0x10,
            0x7F,
            code_descriptor(0, true, false, true, false, 0),
            0x0000_8000_0000_0000,
            ExitReason::GeneralProtection {
                addr: 0x1000,
                error_code: 0,
            },
        ),
    ] {
        let mut context = context_for_far_jump(POINTER, 0);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.gdtr_limit = limit;
        x86.rip = 0xAAAA;
        let original_cs = x86.cs_selector;
        let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x400);
        memory.load(
            (POINTER - MEMORY_BASE) as usize,
            &far_pointer(target, selector, OpWidth::W64),
        );
        memory.load(
            (GDT + u64::from(selector & 0xFFFC) - MEMORY_BASE) as usize,
            &descriptor,
        );
        let result = run_far_jump(&[0x48, 0xFF, 0x28], &mut context, &mut memory);
        let exact_fault = match (&result, &expected) {
            (
                BlockResult::Exit(ExitReason::GeneralProtection {
                    addr: actual_addr,
                    error_code: actual_code,
                }),
                ExitReason::GeneralProtection {
                    addr: expected_addr,
                    error_code: expected_code,
                },
            ) => actual_addr == expected_addr && actual_code == expected_code,
            (
                BlockResult::Exit(ExitReason::SegmentNotPresent {
                    addr: actual_addr,
                    error_code: actual_code,
                }),
                ExitReason::SegmentNotPresent {
                    addr: expected_addr,
                    error_code: expected_code,
                },
            ) => actual_addr == expected_addr && actual_code == expected_code,
            _ => false,
        };
        assert!(
            exact_fault,
            "{name}: actual={result:?}, expected={expected:?}"
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.rip, 0xAAAA, "{name}");
        assert_eq!(x86.cs_selector, original_cs, "{name}");
        assert_eq!(x86.cs_cache.base, 0xDEAD_BEEF, "{name}");
    }

    let mut context = context_for_far_jump(POINTER, 0);
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.apx_enabled = false;
    x86.rip = 0xBBBB;
    context.write_vreg(x86_gpr(16), POINTER);
    let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x400);
    let result = run_far_jump(&[0xD5, 0x18, 0xFF, 0x28], &mut context, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rip, 0xBBBB);
    assert_eq!(x86.cs_cache.base, 0xDEAD_BEEF);
}

#[test]
fn far_jump_interpreter_noncanonical_pointer_range_selects_ss_or_gp_before_memory() {
    let crossing_pointer = 0x0000_7FFF_FFFF_FFFC;
    for (name, bytes, base, expected) in [
        (
            "SS default",
            &[0x48, 0xFF, 0x2C, 0x24][..],
            x86_gpr(4),
            ExitReason::StackSegment {
                addr: 0x1000,
                error_code: 0,
            },
        ),
        (
            "DS default",
            &[0x48, 0xFF, 0x28],
            x86_gpr(0),
            ExitReason::GeneralProtection {
                addr: 0x1000,
                error_code: 0,
            },
        ),
    ] {
        let mut context = context_for_far_jump(0, 0);
        context.write_vreg(base, crossing_pointer);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.rip = 0xCAFE;
        let mut memory = FlatMemory::with_base(MEMORY_BASE, 0x400);
        let result = run_far_jump(bytes, &mut context, &mut memory);
        let matches_fault = match (&result, &expected) {
            (
                BlockResult::Exit(ExitReason::StackSegment {
                    addr: actual_addr,
                    error_code: actual_code,
                }),
                ExitReason::StackSegment {
                    addr: expected_addr,
                    error_code: expected_code,
                },
            ) => actual_addr == expected_addr && actual_code == expected_code,
            (
                BlockResult::Exit(ExitReason::GeneralProtection {
                    addr: actual_addr,
                    error_code: actual_code,
                }),
                ExitReason::GeneralProtection {
                    addr: expected_addr,
                    error_code: expected_code,
                },
            ) => actual_addr == expected_addr && actual_code == expected_code,
            _ => false,
        };
        assert!(
            matches_fault,
            "{name}: actual={result:?}, expected={expected:?}"
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.rip, 0xCAFE, "{name}");
        assert_eq!(x86.cs_selector, 0x8, "{name}");
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
fn far_jump_interpreter_accessed_write_fault_does_not_commit_cs_or_rip() {
    let selector = 0x10;
    let descriptor = code_descriptor(0, true, false, true, false, 0);
    let mut inner = FlatMemory::with_base(MEMORY_BASE, 0x400);
    inner.load(
        (POINTER - MEMORY_BASE) as usize,
        &far_pointer(0x1234, selector, OpWidth::W64),
    );
    inner.load(
        (GDT + u64::from(selector) - MEMORY_BASE) as usize,
        &descriptor,
    );
    let mut memory = ReadOnlyMemory { inner };
    let mut context = context_for_far_jump(POINTER, 0);
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.rip = 0xCAFE;

    let result = run_far_jump(&[0x48, 0xFF, 0x28], &mut context, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault {
            addr,
            write: true,
        }) if addr == GDT + u64::from(selector)
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rip, 0xCAFE);
    assert_eq!(x86.cs_selector, 0x8);
    assert_eq!(x86.cs_cache.base, 0xDEAD_BEEF);
}
