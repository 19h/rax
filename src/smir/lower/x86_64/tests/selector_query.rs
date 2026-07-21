//! Fault-precise helper-backed native lowering for LAR/LSL.

use super::*;
use crate::smir::ir::ops::{X86SelectorQueryKind, X86SelectorQueryOp, X86SelectorQuerySource};
use crate::smir::lower::x86_64::x86_selector_query_shape_valid;
use crate::smir::lower::{
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CR0_OFFSET, X86_GUEST_RFLAGS_OFFSET,
    X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET, X86_SELECTOR_QUERY_HELPER_APX,
    X86_SELECTOR_QUERY_HELPER_DST_MASK, X86_SELECTOR_QUERY_HELPER_DST_SHIFT,
    X86_SELECTOR_QUERY_HELPER_LIMIT, X86_SELECTOR_QUERY_HELPER_MEMORY,
    X86_SELECTOR_QUERY_HELPER_TAG, X86_SELECTOR_QUERY_HELPER_WIDTH_MASK,
    X86_SELECTOR_QUERY_HELPER_WIDTH_SHIFT,
};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn register(
    kind: X86SelectorQueryKind,
    dst: u8,
    src: u8,
    width: OpWidth,
    requires_apx: bool,
    next_pc: u64,
) -> OpKind {
    OpKind::X86SelectorQuery(X86SelectorQueryOp {
        kind,
        dst: x86(X86Reg::gpr(dst)),
        source: X86SelectorQuerySource::Register {
            src: x86(X86Reg::gpr(src)),
        },
        width,
        requires_apx,
        next_pc,
    })
}

fn memory(
    kind: X86SelectorQueryKind,
    dst: u8,
    addr: Address,
    stack_segment: bool,
    width: OpWidth,
    requires_apx: bool,
    next_pc: u64,
) -> OpKind {
    OpKind::X86SelectorQuery(X86SelectorQueryOp {
        kind,
        dst: x86(X86Reg::gpr(dst)),
        source: X86SelectorQuerySource::Memory {
            addr,
            stack_segment,
        },
        width,
        requires_apx,
        next_pc,
    })
}

fn function(kind: OpKind) -> SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

fn lower(
    kind: OpKind,
    mem_helpers: bool,
    fault_guards: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&function(kind))?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

fn instruction_len(width: OpWidth, requires_apx: bool) -> u64 {
    if requires_apx {
        4 + u64::from(width == OpWidth::W16)
    } else {
        3 + u64::from(width != OpWidth::W32)
    }
}

#[test]
fn lower_selector_query_requires_guards_and_implicit_memory_helpers() {
    let op = register(
        X86SelectorQueryKind::AccessRights,
        1,
        0,
        OpWidth::W32,
        false,
        0x1003,
    );
    assert!(matches!(
        lower(op.clone(), true, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower(op.clone(), false, true),
        Err(LowerError::UnsupportedOp { .. })
    ));

    let (register_code, _) = lower(op, true, true).expect("guarded LAR register lowering");
    let (memory_code, _) = lower(
        memory(
            X86SelectorQueryKind::Limit,
            2,
            Address::Direct(x86(X86Reg::Rsp)),
            true,
            OpWidth::W64,
            false,
            0x1004,
        ),
        true,
        true,
    )
    .expect("guarded LSL memory lowering");
    let (apx_code, _) = lower(
        register(
            X86SelectorQueryKind::AccessRights,
            30,
            31,
            OpWidth::W64,
            true,
            0x1004,
        ),
        true,
        true,
    )
    .expect("guarded APX LAR lowering");

    for code in [&register_code, &memory_code, &apx_code] {
        for offset in [
            X86_GUEST_CR0_OFFSET,
            X86_GUEST_RFLAGS_OFFSET,
            X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET,
        ] {
            assert!(
                code.windows(4)
                    .any(|window| window == (offset as u32).to_le_bytes()),
                "missing selector-query state offset {offset}: {code:02X?}"
            );
        }
        assert!(
            !code
                .windows(3)
                .any(|window| { window[..2] == [0x0F, 0x02] || window[..2] == [0x0F, 0x03] }),
            "guest LAR/LSL must not inspect host descriptors: {code:02X?}"
        );
    }
    assert!(
        apx_code
            .windows(4)
            .any(|window| window == (X86_GUEST_APX_ENABLED_OFFSET as u32).to_le_bytes())
    );
    assert!(
        !register_code
            .windows(4)
            .any(|window| window == (X86_GUEST_APX_ENABLED_OFFSET as u32).to_le_bytes())
    );
}

#[test]
fn lower_selector_query_rejects_every_non_lifter_shape() {
    let malformed = |dst, source, width, requires_apx, next_pc| {
        OpKind::X86SelectorQuery(X86SelectorQueryOp {
            kind: X86SelectorQueryKind::AccessRights,
            dst,
            source,
            width,
            requires_apx,
            next_pc,
        })
    };
    for op in [
        malformed(
            VReg::virt(0),
            X86SelectorQuerySource::Register {
                src: x86(X86Reg::Rax),
            },
            OpWidth::W32,
            false,
            0x1003,
        ),
        malformed(
            VReg::Arch(ArchReg::Arm(crate::smir::ir::types::ArmReg::X(0))),
            X86SelectorQuerySource::Register {
                src: x86(X86Reg::Rax),
            },
            OpWidth::W32,
            false,
            0x1003,
        ),
        malformed(
            x86(X86Reg::Rax),
            X86SelectorQuerySource::Register { src: VReg::virt(0) },
            OpWidth::W32,
            false,
            0x1003,
        ),
        register(
            X86SelectorQueryKind::Limit,
            16,
            0,
            OpWidth::W32,
            false,
            0x1004,
        ),
        register(
            X86SelectorQueryKind::Limit,
            0,
            31,
            OpWidth::W32,
            false,
            0x1004,
        ),
        register(
            X86SelectorQueryKind::Limit,
            0,
            1,
            OpWidth::W8,
            false,
            0x1003,
        ),
        register(
            X86SelectorQueryKind::Limit,
            0,
            1,
            OpWidth::W128,
            false,
            0x1003,
        ),
        memory(
            X86SelectorQueryKind::Limit,
            0,
            Address::Direct(VReg::virt(0)),
            false,
            OpWidth::W32,
            false,
            0x1003,
        ),
        memory(
            X86SelectorQueryKind::Limit,
            0,
            Address::Direct(x86(X86Reg::R31)),
            false,
            OpWidth::W32,
            false,
            0x1004,
        ),
        memory(
            X86SelectorQueryKind::Limit,
            0,
            Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
                x86(X86Reg::Rax),
            ))))),
            false,
            OpWidth::W32,
            false,
            0x1004,
        ),
        memory(
            X86SelectorQueryKind::Limit,
            0,
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rax)),
                index: x86(X86Reg::Rcx),
                scale: 3,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            false,
            OpWidth::W32,
            false,
            0x1004,
        ),
        register(
            X86SelectorQueryKind::AccessRights,
            0,
            1,
            OpWidth::W32,
            false,
            0x1002,
        ),
        register(
            X86SelectorQueryKind::AccessRights,
            0,
            1,
            OpWidth::W32,
            false,
            0x1010,
        ),
        register(
            X86SelectorQueryKind::AccessRights,
            0,
            1,
            OpWidth::W32,
            false,
            0x0FFF,
        ),
    ] {
        let function = function(op.clone());
        assert!(!x86_selector_query_shape_valid(&function.blocks[0].ops[0]));
        assert!(matches!(
            lower(op, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let mut hinted = function(register(
        X86SelectorQueryKind::AccessRights,
        1,
        0,
        OpWidth::W32,
        false,
        0x1003,
    ));
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_selector_query_shape_valid(&hinted.blocks[0].ops[0]));
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(matches!(
        lowerer.lower_function(&hinted),
        Err(LowerError::InvalidOperand { .. })
    ));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct QueryContext {
    calls: u64,
    operand: u64,
    encoding: u32,
    result: u64,
    value: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
unsafe extern "C" fn query_helper(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    operand: u64,
    encoding: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut QueryContext) };
    context.calls += 1;
    context.operand = operand;
    context.encoding = encoding;
    if context.result == 2 {
        let dst = ((encoding & X86_SELECTOR_QUERY_HELPER_DST_MASK)
            >> X86_SELECTOR_QUERY_HELPER_DST_SHIFT) as usize;
        let width = (encoding & X86_SELECTOR_QUERY_HELPER_WIDTH_MASK)
            >> X86_SELECTOR_QUERY_HELPER_WIDTH_SHIFT;
        state.gpr[dst] = match width {
            0 => (state.gpr[dst] & !u64::from(u16::MAX)) | (context.value & u64::from(u16::MAX)),
            1 | 2 => context.value & u64::from(u32::MAX),
            _ => return 0,
        };
    }
    context.result
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute(
    kind: OpKind,
    context: &mut QueryContext,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(kind, true, true).expect("lower guarded selector query");
    let exec = ExecMem::new(&code).expect("map guarded selector query");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.cr0 = 1;
    regs.apx_enabled = 1;
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.ac_flag = 1;
    regs.exit_pc = 0xDEAD_BEEF_DEAD_BEEF;
    regs.ctx = (context as *mut QueryContext) as u64;
    regs.system_selector_load_fn = query_helper as usize as u64;
    configure(&mut regs);
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_selector_query_registers_encode_kind_width_destination_apx_and_commit_only_zf() {
    const FLAGS: u64 = 0x2 | 0x08D5 | (1 << 10);
    const OBSERVED_FLAGS: u64 = 0x08D5 | (1 << 10);
    for kind in [
        X86SelectorQueryKind::AccessRights,
        X86SelectorQueryKind::Limit,
    ] {
        for dst in [0_u8, 4, 5, 8, 15, 16, 31] {
            let src = if dst == 31 { 0 } else { dst + 1 };
            for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
                for result in [1_u64, 2] {
                    let requires_apx = dst >= 16 || src >= 16;
                    let mut context = QueryContext {
                        result,
                        value: 0x89AB_CDEF,
                        ..QueryContext::default()
                    };
                    let regs = execute(
                        register(
                            kind,
                            dst,
                            src,
                            width,
                            requires_apx,
                            0x1000 + instruction_len(width, requires_apx),
                        ),
                        &mut context,
                        |_| {},
                    );
                    assert_eq!(context.calls, 1, "{kind:?} R{dst},R{src} {width:?}");
                    assert_eq!(
                        context.operand,
                        0xA500_0000_0000_0000 | u64::from(src),
                        "{kind:?} R{dst},R{src} {width:?}"
                    );
                    let width_code = match width {
                        OpWidth::W16 => 0,
                        OpWidth::W32 => 1,
                        OpWidth::W64 => 2,
                        _ => unreachable!(),
                    };
                    assert_eq!(
                        context.encoding,
                        X86_SELECTOR_QUERY_HELPER_TAG
                            | (u32::from(dst) << X86_SELECTOR_QUERY_HELPER_DST_SHIFT)
                            | (width_code << X86_SELECTOR_QUERY_HELPER_WIDTH_SHIFT)
                            | if kind == X86SelectorQueryKind::Limit {
                                X86_SELECTOR_QUERY_HELPER_LIMIT
                            } else {
                                0
                            }
                            | if requires_apx {
                                X86_SELECTOR_QUERY_HELPER_APX
                            } else {
                                0
                            }
                    );
                    for (other, observed) in regs.gpr.iter().enumerate() {
                        let incoming = 0xA500_0000_0000_0000 | other as u64;
                        let expected = if result == 2 && other == usize::from(dst) {
                            match width {
                                OpWidth::W16 => (incoming & !0xFFFF) | 0xCDEF,
                                OpWidth::W32 | OpWidth::W64 => 0x89AB_CDEF,
                                _ => unreachable!(),
                            }
                        } else {
                            incoming
                        };
                        assert_eq!(*observed, expected, "{kind:?} R{dst},R{src} {width:?}");
                    }
                    let expected_flags = if result == 2 {
                        FLAGS | crate::isa::x86_64::flags::bits::ZF
                    } else {
                        FLAGS & !crate::isa::x86_64::flags::bits::ZF
                    };
                    assert_eq!(
                        regs.rflags & OBSERVED_FLAGS,
                        expected_flags & OBSERVED_FLAGS
                    );
                    assert_eq!(regs.ac_flag, 1);
                    assert_eq!(regs.exit_pc, 0xDEAD_BEEF_DEAD_BEEF);
                }
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_selector_query_memory_uses_state_backed_stack_and_egpr_addresses() {
    let address = Address::BaseIndexScale {
        base: Some(x86(X86Reg::Rsp)),
        index: x86(X86Reg::R31),
        scale: 2,
        disp: -8,
        disp_size: DispSize::Disp8,
    };
    let mut context = QueryContext {
        result: 2,
        value: 0x1234_5678,
        ..QueryContext::default()
    };
    let regs = execute(
        memory(
            X86SelectorQueryKind::Limit,
            30,
            address,
            true,
            OpWidth::W64,
            true,
            0x1005,
        ),
        &mut context,
        |regs| {
            regs.gpr[4] = 0x2000;
            regs.gpr[31] = 0x24;
        },
    );
    assert_eq!(context.calls, 1);
    assert_eq!(context.operand, 0x2000 + 0x24 * 2 - 8);
    assert_eq!(
        context.encoding,
        X86_SELECTOR_QUERY_HELPER_TAG
            | X86_SELECTOR_QUERY_HELPER_LIMIT
            | X86_SELECTOR_QUERY_HELPER_MEMORY
            | X86_SELECTOR_QUERY_HELPER_APX
            | (30 << X86_SELECTOR_QUERY_HELPER_DST_SHIFT)
            | (2 << X86_SELECTOR_QUERY_HELPER_WIDTH_SHIFT)
    );
    assert_eq!(regs.gpr[4], 0x2000);
    assert_eq!(regs.gpr[31], 0x24);
    assert_eq!(regs.gpr[30], 0x1234_5678);
    assert_ne!(regs.rflags & crate::isa::x86_64::flags::bits::ZF, 0);
    assert_eq!(regs.exit_pc, 0xDEAD_BEEF_DEAD_BEEF);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_selector_query_dynamic_failures_replay_precisely_without_commit() {
    const FLAGS: u64 = 0x2 | 0x08D5 | (1 << 10);
    const OBSERVED_FLAGS: u64 = 0x08D5 | (1 << 10);
    for (name, apx, cr0, rflags, helper_result, expected_calls) in [
        ("APX", 0, 1, 0, 2, 0),
        ("real mode", 1, 0, 0, 2, 0),
        ("VM86", 1, 1, 1 << 17, 2, 0),
        ("helper replay", 1, 1, 0, 0, 1),
    ] {
        let mut context = QueryContext {
            result: helper_result,
            value: 0x1234_5678,
            ..QueryContext::default()
        };
        let regs = execute(
            register(
                X86SelectorQueryKind::AccessRights,
                30,
                31,
                OpWidth::W64,
                true,
                0x1004,
            ),
            &mut context,
            |regs| {
                regs.apx_enabled = apx;
                regs.cr0 = cr0;
                regs.rflags = FLAGS | rflags;
            },
        );
        assert_eq!(context.calls, expected_calls, "{name}");
        for (index, value) in regs.gpr.iter().enumerate() {
            assert_eq!(*value, 0xA500_0000_0000_0000 | index as u64, "{name}");
        }
        assert_eq!(
            regs.rflags & OBSERVED_FLAGS,
            (FLAGS | rflags) & OBSERVED_FLAGS,
            "{name}"
        );
        assert_eq!(regs.ac_flag, 1, "{name}");
        assert_eq!(regs.exit_pc, 0x1000, "{name}");
    }
}
