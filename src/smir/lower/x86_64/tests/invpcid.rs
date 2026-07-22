//! Fault-precise native lowering coverage for x86 INVPCID.

use super::*;
use crate::smir::ir::ops::X86InvpcidOp;
use crate::smir::lower::X86_GUEST_INVPCID_FN_OFFSET;

// linux/amd64 user-mode emulation on an Arm host clears an imported AF across
// pushfq/popfq. Retain every other modeled status/control bit in executable
// cross-host tests; byte-shape tests still require both flag-save operations.
const OBSERVABLE_FLAGS: u64 = (0x08D5 | (1 << 10)) & !(1 << 4);

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn kind(
    invpcid_type: VReg,
    addr: Address,
    requires_apx: bool,
    stack_segment: bool,
    next_pc: u64,
) -> OpKind {
    OpKind::X86Invpcid(X86InvpcidOp {
        invpcid_type,
        addr,
        requires_apx,
        stack_segment,
        next_pc,
    })
}

fn lower_invpcid(
    op: OpKind,
    mem_helpers: bool,
    fault_guards: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, op);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_invpcid_requires_memory_and_fault_helpers_then_serializes_exact_frontiers() {
    let exact = kind(
        x86(X86Reg::Rax),
        Address::Direct(x86(X86Reg::Rbx)),
        false,
        false,
        0x1005,
    );
    assert!(matches!(
        lower_invpcid(exact.clone(), false, true),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower_invpcid(exact, true, false),
        Err(LowerError::UnsupportedOp { .. })
    ));

    for (type_reg, addr, requires_apx, next_pc) in [
        (
            x86(X86Reg::Rax),
            Address::Direct(x86(X86Reg::Rbx)),
            false,
            0x1005,
        ),
        (
            x86(X86Reg::R31),
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R20)),
                index: x86(X86Reg::R29),
                scale: 8,
                disp: 0x40,
                disp_size: DispSize::Disp8,
            },
            true,
            0x1008,
        ),
    ] {
        let (code, _) = lower_invpcid(
            kind(type_reg, addr, requires_apx, false, next_pc),
            true,
            true,
        )
        .expect("guarded helper-backed INVPCID lowering");
        assert!(
            code.windows(4)
                .any(|window| window == (X86_GUEST_INVPCID_FN_OFFSET as u32).to_le_bytes()),
            "missing INVPCID helper offset: {code:02X?}"
        );
        assert!(
            code.windows(2).any(|window| window == [0x0F, 0xA2]),
            "successful INVPCID must serialize"
        );
        assert!(
            code.contains(&0x9C),
            "INVPCID must save RFLAGS: {code:02X?}"
        );
        assert!(
            code.iter().filter(|&&byte| byte == 0x9D).count() >= 2,
            "both INVPCID exits must restore RFLAGS: {code:02X?}"
        );
        assert!(
            code.windows(4)
                .any(|window| window == 0x1000_u32.to_le_bytes()),
            "fault exit must retain the original PC"
        );
        assert!(
            code.windows(4)
                .any(|window| window == (next_pc as u32).to_le_bytes()),
            "success exit must use the exact next PC"
        );
    }
}

#[test]
fn lower_invpcid_rejects_every_non_lifter_register_address_and_frontier_shape() {
    for malformed in [
        kind(
            VReg::virt(0),
            Address::Direct(x86(X86Reg::Rbx)),
            false,
            false,
            0x1005,
        ),
        kind(
            x86(X86Reg::R16),
            Address::Direct(x86(X86Reg::Rbx)),
            false,
            false,
            0x1005,
        ),
        kind(
            x86(X86Reg::Rax),
            Address::Direct(VReg::virt(0)),
            false,
            false,
            0x1005,
        ),
        kind(
            x86(X86Reg::Rax),
            Address::Direct(VReg::Arch(ArchReg::Arm(crate::smir::ir::types::ArmReg::X(
                0,
            )))),
            false,
            false,
            0x1005,
        ),
        kind(
            x86(X86Reg::Rax),
            Address::Direct(x86(X86Reg::R16)),
            false,
            false,
            0x1005,
        ),
        kind(
            x86(X86Reg::Rax),
            Address::Direct(x86(X86Reg::Rbx)),
            false,
            false,
            0x1004,
        ),
        kind(
            x86(X86Reg::Rax),
            Address::Direct(x86(X86Reg::Rbx)),
            true,
            false,
            0x1005,
        ),
        kind(
            x86(X86Reg::Rax),
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rbx)),
                index: x86(X86Reg::Rcx),
                scale: 3,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            false,
            false,
            0x1007,
        ),
    ] {
        assert!(matches!(
            lower_invpcid(malformed, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        kind(
            x86(X86Reg::Rax),
            Address::Direct(x86(X86Reg::Rbx)),
            false,
            false,
            0x1005,
        ),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(matches!(
        lowerer.lower_function(&hinted),
        Err(LowerError::InvalidOperand { .. })
    ));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
unsafe extern "C" fn invpcid_stub(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    addr: u64,
    invpcid_type: u64,
    requires_apx: u64,
) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    if state.cpl != 0 || (requires_apx != 0 && state.apx_enabled == 0) {
        return 0;
    }
    state.cr2 = addr;
    state.cr3 = invpcid_type;
    state.efer = requires_apx;
    u64::from(invpcid_type != 0xDEAD)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native(
    ops: &[(u64, OpKind)],
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (pc, op) in ops {
        builder.push_op(*pc, op.clone());
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower helper-backed INVPCID sequence");
    let code = lowerer.finalize().expect("finalize INVPCID sequence");
    let exec = ExecMem::new(&code).expect("map INVPCID sequence");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.rflags = 0x2 | OBSERVABLE_FLAGS;
    regs.ac_flag = 1;
    regs.cs_l = 1;
    regs.exit_pc = 0xDEAD_BEEF;
    regs.invpcid_fn = invpcid_stub as usize as u64;
    configure(&mut regs);
    exec.run(lowered.entry_offset, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_invpcid_passes_type_and_legacy_addr32_segment_and_egpr_addresses_exactly() {
    let cases: [(
        VReg,
        Address,
        bool,
        u64,
        u64,
        fn(&mut crate::smir::lower::runtime::GuestRegs),
    ); 3] = [
        (
            x86(X86Reg::Rdx),
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rax)),
                index: x86(X86Reg::Rcx),
                scale: 4,
                disp: -8,
                disp_size: DispSize::Disp8,
            },
            false,
            2,
            0x4000_u64.wrapping_add(3 * 4).wrapping_sub(8),
            |regs| {
                regs.gpr[0] = 0x4000;
                regs.gpr[1] = 3;
                regs.gpr[2] = 2;
            },
        ),
        (
            x86(X86Reg::Rsp),
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: Some(x86(X86Reg::Rdx)),
                scale: 2,
                disp: 0x40,
            })),
            false,
            3,
            0x1000_0000 + 0x50,
            |regs| {
                regs.fs_base = 0x1000_0000;
                regs.gpr[3] = 0xFFFF_FFFF_FFFF_FFF0;
                regs.gpr[2] = 0x10;
                regs.gpr[4] = 3;
            },
        ),
        (
            x86(X86Reg::R31),
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R20)),
                index: x86(X86Reg::R29),
                scale: 8,
                disp: 0x40,
                disp_size: DispSize::Disp8,
            },
            true,
            1,
            0x2140,
            |regs| {
                regs.gpr[20] = 0x2000;
                regs.gpr[29] = 0x20;
                regs.gpr[31] = 1;
                regs.apx_enabled = 1;
            },
        ),
    ];

    for (type_reg, addr, requires_apx, expected_type, expected_addr, configure) in cases {
        let regs = execute_native(
            &[(0x1000, kind(type_reg, addr, requires_apx, false, 0x1008))],
            |regs| {
                regs.cpl = 0;
                configure(regs);
            },
        );
        assert_eq!(regs.cr2, expected_addr);
        assert_eq!(regs.cr3, expected_type);
        assert_eq!(regs.efer, u64::from(requires_apx));
        assert_eq!(regs.exit_pc, 0x1008);
        assert_eq!(regs.rflags & OBSERVABLE_FLAGS, OBSERVABLE_FLAGS);
        assert_eq!(regs.ac_flag, 1);
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_invpcid_guards_and_helper_fault_are_noncommitting_and_success_is_terminal() {
    for (name, type_value, cpl, apx_enabled, expected_helper_call) in [
        ("CPL", 2, 3, true, false),
        ("APX", 2, 0, false, false),
        ("helper", 0xDEAD, 0, true, true),
    ] {
        let failed = execute_native(
            &[(
                0x2345,
                kind(
                    x86(X86Reg::Rax),
                    Address::Direct(x86(X86Reg::Rbx)),
                    true,
                    false,
                    0x234B,
                ),
            )],
            |regs| {
                regs.gpr[0] = type_value;
                regs.gpr[3] = 0x4000;
                regs.cpl = cpl;
                regs.apx_enabled = u64::from(apx_enabled);
                regs.cr2 = 0xAAAA;
                regs.cr3 = 0xBBBB;
            },
        );
        assert_eq!(failed.exit_pc, 0x2345, "{name}");
        assert_eq!(failed.cr2 != 0xAAAA, expected_helper_call, "{name}");
        assert_eq!(failed.cr3 != 0xBBBB, expected_helper_call, "{name}");
        assert_eq!(failed.gpr[0], type_value, "{name}");
        assert_eq!(failed.gpr[3], 0x4000, "{name}");
        assert_eq!(failed.rflags & OBSERVABLE_FLAGS, OBSERVABLE_FLAGS, "{name}");
    }

    let completed = execute_native(
        &[
            (
                0x1000,
                kind(
                    x86(X86Reg::Rax),
                    Address::Direct(x86(X86Reg::Rbx)),
                    false,
                    false,
                    0x1005,
                ),
            ),
            (
                0x1005,
                kind(
                    x86(X86Reg::Rcx),
                    Address::Direct(x86(X86Reg::Rdx)),
                    false,
                    false,
                    0x100A,
                ),
            ),
        ],
        |regs| {
            regs.cpl = 0;
            regs.gpr[0] = 2;
            regs.gpr[3] = 0x4000;
            regs.gpr[1] = 3;
            regs.gpr[2] = 0x9000;
        },
    );
    assert_eq!(completed.exit_pc, 0x1005);
    assert_eq!(completed.cr2, 0x4000, "only the first helper executes");
    assert_eq!(completed.cr3, 2);
}
