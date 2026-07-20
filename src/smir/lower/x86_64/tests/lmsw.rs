//! Fault-precise state-backed native lowering for LMSW.

use super::*;
use crate::smir::ir::ops::{X86LmswOp, X86LmswSource};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn register(index: u8, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86Lmsw(X86LmswOp {
        source: X86LmswSource::Register {
            src: x86(X86Reg::gpr(index)),
        },
        requires_apx,
        next_pc,
    })
}

fn memory(addr: Address, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86Lmsw(X86LmswOp {
        source: X86LmswSource::Memory { addr },
        requires_apx,
        next_pc,
    })
}

fn lower(
    kind: OpKind,
    mem_helpers: bool,
    fault_guards: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_lmsw_requires_precise_guards_memory_helpers_and_never_uses_host_lmsw() {
    assert!(matches!(
        lower(register(0, false, 0x1003), false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let (register_code, _) =
        lower(register(31, true, 0x1004), false, true).expect("guarded register LMSW lowering");

    assert!(matches!(
        lower(
            memory(Address::Direct(x86(X86Reg::Rax)), false, 0x1003),
            false,
            true,
        ),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let (memory_code, _) = lower(
        memory(Address::Direct(x86(X86Reg::Rax)), false, 0x1003),
        true,
        true,
    )
    .expect("guarded helper-backed memory LMSW lowering");

    for code in [&register_code, &memory_code] {
        assert!(
            !code
                .windows(3)
                .any(|window| { window[..2] == [0x0F, 0x01] && ((window[2] >> 3) & 7) == 6 }),
            "guest LMSW must not update host CR0: {code:02X?}"
        );
        assert!(
            code.windows(2).any(|window| window == [0x0F, 0xA2]),
            "successful LMSW must execute a serializing barrier"
        );
        for offset in [X86_GUEST_CR0_OFFSET, X86_GUEST_CPL_OFFSET] {
            assert!(
                code.windows(4)
                    .any(|window| window == (offset as u32).to_le_bytes()),
                "missing dynamic LMSW state offset {offset}: {code:02X?}"
            );
        }
    }
    assert!(
        register_code
            .windows(4)
            .any(|window| { window == (X86_GUEST_APX_ENABLED_OFFSET as u32).to_le_bytes() })
    );
}

#[test]
fn lower_lmsw_rejects_every_non_lifter_source_and_frontier_shape() {
    for malformed in [
        OpKind::X86Lmsw(X86LmswOp {
            source: X86LmswSource::Register { src: VReg::virt(0) },
            requires_apx: false,
            next_pc: 0x1003,
        }),
        OpKind::X86Lmsw(X86LmswOp {
            source: X86LmswSource::Register {
                src: VReg::Arch(ArchReg::Arm(crate::smir::ir::types::ArmReg::X(0))),
            },
            requires_apx: false,
            next_pc: 0x1003,
        }),
        register(16, false, 0x1004),
        memory(Address::Direct(VReg::virt(1)), false, 0x1003),
        memory(Address::Direct(x86(X86Reg::R31)), false, 0x1004),
        memory(
            Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
                x86(X86Reg::Rax),
            ))))),
            false,
            0x1004,
        ),
        memory(
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rax)),
                index: x86(X86Reg::Rcx),
                scale: 3,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            false,
            0x1004,
        ),
        register(0, false, 0x1002),
        register(0, false, 0x1010),
        register(0, false, 0x0FFF),
    ] {
        assert!(matches!(
            lower(malformed, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, register(0, false, 0x1003));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(matches!(
        lowerer.lower_function(&hinted),
        Err(LowerError::InvalidOperand { .. })
    ));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_register(
    kind: OpKind,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(kind, false, true).expect("lower guarded register LMSW");
    let exec = ExecMem::new(&code).expect("map guarded register LMSW");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.ac_flag = 1;
    regs.exit_pc = 0xDEAD_BEEF_DEAD_BEEF;
    configure(&mut regs);
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_lmsw_register_form_covers_stack_aliases_egprs_pe_and_exact_handoff() {
    const OLD_CR0: u64 = 0xFEDC_BA98_7654_3211;
    for index in [0_u8, 4, 5, 8, 15, 16, 31] {
        for low_nibble in [0_u64, 2, 4, 8, 0xF] {
            let requires_apx = index >= 16;
            let regs = execute_register(register(index, requires_apx, 0x1004), |regs| {
                regs.cr0 = OLD_CR0;
                regs.cpl = 0;
                regs.apx_enabled = u64::from(requires_apx);
                regs.gpr[usize::from(index)] = 0x1234_5678_9ABC_DEF0 | low_nibble;
            });
            let expected_cr0 = (OLD_CR0 & !0xF) | low_nibble | 1;
            assert_eq!(
                regs.cr0, expected_cr0,
                "source={index}, low={low_nibble:#x}"
            );
            for (other, value) in regs.gpr.iter().enumerate() {
                let expected = if other == usize::from(index) {
                    0x1234_5678_9ABC_DEF0 | low_nibble
                } else {
                    0xA500_0000_0000_0000 | other as u64
                };
                assert_eq!(*value, expected, "source={index}, GPR={other}");
            }
            assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
            assert_eq!(regs.ac_flag, 1);
            assert_eq!(regs.exit_pc, 0x1004);
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_lmsw_dynamic_guards_are_precise_noncommitting_and_allow_real_mode() {
    for (name, apx_enabled, cr0, cpl) in [
        ("APX", 0, 0x8000_0031, 0),
        ("CPL", 1, 0x8000_0031, 3),
        ("APX+CPL", 0, 0x8000_0031, 3),
    ] {
        let regs = execute_register(register(31, true, 0x1004), |regs| {
            regs.gpr[31] = 0xF;
            regs.apx_enabled = apx_enabled;
            regs.cr0 = cr0;
            regs.cpl = cpl;
        });
        assert_eq!(regs.cr0, cr0, "{name}");
        assert_eq!(regs.gpr[31], 0xF, "{name}");
        assert_eq!(regs.exit_pc, 0x1000, "{name}");
        assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    }

    let real_mode = execute_register(register(3, false, 0x1003), |regs| {
        regs.gpr[3] = 0xE;
        regs.cr0 = 0x8000_0030;
        regs.cpl = 3;
    });
    assert_eq!(real_mode.cr0, 0x8000_003E);
    assert_eq!(real_mode.exit_pc, 0x1003);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct LoadContext {
    loads: u64,
    last_addr: u64,
    last_size: u64,
    value: u64,
    ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn load(context: *mut LoadContext, addr: u64, size: u64, _signed: u64) -> LoadResult {
    let context = unsafe { &mut *context };
    context.loads += 1;
    context.last_addr = addr;
    context.last_size = size;
    LoadResult {
        value: context.value,
        ok: context.ok,
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_lmsw_memory_is_b2_fault_precise_guarded_and_stack_state_backed() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const OLD_CR0: u64 = 0xFEDC_BA98_7654_3211;
    const FLAGS: u64 = 0x2 | 0x08D5 | (1 << 10);
    let address = Address::BaseIndexScale {
        base: Some(x86(X86Reg::Rsp)),
        index: x86(X86Reg::R31),
        scale: 2,
        disp: -8,
        disp_size: DispSize::Disp8,
    };
    let (code, entry) = lower(memory(address, true, 0x1005), true, true)
        .expect("lower helper-backed APX LMSW memory form");
    let exec = ExecMem::new(&code).expect("map helper-backed APX LMSW memory form");

    let mut initial_gprs = [0u64; 32];
    for (index, value) in initial_gprs.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    initial_gprs[4] = 0x2000;
    initial_gprs[31] = 0x24;
    let expected_addr = 0x2000 + 0x24 * 2 - 8;

    for (ok, expected_cr0, expected_exit) in [
        (1, (OLD_CR0 & !0xF) | 0xE | 1, 0x1005),
        (0, OLD_CR0, 0x1000),
    ] {
        let mut context = LoadContext {
            value: 0xCAFE_BABE_1234_000E,
            ok,
            ..LoadContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial_gprs;
        regs.cr0 = OLD_CR0;
        regs.cpl = 0;
        regs.apx_enabled = 1;
        regs.rflags = FLAGS;
        regs.ac_flag = 1;
        regs.exit_pc = 0xDEAD_BEEF;
        regs.ctx = (&mut context as *mut LoadContext) as u64;
        regs.load_fn = load as usize as u64;
        exec.run(entry, &mut regs);

        assert_eq!(context.loads, 1);
        assert_eq!(context.last_addr, expected_addr);
        assert_eq!(context.last_size, 2);
        assert_eq!(regs.cr0, expected_cr0);
        assert_eq!(regs.gpr, initial_gprs);
        assert_eq!(
            regs.rflags & (0x08D5 | (1 << 10)),
            FLAGS & (0x08D5 | (1 << 10))
        );
        assert_eq!(regs.ac_flag, 1);
        assert_eq!(regs.exit_pc, expected_exit);
    }

    for (name, apx_enabled, cpl) in [("APX", 0, 0), ("CPL", 1, 3)] {
        let mut context = LoadContext {
            value: 0xF,
            ok: 1,
            ..LoadContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial_gprs;
        regs.cr0 = OLD_CR0;
        regs.cpl = cpl;
        regs.apx_enabled = apx_enabled;
        regs.rflags = FLAGS;
        regs.ctx = (&mut context as *mut LoadContext) as u64;
        regs.load_fn = load as usize as u64;
        exec.run(entry, &mut regs);

        assert_eq!(context.loads, 0, "{name} guard must precede memory");
        assert_eq!(regs.cr0, OLD_CR0, "{name}");
        assert_eq!(regs.gpr, initial_gprs, "{name}");
        assert_eq!(regs.exit_pc, 0x1000, "{name}");
    }
}
