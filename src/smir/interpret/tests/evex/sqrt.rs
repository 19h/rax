//! EVEX square-root broadcast execution tests.

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn lifted_evex_packed_sqrt_broadcast_executes_masks_and_fault_suppression() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let sentinel = [0xCAFE_BABE_DEAD_BEEFu64; 16];
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    // EVEX disp8*N uses the 4-byte broadcast tuple, not the 64-byte ZMM
    // width: [RAX + 0x10 * 4] = 0x140. One scalar source feeds 16 lanes.
    ctx.write_vreg(rax, 0x100);
    memory
        .write(0x140, &81.0f32.to_bits().to_le_bytes())
        .unwrap();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF1, 0x7C, 0x58, 0x51, 0x50, 0x10],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                u64::from(9.0f32.to_bits()),
                "VSQRTPS broadcast lane {lane}"
            );
        }
    }

    // The 8-byte tuple scales the same disp8 to 0x80. Active lanes consume
    // the shared source; inactive merging lanes retain the old destination.
    memory
        .write(0x180, &144.0f64.to_bits().to_le_bytes())
        .unwrap();
    let mask = 0b0101_0101u64;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = sentinel;
        x86.k[2] = mask;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF1, 0xFD, 0x5A, 0x51, 0x58, 0x10],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..8u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[3], lane, 64),
                if mask & (1u64 << lane) != 0 {
                    12.0f64.to_bits()
                } else {
                    sentinel[usize::from(lane)]
                },
                "VSQRTPD broadcast lane {lane}"
            );
        }
    }

    // An all-zero mask suppresses an out-of-bounds scalar source completely;
    // zeroing masking still clears every destination lane.
    ctx.write_vreg(rax, 0x400);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = sentinel;
        x86.k[2] = 0;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0xDA, 0x51, 0x18], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert!(x86.xmm[3].iter().all(|word| *word == 0));
    }

    // Activating any lane makes the shared memory operand architecturally
    // live. The read faults before the destination is committed.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = sentinel;
        x86.k[2] = 1;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0x5A, 0x51, 0x18], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[3], sentinel);
    }
}
