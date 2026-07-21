//! Strict lift, canonical interpretation, optimization, and fault-atomicity
//! coverage for long-mode `LSS/LFS/LGS` (`0F B2/B4/B5`).

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, X86SystemSegmentCache};
use crate::smir::ir::memory::{FlatMemory, MemoryError, SmirMemory};
use crate::smir::ir::ops::{X86SystemSelector, X86SystemSelectorLoadOp, X86SystemSelectorSource};
use crate::smir::optimize::{OptLevel, optimize_function};

const GDT: u64 = 0x2000;
const POINTER: u64 = 0x3000;
const INITIAL_DST: u64 = 0xA5A5_5A5A_DEAD_BEEF;

fn exact_far_load(result: &LiftResult) -> &X86SystemSelectorLoadOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86SystemSelectorLoad(load) => load,
        other => panic!("expected one exact far-pointer selector load, got {other:?}"),
    }
}

fn far_load_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict LSS/LFS/LGS lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn data_descriptor(base: u64, type_: u8, dpl: u8, present: bool, accessed: bool) -> [u8; 8] {
    let type_ = type_ | u8::from(accessed);
    (0xFFFF_u64
        | ((base & 0xFFFF) << 16)
        | (((base >> 16) & 0xFF) << 32)
        | (u64::from(type_ & 0xF) << 40)
        | (1 << 44)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (0xF << 48)
        | (1 << 52)
        | (1 << 54)
        | (1 << 55)
        | (((base >> 24) & 0xFF) << 56))
        .to_le_bytes()
}

fn protected_context(pointer: u64, dst_index: usize) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.efer = 1 << 10;
    x86.cs_l = true;
    x86.cpl = 0;
    x86.gdtr_base = GDT;
    x86.gdtr_limit = 0x1F;
    x86.gpr[0] = pointer;
    x86.gpr[dst_index] = INITIAL_DST;
    x86.ss_selector = 0x28;
    x86.ss_cache = X86SystemSegmentCache {
        base: 0x1111_0000,
        present: true,
        ..X86SystemSegmentCache::default()
    };
    x86.fs_selector = 0x30;
    x86.fs_base = 0x2222_0000;
    x86.fs_cache = X86SystemSegmentCache {
        base: 0x2222_0000,
        present: true,
        ..X86SystemSegmentCache::default()
    };
    x86.gs_selector = 0x38;
    x86.gs_base = 0x3333_0000;
    x86.gs_cache = X86SystemSegmentCache {
        base: 0x3333_0000,
        present: true,
        ..X86SystemSegmentCache::default()
    };
    context
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

fn memory_with_pointer(
    offset: u64,
    selector: u16,
    width: OpWidth,
    descriptor: Option<[u8; 8]>,
) -> FlatMemory {
    let mut memory = FlatMemory::with_base(GDT, 0x1100);
    memory.load(
        (POINTER - GDT) as usize,
        &far_pointer(offset, selector, width),
    );
    if let Some(descriptor) = descriptor {
        memory.load(0x10, &descriptor);
    }
    memory
}

fn selector_state(
    context: &SmirContext,
    selector: X86SystemSelector,
) -> (u16, u64, X86SystemSegmentCache) {
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    match selector {
        X86SystemSelector::Ss => (x86.ss_selector, x86.ss_cache.base, x86.ss_cache.clone()),
        X86SystemSelector::Fs => (x86.fs_selector, x86.fs_base, x86.fs_cache.clone()),
        X86SystemSelector::Gs => (x86.gs_selector, x86.gs_base, x86.gs_cache.clone()),
        _ => unreachable!(),
    }
}

#[test]
fn far_pointer_load_strictly_lifts_all_opcodes_widths_addresses_and_fault_classes() {
    for (bytes, selector, width) in [
        (&[0x0F, 0xB2, 0x08][..], X86SystemSelector::Ss, OpWidth::W32),
        (
            &[0x66, 0x0F, 0xB4, 0x08][..],
            X86SystemSelector::Fs,
            OpWidth::W16,
        ),
        (
            &[0x48, 0x0F, 0xB5, 0x08][..],
            X86SystemSelector::Gs,
            OpWidth::W64,
        ),
        (
            &[0x66, 0x48, 0x0F, 0xB4, 0x08][..],
            X86SystemSelector::Fs,
            OpWidth::W64,
        ),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            exact_far_load(&result),
            X86SystemSelectorLoadOp {
                selector: got_selector,
                source: X86SystemSelectorSource::FarPointer {
                    addr: Address::Direct(base),
                    dst,
                    offset_width,
                    stack_segment: false,
                },
                requires_apx: false,
                next_pc,
            } if *got_selector == selector
                && *base == x86_gpr(0)
                && *dst == x86_gpr(1)
                && *offset_width == width
                && *next_pc == 0x1000 + bytes.len() as u64
        ));
    }

    for (bytes, stack_segment) in [
        (&[0x0F, 0xB2, 0x0C, 0x24][..], true),
        (&[0x3E, 0x0F, 0xB2, 0x0C, 0x24][..], false),
        (&[0x36, 0x0F, 0xB4, 0x08][..], true),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            exact_far_load(&result).source,
            X86SystemSelectorSource::FarPointer {
                stack_segment: got,
                ..
            } if got == stack_segment
        ));
    }

    let addr32 = lift_single(&[0x67, 0x0F, 0xB5, 0x08]).unwrap();
    assert!(matches!(
        &exact_far_load(&addr32).source,
        X86SystemSelectorSource::FarPointer {
            addr: Address::X86Addr32(inner),
            ..
        } if matches!(inner.as_ref(), Address::Direct(base) if *base == x86_gpr(0))
    ));

    let fs_relative = lift_single(&[0x64, 0x0F, 0xB4, 0x08]).unwrap();
    assert!(matches!(
        &exact_far_load(&fs_relative).source,
        X86SystemSelectorSource::FarPointer {
            addr: Address::SegmentRel {
                segment,
                base: Some(base),
                index: None,
                scale: 1,
                disp: 0,
            },
            stack_segment: false,
            ..
        } if *segment == VReg::Arch(ArchReg::X86(X86Reg::FsBase)) && *base == x86_gpr(0)
    ));
}

#[test]
fn far_pointer_load_rex2_map1_exhaustively_extends_destination_address_and_width() {
    for payload in 0x80_u8..=0xFF {
        for (opcode, selector) in [
            (0xB2, X86SystemSelector::Ss),
            (0xB4, X86SystemSelector::Fs),
            (0xB5, X86SystemSelector::Gs),
        ] {
            for legacy_66 in [false, true] {
                let mut bytes = Vec::new();
                if legacy_66 {
                    bytes.push(0x66);
                }
                bytes.extend_from_slice(&[0xD5, payload, opcode, 0x08]);
                let result = lift_single(&bytes).unwrap_or_else(|error| {
                    panic!("payload={payload:#04x} opcode={opcode:#04x}: {error:?}")
                });
                let dst = 1
                    | if payload & 0x40 != 0 { 16 } else { 0 }
                    | if payload & 0x04 != 0 { 8 } else { 0 };
                let base = if payload & 0x10 != 0 { 16 } else { 0 }
                    | if payload & 0x01 != 0 { 8 } else { 0 };
                let width = if payload & 0x08 != 0 {
                    OpWidth::W64
                } else if legacy_66 {
                    OpWidth::W16
                } else {
                    OpWidth::W32
                };
                assert_eq!(result.bytes_consumed, bytes.len());
                assert!(matches!(
                    exact_far_load(&result),
                    X86SystemSelectorLoadOp {
                        selector: got_selector,
                        source: X86SystemSelectorSource::FarPointer {
                            addr: Address::Direct(got_base),
                            dst: got_dst,
                            offset_width,
                            stack_segment: false,
                        },
                        requires_apx: true,
                        next_pc,
                    } if *got_selector == selector
                        && *got_base == x86_gpr(base)
                        && *got_dst == x86_gpr(dst)
                        && *offset_width == width
                        && *next_pc == 0x1000 + bytes.len() as u64
                ));
            }
        }
    }
}

#[test]
fn far_pointer_load_rejects_lock_register_forms_and_invalid_rex2_order() {
    for opcode in [0xB2, 0xB4, 0xB5] {
        let result = lift_single(&[0x0F, opcode, 0xC1]).unwrap();
        assert_eq!(result.bytes_consumed, 3);
        assert!(result.ops.is_empty());
        assert!(matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
        assert!(matches!(
            lift_single(&[0xF0, 0x0F, opcode, 0x08]),
            Err(LiftError::InvalidEncoding { .. })
        ));
        assert!(matches!(
            lift_single(&[0x48, 0xD5, 0x80, opcode, 0x08]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn far_pointer_load_interpreter_commits_segment_then_exact_gpr_width() {
    for (bytes, selector, width, offset, expected_dst, base) in [
        (
            &[0x66, 0x0F, 0xB4, 0x08][..],
            X86SystemSelector::Fs,
            OpWidth::W16,
            0x1234_BEEF_u64,
            0xA5A5_5A5A_DEAD_BEEF,
            0x1234_5000,
        ),
        (
            &[0x0F, 0xB5, 0x08][..],
            X86SystemSelector::Gs,
            OpWidth::W32,
            0x1234_89AB_CDEF,
            0x89AB_CDEF,
            0x2345_6000,
        ),
        (
            &[0x48, 0x0F, 0xB2, 0x08][..],
            X86SystemSelector::Ss,
            OpWidth::W64,
            0x0123_4567_89AB_CDEF,
            0x0123_4567_89AB_CDEF,
            0x3456_7000,
        ),
    ] {
        let descriptor = data_descriptor(base, 0x2, 0, true, false);
        let mut context = protected_context(POINTER, 1);
        let initial_flags = 0x08D7;
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.rflags = initial_flags;
        let mut memory = memory_with_pointer(offset, 0x10, width, Some(descriptor));
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &far_load_block(bytes));
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{bytes:02X?}"
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.gpr[1], expected_dst, "{bytes:02X?}");
        assert_eq!(x86.rflags, initial_flags, "{bytes:02X?}");
        assert_eq!(selector_state(&context, selector).0, 0x10, "{bytes:02X?}");
        assert_eq!(selector_state(&context, selector).1, base, "{bytes:02X?}");
        assert_eq!(
            selector_state(&context, selector).2.type_,
            0x3,
            "{bytes:02X?}"
        );
        assert_eq!(x86.interrupt_inhibit, selector == X86SystemSelector::Ss);
        let mut accessed = [0_u8; 8];
        memory.read(GDT + 0x10, &mut accessed).unwrap();
        assert_ne!(u64::from_le_bytes(accessed) & (1 << 40), 0);
    }
}

#[test]
fn far_pointer_load_source_and_descriptor_faults_are_precise_and_noncommitting() {
    for (name, selector, descriptor, expected) in [
        ("GDT limit", 0x20, None, 13_u8),
        (
            "wrong type",
            0x10,
            Some(data_descriptor(0, 0x8, 0, true, false)),
            13,
        ),
        (
            "FS not present",
            0x10,
            Some(data_descriptor(0, 0x2, 0, false, false)),
            11,
        ),
    ] {
        let mut context = protected_context(POINTER, 1);
        let before = (INITIAL_DST, selector_state(&context, X86SystemSelector::Fs));
        let mut memory = memory_with_pointer(0x89AB_CDEF, selector, OpWidth::W32, descriptor);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            &far_load_block(&[0x0F, 0xB4, 0x08]),
        );
        let expected_fault = match expected {
            13 => matches!(
                &result,
                BlockResult::Exit(ExitReason::GeneralProtection { addr: 0x1000, .. })
            ),
            11 => matches!(
                &result,
                BlockResult::Exit(ExitReason::SegmentNotPresent { addr: 0x1000, .. })
            ),
            _ => unreachable!(),
        };
        assert!(expected_fault, "{name}: {result:?}");
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.gpr[1], before.0, "{name}");
        assert_eq!(
            selector_state(&context, X86SystemSelector::Fs),
            before.1,
            "{name}"
        );
    }

    let mut context = protected_context(POINTER, 1);
    let before = (INITIAL_DST, selector_state(&context, X86SystemSelector::Ss));
    let descriptor = data_descriptor(0, 0x2, 0, false, false);
    let mut memory =
        memory_with_pointer(0x0123_4567_89AB_CDEF, 0x10, OpWidth::W64, Some(descriptor));
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        &far_load_block(&[0x48, 0x0F, 0xB2, 0x08]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::StackSegment {
            addr: 0x1000,
            error_code: 0x10,
        })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[1], before.0);
    assert_eq!(selector_state(&context, X86SystemSelector::Ss), before.1);
    assert!(!x86.interrupt_inhibit);

    // The offset is readable but the trailing 16-bit selector is truncated.
    let mut context = protected_context(POINTER, 1);
    let before = selector_state(&context, X86SystemSelector::Fs);
    let mut memory = FlatMemory::with_base(GDT, (POINTER - GDT + 5) as usize);
    memory.load((POINTER - GDT) as usize, &[0xEF, 0xCD, 0xAB, 0x89, 0x10]);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        &far_load_block(&[0x0F, 0xB4, 0x08]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[1], INITIAL_DST);
    assert_eq!(selector_state(&context, X86SystemSelector::Fs), before);
}

#[test]
fn far_pointer_load_full_range_canonical_fault_is_gp_or_ss_before_memory() {
    for (name, bytes, pointer, stack_segment) in [
        (
            "W16 lower-bound crossing GP",
            &[0x66, 0x0F, 0xB4, 0x08][..],
            0x0000_7FFF_FFFF_FFFD,
            false,
        ),
        (
            "W32 upper-gap GP",
            &[0x0F, 0xB4, 0x08][..],
            0xFFFF_7FFF_FFFF_FFFF,
            false,
        ),
        (
            "W64 wrap GP",
            &[0x48, 0x0F, 0xB4, 0x08][..],
            u64::MAX - 8,
            false,
        ),
        (
            "W64 lower-bound crossing SS",
            &[0x48, 0x0F, 0xB2, 0x0C, 0x24][..],
            0x0000_7FFF_FFFF_FFF7,
            true,
        ),
        (
            "W32 wrap SS",
            &[0x0F, 0xB2, 0x0C, 0x24][..],
            u64::MAX - 4,
            true,
        ),
    ] {
        let mut context = protected_context(POINTER, 1);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        if stack_segment {
            x86.gpr[4] = pointer;
        } else {
            x86.gpr[0] = pointer;
        }
        let before_dst = x86.gpr[1];
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            &far_load_block(bytes),
        );
        assert!(
            if stack_segment {
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::StackSegment {
                        addr: 0x1000,
                        error_code: 0,
                    })
                )
            } else {
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::GeneralProtection {
                        addr: 0x1000,
                        error_code: 0,
                    })
                )
            },
            "{name}: {result:?}"
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.gpr[1], before_dst, "{name}");
    }
}

#[test]
fn far_pointer_load_apx_mode_and_injected_shape_guards_fail_closed() {
    let apx_block = far_load_block(&[0xD5, 0x80, 0xB4, 0x08]);
    let mut context = protected_context(POINTER, 1);
    let mut memory = memory_with_pointer(0, 0, OpWidth::W32, None);
    let result = SmirInterpreter::new().execute_block(&mut context, &mut memory, &apx_block);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));

    for mode in ["no LMA", "not 64-bit CS", "not protected", "VM86"] {
        let mut context = protected_context(POINTER, 1);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        match mode {
            "no LMA" => x86.efer = 0,
            "not 64-bit CS" => x86.cs_l = false,
            "not protected" => x86.cr0 = 0,
            "VM86" => x86.rflags |= crate::isa::x86_64::flags::bits::VM,
            _ => unreachable!(),
        }
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            &far_load_block(&[0x0F, 0xB4, 0x08]),
        );
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Undefined { .. })),
            "{mode}"
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.gpr[1], INITIAL_DST, "{mode}");
    }

    for mutate in 0..5 {
        let mut block = far_load_block(&[0x0F, 0xB4, 0x08]);
        let OpKind::X86SystemSelectorLoad(load) = &mut block.ops[0].kind else {
            unreachable!()
        };
        let X86SystemSelectorSource::FarPointer {
            addr,
            dst,
            offset_width,
            ..
        } = &mut load.source
        else {
            unreachable!()
        };
        match mutate {
            0 => load.selector = X86SystemSelector::Ds,
            1 => *dst = VReg::virt(0),
            2 => *offset_width = OpWidth::W8,
            3 => {
                *dst = x86_gpr(16);
                load.requires_apx = false;
            }
            4 => load.next_pc = 0x1002,
            _ => unreachable!(),
        }
        let mut context = protected_context(POINTER, 1);
        let result = SmirInterpreter::new().execute_block(&mut context, &mut memory, &block);
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Undefined { .. })),
            "mutation {mutate}"
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.gpr[1], INITIAL_DST, "mutation {mutate}");
    }
}

#[test]
fn far_pointer_load_metadata_and_optimizer_preserve_atomic_partial_write() {
    for (bytes, expected_sources, width) in [
        (
            &[0x66, 0x0F, 0xB4, 0x08][..],
            vec![x86_gpr(0), x86_gpr(1)],
            OpWidth::W16,
        ),
        (&[0x0F, 0xB4, 0x08][..], vec![x86_gpr(0)], OpWidth::W32),
        (
            &[0x48, 0x0F, 0xB4, 0x08][..],
            vec![x86_gpr(0)],
            OpWidth::W64,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        let op = &lifted.ops[0];
        assert_eq!(op.kind.source_vregs(), expected_sources);
        assert_eq!(op.kind.dests(), vec![x86_gpr(1)]);
        assert!(op.kind.reads_memory());
        assert!(op.kind.writes_memory());
        assert!(op.kind.has_side_effects());
        assert!(op.is_jit_safe());

        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, op.kind.clone());
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            optimize_function(&mut function, level);
            assert!(matches!(
                function.blocks[0].ops.as_slice(),
                [SmirOp {
                    kind: OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
                        source: X86SystemSelectorSource::FarPointer {
                            offset_width,
                            ..
                        },
                        ..
                    }),
                    ..
                }] if *offset_width == width
            ));
        }
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
fn far_pointer_load_accessed_write_fault_does_not_commit_gpr_selector_or_cache() {
    let mut context = protected_context(POINTER, 1);
    let before = (INITIAL_DST, selector_state(&context, X86SystemSelector::Fs));
    let inner = memory_with_pointer(
        0x89AB_CDEF,
        0x10,
        OpWidth::W32,
        Some(data_descriptor(0x1234_5000, 0x2, 0, true, false)),
    );
    let mut memory = ReadOnlyMemory { inner };
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        &far_load_block(&[0x0F, 0xB4, 0x08]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[1], before.0);
    assert_eq!(selector_state(&context, X86SystemSelector::Fs), before.1);
}
