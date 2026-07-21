//! Strict lift, canonical interpretation, optimization, and metadata coverage
//! for long-mode `POP FS` (`0F A1`) and `POP GS` (`0F A9`).

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, X86SystemSegmentCache};
use crate::smir::ir::memory::{FlatMemory, MemoryError, SmirMemory};
use crate::smir::ir::ops::{X86SystemSelector, X86SystemSelectorLoadOp, X86SystemSelectorSource};
use crate::smir::optimize::{OptLevel, optimize_function};

const GDT: u64 = 0x2000;
const STACK: u64 = 0x4000;

fn exact_pop(result: &LiftResult) -> &X86SystemSelectorLoadOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86SystemSelectorLoad(load) => load,
        other => panic!("expected one selector-load stack op, got {other:?}"),
    }
}

fn pop_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict POP FS/GS lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn data_descriptor(base: u64, present: bool, accessed: bool) -> [u8; 8] {
    let type_ = if accessed { 0x3_u64 } else { 0x2 };
    (0xFFFF_u64
        | ((base & 0xFFFF) << 16)
        | (((base >> 16) & 0xFF) << 32)
        | (type_ << 40)
        | (1 << 44)
        | (u64::from(present) << 47)
        | (1 << 54)
        | (0xF << 48)
        | (1 << 55)
        | (((base >> 24) & 0xFF) << 56))
        .to_le_bytes()
}

fn protected_context(rsp: u64) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.efer = 1 << 10;
    x86.cs_l = true;
    x86.gdtr_base = GDT;
    x86.gdtr_limit = 0x1F;
    x86.gpr[4] = rsp;
    x86.fs_selector = 0x28;
    x86.fs_base = 0xAAAA_BBBB;
    x86.fs_cache = X86SystemSegmentCache {
        base: 0xAAAA_BBBB,
        present: true,
        ..X86SystemSegmentCache::default()
    };
    x86.gs_selector = 0x30;
    x86.gs_base = 0xCCCC_DDDD;
    x86.gs_cache = X86SystemSegmentCache {
        base: 0xCCCC_DDDD,
        present: true,
        ..X86SystemSegmentCache::default()
    };
    context
}

fn memory_with_stack(selector: u16, descriptor: Option<[u8; 8]>) -> FlatMemory {
    let mut memory = FlatMemory::with_base(GDT, 0x2100);
    memory.load((STACK - GDT) as usize, &u64::from(selector).to_le_bytes());
    if let Some(descriptor) = descriptor {
        memory.load(0x10, &descriptor);
    }
    memory
}

#[test]
fn pop_fs_gs_strictly_lift_exact_stack_width_frontier_and_rex_w_precedence() {
    for (bytes, selector, width) in [
        (&[0x0F, 0xA1][..], X86SystemSelector::Fs, MemWidth::B8),
        (&[0x66, 0x0F, 0xA9][..], X86SystemSelector::Gs, MemWidth::B2),
        (&[0x48, 0x0F, 0xA1][..], X86SystemSelector::Fs, MemWidth::B8),
        (
            &[0x66, 0x48, 0x0F, 0xA9][..],
            X86SystemSelector::Gs,
            MemWidth::B8,
        ),
        (
            &[0xF3, 0x67, 0x64, 0x0F, 0xA1][..],
            X86SystemSelector::Fs,
            MemWidth::B8,
        ),
        (
            &[0x66, 0x47, 0x0F, 0xA9][..],
            X86SystemSelector::Gs,
            MemWidth::B2,
        ),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            exact_pop(&result),
            X86SystemSelectorLoadOp {
                selector: got_selector,
                source: X86SystemSelectorSource::Stack {
                    stack_pointer,
                    width: got_width,
                },
                requires_apx: false,
                next_pc,
            } if *got_selector == selector
                && *stack_pointer == x86_gpr(4)
                && *got_width == width
                && *next_pc == 0x1000 + bytes.len() as u64
        ));
    }
}

#[test]
fn pop_fs_gs_rex2_map1_exhaustively_ignores_non_w_payload_and_requires_apx() {
    for payload in 0x80_u8..=0xFF {
        for (legacy_prefix, selector, opcode) in [
            (&[][..], X86SystemSelector::Fs, 0xA1),
            (&[0x66][..], X86SystemSelector::Gs, 0xA9),
        ] {
            let mut bytes = legacy_prefix.to_vec();
            bytes.extend_from_slice(&[0xD5, payload, opcode]);
            let result = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("REX2 payload {payload:#04x}: {error:?}"));
            let expected_width = if legacy_prefix.is_empty() || payload & 0x08 != 0 {
                MemWidth::B8
            } else {
                MemWidth::B2
            };
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(matches!(
                exact_pop(&result),
                X86SystemSelectorLoadOp {
                    selector: got_selector,
                    source: X86SystemSelectorSource::Stack {
                        stack_pointer,
                        width,
                    },
                    requires_apx: true,
                    next_pc,
                } if *got_selector == selector
                    && *stack_pointer == x86_gpr(4)
                    && *width == expected_width
                    && *next_pc == 0x1000 + bytes.len() as u64
            ));
        }
    }
}

#[test]
fn pop_fs_gs_reject_lock_and_invalid_rex2_order() {
    for bytes in [
        &[0xF0, 0x0F, 0xA1][..],
        &[0xF0, 0xD5, 0x80, 0xA9],
        &[0x48, 0xD5, 0x80, 0xA1],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn pop_fs_gs_interpreter_loads_cache_sets_accessed_then_commits_exact_width() {
    for (bytes, selector, width, base) in [
        (&[0x0F, 0xA1][..], X86SystemSelector::Fs, 8_u64, 0x1234_5000),
        (
            &[0x66, 0x0F, 0xA9][..],
            X86SystemSelector::Gs,
            2,
            0x2345_6000,
        ),
        (
            &[0x66, 0x48, 0x0F, 0xA1][..],
            X86SystemSelector::Fs,
            8,
            0x3456_7000,
        ),
    ] {
        let descriptor = data_descriptor(base, true, false);
        let mut context = protected_context(STACK);
        let initial_flags = 0x08D7;
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.rflags = initial_flags;
        let mut memory = memory_with_stack(0x10, Some(descriptor));
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &pop_block(bytes));
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));

        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        let (visible, observed_base, cache) = match selector {
            X86SystemSelector::Fs => (x86.fs_selector, x86.fs_base, &x86.fs_cache),
            X86SystemSelector::Gs => (x86.gs_selector, x86.gs_base, &x86.gs_cache),
            _ => unreachable!(),
        };
        assert_eq!(visible, 0x10, "{bytes:02X?}");
        assert_eq!(observed_base, base, "{bytes:02X?}");
        assert_eq!(cache.type_, 0x3, "{bytes:02X?}");
        assert_eq!(x86.gpr[4], STACK + width, "{bytes:02X?}");
        assert_eq!(x86.rflags, initial_flags, "{bytes:02X?}");
        let mut accessed = [0_u8; 8];
        memory.read(GDT + 0x10, &mut accessed).unwrap();
        assert_ne!(u64::from_le_bytes(accessed) & (1 << 40), 0);
    }
}

#[test]
fn pop_fs_gs_null_selector_commits_unusable_cache_and_rsp() {
    for (bytes, selector) in [
        (&[0x0F, 0xA1][..], X86SystemSelector::Fs),
        (&[0x66, 0x0F, 0xA9][..], X86SystemSelector::Gs),
    ] {
        let mut context = protected_context(STACK);
        let mut memory = memory_with_stack(3, None);
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &pop_block(bytes));
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        let (visible, cache) = match selector {
            X86SystemSelector::Fs => (x86.fs_selector, &x86.fs_cache),
            X86SystemSelector::Gs => (x86.gs_selector, &x86.gs_cache),
            _ => unreachable!(),
        };
        assert_eq!(visible, 3);
        assert!(cache.unusable);
        assert_eq!(x86.gpr[4], STACK + if bytes[0] == 0x66 { 2 } else { 8 });
    }
}

#[test]
fn pop_fs_gs_source_and_descriptor_faults_are_precise_and_noncommitting() {
    let baseline = protected_context(STACK);
    let ArchRegState::X86_64(before_x86) = &baseline.arch_regs else {
        unreachable!()
    };
    let before_fs = (
        before_x86.fs_selector,
        before_x86.fs_base,
        before_x86.fs_cache.clone(),
    );

    for (name, selector, descriptor, vector) in [
        ("selector beyond GDT limit", 0x20, None, 13_u8),
        (
            "not-present data descriptor",
            0x10,
            Some(data_descriptor(0, false, false)),
            11,
        ),
    ] {
        let mut context = protected_context(STACK);
        let mut memory = memory_with_stack(selector, descriptor);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            &pop_block(&[0x0F, 0xA1]),
        );
        match (vector, result) {
            (
                13,
                BlockResult::Exit(ExitReason::GeneralProtection {
                    addr: 0x1000,
                    error_code: 0x20,
                }),
            )
            | (
                11,
                BlockResult::Exit(ExitReason::SegmentNotPresent {
                    addr: 0x1000,
                    error_code: 0x10,
                }),
            ) => {}
            (_, other) => panic!("{name}: {other:?}"),
        }
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.gpr[4], STACK, "{name}");
        assert_eq!(
            (x86.fs_selector, x86.fs_base, x86.fs_cache.clone()),
            before_fs,
            "{name}"
        );
    }

    let mut context = protected_context(STACK);
    let mut memory = FlatMemory::with_base(GDT, (STACK - GDT + 2) as usize);
    memory.load((STACK - GDT) as usize, &0x10_u16.to_le_bytes());
    memory.load(0x10, &data_descriptor(0, true, false));
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &pop_block(&[0x0F, 0xA1]));
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[4], STACK);
    assert_eq!(
        (x86.fs_selector, x86.fs_base, x86.fs_cache.clone()),
        before_fs
    );
}

#[test]
fn pop_fs_gs_noncanonical_or_wrapping_range_raises_ss_before_memory() {
    for (name, bytes, rsp) in [
        (
            "B8 lower canonical boundary",
            &[0x0F, 0xA1][..],
            0x0000_7FFF_FFFF_FFFC_u64,
        ),
        (
            "B8 noncanonical upper gap",
            &[0x0F, 0xA1][..],
            0xFFFF_7FFF_FFFF_FFFF,
        ),
        ("B8 64-bit wrap", &[0x0F, 0xA1][..], u64::MAX - 3),
        (
            "B2 lower canonical boundary",
            &[0x66, 0x0F, 0xA1][..],
            0x0000_7FFF_FFFF_FFFF,
        ),
        (
            "B2 noncanonical upper gap",
            &[0x66, 0x0F, 0xA1][..],
            0xFFFF_7FFF_FFFF_FFFF,
        ),
        ("B2 64-bit wrap", &[0x66, 0x0F, 0xA1][..], u64::MAX),
    ] {
        let mut context = protected_context(rsp);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            &pop_block(bytes),
        );
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::StackSegment {
                    addr: 0x1000,
                    error_code: 0,
                })
            ),
            "{name}: {result:?}"
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.gpr[4], rsp, "{name}");
    }
}

#[test]
fn pop_fs_gs_apx_mode_and_shape_guards_are_noncommitting() {
    let mut context = protected_context(STACK);
    let mut memory = memory_with_stack(0, None);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        &pop_block(&[0xD5, 0x80, 0xA1]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));

    let mut legacy = protected_context(STACK);
    let ArchRegState::X86_64(x86) = &mut legacy.arch_regs else {
        unreachable!()
    };
    x86.cs_l = false;
    let result =
        SmirInterpreter::new().execute_block(&mut legacy, &mut memory, &pop_block(&[0x0F, 0xA1]));
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { .. })
    ));

    let mut malformed = pop_block(&[0x0F, 0xA1]);
    let OpKind::X86SystemSelectorLoad(load) = &mut malformed.ops[0].kind else {
        unreachable!()
    };
    load.selector = X86SystemSelector::Ds;
    let result = SmirInterpreter::new().execute_block(&mut context, &mut memory, &malformed);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[4], STACK);

    let mut malformed_frontier = pop_block(&[0x66, 0xD5, 0x80, 0xA1]);
    let OpKind::X86SystemSelectorLoad(load) = &mut malformed_frontier.ops[0].kind else {
        unreachable!()
    };
    load.next_pc = 0x1003;
    let mut malformed_context = protected_context(STACK);
    let result = SmirInterpreter::new().execute_block(
        &mut malformed_context,
        &mut memory,
        &malformed_frontier,
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { .. })
    ));
    let ArchRegState::X86_64(x86) = &malformed_context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[4], STACK);
}

#[test]
fn pop_fs_gs_metadata_and_optimizer_preserve_atomic_stack_effect() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for (bytes, selector, width, requires_apx) in [
            (
                &[0x0F, 0xA1][..],
                X86SystemSelector::Fs,
                MemWidth::B8,
                false,
            ),
            (
                &[0x66, 0x0F, 0xA9][..],
                X86SystemSelector::Gs,
                MemWidth::B2,
                false,
            ),
            (
                &[0x66, 0xD5, 0x88, 0xA9][..],
                X86SystemSelector::Gs,
                MemWidth::B8,
                true,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            let kind = &lifted.ops[0].kind;
            assert_eq!(kind.source_vregs(), vec![x86_gpr(4)]);
            assert_eq!(kind.dests(), vec![x86_gpr(4)]);
            assert!(kind.reads_memory());
            assert!(kind.writes_memory());
            assert!(kind.has_side_effects());
            assert!(lifted.ops[0].is_jit_safe());

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind.clone());
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            optimize_function(&mut function, level);
            assert!(matches!(
                function.blocks[0].ops.as_slice(),
                [SmirOp {
                    kind: OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
                        selector: got_selector,
                        source: X86SystemSelectorSource::Stack {
                            stack_pointer,
                            width: got_width,
                        },
                        requires_apx: got_apx,
                        ..
                    }),
                    ..
                }] if *got_selector == selector
                    && *stack_pointer == x86_gpr(4)
                    && *got_width == width
                    && *got_apx == requires_apx
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
fn pop_fs_gs_accessed_bit_write_fault_does_not_commit_selector_cache_or_rsp() {
    let mut context = protected_context(STACK);
    let ArchRegState::X86_64(before) = &context.arch_regs else {
        unreachable!()
    };
    let before = (
        before.gpr[4],
        before.fs_selector,
        before.fs_base,
        before.fs_cache.clone(),
    );
    let inner = memory_with_stack(0x10, Some(data_descriptor(0x1234_5000, true, false)));
    let mut memory = ReadOnlyMemory { inner };
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &pop_block(&[0x0F, 0xA1]));
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(
        (
            x86.gpr[4],
            x86.fs_selector,
            x86.fs_base,
            x86.fs_cache.clone(),
        ),
        before
    );
}
