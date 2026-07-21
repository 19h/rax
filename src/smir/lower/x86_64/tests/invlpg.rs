//! Fault-precise native lowering coverage for x86 INVLPG.

use super::*;
use crate::smir::ir::ops::X86InvlpgOp;
use crate::smir::lower::X86_GUEST_INVLPG_FN_OFFSET;

// linux/amd64 user-mode emulation on an Arm host clears an imported AF across
// pushfq/popfq. Retain every other modeled status/control bit in executable
// cross-host tests; byte-shape tests still require both flag-save operations.
const OBSERVABLE_FLAGS: u64 = (0x08D5 | (1 << 10)) & !(1 << 4);

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn kind(addr: Address, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86Invlpg(X86InvlpgOp {
        addr,
        requires_apx,
        next_pc,
    })
}

fn lower_invlpg(op: OpKind, fault_guards: bool) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, op);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_invlpg_requires_guards_calls_helper_serializes_and_encodes_exact_frontiers() {
    assert!(matches!(
        lower_invlpg(
            kind(Address::Direct(x86(X86Reg::Rax)), false, 0x1003),
            false
        ),
        Err(LowerError::UnsupportedOp { .. })
    ));

    for (addr, requires_apx, next_pc) in [
        (Address::Direct(x86(X86Reg::Rax)), false, 0x1003),
        (
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R31)),
                index: x86(X86Reg::R16),
                scale: 8,
                disp: -16,
                disp_size: DispSize::Disp8,
            },
            true,
            0x1006,
        ),
    ] {
        let (code, _) = lower_invlpg(kind(addr, requires_apx, next_pc), true)
            .expect("guarded helper-backed INVLPG lowering");
        assert!(
            code.windows(4)
                .any(|window| window == (X86_GUEST_INVLPG_FN_OFFSET as u32).to_le_bytes()),
            "missing INVLPG helper offset: {code:02X?}"
        );
        assert!(
            code.windows(2).any(|window| window == [0x0F, 0xA2]),
            "successful INVLPG must serialize"
        );
        assert!(code.contains(&0x9C), "INVLPG must save RFLAGS: {code:02X?}");
        assert!(
            code.iter().filter(|&&byte| byte == 0x9D).count() >= 2,
            "both INVLPG exits must restore RFLAGS: {code:02X?}"
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
fn lower_invlpg_rejects_every_non_lifter_address_and_frontier_shape() {
    for malformed in [
        kind(Address::Direct(VReg::virt(0)), false, 0x1003),
        kind(Address::Direct(x86(X86Reg::Rax)), true, 0x1003),
        kind(
            Address::Direct(VReg::Arch(ArchReg::Arm(crate::smir::ir::types::ArmReg::X(
                0,
            )))),
            false,
            0x1003,
        ),
        kind(Address::Direct(x86(X86Reg::R16)), false, 0x1004),
        kind(Address::Direct(x86(X86Reg::Rax)), false, 0x1002),
        kind(Address::Direct(x86(X86Reg::Rax)), false, 0x1010),
        kind(
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
    ] {
        assert!(matches!(
            lower_invlpg(malformed, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        kind(Address::Direct(x86(X86Reg::Rax)), false, 0x1003),
    );
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
unsafe extern "C" fn invlpg_stub(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    addr: u64,
    requires_apx: u64,
) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    if state.cpl != 0 || (requires_apx != 0 && state.apx_enabled == 0) {
        return 0;
    }
    state.cr2 = addr;
    state.cr3 = requires_apx;
    1
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
    lowerer.set_jit_fault_deopt_guards(true);
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower helper-backed INVLPG sequence");
    let code = lowerer.finalize().expect("finalize INVLPG sequence");
    let exec = ExecMem::new(&code).expect("map INVLPG sequence");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.rflags = 0x2 | OBSERVABLE_FLAGS;
    regs.ac_flag = 1;
    regs.exit_pc = 0xDEAD_BEEF;
    regs.invlpg_fn = invlpg_stub as usize as u64;
    configure(&mut regs);
    exec.run(lowered.entry_offset, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_invlpg_computes_legacy_addr32_segment_and_egpr_addresses_exactly() {
    let cases: [(
        Address,
        bool,
        u64,
        fn(&mut crate::smir::lower::runtime::GuestRegs),
    ); 3] = [
        (
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rax)),
                index: x86(X86Reg::Rcx),
                scale: 4,
                disp: -8,
                disp_size: DispSize::Disp8,
            },
            false,
            0x4000_u64.wrapping_add(3 * 4).wrapping_sub(8),
            |regs: &mut crate::smir::lower::runtime::GuestRegs| {
                regs.gpr[0] = 0x4000;
                regs.gpr[1] = 3;
            },
        ),
        (
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: Some(x86(X86Reg::Rdx)),
                scale: 2,
                disp: 0x40,
            })),
            false,
            0x1000_0000 + 0x50,
            |regs: &mut crate::smir::lower::runtime::GuestRegs| {
                regs.fs_base = 0x1000_0000;
                regs.gpr[3] = 0xFFFF_FFFF_FFFF_FFF0;
                regs.gpr[2] = 0x10;
            },
        ),
        (
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R31)),
                index: x86(X86Reg::R16),
                scale: 8,
                disp: 0x20,
                disp_size: DispSize::Disp8,
            },
            true,
            0x8120,
            |regs: &mut crate::smir::lower::runtime::GuestRegs| {
                regs.gpr[31] = 0x8000;
                regs.gpr[16] = 0x20;
                regs.apx_enabled = 1;
            },
        ),
    ];

    for (addr, requires_apx, expected, configure) in cases {
        let regs = execute_native(&[(0x1000, kind(addr, requires_apx, 0x1006))], |regs| {
            regs.cpl = 0;
            configure(regs);
        });
        assert_eq!(regs.cr2, expected);
        assert_eq!(regs.cr3, u64::from(requires_apx));
        assert_eq!(regs.exit_pc, 0x1006);
        assert_eq!(regs.rflags & OBSERVABLE_FLAGS, OBSERVABLE_FLAGS);
        assert_eq!(regs.ac_flag, 1);
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_invlpg_fault_is_noncommitting_and_success_ends_before_later_ops() {
    let failed = execute_native(
        &[(
            0x2345,
            kind(Address::Direct(x86(X86Reg::Rax)), false, 0x2348),
        )],
        |regs| {
            regs.gpr[0] = 0x4000;
            regs.cpl = 3;
            regs.cr2 = 0xAAAA;
            regs.cr3 = 0xBBBB;
        },
    );
    assert_eq!(failed.exit_pc, 0x2345);
    assert_eq!(failed.cr2, 0xAAAA);
    assert_eq!(failed.cr3, 0xBBBB);
    assert_eq!(failed.gpr[0], 0x4000);
    assert_eq!(failed.rflags & OBSERVABLE_FLAGS, OBSERVABLE_FLAGS);

    let completed = execute_native(
        &[
            (
                0x1000,
                kind(Address::Direct(x86(X86Reg::Rax)), false, 0x1003),
            ),
            (
                0x1003,
                kind(Address::Direct(x86(X86Reg::Rbx)), false, 0x1006),
            ),
        ],
        |regs| {
            regs.cpl = 0;
            regs.gpr[0] = 0x4000;
            regs.gpr[3] = 0x9000;
        },
    );
    assert_eq!(completed.exit_pc, 0x1003);
    assert_eq!(completed.cr2, 0x4000, "only the first helper call executes");
}
