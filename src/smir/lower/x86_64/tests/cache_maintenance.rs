//! Cache-maintenance serialization and privilege handoff in emitted x86 code.

use super::*;
use crate::smir::ir::ops::X86CacheMaintenanceKind;

fn lower_cache(
    kind: X86CacheMaintenanceKind,
    next_pc: u64,
    guards: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, OpKind::X86CacheMaintenance { kind, next_pc });
    // A successful cache operation must return before this later instruction.
    builder.push_op(
        next_pc,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            src: SrcOperand::Imm(0xbad),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn cache_lowering_requires_guards_and_never_emits_a_privileged_host_cache_opcode() {
    for (kind, length) in [
        (X86CacheMaintenanceKind::Invd, 2),
        (X86CacheMaintenanceKind::Wbinvd, 2),
        (X86CacheMaintenanceKind::Wbnoinvd, 3),
    ] {
        assert!(matches!(
            lower_cache(kind, 0x1000 + length, false),
            Err(LowerError::UnsupportedOp { .. })
        ));
        let (code, _) = lower_cache(kind, 0x1000 + length, true).unwrap();
        assert!(
            !code
                .windows(2)
                .any(|bytes| bytes == [0x0f, 0x08] || bytes == [0x0f, 0x09])
        );
        assert!(code.windows(3).any(|bytes| bytes == [0x0f, 0xae, 0xf0]));
    }
}

#[test]
fn cache_lowering_rejects_impossible_instruction_boundaries() {
    for (kind, next_pc) in [
        (X86CacheMaintenanceKind::Invd, 0x1000),
        (X86CacheMaintenanceKind::Wbinvd, 0x1001),
        (X86CacheMaintenanceKind::Wbnoinvd, 0x1002),
        (X86CacheMaintenanceKind::Wbinvd, 0x1010),
    ] {
        assert!(matches!(
            lower_cache(kind, next_pc, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn native(
    kind: X86CacheMaintenanceKind,
    next_pc: u64,
    cr0: u64,
    cpl: u64,
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};
    let (code, entry) = lower_cache(kind, next_pc, true).unwrap();
    let exec = ExecMem::new(&code).unwrap();
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xa500000000000000 | index as u64;
    }
    regs.rflags = 0x2 | 0x08d5 | (1 << 10);
    regs.ac_flag = 1;
    regs.cr0 = cr0;
    regs.cpl = cpl;
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_cache_maintenance_preserves_state_and_serializes_at_the_next_guest_pc() {
    for (kind, length) in [
        (X86CacheMaintenanceKind::Invd, 2),
        (X86CacheMaintenanceKind::Wbinvd, 2),
        (X86CacheMaintenanceKind::Wbnoinvd, 3),
    ] {
        for (cr0, cpl) in [(0x8005003b, 0), (0x8005003a, 3)] {
            let regs = native(kind, 0x1000 + length, cr0, cpl);
            assert_eq!(regs.exit_pc, 0x1000 + length);
            assert_eq!(regs.cr0, cr0);
            assert_eq!(regs.ac_flag, 1);
            for (index, value) in regs.gpr.iter().enumerate() {
                assert_eq!(*value, 0xa500000000000000 | index as u64);
            }
            assert_eq!(regs.rflags & (0x08d5 | (1 << 10)), 0x08d5 | (1 << 10));
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_cache_maintenance_privilege_failures_return_at_the_faulting_guest_pc() {
    for (kind, length) in [
        (X86CacheMaintenanceKind::Invd, 2),
        (X86CacheMaintenanceKind::Wbinvd, 2),
        (X86CacheMaintenanceKind::Wbnoinvd, 3),
    ] {
        for cpl in 1..=3 {
            let regs = native(kind, 0x1000 + length, 0x8005003b, cpl);
            assert_eq!(regs.exit_pc, 0x1000);
            assert_eq!(regs.cr0, 0x8005003b);
            assert_eq!(regs.gpr[0], 0xa500000000000000);
            assert_eq!(regs.rflags & (0x08d5 | (1 << 10)), 0x08d5 | (1 << 10));
        }
    }
}
