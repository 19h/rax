//! Fault-precise helper-backed native lowering for VERR/VERW.

use super::*;
use crate::smir::ir::ops::{X86SelectorVerifyKind, X86SelectorVerifyOp, X86SelectorVerifySource};
use crate::smir::lower::x86_64::x86_selector_verify_shape_valid;
use crate::smir::lower::{
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CR0_OFFSET, X86_GUEST_RFLAGS_OFFSET,
    X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET, X86_SELECTOR_VERIFY_HELPER_APX,
    X86_SELECTOR_VERIFY_HELPER_MEMORY, X86_SELECTOR_VERIFY_HELPER_TAG,
    X86_SELECTOR_VERIFY_HELPER_WRITE,
};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn register(kind: X86SelectorVerifyKind, index: u8, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86SelectorVerify(X86SelectorVerifyOp {
        kind,
        source: X86SelectorVerifySource::Register {
            src: x86(X86Reg::gpr(index)),
        },
        requires_apx,
        next_pc,
    })
}

fn memory(
    kind: X86SelectorVerifyKind,
    addr: Address,
    stack_segment: bool,
    requires_apx: bool,
    next_pc: u64,
) -> OpKind {
    OpKind::X86SelectorVerify(X86SelectorVerifyOp {
        kind,
        source: X86SelectorVerifySource::Memory {
            addr,
            stack_segment,
        },
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

#[test]
fn lower_selector_verify_requires_guards_and_implicit_memory_helpers() {
    let op = register(X86SelectorVerifyKind::Read, 0, false, 0x1003);
    assert!(matches!(
        lower(op.clone(), true, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower(op.clone(), false, true),
        Err(LowerError::UnsupportedOp { .. })
    ));

    let (register_code, _) = lower(op, true, true).expect("guarded VERR register lowering");
    let (memory_code, _) = lower(
        memory(
            X86SelectorVerifyKind::Write,
            Address::Direct(x86(X86Reg::Rsp)),
            true,
            false,
            0x1003,
        ),
        true,
        true,
    )
    .expect("guarded VERW memory lowering");
    let (apx_code, _) = lower(
        register(X86SelectorVerifyKind::Read, 31, true, 0x1004),
        true,
        true,
    )
    .expect("guarded APX VERR lowering");

    for code in [&register_code, &memory_code, &apx_code] {
        for offset in [
            X86_GUEST_CR0_OFFSET,
            X86_GUEST_RFLAGS_OFFSET,
            X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET,
        ] {
            assert!(
                code.windows(4)
                    .any(|window| window == (offset as u32).to_le_bytes()),
                "missing selector-verification state offset {offset}: {code:02X?}"
            );
        }
        assert!(
            !code.windows(3).any(|window| {
                window[..2] == [0x0F, 0x00] && matches!((window[2] >> 3) & 7, 4 | 5)
            }),
            "guest VERR/VERW must not inspect host descriptors: {code:02X?}"
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
fn lower_selector_verify_rejects_every_non_lifter_shape() {
    let malformed_register = |src, requires_apx, next_pc| {
        OpKind::X86SelectorVerify(X86SelectorVerifyOp {
            kind: X86SelectorVerifyKind::Read,
            source: X86SelectorVerifySource::Register { src },
            requires_apx,
            next_pc,
        })
    };
    for malformed in [
        malformed_register(VReg::virt(0), false, 0x1003),
        malformed_register(
            VReg::Arch(ArchReg::Arm(crate::smir::ir::types::ArmReg::X(0))),
            false,
            0x1003,
        ),
        register(X86SelectorVerifyKind::Read, 16, false, 0x1004),
        memory(
            X86SelectorVerifyKind::Write,
            Address::Direct(VReg::virt(0)),
            false,
            false,
            0x1003,
        ),
        memory(
            X86SelectorVerifyKind::Write,
            Address::Direct(x86(X86Reg::R31)),
            false,
            false,
            0x1004,
        ),
        memory(
            X86SelectorVerifyKind::Read,
            Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
                x86(X86Reg::Rax),
            ))))),
            false,
            false,
            0x1004,
        ),
        memory(
            X86SelectorVerifyKind::Read,
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rax)),
                index: x86(X86Reg::Rcx),
                scale: 3,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            false,
            false,
            0x1004,
        ),
        register(X86SelectorVerifyKind::Read, 0, false, 0x1002),
        register(X86SelectorVerifyKind::Read, 0, false, 0x1010),
        register(X86SelectorVerifyKind::Read, 0, false, 0x0FFF),
    ] {
        let function = function(malformed.clone());
        assert!(!x86_selector_verify_shape_valid(&function.blocks[0].ops[0]));
        assert!(matches!(
            lower(malformed, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let mut hinted = function(register(X86SelectorVerifyKind::Read, 0, false, 0x1003));
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_selector_verify_shape_valid(&hinted.blocks[0].ops[0]));
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
struct VerifyContext {
    calls: u64,
    operand: u64,
    encoding: u32,
    result: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
unsafe extern "C" fn verify_helper(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    operand: u64,
    encoding: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut VerifyContext) };
    context.calls += 1;
    context.operand = operand;
    context.encoding = encoding;
    context.result
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute(
    kind: OpKind,
    context: &mut VerifyContext,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(kind, true, true).expect("lower guarded selector verification");
    let exec = ExecMem::new(&code).expect("map guarded selector verification");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.cr0 = 1;
    regs.apx_enabled = 1;
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.ac_flag = 1;
    regs.exit_pc = 0xDEAD_BEEF_DEAD_BEEF;
    regs.ctx = (context as *mut VerifyContext) as u64;
    regs.system_selector_load_fn = verify_helper as usize as u64;
    configure(&mut regs);
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_selector_verify_registers_encode_access_apx_and_commit_only_zf() {
    const FLAGS: u64 = 0x2 | 0x08D5 | (1 << 10);
    const OBSERVED_FLAGS: u64 = 0x08D5 | (1 << 10);
    for kind in [X86SelectorVerifyKind::Read, X86SelectorVerifyKind::Write] {
        for index in [0_u8, 4, 5, 8, 15, 16, 31] {
            for result in [1_u64, 2] {
                let requires_apx = index >= 16;
                let mut context = VerifyContext {
                    result,
                    ..VerifyContext::default()
                };
                let regs = execute(
                    register(kind, index, requires_apx, 0x1004),
                    &mut context,
                    |_| {},
                );
                assert_eq!(context.calls, 1, "{kind:?} R{index} result={result}");
                assert_eq!(
                    context.operand,
                    0xA500_0000_0000_0000 | u64::from(index),
                    "{kind:?} R{index} result={result}"
                );
                assert_eq!(
                    context.encoding,
                    X86_SELECTOR_VERIFY_HELPER_TAG
                        | if kind == X86SelectorVerifyKind::Write {
                            X86_SELECTOR_VERIFY_HELPER_WRITE
                        } else {
                            0
                        }
                        | if requires_apx {
                            X86_SELECTOR_VERIFY_HELPER_APX
                        } else {
                            0
                        }
                );
                for (other, value) in regs.gpr.iter().enumerate() {
                    assert_eq!(*value, 0xA500_0000_0000_0000 | other as u64);
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

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_selector_verify_memory_uses_state_backed_stack_and_egpr_addresses() {
    let address = Address::BaseIndexScale {
        base: Some(x86(X86Reg::Rsp)),
        index: x86(X86Reg::R31),
        scale: 2,
        disp: -8,
        disp_size: DispSize::Disp8,
    };
    let mut context = VerifyContext {
        result: 2,
        ..VerifyContext::default()
    };
    let regs = execute(
        memory(X86SelectorVerifyKind::Write, address, true, true, 0x1005),
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
        X86_SELECTOR_VERIFY_HELPER_TAG
            | X86_SELECTOR_VERIFY_HELPER_WRITE
            | X86_SELECTOR_VERIFY_HELPER_MEMORY
            | X86_SELECTOR_VERIFY_HELPER_APX
    );
    assert_eq!(regs.gpr[4], 0x2000);
    assert_eq!(regs.gpr[31], 0x24);
    assert_ne!(regs.rflags & crate::isa::x86_64::flags::bits::ZF, 0);
    assert_eq!(regs.exit_pc, 0xDEAD_BEEF_DEAD_BEEF);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_selector_verify_dynamic_failures_are_precise_and_noncommitting() {
    const FLAGS: u64 = 0x2 | 0x08D5 | (1 << 10);
    // ExecMem exposes the host-safe RFLAGS image; VM is carried by the vCPU's
    // separate interrupt-control shadow during production marshalling.
    const OBSERVED_FLAGS: u64 = 0x08D5 | (1 << 10);
    for (name, apx, cr0, rflags, helper_result, expected_calls) in [
        ("APX", 0, 1, 0, 2, 0),
        ("real mode", 1, 0, 0, 2, 0),
        ("VM86", 1, 1, 1 << 17, 2, 0),
        ("helper replay", 1, 1, 0, 0, 1),
    ] {
        let mut context = VerifyContext {
            result: helper_result,
            ..VerifyContext::default()
        };
        let regs = execute(
            register(X86SelectorVerifyKind::Read, 31, true, 0x1004),
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
