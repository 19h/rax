//! Fault-precise state-backed native lowering for SMSW.

use super::*;
use crate::smir::ir::ops::{X86SmswOp, X86SmswTarget};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn register(index: u8, width: OpWidth, requires_apx: bool) -> OpKind {
    OpKind::X86Smsw(X86SmswOp {
        target: X86SmswTarget::Register {
            dst: x86(X86Reg::gpr(index)),
            width,
        },
        requires_apx,
    })
}

fn memory(addr: Address, requires_apx: bool) -> OpKind {
    OpKind::X86Smsw(X86SmswOp {
        target: X86SmswTarget::Memory { addr },
        requires_apx,
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
fn lower_smsw_requires_precise_guards_and_memory_helpers_without_host_smsw() {
    assert!(matches!(
        lower(register(0, OpWidth::W32, false), false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let (register_code, _) = lower(register(15, OpWidth::W64, false), false, true)
        .expect("guarded register SMSW lowering");

    assert!(matches!(
        lower(
            memory(Address::Direct(x86(X86Reg::Rax)), false),
            false,
            true,
        ),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let (memory_code, _) = lower(memory(Address::Direct(x86(X86Reg::Rax)), false), true, true)
        .expect("guarded helper-backed memory SMSW lowering");

    for code in [&register_code, &memory_code] {
        assert!(
            !code
                .windows(3)
                .any(|window| window[..2] == [0x0F, 0x01] && (window[2] >> 3) & 7 == 4),
            "guest SMSW must not read host CR0: {code:02X?}"
        );
        for offset in [
            X86_GUEST_CR0_OFFSET,
            X86_GUEST_CR4_OFFSET,
            X86_GUEST_CPL_OFFSET,
        ] {
            assert!(
                code.windows(4)
                    .any(|window| window == (offset as u32).to_le_bytes()),
                "missing dynamic SMSW state offset {offset}: {code:02X?}"
            );
        }
    }
}

#[test]
fn lower_smsw_rejects_every_non_lifter_shape() {
    for malformed in [
        OpKind::X86Smsw(X86SmswOp {
            target: X86SmswTarget::Register {
                dst: VReg::virt(0),
                width: OpWidth::W64,
            },
            requires_apx: false,
        }),
        OpKind::X86Smsw(X86SmswOp {
            target: X86SmswTarget::Register {
                dst: x86(X86Reg::Rax),
                width: OpWidth::W8,
            },
            requires_apx: false,
        }),
        register(0, OpWidth::W128, false),
        register(16, OpWidth::W64, false),
        memory(Address::Direct(VReg::virt(1)), false),
        memory(
            Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
                x86(X86Reg::Rax),
            ))))),
            false,
        ),
        memory(Address::Direct(x86(X86Reg::R31)), false),
        memory(
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rax)),
                index: x86(X86Reg::Rcx),
                scale: 3,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            false,
        ),
    ] {
        assert!(!x86_smsw_shape_valid(&malformed), "{malformed:?}");
        assert!(matches!(
            lower(malformed, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_register(
    kind: OpKind,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(kind, false, true).expect("lower guarded SMSW register form");
    let exec = ExecMem::new(&code).expect("map guarded SMSW register form");
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
fn native_smsw_register_form_covers_widths_stack_aliases_egprs_and_flags() {
    const CR0: u64 = 0xFEDC_BA98_7654_3211;
    for index in [0_u8, 4, 5, 8, 15, 16, 31] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let requires_apx = index >= 16;
            let regs = execute_register(register(index, width, requires_apx), |regs| {
                regs.cr0 = CR0;
                regs.cpl = 3;
                regs.apx_enabled = u64::from(requires_apx);
            });
            let incoming = 0xA500_0000_0000_0000 | u64::from(index);
            let expected = match width {
                OpWidth::W16 => (incoming & !0xFFFF) | (CR0 & 0xFFFF),
                OpWidth::W32 => CR0 & u32::MAX as u64,
                OpWidth::W64 => CR0,
                _ => unreachable!(),
            };
            for (other, value) in regs.gpr.iter().enumerate() {
                let expected_value = if other == usize::from(index) {
                    expected
                } else {
                    0xA500_0000_0000_0000 | other as u64
                };
                assert_eq!(*value, expected_value, "dst={index} {width:?}, GPR={other}");
            }
            assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
            assert_eq!(regs.ac_flag, 1);
            assert_eq!(regs.exit_pc, 0xDEAD_BEEF_DEAD_BEEF);
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_smsw_dynamic_guards_are_precise_and_noncommitting() {
    let sentinel = 0xA500_0000_0000_001F;
    for apx_enabled in [false, true] {
        let regs = execute_register(register(31, OpWidth::W32, true), |regs| {
            regs.cr0 = 1;
            regs.cr4 = 1 << 11;
            regs.cpl = 3;
            regs.apx_enabled = u64::from(apx_enabled);
        });
        assert_eq!(regs.exit_pc, 0x1000);
        assert_eq!(regs.gpr[31], sentinel);
        assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    }

    for (name, cr0, cr4, cpl) in [
        ("real mode", 0x8000_0030, 1 << 11, 3),
        ("UMIP clear", 0x8000_0031, 0, 3),
        ("CPL0", 0x8000_0031, 1 << 11, 0),
    ] {
        let regs = execute_register(register(3, OpWidth::W32, false), |regs| {
            regs.cr0 = cr0;
            regs.cr4 = cr4;
            regs.cpl = cpl;
        });
        assert_eq!(regs.gpr[3], cr0 & u32::MAX as u64, "{name}");
        assert_eq!(regs.exit_pc, 0xDEAD_BEEF_DEAD_BEEF, "{name}");
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct StoreContext {
    stores: u64,
    last_addr: u64,
    last_value: u64,
    last_size: u64,
    store_ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn store(context: *mut StoreContext, addr: u64, value: u64, size: u64) -> u64 {
    let context = unsafe { &mut *context };
    context.stores += 1;
    context.last_addr = addr;
    context.last_value = value;
    context.last_size = size;
    context.store_ok
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_smsw_memory_form_is_two_bytes_fault_precise_and_stack_state_backed() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const CR0: u64 = 0xFEDC_BA98_7654_3211;
    const FLAGS: u64 = 0x2 | 0x08D5 | (1 << 10);
    const SENTINEL_PC: u64 = 0xDEAD_BEEF_DEAD_BEEF;
    let address = Address::BaseIndexScale {
        base: Some(x86(X86Reg::Rsp)),
        index: x86(X86Reg::R31),
        scale: 2,
        disp: -8,
        disp_size: DispSize::Disp8,
    };
    let (code, entry) =
        lower(memory(address, true), true, true).expect("lower helper-backed APX SMSW memory form");
    let exec = ExecMem::new(&code).expect("map helper-backed APX SMSW memory form");

    let initial_gprs = {
        let mut gprs = [0u64; 32];
        for (index, value) in gprs.iter_mut().enumerate() {
            *value = 0xA500_0000_0000_0000 | index as u64;
        }
        gprs[4] = 0x2000;
        gprs[31] = 0x24;
        gprs
    };
    let expected_addr = 0x2000 + 0x24 * 2 - 8;

    for (store_ok, expected_exit) in [(1, SENTINEL_PC), (0, 0x1000)] {
        let mut context = StoreContext {
            store_ok,
            ..StoreContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial_gprs;
        regs.cr0 = CR0;
        regs.cpl = 3;
        regs.apx_enabled = 1;
        regs.rflags = FLAGS;
        regs.ac_flag = 1;
        regs.exit_pc = SENTINEL_PC;
        regs.ctx = (&mut context as *mut StoreContext) as u64;
        regs.store_fn = store as usize as u64;
        exec.run(entry, &mut regs);

        assert_eq!(context.stores, 1);
        assert_eq!(context.last_addr, expected_addr);
        assert_eq!(context.last_value, CR0);
        assert_eq!(context.last_size, 2);
        assert_eq!(regs.gpr, initial_gprs);
        assert_eq!(
            regs.rflags & (0x08D5 | (1 << 10)),
            FLAGS & (0x08D5 | (1 << 10))
        );
        assert_eq!(regs.ac_flag, 1);
        assert_eq!(regs.exit_pc, expected_exit);
    }

    for (name, apx_enabled, umip) in [("APX", 0, false), ("UMIP", 1, true)] {
        let mut context = StoreContext {
            store_ok: 1,
            ..StoreContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial_gprs;
        regs.cr0 = 1;
        regs.cr4 = u64::from(umip) << 11;
        regs.cpl = 3;
        regs.apx_enabled = apx_enabled;
        regs.rflags = FLAGS;
        regs.ctx = (&mut context as *mut StoreContext) as u64;
        regs.store_fn = store as usize as u64;
        exec.run(entry, &mut regs);

        assert_eq!(context.stores, 0, "{name} guard must precede memory");
        assert_eq!(regs.gpr, initial_gprs, "{name}");
        assert_eq!(regs.exit_pc, 0x1000, "{name}");
    }
}
