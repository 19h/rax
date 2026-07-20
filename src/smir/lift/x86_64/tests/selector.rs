//! Strict lift, metadata, optimizer, and interpreter coverage for
//! SLDT/STR/LLDT/LTR.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, MemoryError, SmirMemory};
use crate::smir::ir::ops::{
    X86SystemSelector, X86SystemSelectorLoadOp, X86SystemSelectorSource, X86SystemSelectorStoreOp,
    X86SystemSelectorTarget,
};
use crate::smir::optimize::{OptLevel, optimize_function, redundant_load_elimination};

fn exact_selector(result: &LiftResult) -> &X86SystemSelectorStoreOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86SystemSelectorStore(store) => store,
        other => panic!("expected one exact X86SystemSelectorStore op, got {other:?}"),
    }
}

fn exact_selector_load(result: &LiftResult) -> &X86SystemSelectorLoadOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86SystemSelectorLoad(load) => load,
        other => panic!("expected one exact X86SystemSelectorLoad op, got {other:?}"),
    }
}

fn selector_block(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict selector lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn execute_register(
    bytes: &[u8],
    configure: impl FnOnce(&mut SmirContext),
) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    configure(&mut context);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &selector_block(bytes),
    );
    (result, context)
}

fn ldt_descriptor(
    base: u64,
    raw_limit: u32,
    dpl: u8,
    present: bool,
    granularity: bool,
) -> [u8; 16] {
    assert!(raw_limit <= 0xF_FFFF);
    let mut low = u64::from(raw_limit & 0xFFFF)
        | ((base & 0xFFFF) << 16)
        | (((base >> 16) & 0xFF) << 32)
        | (0x2_u64 << 40)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (u64::from((raw_limit >> 16) & 0xF) << 48)
        | (((base >> 24) & 0xFF) << 56);
    if granularity {
        low |= 1 << 55;
    }
    let high = (base >> 32) & 0xFFFF_FFFF;
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&low.to_le_bytes());
    bytes[8..].copy_from_slice(&high.to_le_bytes());
    bytes
}

fn tss_descriptor(
    base: u64,
    raw_limit: u32,
    dpl: u8,
    present: bool,
    type_: u8,
    granularity: bool,
) -> [u8; 16] {
    assert!(raw_limit <= 0xF_FFFF);
    let mut low = u64::from(raw_limit & 0xFFFF)
        | ((base & 0xFFFF) << 16)
        | (((base >> 16) & 0xFF) << 32)
        | (u64::from(type_ & 0xF) << 40)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (u64::from((raw_limit >> 16) & 0xF) << 48)
        | (((base >> 24) & 0xFF) << 56);
    if granularity {
        low |= 1 << 55;
    }
    let high = (base >> 32) & 0xFFFF_FFFF;
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&low.to_le_bytes());
    bytes[8..].copy_from_slice(&high.to_le_bytes());
    bytes
}

fn execute_lldt(
    bytes: &[u8],
    configure: impl FnOnce(&mut SmirContext, &mut FlatMemory),
) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cs_l = true;
    let mut memory = FlatMemory::with_base(0x2000, 0x100);
    configure(&mut context, &mut memory);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &selector_block(bytes));
    (result, context)
}

fn execute_ltr(
    bytes: &[u8],
    configure: impl FnOnce(&mut SmirContext, &mut FlatMemory),
) -> (BlockResult, SmirContext, FlatMemory) {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cs_l = true;
    x86.efer = 1 << 10;
    let mut memory = FlatMemory::with_base(0x2000, 0x100);
    configure(&mut context, &mut memory);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &selector_block(bytes));
    (result, context, memory)
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
fn selector_stores_strictly_lift_both_selectors_register_widths_and_rex_extensions() {
    for (bytes, selector, dst, width, requires_apx) in [
        (
            &[0x0F, 0x00, 0xC0][..],
            X86SystemSelector::Ldtr,
            0,
            OpWidth::W32,
            false,
        ),
        (
            &[0x66, 0x0F, 0x00, 0xCD],
            X86SystemSelector::Tr,
            5,
            OpWidth::W16,
            false,
        ),
        (
            &[0x4D, 0x0F, 0x00, 0xCF],
            X86SystemSelector::Tr,
            15,
            OpWidth::W64,
            false,
        ),
        (
            &[0xD5, 0x91, 0x00, 0xC7],
            X86SystemSelector::Ldtr,
            31,
            OpWidth::W32,
            true,
        ),
        (
            &[0xD5, 0x99, 0x00, 0xC8],
            X86SystemSelector::Tr,
            24,
            OpWidth::W64,
            true,
        ),
    ] {
        let result = lift_single(bytes).expect("SLDT/STR register form must strictly lift");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            exact_selector(&result),
            X86SystemSelectorStoreOp {
                selector: got_selector,
                target: X86SystemSelectorTarget::Register {
                    dst: got_dst,
                    width: got_width,
                },
                requires_apx: got_apx,
            } if *got_selector == selector
                && *got_dst == x86_gpr(dst)
                && *got_width == width
                && *got_apx == requires_apx
        ));
    }
}

#[test]
fn selector_stores_lift_fixed_two_byte_memory_addresses_and_apx_components() {
    let direct = lift_single(&[0x0F, 0x00, 0x08]).unwrap();
    assert!(matches!(
        exact_selector(&direct),
        X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Tr,
            target: X86SystemSelectorTarget::Memory {
                addr: Address::Direct(base),
            },
            requires_apx: false,
        } if *base == x86_gpr(0)
    ));

    let sib = lift_single(&[0x48, 0x0F, 0x00, 0x44, 0x88, 0x7F]).unwrap();
    assert!(matches!(
        &exact_selector(&sib).target,
        X86SystemSelectorTarget::Memory {
            addr: Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x7F,
                disp_size: DispSize::Disp8,
            }
        } if *base == x86_gpr(0) && *index == x86_gpr(1)
    ));

    let addr32 = lift_single(&[0x67, 0x0F, 0x00, 0x8C, 0x8D, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert!(matches!(
        &exact_selector(&addr32).target,
        X86SystemSelectorTarget::Memory {
            addr: Address::X86Addr32(inner),
        } if matches!(
            inner.as_ref(),
            Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x1234_5678,
                disp_size: DispSize::Disp32,
            } if *base == x86_gpr(5) && *index == x86_gpr(1)
        )
    ));

    let apx = lift_single(&[0xD5, 0xB3, 0x00, 0x0C, 0xD1]).unwrap();
    assert!(matches!(
        exact_selector(&apx),
        X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Tr,
            target: X86SystemSelectorTarget::Memory {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 8,
                    ..
                },
            },
            requires_apx: true,
        } if *base == x86_gpr(25) && *index == x86_gpr(26)
    ));
}

#[test]
fn selector_stores_honor_prefixes_reject_lock_and_leave_other_group6_unsupported() {
    let rex_w_wins = lift_single(&[0x66, 0x48, 0x0F, 0x00, 0xC0]).unwrap();
    assert!(matches!(
        exact_selector(&rex_w_wins).target,
        X86SystemSelectorTarget::Register {
            width: OpWidth::W64,
            ..
        }
    ));
    for prefix in [0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x40, 0xF2, 0xF3] {
        let bytes = [prefix, 0x0F, 0x00, 0xC8];
        assert_eq!(lift_single(&bytes).unwrap().bytes_consumed, bytes.len());
    }
    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x00, 0xC0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
    for modrm in [0xE0, 0xE8] {
        assert!(matches!(
            lift_single(&[0x0F, 0x00, modrm]),
            Err(LiftError::Unsupported { .. })
        ));
    }
}

#[test]
fn selector_loads_strictly_lift_fixed_width_register_sources_and_apx() {
    for (bytes, selector, src, requires_apx) in [
        (&[0x0F, 0x00, 0xD0][..], X86SystemSelector::Ldtr, 0, false),
        (&[0x66, 0x0F, 0x00, 0xD5], X86SystemSelector::Ldtr, 5, false),
        (&[0x4D, 0x0F, 0x00, 0xDF], X86SystemSelector::Tr, 15, false),
        (&[0xD5, 0x91, 0x00, 0xDF], X86SystemSelector::Tr, 31, true),
    ] {
        let result = lift_single(bytes).expect("LLDT/LTR register form must strictly lift");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            exact_selector_load(&result),
            X86SystemSelectorLoadOp {
                selector: got_selector,
                source: X86SystemSelectorSource::Register { src: got_src },
                requires_apx: got_apx,
                next_pc,
            } if *got_selector == selector
                && *got_src == x86_gpr(src)
                && *got_apx == requires_apx
                && *next_pc == 0x1000 + bytes.len() as u64
        ));
    }
}

#[test]
fn selector_loads_lift_fixed_two_byte_memory_addresses_and_apx_components() {
    let direct = lift_single(&[0x0F, 0x00, 0x10]).unwrap();
    assert!(matches!(
        exact_selector_load(&direct),
        X86SystemSelectorLoadOp {
            source: X86SystemSelectorSource::Memory {
                addr: Address::Direct(base),
            },
            requires_apx: false,
            ..
        } if *base == x86_gpr(0)
    ));

    let sib = lift_single(&[0x48, 0x0F, 0x00, 0x54, 0x88, 0x7F]).unwrap();
    assert!(matches!(
        &exact_selector_load(&sib).source,
        X86SystemSelectorSource::Memory {
            addr: Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x7F,
                disp_size: DispSize::Disp8,
            }
        } if *base == x86_gpr(0) && *index == x86_gpr(1)
    ));

    let addr32 = lift_single(&[0x67, 0x0F, 0x00, 0x94, 0x8D, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert!(matches!(
        &exact_selector_load(&addr32).source,
        X86SystemSelectorSource::Memory {
            addr: Address::X86Addr32(inner),
        } if matches!(
            inner.as_ref(),
            Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x1234_5678,
                disp_size: DispSize::Disp32,
            } if *base == x86_gpr(5) && *index == x86_gpr(1)
        )
    ));

    let apx = lift_single(&[0xD5, 0xB3, 0x00, 0x14, 0xD1]).unwrap();
    assert!(matches!(
        exact_selector_load(&apx),
        X86SystemSelectorLoadOp {
            source: X86SystemSelectorSource::Memory {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 8,
                    ..
                },
            },
            requires_apx: true,
            ..
        } if *base == x86_gpr(25) && *index == x86_gpr(26)
    ));

    let ltr = lift_single(&[0x67, 0x0F, 0x00, 0x1C, 0x25, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert!(matches!(
        exact_selector_load(&ltr),
        X86SystemSelectorLoadOp {
            selector: X86SystemSelector::Tr,
            source: X86SystemSelectorSource::Memory {
                addr: Address::X86Addr32(inner),
            },
            ..
        } if matches!(inner.as_ref(), Address::Absolute(0x1234_5678))
    ));
}

#[test]
fn selector_loads_ignore_operand_size_prefixes_reject_lock_and_expose_effects() {
    for (bytes, selector) in [
        (&[0x0F, 0x00, 0xD0][..], X86SystemSelector::Ldtr),
        (&[0x66, 0x0F, 0x00, 0xD8], X86SystemSelector::Tr),
        (&[0x48, 0x0F, 0x00, 0xD0], X86SystemSelector::Ldtr),
        (&[0xF2, 0x0F, 0x00, 0xD8], X86SystemSelector::Tr),
        (&[0xF3, 0x0F, 0x00, 0xD0], X86SystemSelector::Ldtr),
    ] {
        let lifted = lift_single(bytes).unwrap();
        let load = exact_selector_load(&lifted);
        assert_eq!(load.selector, selector);
        assert!(
            matches!(load.source, X86SystemSelectorSource::Register { src } if src == x86_gpr(0))
        );
    }
    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x00, 0xD0]),
        Err(LiftError::InvalidEncoding { .. })
    ));

    let register = &lift_single(&[0x0F, 0x00, 0xD5]).unwrap().ops[0];
    assert_eq!(register.kind.source_vregs(), vec![x86_gpr(5)]);
    assert!(register.kind.dests().is_empty());
    assert!(register.kind.has_side_effects());
    assert!(register.kind.reads_memory());
    assert!(!register.kind.writes_memory());
    assert!(register.is_jit_safe());

    let memory = &lift_single(&[0x0F, 0x00, 0x54, 0x48, 0x08]).unwrap().ops[0];
    assert_eq!(memory.kind.source_vregs(), vec![x86_gpr(1), x86_gpr(0)]);
    assert!(memory.kind.dests().is_empty());
    assert!(memory.kind.has_side_effects());
    assert!(memory.kind.reads_memory());
    assert!(!memory.kind.writes_memory());
    assert!(memory.is_jit_safe());

    let ltr = &lift_single(&[0x0F, 0x00, 0xD8]).unwrap().ops[0];
    assert!(ltr.kind.reads_memory());
    assert!(ltr.kind.writes_memory());
    assert!(ltr.kind.has_side_effects());
}

#[test]
fn ltr_implicit_busy_write_invalidates_proven_load_forwarding() {
    let build = |selector| {
        let first = VReg::virt(0);
        let second = VReg::virt(1);
        let addr = Address::Direct(x86_gpr(3));
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: first,
                addr: addr.clone(),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1003,
            OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
                selector,
                source: X86SystemSelectorSource::Register { src: x86_gpr(0) },
                requires_apx: false,
                next_pc: 0x1006,
            }),
        );
        builder.push_op(
            0x1006,
            OpKind::Load {
                dst: second,
                addr,
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        builder.set_terminator(Terminator::Return {
            values: vec![first, second],
        });
        let mut function = builder.finish();
        function.attrs.allow_redundant_load_elimination = true;
        function
    };

    let mut lldt = build(X86SystemSelector::Ldtr);
    assert_eq!(redundant_load_elimination(&mut lldt), 1);
    assert!(matches!(
        lldt.entry_block().unwrap().ops[2].kind,
        OpKind::Mov { .. }
    ));

    let mut ltr = build(X86SystemSelector::Tr);
    assert_eq!(redundant_load_elimination(&mut ltr), 0);
    assert!(matches!(
        ltr.entry_block().unwrap().ops[2].kind,
        OpKind::Load { .. }
    ));
}

#[test]
fn lldt_interpreter_loads_visible_selector_complete_hidden_cache_and_preserves_flags() {
    let base = 0xFFFF_8000_1234_5000;
    let raw_limit = 0xA_BCDE;
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
    let (result, context) = execute_lldt(&[0x0F, 0x00, 0xD0], |context, memory| {
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 1;
        x86.cpl = 0;
        x86.gdtr_base = 0x2000;
        x86.gdtr_limit = 0x1F;
        context.flags.materialized = flags;
        context.write_vreg(x86_gpr(0), 0x13);
        memory.load(0x10, &ldt_descriptor(base, raw_limit, 3, true, true));
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.ldtr_selector, 0x13);
    assert_eq!(x86.ldtr_cache.base, base);
    assert_eq!(x86.ldtr_cache.limit, (raw_limit << 12) | 0xFFF);
    assert_eq!(x86.ldtr_cache.type_, 0x2);
    assert!(x86.ldtr_cache.present);
    assert_eq!(x86.ldtr_cache.dpl, 3);
    assert!(x86.ldtr_cache.g);
    assert!(!x86.ldtr_cache.s);
    assert!(!x86.ldtr_cache.unusable);
    assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
}

#[test]
fn lldt_interpreter_compatibility_mode_reads_only_the_legacy_descriptor() {
    let (result, context) = execute_lldt(&[0x0F, 0x00, 0xD0], |context, memory| {
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cs_l = false;
        x86.cr0 = 1;
        x86.cpl = 0;
        x86.gdtr_base = 0x2000;
        x86.gdtr_limit = 0x17;
        context.write_vreg(x86_gpr(0), 0x10);
        let descriptor = ldt_descriptor(0xDEAD_BEEF, 0xABCDE, 2, true, true);
        memory.load(0x10, &descriptor[..8]);
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.ldtr_selector, 0x10);
    assert_eq!(x86.ldtr_cache.base, 0xDEAD_BEEF);
    assert_eq!(x86.ldtr_cache.limit, 0xABCDEFFF);
    assert_eq!(x86.ldtr_cache.dpl, 2);
    assert!(!x86.ldtr_cache.unusable);
}

#[test]
fn lldt_interpreter_null_selectors_invalidate_without_descriptor_access() {
    for selector in 0_u64..=3 {
        let (result, context) = execute_lldt(&[0x0F, 0x00, 0xD0], |context, _| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = 1;
            x86.cpl = 0;
            x86.gdtr_base = u64::MAX;
            x86.gdtr_limit = 0;
            x86.ldtr_selector = 0x1234;
            x86.ldtr_cache.base = 0xDEAD_BEEF;
            context.write_vreg(x86_gpr(0), selector);
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(u64::from(x86.ldtr_selector), selector);
        assert!(x86.ldtr_cache.unusable);
        assert_eq!(x86.ldtr_cache.base, 0);
        assert!(!x86.ldtr_cache.present);
    }
}

#[test]
fn lldt_interpreter_faults_are_ordered_and_noncommitting() {
    for (name, selector, limit, descriptor, expected_np) in [
        (
            "TI",
            0x14,
            0x1F,
            ldt_descriptor(0, 0, 0, true, false),
            false,
        ),
        (
            "limit",
            0x10,
            0x1E,
            ldt_descriptor(0, 0, 0, true, false),
            false,
        ),
        (
            "wrong type",
            0x10,
            0x1F,
            {
                let mut value = ldt_descriptor(0, 0, 0, true, false);
                value[5] = (value[5] & 0xF0) | 0x9;
                value
            },
            false,
        ),
        (
            "not present",
            0x10,
            0x1F,
            ldt_descriptor(0, 0, 0, false, false),
            true,
        ),
        (
            "noncanonical base",
            0x10,
            0x1F,
            ldt_descriptor(0x0000_8000_0000_0000, 0, 0, true, false),
            false,
        ),
        (
            "reserved high",
            0x10,
            0x1F,
            {
                let mut value = ldt_descriptor(0, 0, 0, true, false);
                value[12] = 1;
                value
            },
            false,
        ),
    ] {
        let (result, context) = execute_lldt(&[0x0F, 0x00, 0xD0], |context, memory| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = 1;
            x86.cpl = 0;
            x86.gdtr_base = 0x2000;
            x86.gdtr_limit = limit;
            x86.ldtr_selector = 0x2468;
            x86.ldtr_cache.base = 0xDEAD_BEEF;
            context.write_vreg(x86_gpr(0), selector);
            memory.load(0x10, &descriptor);
        });
        if expected_np {
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::SegmentNotPresent {
                        error_code: 0x10,
                        ..
                    })
                ),
                "{name}: {result:?}"
            );
        } else {
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::GeneralProtection { error_code, .. })
                        if error_code == (selector & 0xFFFC) as u32
                ),
                "{name}: {result:?}"
            );
        }
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.ldtr_selector, 0x2468, "{name}");
        assert_eq!(x86.ldtr_cache.base, 0xDEAD_BEEF, "{name}");
    }

    let mut wrong_type = ldt_descriptor(0, 0, 0, true, false);
    wrong_type[5] = (wrong_type[5] & 0xF0) | 0x9;
    let (result, context) = execute_lldt(&[0x0F, 0x00, 0xD0], |context, memory| {
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 1;
        x86.cpl = 0;
        x86.gdtr_base = 0x20E8;
        x86.gdtr_limit = 0x1F;
        x86.ldtr_selector = 0x2468;
        x86.ldtr_cache.base = 0xDEAD_BEEF;
        context.write_vreg(x86_gpr(0), 0x10);
        memory.load(0xF8, &wrong_type[..8]);
    });
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.ldtr_selector, 0x2468);
    assert_eq!(x86.ldtr_cache.base, 0xDEAD_BEEF);
}

#[test]
fn lldt_interpreter_apx_mode_privilege_then_memory_fault_priority_is_precise() {
    let bytes = [0xD5, 0x91, 0x00, 0x17]; // LLDT word ptr [R31]
    for (name, apx, cr0, rflags, cpl, expected_undefined) in [
        ("APX", false, 0, 1 << 17, 3, true),
        ("real mode", true, 0, 0, 3, true),
        ("VM86", true, 1, 1 << 17, 3, true),
        ("CPL", true, 1, 0, 3, false),
    ] {
        let (result, context) = execute_lldt(&bytes, |context, _| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.apx_enabled = apx;
            x86.cr0 = cr0;
            x86.rflags = rflags;
            x86.cpl = cpl;
            x86.ldtr_selector = 0x2468;
            context.write_vreg(x86_gpr(31), 0x3000);
        });
        if expected_undefined {
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Undefined { .. })),
                "{name}: {result:?}"
            );
        } else {
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
                ),
                "{name}: {result:?}"
            );
        }
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.ldtr_selector, 0x2468, "{name}");
    }

    let (result, _) = execute_lldt(&[0x0F, 0x00, 0x10], |context, _| {
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 1;
        x86.cpl = 0;
        context.write_vreg(x86_gpr(0), 0x3000);
    });
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
}

#[test]
fn ltr_interpreter_marks_descriptor_busy_loads_complete_cache_and_preserves_state() {
    let base = 0xFFFF_8000_1234_5000;
    let raw_limit = 0xA_BCDE;
    let descriptor = tss_descriptor(base, raw_limit, 3, true, 0x9, true);
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
    let (result, context, mut memory) = execute_ltr(&[0x0F, 0x00, 0xD8], |context, memory| {
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 1;
        x86.cpl = 0;
        x86.gdtr_base = 0x2000;
        x86.gdtr_limit = 0x1F;
        x86.tr_selector = 0x2468;
        x86.tr_type = 0x3;
        x86.tr_cache.base = 0xDEAD_BEEF;
        context.flags.materialized = flags;
        context.write_vreg(x86_gpr(0), 0xA5A5_5A5A_0000_0013);
        memory.load(0x10, &descriptor);
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.tr_selector, 0x13);
    assert_eq!(x86.tr_cache.base, base);
    assert_eq!(x86.tr_cache.limit, (raw_limit << 12) | 0xFFF);
    assert_eq!(x86.tr_cache.type_, 0xB);
    assert_eq!(x86.tr_type, 0xB);
    assert!(x86.tr_cache.present);
    assert_eq!(x86.tr_cache.dpl, 3);
    assert!(x86.tr_cache.g);
    assert!(!x86.tr_cache.s);
    assert!(!x86.tr_cache.unusable);
    assert_eq!(context.read_vreg(x86_gpr(0)), 0xA5A5_5A5A_0000_0013);
    assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());

    let mut observed = [0_u8; 16];
    memory.read(0x2010, &mut observed).unwrap();
    assert_eq!(observed[5] & 0x0F, 0xB);
    assert_eq!(&observed[8..], &descriptor[8..]);
}

#[test]
fn ltr_interpreter_busy_store_fault_leaves_descriptor_and_tr_uncommitted() {
    let descriptor = tss_descriptor(0x1234_5000, 0x67, 0, true, 0x9, false);
    let mut inner = FlatMemory::with_base(0x2000, 0x100);
    inner.load(0x10, &descriptor);
    let mut memory = ReadOnlyMemory { inner };
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cs_l = true;
    x86.efer = 1 << 10;
    x86.cr0 = 1;
    x86.cpl = 0;
    x86.gdtr_base = 0x2000;
    x86.gdtr_limit = 0x1F;
    x86.tr_selector = 0x2468;
    x86.tr_type = 0x3;
    x86.tr_cache.base = 0xDEAD_BEEF;
    context.write_vreg(x86_gpr(0), 0x10);

    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        &selector_block(&[0x0F, 0x00, 0xD8]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.tr_selector, 0x2468);
    assert_eq!(x86.tr_type, 0x3);
    assert_eq!(x86.tr_cache.base, 0xDEAD_BEEF);
    let mut observed = [0_u8; 16];
    memory.read(0x2010, &mut observed).unwrap();
    assert_eq!(observed, descriptor);
}

#[test]
fn ltr_interpreter_fault_matrix_is_ordered_and_noncommitting() {
    let mut wrong_type = tss_descriptor(0, 0x67, 0, true, 0x2, false);
    let busy = tss_descriptor(0, 0x67, 0, true, 0xB, false);
    let not_present = tss_descriptor(0, 0x67, 0, false, 0x9, false);
    let noncanonical = tss_descriptor(0x0000_8000_0000_0000, 0x67, 0, true, 0x9, false);
    let mut reserved_high = tss_descriptor(0, 0x67, 0, true, 0x9, false);
    reserved_high[12] = 1;
    let mut reserved_low = tss_descriptor(0, 0x67, 0, true, 0x9, false);
    reserved_low[6] |= 1 << 5;
    let mut absent_reserved = reserved_high;
    absent_reserved[5] &= !(1 << 7);
    // Keep this mutation explicit: S=1 with type 2 is a code/data descriptor,
    // not an LDT system descriptor that could accidentally satisfy LTR.
    wrong_type[5] |= 1 << 4;

    for (name, selector, limit, descriptor, expected_np) in [
        (
            "null",
            0x0003_u16,
            0x1F,
            tss_descriptor(0, 0x67, 0, true, 0x9, false),
            false,
        ),
        (
            "TI",
            0x0014,
            0x1F,
            tss_descriptor(0, 0x67, 0, true, 0x9, false),
            false,
        ),
        (
            "limit",
            0x0010,
            0x1E,
            tss_descriptor(0, 0x67, 0, true, 0x9, false),
            false,
        ),
        ("wrong type", 0x0010, 0x1F, wrong_type, false),
        ("busy", 0x0010, 0x1F, busy, false),
        ("not present", 0x0010, 0x1F, not_present, true),
        ("noncanonical", 0x0010, 0x1F, noncanonical, false),
        ("reserved high", 0x0010, 0x1F, reserved_high, false),
        ("reserved low", 0x0010, 0x1F, reserved_low, false),
        (
            "reserved precedes presence",
            0x0010,
            0x1F,
            absent_reserved,
            false,
        ),
    ] {
        let original = descriptor;
        let (result, context, mut memory) = execute_ltr(&[0x0F, 0x00, 0xD8], |context, memory| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = 1;
            x86.cpl = 0;
            x86.gdtr_base = 0x2000;
            x86.gdtr_limit = limit;
            x86.tr_selector = 0x2468;
            x86.tr_type = 0x3;
            x86.tr_cache.base = 0xDEAD_BEEF;
            context.write_vreg(x86_gpr(0), u64::from(selector));
            memory.load(0x10, &descriptor);
        });
        if expected_np {
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::SegmentNotPresent {
                        error_code: 0x10,
                        ..
                    })
                ),
                "{name}: {result:?}"
            );
        } else {
            let expected_error = u32::from(selector & 0xFFFC);
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::GeneralProtection { error_code, .. })
                        if error_code == expected_error
                ),
                "{name}: {result:?}"
            );
        }
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.tr_selector, 0x2468, "{name}");
        assert_eq!(x86.tr_type, 0x3, "{name}");
        assert_eq!(x86.tr_cache.base, 0xDEAD_BEEF, "{name}");
        let mut observed = [0_u8; 16];
        memory.read(0x2010, &mut observed).unwrap();
        assert_eq!(observed, original, "{name}");
    }
}

#[test]
fn ltr_interpreter_compatibility_and_legacy_tss_types_are_exact() {
    for (name, ia32e_active, type_, expected_busy) in [
        ("compatibility 32-bit TSS", true, 0x9, 0xB),
        ("legacy 16-bit TSS", false, 0x1, 0x3),
        ("legacy 32-bit TSS", false, 0x9, 0xB),
    ] {
        let descriptor = tss_descriptor(0xDEAD_BEEF, 0xABCDE, 2, true, type_, true);
        let (result, context, mut memory) = execute_ltr(&[0x0F, 0x00, 0xD8], |context, memory| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cs_l = false;
            x86.efer = u64::from(ia32e_active) << 10;
            x86.cr0 = 1;
            x86.cpl = 0;
            x86.gdtr_base = 0x2000;
            x86.gdtr_limit = 0x17;
            context.write_vreg(x86_gpr(0), 0x10);
            memory.load(0x10, &descriptor[..8]);
        });
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{name}: {result:?}"
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.tr_selector, 0x10, "{name}");
        assert_eq!(x86.tr_cache.base, 0xDEAD_BEEF, "{name}");
        assert_eq!(x86.tr_cache.limit, 0xABCDEFFF, "{name}");
        assert_eq!(x86.tr_type, expected_busy, "{name}");
        let mut low = [0_u8; 8];
        memory.read(0x2010, &mut low).unwrap();
        assert_eq!(low[5] & 0x0F, expected_busy, "{name}");
    }

    let descriptor = tss_descriptor(0, 0x67, 0, true, 0x1, false);
    let (result, context, mut memory) = execute_ltr(&[0x0F, 0x00, 0xD8], |context, memory| {
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cs_l = false;
        x86.efer = 1 << 10;
        x86.cr0 = 1;
        x86.cpl = 0;
        x86.gdtr_base = 0x2000;
        x86.gdtr_limit = 0x17;
        x86.tr_selector = 0x2468;
        context.write_vreg(x86_gpr(0), 0x10);
        memory.load(0x10, &descriptor[..8]);
    });
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::GeneralProtection {
            error_code: 0x10,
            ..
        })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.tr_selector, 0x2468);
    let mut low = [0_u8; 8];
    memory.read(0x2010, &mut low).unwrap();
    assert_eq!(low, descriptor[..8]);
}

#[test]
fn selector_store_metadata_exposes_register_and_faulting_memory_effects() {
    let register = &lift_single(&[0x66, 0x0F, 0x00, 0xCD]).unwrap().ops[0];
    assert!(register.kind.source_vregs().is_empty());
    assert_eq!(register.kind.dests(), vec![x86_gpr(5)]);
    assert!(register.kind.flags_read().is_empty());
    assert!(register.kind.flags_written().is_empty());
    assert!(register.kind.has_side_effects());
    assert!(!register.kind.reads_memory());
    assert!(!register.kind.writes_memory());
    assert!(register.is_jit_safe());

    let memory = &lift_single(&[0x0F, 0x00, 0x4C, 0x48, 0x08]).unwrap().ops[0];
    assert_eq!(memory.kind.source_vregs(), vec![x86_gpr(1), x86_gpr(0)]);
    assert!(memory.kind.dests().is_empty());
    assert!(memory.kind.has_side_effects());
    assert!(!memory.kind.reads_memory());
    assert!(memory.kind.writes_memory());
    assert!(memory.is_jit_safe());
}

#[test]
fn selector_store_interpreter_commits_exact_widths_selectors_and_preserves_flags() {
    let incoming = 0xA5A5_5A5A_DEAD_BEEF;
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
    for (bytes, dst, selector, expected, apx) in [
        (
            &[0x66, 0x0F, 0x00, 0xC3][..],
            3,
            0x1357_u16,
            (incoming & !0xFFFF) | 0x1357,
            false,
        ),
        (&[0x0F, 0x00, 0xCB], 3, 0x2468, 0x2468, false),
        (&[0x48, 0x0F, 0x00, 0xCB], 3, 0x2468, 0x2468, false),
        (&[0xD5, 0x91, 0x00, 0xC7], 31, 0xBEEF, 0xBEEF, true),
    ] {
        let (result, context) = execute_register(bytes, |context| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = 1;
            x86.ldtr_selector = selector;
            x86.tr_selector = selector;
            x86.apx_enabled = apx;
            context.flags.materialized = flags;
            context.write_vreg(x86_gpr(dst), incoming);
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(context.read_vreg(x86_gpr(dst)), expected, "{bytes:02X?}");
        assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
        assert!(context.flags.lazy.is_none());
    }
}

#[test]
fn selector_store_interpreter_fault_order_is_apx_mode_umip_then_memory() {
    let bytes = [0xD5, 0x91, 0x00, 0xC7];
    let sentinel = 0x3131_3131_3131_3131;
    for (name, apx, cr0, rflags, cr4, cpl, expected_undefined) in [
        ("APX", false, 0, 1 << 17, 1 << 11, 3, true),
        ("real mode", true, 0, 0, 1 << 11, 3, true),
        ("VM86", true, 1, 1 << 17, 1 << 11, 3, true),
        ("UMIP", true, 1, 0, 1 << 11, 3, false),
    ] {
        let (fault, context) = execute_register(&bytes, |context| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.apx_enabled = apx;
            x86.cr0 = cr0;
            x86.rflags = rflags;
            x86.cr4 = cr4;
            x86.cpl = cpl;
            x86.ldtr_selector = 0x1234;
            context.write_vreg(x86_gpr(31), sentinel);
        });
        if expected_undefined {
            assert!(
                matches!(fault, BlockResult::Exit(ExitReason::Undefined { .. })),
                "{name}"
            );
        } else {
            assert!(
                matches!(
                    fault,
                    BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
                ),
                "{name}"
            );
        }
        assert_eq!(context.read_vreg(x86_gpr(31)), sentinel, "{name}");
    }

    let block = selector_block(&[0x0F, 0x00, 0x08]);
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.tr_selector = 0xBEEF;
    context.write_vreg(x86_gpr(0), 0x2001);
    let mut memory = FlatMemory::with_base(0x2000, 4);
    memory.load(0, &[0xA5; 4]);
    let result = SmirInterpreter::new().execute_block(&mut context, &mut memory, &block);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let mut observed = [0; 4];
    memory.read(0x2000, &mut observed).unwrap();
    assert_eq!(observed, [0xA5, 0xEF, 0xBE, 0xA5]);

    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr4 = 1 << 11;
    x86.cpl = 3;
    memory.load(0, &[0x5A; 4]);
    let fault = SmirInterpreter::new().execute_block(&mut context, &mut memory, &block);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::GeneralProtection { .. })
    ));
    memory.read(0x2000, &mut observed).unwrap();
    assert_eq!(observed, [0x5A; 4]);
}

#[test]
fn selector_store_interpreter_rejects_malformed_target_and_o2_retains_effects() {
    let malformed = OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
        selector: X86SystemSelector::Ldtr,
        target: X86SystemSelectorTarget::Register {
            dst: x86_gpr(16),
            width: OpWidth::W64,
        },
        requires_apx: false,
    });
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, malformed);
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let function = builder.finish();
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.ldtr_selector = 0x1234;
    context.write_vreg(x86_gpr(16), 0xA5A5);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        function.entry_block().unwrap(),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { .. })
    ));
    assert_eq!(context.read_vreg(x86_gpr(16)), 0xA5A5);

    let mut builder = FunctionBuilder::new(FunctionId(1), 0x2000);
    builder.push_op(
        0x2000,
        OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Ldtr,
            target: X86SystemSelectorTarget::Register {
                dst: x86_gpr(0),
                width: OpWidth::W32,
            },
            requires_apx: false,
        }),
    );
    builder.push_op(
        0x2003,
        OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
            selector: X86SystemSelector::Tr,
            target: X86SystemSelectorTarget::Memory {
                addr: Address::Direct(x86_gpr(3)),
            },
            requires_apx: false,
        }),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);
    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86SystemSelectorStore(..)))
            .count(),
        2
    );
}
