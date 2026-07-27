//! Exact lifted legacy, VEX, and REX2 MXCSR memory semantics.

use super::*;

#[test]
fn lifted_legacy_vex_and_rex2_mxcsr_roundtrip_and_fault_atomicity() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rsi = VReg::Arch(ArchReg::X86(X86Reg::Rsi));
    let rdi = VReg::Arch(ArchReg::X86(X86Reg::Rdi));
    let r31 = VReg::Arch(ArchReg::X86(X86Reg::R31));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.write_vreg(rax, 0x200);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    memory.write(0x200, &0x5F80u32.to_le_bytes()).unwrap();
    execute_lifted_x86(&[0x0F, 0xAE, 0x10], &mut ctx, &mut memory); // LDMXCSR [RAX]
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mxcsr, 0x5F80);
    }
    execute_lifted_x86(&[0x0F, 0xAE, 0x58, 0x04], &mut ctx, &mut memory); // STMXCSR [RAX+4]
    let mut stored = [0u8; 4];
    memory.read(0x204, &mut stored).unwrap();
    assert_eq!(u32::from_le_bytes(stored), 0x5F80);

    memory.write(0x208, &0x3F80u32.to_le_bytes()).unwrap();
    execute_lifted_x86(&[0xC5, 0xF8, 0xAE, 0x50, 0x08], &mut ctx, &mut memory); // VLDMXCSR [RAX+8]
    execute_lifted_x86(&[0xC5, 0xF8, 0xAE, 0x58, 0x0C], &mut ctx, &mut memory); // VSTMXCSR [RAX+12]
    memory.read(0x20C, &mut stored).unwrap();
    assert_eq!(u32::from_le_bytes(stored), 0x3F80);

    memory.write(0x200, &0x7F80u32.to_le_bytes()).unwrap();
    ctx.write_vreg(rdi, 0xAAAA_BBBB_0000_0100);
    ctx.write_vreg(rsi, 0xCCCC_DDDD_0000_0080);
    execute_lifted_x86(&[0x67, 0x0F, 0xAE, 0x14, 0x77], &mut ctx, &mut memory); // LDMXCSR [edi+esi*2]
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mxcsr, 0x7F80);
    }

    let mut short_memory = FlatMemory::new(0x202);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mxcsr = 0x1F80;
    }
    let exit = execute_lifted_x86(&[0x0F, 0xAE, 0x10], &mut ctx, &mut short_memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mxcsr, 0x1F80, "faulting load changed MXCSR");
    }

    memory.write(0x200, &0x0001_1F80u32.to_le_bytes()).unwrap();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mxcsr = 0x3F80;
    }
    let exit = execute_lifted_x86(&[0xC5, 0xF8, 0xAE, 0x10], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0
        })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mxcsr, 0x3F80, "reserved-bit load changed MXCSR");
    }

    // A REX2 store owns its APX guard inside the operation so #UD precedes
    // both address evaluation and the memory write without a duplicate guard.
    memory.write(0x210, &0xDEAD_BEEFu32.to_le_bytes()).unwrap();
    ctx.write_vreg(r31, 0x1000);
    let exit = execute_lifted_x86(&[0xD5, 0x91, 0xAE, 0x1F], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::Undefined {
            addr: 0x1000,
            opcode: 0
        })
    ));
    memory.read(0x210, &mut stored).unwrap();
    assert_eq!(u32::from_le_bytes(stored), 0xDEAD_BEEF);

    ctx.write_vreg(r31, 0x210);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.apx_enabled = true;
    }
    execute_lifted_x86(&[0xD5, 0x91, 0xAE, 0x1F], &mut ctx, &mut memory);
    memory.read(0x210, &mut stored).unwrap();
    assert_eq!(u32::from_le_bytes(stored), 0x3F80);

    // CR0.TS requests direct replay for #NM before either memory direction.
    // Use an out-of-range address so a missing guard reports a memory fault
    // instead, and preserve the existing MXCSR value across both forms.
    ctx.write_vreg(rax, 0x1000);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.cr0 |= 1 << 3;
    }
    for bytes in [&[0x0F, 0xAE, 0x10][..], &[0xC5, 0xF8, 0xAE, 0x18][..]] {
        let exit = execute_lifted_x86(bytes, &mut ctx, &mut memory);
        assert!(matches!(
            exit,
            BlockResult::Exit(ExitReason::Undefined {
                addr: 0x1000,
                opcode: 0
            })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr, 0x3F80, "CR0.TS path changed MXCSR");
        }
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
