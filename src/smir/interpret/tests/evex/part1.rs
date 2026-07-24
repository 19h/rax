//! evex part 1 tests

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn lifted_evex_unaligned_fp_moves_execute_masks_fault_suppression_and_partial_stores() {
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let sentinel = [0xA5A5_A5A5_A5A5_A5A5; 16];
    let mut memory = FlatMemory::new(0x500);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    let mask32 = 0xA55Au64;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
        for lane in 0..16u8 {
            SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 32, 0x1020_3000 + u64::from(lane));
        }
        x86.k[1] = mask32;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x49, 0x10, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                if mask32 & (1u64 << lane) != 0 {
                    0x1020_3000 + u64::from(lane)
                } else {
                    0xA5A5_A5A5
                }
            );
        }
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    let mask64 = 0x5Au64;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
        for lane in 0..8u8 {
            SmirInterpreter::set_lane(
                &mut x86.xmm[1],
                lane,
                64,
                0x1122_3344_5566_7700 + u64::from(lane),
            );
        }
        x86.k[2] = mask64;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0xCA, 0x10, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..8u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[2], lane, 64),
                if mask64 & (1u64 << lane) != 0 {
                    0x1122_3344_5566_7700 + u64::from(lane)
                } else {
                    0
                }
            );
        }
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    let load_mask = 0x9669u64;
    let load_values = (0..16u32)
        .map(|lane| 0xC000_0000u32.wrapping_add(lane * 0x0101_0101))
        .collect::<Vec<_>>();
    let load_bytes = load_values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    memory.write(0x101, &load_bytes).unwrap();
    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
        x86.k[1] = load_mask;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x49, 0x10, 0x10], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                if load_mask & (1u64 << lane) != 0 {
                    u64::from(load_values[usize::from(lane)])
                } else {
                    0xA5A5_A5A5
                }
            );
        }
    }

    // An all-zero writemask suppresses every element access even when the
    // base is outside mapped memory. The first active lane exposes the
    // fault before any architectural destination lane is committed.
    ctx.write_vreg(rax, 0x500);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
        x86.k[1] = 0;
    }
    let suppressed =
        execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x49, 0x10, 0x10], &mut ctx, &mut memory);
    assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[2][..8], &sentinel[..8]);
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
        x86.k[1] = 1;
    }
    let load_fault =
        execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x49, 0x10, 0x10], &mut ctx, &mut memory);
    assert!(matches!(
        load_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], sentinel);
    }

    let store_mask = 0xA55Au64;
    let store_values = (0..16u32)
        .map(|lane| 0x5060_7000u32.wrapping_add(lane * 0x0011_2233))
        .collect::<Vec<_>>();
    memory.write(0x201, &[0x5A; 64]).unwrap();
    ctx.write_vreg(rax, 0x201);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        for (lane, value) in store_values.iter().copied().enumerate() {
            SmirInterpreter::set_lane(&mut x86.xmm[1], lane as u8, 32, u64::from(value));
        }
        x86.k[1] = store_mask;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x49, 0x11, 0x08], &mut ctx, &mut memory);
    let mut stored = [0; 64];
    memory.read(0x201, &mut stored).unwrap();
    for lane in 0..16 {
        let actual = &stored[lane * 4..lane * 4 + 4];
        if store_mask & (1u64 << lane) != 0 {
            assert_eq!(actual, &store_values[lane].to_le_bytes());
        } else {
            assert_eq!(actual, &[0x5A; 4]);
        }
    }

    ctx.write_vreg(rax, 0x500);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.k[1] = 0;
    }
    let suppressed_store =
        execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x49, 0x11, 0x08], &mut ctx, &mut memory);
    assert!(matches!(
        suppressed_store,
        BlockResult::Exit(ExitReason::Halt)
    ));

    // Stores commit in increasing element order. Lane 0 is mapped, lane 1
    // is inactive, and active lane 2 faults at the exact memory boundary.
    memory.write(0x4F8, &[0x6B; 8]).unwrap();
    ctx.write_vreg(rax, 0x4F8);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.k[1] = 0b0101;
    }
    let store_fault =
        execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x49, 0x11, 0x08], &mut ctx, &mut memory);
    assert!(matches!(
        store_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
    ));
    let mut partial = [0; 8];
    memory.read(0x4F8, &mut partial).unwrap();
    assert_eq!(&partial[..4], &store_values[0].to_le_bytes());
    assert_eq!(&partial[4..], &[0x6B; 4]);

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_evex_unaligned_integer_moves_execute_all_element_masks_and_faults() {
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let sentinel = [0xA5A5_A5A5_A5A5_A5A5; 16];
    let mut memory = FlatMemory::new(0x500);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    let mask8 = 0xA55A_9669_3CC3_F00Fu64;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
        for lane in 0..64u8 {
            SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 8, 0x40 + u64::from(lane));
        }
        x86.k[1] = mask8;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0x7F, 0x49, 0x6F, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..64u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[2], lane, 8),
                if mask8 & (1u64 << lane) != 0 {
                    0x40 + u64::from(lane)
                } else {
                    0xA5
                }
            );
        }
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    let mask16 = 0xA55A_9669u64;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[4] = sentinel;
        for lane in 0..32u8 {
            SmirInterpreter::set_lane(&mut x86.xmm[3], lane, 16, 0x1100 + u64::from(lane));
        }
        x86.k[2] = mask16;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0xFF, 0xCA, 0x6F, 0xE3], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..32u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[4], lane, 16),
                if mask16 & (1u64 << lane) != 0 {
                    0x1100 + u64::from(lane)
                } else {
                    0
                }
            );
        }
        assert!(x86.xmm[4][8..].iter().all(|word| *word == 0));
    }

    let mask32 = 0xA55Au64;
    let load_values = (0..16u32)
        .map(|lane| 0xC010_2000u32.wrapping_add(lane * 0x0101_0101))
        .collect::<Vec<_>>();
    let load_bytes = load_values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    memory.write(0x101, &load_bytes).unwrap();
    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[5] = sentinel;
        x86.k[3] = mask32;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x4B, 0x6F, 0x28], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[5], lane, 32),
                if mask32 & (1u64 << lane) != 0 {
                    u64::from(load_values[usize::from(lane)])
                } else {
                    0xA5A5_A5A5
                }
            );
        }
    }

    // Masked loads are element-fault-suppressing and commit the register
    // only after every active load has succeeded.
    ctx.write_vreg(rax, 0x500);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[5] = sentinel;
        x86.k[3] = 0;
    }
    let suppressed =
        execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x4B, 0x6F, 0x28], &mut ctx, &mut memory);
    assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[5][..8], &sentinel[..8]);
        assert!(x86.xmm[5][8..].iter().all(|word| *word == 0));
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[5] = sentinel;
        x86.k[3] = 1;
    }
    let load_fault =
        execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x4B, 0x6F, 0x28], &mut ctx, &mut memory);
    assert!(matches!(
        load_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[5], sentinel);
    }

    let mask64 = 0b1010_0101u64;
    let store_values = (0..8u64)
        .map(|lane| 0x5060_7080_90A0_B000u64.wrapping_add(lane * 0x0011_2233_4455_6677))
        .collect::<Vec<_>>();
    memory.write(0x201, &[0x5A; 64]).unwrap();
    ctx.write_vreg(rax, 0x201);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        for (lane, value) in store_values.iter().copied().enumerate() {
            SmirInterpreter::set_lane(&mut x86.xmm[6], lane as u8, 64, value);
        }
        x86.k[4] = mask64;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0xFE, 0x4C, 0x7F, 0x30], &mut ctx, &mut memory);
    let mut stored = [0; 64];
    memory.read(0x201, &mut stored).unwrap();
    for lane in 0..8 {
        let actual = &stored[lane * 8..lane * 8 + 8];
        if mask64 & (1u64 << lane) != 0 {
            assert_eq!(actual, &store_values[lane].to_le_bytes());
        } else {
            assert_eq!(actual, &[0x5A; 8]);
        }
    }

    ctx.write_vreg(rax, 0x500);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.k[4] = 0;
    }
    let suppressed_store =
        execute_lifted_x86(&[0x62, 0xF1, 0xFE, 0x4C, 0x7F, 0x30], &mut ctx, &mut memory);
    assert!(matches!(
        suppressed_store,
        BlockResult::Exit(ExitReason::Halt)
    ));

    // Active stores commit in lane order. Lane 0 completes at the last
    // mapped qword; lane 1 then faults at the exact memory boundary.
    memory.write(0x4F8, &[0x6B; 8]).unwrap();
    ctx.write_vreg(rax, 0x4F8);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.k[4] = 0b0011;
    }
    let store_fault =
        execute_lifted_x86(&[0x62, 0xF1, 0xFE, 0x4C, 0x7F, 0x30], &mut ctx, &mut memory);
    assert!(matches!(
        store_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
    ));
    let mut partial = [0; 8];
    memory.read(0x4F8, &mut partial).unwrap();
    assert_eq!(partial, store_values[0].to_le_bytes());

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_evex_aligned_moves_execute_masks_and_type_e1_fault_order() {
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let sentinel = [0xA5A5_A5A5_A5A5_A5A5; 16];
    let mut memory = FlatMemory::new(0x500);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    let mask32 = 0xA55Au64;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
        for lane in 0..16u8 {
            SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 32, 0x4010_2000 + u64::from(lane));
        }
        x86.k[1] = mask32;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x49, 0x28, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                if mask32 & (1u64 << lane) != 0 {
                    0x4010_2000 + u64::from(lane)
                } else {
                    0xA5A5_A5A5
                }
            );
        }
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    let mask64 = 0b1010_0101u64;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[4] = sentinel;
        for lane in 0..8u8 {
            SmirInterpreter::set_lane(
                &mut x86.xmm[3],
                lane,
                64,
                0x5010_2030_4050_6000 + u64::from(lane),
            );
        }
        x86.k[2] = mask64;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0xCA, 0x28, 0xE3], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..8u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[4], lane, 64),
                if mask64 & (1u64 << lane) != 0 {
                    0x5010_2030_4050_6000 + u64::from(lane)
                } else {
                    0
                }
            );
        }
        assert!(x86.xmm[4][8..].iter().all(|word| *word == 0));
    }

    let load_values = (0..16u32)
        .map(|lane| 0xC010_2000u32.wrapping_add(lane * 0x0101_0101))
        .collect::<Vec<_>>();
    let load_bytes = load_values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    memory.write(0x100, &load_bytes).unwrap();
    ctx.write_vreg(rax, 0x100);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[5] = sentinel;
        x86.k[3] = mask32;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x4B, 0x6F, 0x28], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[5], lane, 32),
                if mask32 & (1u64 << lane) != 0 {
                    u64::from(load_values[usize::from(lane)])
                } else {
                    0xA5A5_A5A5
                }
            );
        }
    }

    // E1 suppresses address/page faults for all-zero masks, but its
    // vector-width alignment #GP is unconditional and executes first.
    ctx.write_vreg(rax, 0x500);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[5] = sentinel;
        x86.k[3] = 0;
    }
    let suppressed_load =
        execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x4B, 0x6F, 0x28], &mut ctx, &mut memory);
    assert!(matches!(
        suppressed_load,
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[5][..8], &sentinel[..8]);
        assert!(x86.xmm[5][8..].iter().all(|word| *word == 0));
    }

    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[5] = sentinel;
    }
    let misaligned_load =
        execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x4B, 0x6F, 0x28], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned_load,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[5], sentinel);
    }

    let store_values = (0..8u64)
        .map(|lane| 0x6070_8090_A0B0_C000u64.wrapping_add(lane * 0x0011_2233_4455_6677))
        .collect::<Vec<_>>();
    memory.write(0x200, &[0x5A; 64]).unwrap();
    ctx.write_vreg(rax, 0x200);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        for (lane, value) in store_values.iter().copied().enumerate() {
            SmirInterpreter::set_lane(&mut x86.xmm[6], lane as u8, 64, value);
        }
        x86.k[4] = mask64;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0x4C, 0x7F, 0x30], &mut ctx, &mut memory);
    let mut stored = [0; 64];
    memory.read(0x200, &mut stored).unwrap();
    for lane in 0..8 {
        let actual = &stored[lane * 8..lane * 8 + 8];
        if mask64 & (1u64 << lane) != 0 {
            assert_eq!(actual, &store_values[lane].to_le_bytes());
        } else {
            assert_eq!(actual, &[0x5A; 8]);
        }
    }

    ctx.write_vreg(rax, 0x500);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.k[4] = 0;
    }
    let suppressed_store =
        execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0x4C, 0x7F, 0x30], &mut ctx, &mut memory);
    assert!(matches!(
        suppressed_store,
        BlockResult::Exit(ExitReason::Halt)
    ));

    memory.write(0x201, &[0x6B; 64]).unwrap();
    ctx.write_vreg(rax, 0x201);
    let misaligned_store =
        execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0x4C, 0x7F, 0x30], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned_store,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    let mut unchanged = [0; 64];
    memory.read(0x201, &mut unchanged).unwrap();
    assert_eq!(unchanged, [0x6B; 64]);

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_evex_scalar_masks_merge_zero_fault_suppress_and_address_high_registers() {
    fn seeded(low: f32, lanes: [u32; 3], upper: u64) -> VecValue {
        let mut value = [upper; 16];
        value[0] = low.to_bits() as u64 | ((lanes[0] as u64) << 32);
        value[1] = lanes[1] as u64 | ((lanes[2] as u64) << 32);
        value
    }

    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let src1 = seeded(8.0, [0x1111_2222, 0x3333_4444, 0x5555_6666], 0xAAAA);
    let src2 = seeded(2.0, [7, 8, 9], 0xBBBB);
    let old_dst = seeded(-5.0, [1, 2, 3], 0xCCCC);
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = src1;
        x86.xmm[1] = src2;
        x86.xmm[2] = old_dst;
    }
    ctx.write_vreg(k1, 0);
    execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x09, 0x58, 0xD1], &mut ctx, &mut memory); // VADDSS XMM2{k1},XMM0,XMM1
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2][0] as u32, (-5.0f32).to_bits());
        assert_eq!(x86.xmm[2][0] >> 32, src1[0] >> 32);
        assert_eq!(x86.xmm[2][1], src1[1]);
        assert!(x86.xmm[2][2..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = old_dst;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x89, 0x5C, 0xD1], &mut ctx, &mut memory); // VSUBSS XMM2{k1}{z},XMM0,XMM1
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2][0] as u32, 0);
        assert_eq!(x86.xmm[2][0] >> 32, src1[0] >> 32);
        assert_eq!(x86.xmm[2][1], src1[1]);
        assert!(x86.xmm[2][2..].iter().all(|word| *word == 0));
    }

    ctx.write_vreg(k1, 1);
    execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x09, 0x58, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2][0] as u32, 10.0f32.to_bits());
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0][0] = 9.0f64.to_bits();
        x86.xmm[0][1] = 0x0123_4567_89AB_CDEF;
        x86.xmm[1][0] = 3.0f64.to_bits();
        x86.xmm[3] = [u64::MAX; 16];
    }
    execute_lifted_x86(&[0x62, 0xF1, 0xFF, 0x09, 0x5E, 0xD9], &mut ctx, &mut memory); // VDIVSD XMM3{k1},XMM0,XMM1
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[3][0], 3.0f64.to_bits());
        assert_eq!(x86.xmm[3][1], 0x0123_4567_89AB_CDEF);
        assert!(x86.xmm[3][2..].iter().all(|word| *word == 0));
    }

    // A false scalar mask suppresses the memory read and its fault while
    // still applying the architectural merge/upper-zero result.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = src1;
    }
    ctx.write_vreg(rax, 0x2000);
    ctx.write_vreg(k1, 0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = old_dst;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x09, 0x58, 0x10], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2][0] as u32, old_dst[0] as u32);
        assert_eq!(x86.xmm[2][0] >> 32, src1[0] >> 32);
    }

    ctx.write_vreg(k1, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = old_dst;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x09, 0x58, 0x10], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], old_dst);
    }

    let high_src1 = seeded(3.0, [0x10, 0x20, 0x30], 0x1111);
    let high_src2 = seeded(4.0, [0x40, 0x50, 0x60], 0x2222);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[16] = high_src1;
        x86.xmm[17] = high_src2;
        x86.xmm[18] = [u64::MAX; 16];
    }
    execute_lifted_x86(&[0x62, 0xA1, 0x7E, 0x00, 0x58, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[18][0] as u32, 7.0f32.to_bits());
        assert_eq!(x86.xmm[18][0] >> 32, high_src1[0] >> 32);
        assert_eq!(x86.xmm[18][1], high_src1[1]);
        assert!(x86.xmm[18][2..].iter().all(|word| *word == 0));
    }
}
#[test]
fn lifted_evex_packed_logic_masks_broadcast_and_fault_suppression_are_exact() {
    fn lane32(value: &VecValue, lane: u8) -> u32 {
        ((value[(lane / 2) as usize] >> (u32::from(lane & 1) * 32)) & 0xFFFF_FFFF) as u32
    }

    fn set_lane32(value: &mut VecValue, lane: u8, bits: u32) {
        let word = &mut value[(lane / 2) as usize];
        let shift = u32::from(lane & 1) * 32;
        *word = (*word & !(0xFFFF_FFFFu64 << shift)) | (u64::from(bits) << shift);
    }

    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut src1 = [0u64; 16];
    let mut src2 = [0u64; 16];
    let mut old = [0xD0D0_D0D0_D0D0_D0D0u64; 16];
    for lane in 0..16u8 {
        set_lane32(&mut src1, lane, 0x0F0F_0000 | u32::from(lane));
        set_lane32(&mut src2, lane, 0xF0FF_00F0 ^ (u32::from(lane) * 0x101));
        set_lane32(&mut old, lane, 0xA000_0000 | u32::from(lane));
    }
    let mask = 0xA55Au64;
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();

    for (opcode, apply) in [
        (0x54, (|a: u32, b: u32| a & b) as fn(u32, u32) -> u32),
        (0x55, (|a: u32, b: u32| !a & b) as fn(u32, u32) -> u32),
        (0x56, (|a: u32, b: u32| a | b) as fn(u32, u32) -> u32),
        (0x57, (|a: u32, b: u32| a ^ b) as fn(u32, u32) -> u32),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = src1;
            x86.xmm[1] = src2;
            x86.xmm[2] = old;
        }
        ctx.write_vreg(k1, mask);
        let exit = execute_lifted_x86(
            &[0x62, 0xF1, 0x7C, 0x49, opcode, 0xD1],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        let actual = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.xmm[2],
            _ => unreachable!(),
        };
        for lane in 0..16u8 {
            let expected = if mask & (1 << lane) != 0 {
                apply(lane32(&src1, lane), lane32(&src2, lane))
            } else {
                lane32(&old, lane)
            };
            assert_eq!(
                lane32(&actual, lane),
                expected,
                "opcode {opcode:02X}, lane {lane}"
            );
        }
        assert!(actual[8..].iter().all(|word| *word == 0));
    }

    // Zeroing masking writes zero rather than the prior destination for
    // inactive dword elements and still clears all state above VL.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = src1;
        x86.xmm[1] = src2;
        x86.xmm[2] = old;
    }
    ctx.write_vreg(k1, mask);
    execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0xC9, 0x55, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            let expected = if mask & (1 << lane) != 0 {
                !lane32(&src1, lane) & lane32(&src2, lane)
            } else {
                0
            };
            assert_eq!(lane32(&x86.xmm[2], lane), expected, "zero lane {lane}");
        }
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    // The PD form consumes one mask bit per 64-bit element, not per dword.
    let pd_src1 = [0x00FF_00FF_00FF_00FFu64; 16];
    let pd_src2 = [0xF0F0_F0F0_F0F0_F0F0u64; 16];
    let pd_old = [0x1234_5678_9ABC_DEF0u64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = pd_src1;
        x86.xmm[1] = pd_src2;
        x86.xmm[2] = pd_old;
    }
    ctx.write_vreg(k1, 1);
    execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0x49, 0x55, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2][0], !pd_src1[0] & pd_src2[0]);
        assert_eq!(&x86.xmm[2][1..8], &pd_old[1..8]);
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    // A full-vector memory operand loads only active elements. Lane 0 at
    // the final valid dword succeeds; activating lane 1 faults without any
    // architectural destination update.
    memory.write(0x3FC, &0xCAFE_BABEu32.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x3FC);
    ctx.write_vreg(k1, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = src1;
        x86.xmm[2] = old;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x49, 0x55, 0x10], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lane32(&x86.xmm[2], 0), !lane32(&src1, 0) & 0xCAFE_BABE);
        for lane in 1..16u8 {
            assert_eq!(lane32(&x86.xmm[2], lane), lane32(&old, lane));
        }
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    ctx.write_vreg(k1, 2);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = old;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x49, 0x55, 0x10], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], old);
    }

    // Broadcast performs one scalar access if any mask bit is active and
    // no access if all bits are inactive.
    ctx.write_vreg(rax, 0x1000);
    ctx.write_vreg(k1, 0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = old;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x59, 0x55, 0x10], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[2][..8], &old[..8]);
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    ctx.write_vreg(k1, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = old;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x59, 0x55, 0x10], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], old);
    }
}
#[test]
fn lifted_evex_packed_integer_logic_masks_broadcast_and_fault_suppression_are_exact() {
    fn lane32(value: &VecValue, lane: u8) -> u32 {
        ((value[(lane / 2) as usize] >> (u32::from(lane & 1) * 32)) & 0xFFFF_FFFF) as u32
    }
    fn set_lane32(value: &mut VecValue, lane: u8, bits: u32) {
        let word = &mut value[(lane / 2) as usize];
        let shift = u32::from(lane & 1) * 32;
        *word = (*word & !(0xFFFF_FFFFu64 << shift)) | (u64::from(bits) << shift);
    }

    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut src1 = [0u64; 16];
    let mut src2 = [0u64; 16];
    let mut old = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    for lane in 0..16u8 {
        set_lane32(&mut src1, lane, 0x0F0F_0000 | u32::from(lane));
        set_lane32(&mut src2, lane, 0xF0FF_00F0 ^ (u32::from(lane) * 0x101));
        set_lane32(&mut old, lane, 0xA000_0000 | u32::from(lane));
    }
    let mask = 0xA55Au64;
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = src1;
        x86.xmm[1] = src2;
        x86.xmm[2] = old;
    }
    ctx.write_vreg(k1, mask);
    execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x49, 0xDB, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            let expected = if mask & (1 << lane) != 0 {
                lane32(&src1, lane) & lane32(&src2, lane)
            } else {
                lane32(&old, lane)
            };
            assert_eq!(lane32(&x86.xmm[2], lane), expected, "merge lane {lane}");
        }
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = old;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0xC9, 0xDF, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            let expected = if mask & (1 << lane) != 0 {
                !lane32(&src1, lane) & lane32(&src2, lane)
            } else {
                0
            };
            assert_eq!(lane32(&x86.xmm[2], lane), expected, "zero lane {lane}");
        }
    }

    // W=1 selects eight qword mask elements.
    let qsrc1 = [0x00FF_00FF_00FF_00FFu64; 16];
    let qsrc2 = [0xF0F0_F0F0_F0F0_F0F0u64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = qsrc1;
        x86.xmm[1] = qsrc2;
        x86.xmm[2] = old;
    }
    ctx.write_vreg(k1, 1);
    execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0x49, 0xEF, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2][0], qsrc1[0] ^ qsrc2[0]);
        assert_eq!(&x86.xmm[2][1..8], &old[1..8]);
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    // Masked full-vector memory loads access only active dword elements.
    memory.write(0x3FC, &0xCAFE_BABEu32.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x3FC);
    ctx.write_vreg(k1, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = src1;
        x86.xmm[2] = old;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x49, 0xDB, 0x10], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lane32(&x86.xmm[2], 0), lane32(&src1, 0) & 0xCAFE_BABE);
    }

    ctx.write_vreg(k1, 2);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = old;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x49, 0xDB, 0x10], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], old);
    }

    // A broadcast performs no memory access when all mask bits are clear.
    ctx.write_vreg(rax, 0x1000);
    ctx.write_vreg(k1, 0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = old;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0x59, 0xEB, 0x10], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[2][..8], &old[..8]);
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }
}
#[test]
fn lifted_legacy_vex_evex_packed_add_wraps_masks_broadcasts_and_faults_exactly() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();
    let flags_before = 0xCD7;
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    for (opcode, one_per_lane) in [
        (0xFC, 0x0101_0101_0101_0101u64),
        (0xFD, 0x0001_0001_0001_0001),
        (0xFE, 0x0000_0001_0000_0001),
        (0xD4, 1),
    ] {
        let mut lhs = [0xA5A5_A5A5_A5A5_A5A5u64; 16];
        lhs[0] = u64::MAX;
        lhs[1] = u64::MAX;
        let rhs = [one_per_lane; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::MAX;
            x86.mm[1] = one_per_lane;
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 6 << 11;
        }
        execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], 0, "MMX opcode {opcode:02X}");
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 6 << 11);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = lhs;
            x86.xmm[1] = rhs;
        }
        execute_lifted_x86(&[0x66, 0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], 0, "legacy opcode {opcode:02X}");
            assert_eq!(x86.xmm[0][1], 0, "legacy opcode {opcode:02X}");
            assert_eq!(&x86.xmm[0][2..], &lhs[2..]);
        }

        let mut vex_lhs = [0u64; 16];
        vex_lhs[..4].fill(u64::MAX);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1] = vex_lhs;
            x86.xmm[2] = rhs;
        }
        execute_lifted_x86(&[0xC5, 0xF5, opcode, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert!(x86.xmm[0].iter().all(|word| *word == 0), "VEX {opcode:02X}");
        }
    }

    // The MMX memory form reads exactly 8 bytes before entering MMX state.
    memory
        .write(0x3F8, &0x0101_0101_0101_0101u64.to_le_bytes())
        .unwrap();
    ctx.write_vreg(rax, 0x3F8);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::MAX;
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0xFC, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0);
        assert_eq!(x86.x87.tag_word, 0);
    }

    // A faulting source leaves both the MMX destination and x87 tags intact.
    ctx.write_vreg(rax, 0x1000);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xA5A5_5A5A_C3C3_3C3C;
        x86.x87.tag_word = 0xFFFF;
    }
    let mmx_fault = execute_lifted_x86(&[0x0F, 0xFC, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        mmx_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xA5A5_5A5A_C3C3_3C3C);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    // Byte masking uses all 64 K bits and preserves or zeroes individual bytes.
    let src1 = [0x0101_0101_0101_0101u64; 16];
    let src2 = [0x0202_0202_0202_0202u64; 16];
    let old = [0xA5A5_A5A5_A5A5_A5A5u64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = src1;
        x86.xmm[1] = src2;
        x86.xmm[2] = old;
    }
    ctx.write_vreg(k1, 0xAAAA_AAAA_AAAA_AAAA);
    execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x49, 0xFC, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[2][..8], &[0x03A5_03A5_03A5_03A5u64; 8]);
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = old;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0xC9, 0xFC, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[2][..8], &[0x0300_0300_0300_0300u64; 8]);
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    // VPADDD broadcast reads one dword and applies it to active lanes.
    memory.write(0x100, &5u32.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x100);
    ctx.write_vreg(k1, 0b0101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [0x0000_000A_0000_000Au64; 16];
        x86.xmm[2] = old;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x59, 0xFE, 0x10], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2][0] as u32, 15);
        assert_eq!(x86.xmm[2][0] >> 32, old[0] >> 32);
        assert_eq!(x86.xmm[2][1] as u32, 15);
    }

    // Per-byte fault suppression: the final valid byte succeeds alone;
    // activating the following byte faults before destination commit.
    memory.write(0x3FF, &[1]).unwrap();
    ctx.write_vreg(rax, 0x3FF);
    ctx.write_vreg(k1, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = src1;
        x86.xmm[2] = old;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x49, 0xFC, 0x10], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));

    ctx.write_vreg(k1, 2);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = old;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x49, 0xFC, 0x10], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], old);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_legacy_vex_evex_packed_subtract_wraps_masks_and_faults_exactly() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();
    for (opcode, one_per_lane) in [
        (0xF8, 0x0101_0101_0101_0101u64),
        (0xF9, 0x0001_0001_0001_0001),
        (0xFA, 0x0000_0001_0000_0001),
        (0xFB, 1),
    ] {
        let mut lhs = [0xA5A5_A5A5_A5A5_A5A5u64; 16];
        lhs[0] = 0;
        lhs[1] = 0;
        let rhs = [one_per_lane; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = 0;
            x86.mm[1] = one_per_lane;
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], u64::MAX, "MMX {opcode:02X}");
            assert_eq!(x86.x87.tag_word, 0);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = lhs;
            x86.xmm[1] = rhs;
        }
        execute_lifted_x86(&[0x66, 0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], u64::MAX, "legacy {opcode:02X}");
            assert_eq!(x86.xmm[0][1], u64::MAX, "legacy {opcode:02X}");
            assert_eq!(&x86.xmm[0][2..], &lhs[2..]);
        }

        let vex_lhs = [0u64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0xCCCC_CCCC_CCCC_CCCC; 16];
            x86.xmm[1] = vex_lhs;
            x86.xmm[2] = rhs;
        }
        execute_lifted_x86(&[0xC5, 0xF5, opcode, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[0][..4], &[u64::MAX; 4], "VEX {opcode:02X}");
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }
    }

    // Byte writemask merge and zeroing semantics.
    let src1 = [0x0505_0505_0505_0505u64; 16];
    let src2 = [0x0202_0202_0202_0202u64; 16];
    let old = [0xA5A5_A5A5_A5A5_A5A5u64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = src1;
        x86.xmm[1] = src2;
        x86.xmm[2] = old;
    }
    ctx.write_vreg(k1, 0xAAAA_AAAA_AAAA_AAAA);
    execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x49, 0xF8, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[2][..8], &[0x03A5_03A5_03A5_03A5u64; 8]);
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    // The active byte immediately beyond memory faults before destination commit.
    memory.write(0x3FF, &[2]).unwrap();
    ctx.write_vreg(rax, 0x3FF);
    ctx.write_vreg(k1, 2);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = src1;
        x86.xmm[2] = old;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x49, 0xF8, 0x10], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], old);
    }
}
#[test]
fn lifted_evex_scalar_masked_moves_suppress_memory_faults_and_stores() {
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let old = [0x1111_2222_3333_4444u64; 16];
    let source = [0xAAAA_BBBB_CCCC_DDDDu64; 16];
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = old;
        x86.xmm[1] = source;
        x86.xmm[2] = old;
    }

    ctx.write_vreg(k1, 0);
    execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x09, 0x10, 0xD1], &mut ctx, &mut memory); // VMOVSS XMM2{k1},XMM0,XMM1
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2][0] as u32, old[0] as u32);
        assert_eq!(x86.xmm[2][0] >> 32, old[0] >> 32);
        assert_eq!(x86.xmm[2][1], old[1]);
        assert!(x86.xmm[2][2..].iter().all(|word| *word == 0));
    }

    ctx.write_vreg(rax, 0x1000);
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x09, 0x10, 0x10], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));

    let seed = [0x5A; 16];
    memory.write(0x200, &seed).unwrap();
    ctx.write_vreg(rax, 0x200);
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x09, 0x11, 0x10], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    let mut actual = [0u8; 16];
    memory.read(0x200, &mut actual).unwrap();
    assert_eq!(actual, seed, "masked-off EVEX scalar store");

    ctx.write_vreg(k1, 1);
    execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x09, 0x11, 0x10], &mut ctx, &mut memory);
    memory.read(0x200, &mut actual).unwrap();
    assert_eq!(&actual[..4], &(old[0] as u32).to_le_bytes());
    assert_eq!(&actual[4..], &seed[4..]);
}
#[test]
fn lifted_legacy_vex_evex_scalar_and_packed_sqrt_execute_exact_lanes() {
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    let mut legacy_dst = [0xAAAA_BBBB_CCCC_DDDDu64; 16];
    legacy_dst[0] = 1.0f32.to_bits() as u64 | (0x1234_5678u64 << 32);
    let mut source = [0x1111_2222_3333_4444u64; 16];
    source[0] = 16.0f32.to_bits() as u64;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = legacy_dst;
        x86.xmm[1] = source;
    }
    execute_lifted_x86(&[0xF3, 0x0F, 0x51, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut expected = legacy_dst;
        expected[0] = 4.0f32.to_bits() as u64 | (legacy_dst[0] & 0xFFFF_FFFF_0000_0000);
        assert_eq!(x86.xmm[0], expected);
    }

    let mut merge = [0x7777_8888_9999_AAAAu64; 16];
    merge[0] = 25.0f32.to_bits() as u64 | (0xDEAD_BEEFu64 << 32);
    merge[1] = 0x0123_4567_89AB_CDEF;
    let mut radicand = [0xBBBB_CCCC_DDDD_EEEEu64; 16];
    radicand[0] = 81.0f32.to_bits() as u64;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [u64::MAX; 16];
        x86.xmm[1] = merge;
        x86.xmm[2] = radicand;
    }
    execute_lifted_x86(&[0xC5, 0xF2, 0x51, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0] as u32, 9.0f32.to_bits());
        assert_eq!(x86.xmm[0][0] >> 32, merge[0] >> 32);
        assert_eq!(x86.xmm[0][1], merge[1]);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
    }

    // EVEX masked-off memory SQRT suppresses the invalid read. Merge and
    // zeroing still control the low lane while upper XMM lanes come from src1.
    ctx.write_vreg(rax, 0x2000);
    ctx.write_vreg(k1, 0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = merge;
        x86.xmm[2] = legacy_dst;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x09, 0x51, 0x10], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2][0] as u32, legacy_dst[0] as u32);
        assert_eq!(x86.xmm[2][0] >> 32, merge[0] >> 32);
        assert_eq!(x86.xmm[2][1], merge[1]);
        assert!(x86.xmm[2][2..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = legacy_dst;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7E, 0x89, 0x51, 0x10], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2][0] as u32, 0);
        assert_eq!(x86.xmm[2][0] >> 32, merge[0] >> 32);
    }

    // EVEX disp8 is compressed by the scalar tuple width: 0x10 * 4 = 64.
    ctx.write_vreg(rax, 0x200);
    ctx.write_vreg(k1, 1);
    memory
        .write(0x240, &36.0f32.to_bits().to_le_bytes())
        .unwrap();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = merge;
    }
    execute_lifted_x86(
        &[0x62, 0xF1, 0x7E, 0x08, 0x51, 0x50, 0x10],
        &mut ctx,
        &mut memory,
    ); // VSQRTSS XMM2,XMM0,[RAX+64]
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2][0] as u32, 6.0f32.to_bits());
    }

    let mut packed = [0u64; 16];
    packed[0] = 1.0f32.to_bits() as u64 | ((4.0f32.to_bits() as u64) << 32);
    packed[1] = 9.0f32.to_bits() as u64 | ((16.0f32.to_bits() as u64) << 32);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [0xCAFE_BABE_DEAD_BEEFu64; 16];
        x86.xmm[1] = packed;
    }
    execute_lifted_x86(&[0x0F, 0x51, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0] as u32, 1.0f32.to_bits());
        assert_eq!((x86.xmm[0][0] >> 32) as u32, 2.0f32.to_bits());
        assert_eq!(x86.xmm[0][1] as u32, 3.0f32.to_bits());
        assert_eq!((x86.xmm[0][1] >> 32) as u32, 4.0f32.to_bits());
        assert!(
            x86.xmm[0][2..]
                .iter()
                .all(|word| *word == 0xCAFE_BABE_DEAD_BEEF),
            "legacy packed SQRT must preserve shared AVX state above XMM"
        );
    }

    // A full EVEX tuple compresses disp8 by 64 bytes for a ZMM operand.
    ctx.write_vreg(rax, 0x300);
    let mut zmm_source = [0u8; 64];
    for lane in 0..16 {
        let value = ((lane + 1) * (lane + 1)) as f32;
        zmm_source[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_bits().to_le_bytes());
    }
    memory.write(0x340, &zmm_source).unwrap();
    execute_lifted_x86(
        &[0x62, 0xF1, 0x7C, 0x48, 0x51, 0x50, 0x01],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16 {
            let word = x86.xmm[2][lane / 2];
            let actual = if lane % 2 == 0 {
                word as u32
            } else {
                (word >> 32) as u32
            };
            assert_eq!(actual, (lane as f32 + 1.0).to_bits(), "ZMM lane {lane}");
        }
    }
}
#[test]
fn lifted_x86_packed_int_to_fp16_is_exact_masked_atomic_and_sae_aware() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let k5 = VReg::Arch(ArchReg::X86(X86Reg::K(5)));
    let k6 = VReg::Arch(ArchReg::X86(X86Reg::K(6)));
    let sentinel = 0xCAFE_BABE_DEAD_BEEFu64;
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    // Nearest-even integer conversion is performed directly at binary16
    // precision. A zero-masked lane contributes no precision status, and
    // narrowing clears every architectural bit above the FP16 results.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = [sentinel; 16];
        for (lane, value) in [2049i32, -2049, 65_520, 3].into_iter().enumerate() {
            SmirInterpreter::set_lane(&mut x86.xmm[3], lane as u8, 32, u64::from(value as u32));
        }
        x86.mxcsr = 0x1F80;
    }
    ctx.write_vreg(k2, 0b1101);
    let exit = execute_lifted_x86(&[0x62, 0xF5, 0x7C, 0x8A, 0x5B, 0xCB], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16), 0x6800);
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 1, 16), 0);
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 2, 16), 0x7C00);
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 3, 16), 0x4200);
        assert!(x86.xmm[1][1..].iter().all(|word| *word == 0));
        assert_eq!(x86.mxcsr & ((1 << 3) | (1 << 5)), (1 << 3) | (1 << 5));
    }

    // A 512-bit unsigned-quadword form with RZ-SAE retains merging lanes,
    // rounds the full u64 domain to max-finite FP16, and leaves MXCSR exact.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[4] = [sentinel; 16];
        x86.xmm[6] = [u64::MAX; 16];
        x86.mxcsr = (0x1F80 & !(3 << 13)) | (2 << 13);
    }
    ctx.write_vreg(k5, 1);
    let mxcsr_before = match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.mxcsr,
        _ => unreachable!(),
    };
    let exit = execute_lifted_x86(&[0x62, 0xF5, 0xFF, 0x7D, 0x7A, 0xE6], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[4], 0, 16), 0x7BFF);
        for lane in 1..8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[4], lane, 16),
                SmirInterpreter::get_lane(&[sentinel; 16], lane, 16),
            );
        }
        assert!(x86.xmm[4][2..].iter().all(|word| *word == 0));
        assert_eq!(x86.mxcsr, mxcsr_before);
    }

    // Unmasked overflow sets OE+PE and traps before the destination commits.
    let original = [0x0123_4567_89AB_CDEFu64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[7] = original;
        x86.xmm[8] = [0; 16];
        SmirInterpreter::set_lane(&mut x86.xmm[8], 0, 16, 65_535);
        x86.mxcsr = 0x1F80 & !(1 << 10);
    }
    ctx.write_vreg(k6, 1);
    let exit = execute_lifted_x86(&[0x62, 0xD5, 0x7F, 0x2E, 0x7D, 0xF8], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[7], original);
        assert_eq!(x86.mxcsr & ((1 << 3) | (1 << 5)), (1 << 3) | (1 << 5));
    }

    // An all-zero mask suppresses an out-of-range broadcast access. Making
    // one lane active exposes the fault and preserves the old destination.
    ctx.write_vreg(rax, 0x1000);
    ctx.write_vreg(k2, 0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = [sentinel; 16];
        x86.mxcsr = 0x1F80;
    }
    let bytes = [0x62, 0xF5, 0x7C, 0x9A, 0x5B, 0x48, 0x7F];
    let exit = execute_lifted_x86(&bytes, &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert!(x86.xmm[1].iter().all(|word| *word == 0));
    }
    ctx.write_vreg(k2, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
    }
    let exit = execute_lifted_x86(&bytes, &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], original);
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_x86_packed_fp16_to_int_is_exact_masked_atomic_and_sae_aware() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let original = [0xCAFE_BABE_DEAD_BEEFu64; 16];
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    // Nearest-even conversion is exact at the binary16 source precision;
    // masked-invalid and inexact lanes accumulate IE and PE atomically.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
        for (lane, bits) in [0x4100u16, 0xC100, 0x3A00, 0x7E01].into_iter().enumerate() {
            SmirInterpreter::set_lane(&mut x86.xmm[3], lane as u8, 16, u64::from(bits));
        }
        x86.mxcsr = 0x1F80;
    }
    ctx.write_vreg(k2, 0b1111);
    let exit = execute_lifted_x86(&[0x62, 0xF5, 0x7D, 0x8A, 0x5B, 0xCB], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 32), 2);
        assert_eq!(
            SmirInterpreter::get_lane(&x86.xmm[1], 1, 32),
            (-2i32) as u32 as u64
        );
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 2, 32), 1);
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 3, 32), 0x8000_0000);
        assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        assert_eq!(x86.mxcsr & ((1 << 0) | (1 << 5)), (1 << 0) | (1 << 5));
    }

    // An unmasked precision exception updates MXCSR but commits no vector
    // destination state.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
        x86.xmm[3] = [0; 16];
        SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 16, 0x4100);
        x86.mxcsr = 0x1F80 & !(1 << 12);
    }
    let exit = execute_lifted_x86(&[0x62, 0xF5, 0x7D, 0x08, 0x5B, 0xCB], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], original);
        assert_ne!(x86.mxcsr & (1 << 5), 0);
    }

    // Embedded rounding with SAE is fixed to the 512-bit form and leaves
    // MXCSR unchanged while applying directed rounding per active lane.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
        x86.xmm[3] = [0; 16];
        SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 16, 0x4100);
        SmirInterpreter::set_lane(&mut x86.xmm[3], 1, 16, 0xC100);
        x86.mxcsr = 0x1F80 | 0x21;
    }
    let mxcsr_before = match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.mxcsr,
        _ => unreachable!(),
    };
    let exit = execute_lifted_x86(&[0x62, 0xF5, 0x7D, 0x38, 0x7D, 0xCB], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16), 2);
        assert_eq!(
            SmirInterpreter::get_lane(&x86.xmm[1], 1, 16),
            (-3i16) as u16 as u64
        );
        assert_eq!(x86.mxcsr, mxcsr_before);
    }

    // The 128-bit quadword destination consumes exactly two FP16 lanes
    // (four bytes), not the eight-byte minimum architectural XMM region.
    ctx.write_vreg(rax, 0x3FC);
    memory.write(0x3FC, &[0x00, 0x3C, 0x00, 0x40]).unwrap(); // +1, +2
    let exit = execute_lifted_x86(&[0x62, 0xF5, 0x7D, 0x08, 0x7B, 0x08], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 64), 1);
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 1, 64), 2);
    }

    // A zero opmask suppresses every broadcast access. Activating one lane
    // exposes the fault before the old destination can be modified.
    ctx.write_vreg(rax, 0x1000);
    ctx.write_vreg(k2, 0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
        x86.mxcsr = 0x1F80;
    }
    let bytes = [0x62, 0xF5, 0x7D, 0x1A, 0x7B, 0x48, 0x7F];
    let exit = execute_lifted_x86(&bytes, &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1][0], original[0]);
        assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
    }
    ctx.write_vreg(k2, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
    }
    let exit = execute_lifted_x86(&bytes, &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], original);
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn x86_fp16_to_integer_exact_boundaries_cover_rounding_signedness_and_status() {
    let cvt = |bits, width, signed, round| {
        SmirInterpreter::x86_simd_fp_to_int(bits, X86_SIMD_F16, width, signed, round)
    };
    for (round, positive, negative) in [
        (FpRoundMode::RoundNearest, 0, 0),
        (FpRoundMode::RoundDown, 0, u32::MAX as u64),
        (FpRoundMode::RoundUp, 1, 0),
        (FpRoundMode::RoundTowardZero, 0, 0),
    ] {
        let plus_half = cvt(0x3800, 32, true, round);
        let minus_half = cvt(0xB800, 32, true, round);
        assert_eq!(plus_half.bits, positive);
        assert_eq!(minus_half.bits, negative);
        assert_eq!(plus_half.status, 1 << 5);
        assert_eq!(minus_half.status, 1 << 5);
    }

    for (bits, signed, expected, status) in [
        (0x77FF, true, 32_752, 0),
        (0x7800, true, 0x8000, 1),
        (0xF800, true, 0x8000, 0),
        (0x7BFF, false, 65_504, 0),
        (0xBC00, false, 0xFFFF, 1),
        (0x7C00, false, 0xFFFF, 1),
        (0x7E01, false, 0xFFFF, 1),
        (0x0001, false, 0, 1 << 5),
    ] {
        let actual = cvt(bits, 16, signed, FpRoundMode::RoundNearest);
        assert_eq!(actual.bits, expected, "bits={bits:04X}");
        assert_eq!(actual.status, status, "bits={bits:04X}");
    }

    let unsigned_negative_half_nearest = cvt(0xB800, 16, false, FpRoundMode::RoundNearest);
    assert_eq!(unsigned_negative_half_nearest.bits, 0);
    assert_eq!(unsigned_negative_half_nearest.status, 1 << 5);
    let unsigned_negative_half_down = cvt(0xB800, 16, false, FpRoundMode::RoundDown);
    assert_eq!(unsigned_negative_half_down.bits, 0xFFFF);
    assert_eq!(unsigned_negative_half_down.status, 1);

    for width in [32, 64] {
        let max_finite = cvt(0x7BFF, width, true, FpRoundMode::RoundNearest);
        assert_eq!(max_finite.bits, 65_504);
        assert_eq!(max_finite.status, 0);
    }
}
#[test]
fn lifted_evex_scalar_precision_family_executes_masks_er_sae_faults_and_exceptions() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;
    let merge = [0x0123_4567_89AB_CDEFu64; 16];

    for (name, bytes, source, to_bits, expected) in [
        (
            "VCVTSD2SS",
            &[0x62, 0xF1, 0xFF, 0x09, 0x5A, 0xCA][..],
            1.5f64.to_bits(),
            32u32,
            u64::from(1.5f32.to_bits()),
        ),
        (
            "VCVTSS2SD",
            &[0x62, 0xF1, 0x7E, 0x09, 0x5A, 0xCA][..],
            u64::from(1.5f32.to_bits()),
            64,
            1.5f64.to_bits(),
        ),
        (
            "VCVTSD2SH",
            &[0x62, 0xF5, 0xFF, 0x09, 0x5A, 0xCA][..],
            1.5f64.to_bits(),
            16,
            0x3E00,
        ),
        (
            "VCVTSH2SD",
            &[0x62, 0xF5, 0x7E, 0x09, 0x5A, 0xCA][..],
            0x3E00,
            64,
            1.5f64.to_bits(),
        ),
        (
            "VCVTSS2SH",
            &[0x62, 0xF5, 0x7C, 0x09, 0x1D, 0xCA][..],
            u64::from(1.5f32.to_bits()),
            16,
            0x3E00,
        ),
        (
            "VCVTSH2SS",
            &[0x62, 0xF6, 0x7C, 0x09, 0x13, 0xCA][..],
            0x3E00,
            32,
            u64::from(1.5f32.to_bits()),
        ),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = merge;
            x86.xmm[1] = [u64::MAX; 16];
            x86.xmm[2] = [0; 16];
            x86.xmm[2][0] = source;
            x86.k[1] = 1;
            x86.mxcsr = 0x1F80;
        }
        assert!(matches!(
            execute_lifted_x86(bytes, &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let low_mask = if to_bits == 64 {
                u64::MAX
            } else {
                (1u64 << to_bits) - 1
            };
            assert_eq!(x86.xmm[1][0] & low_mask, expected, "{name}: result");
            assert_eq!(
                x86.xmm[1][0] & !low_mask,
                merge[0] & !low_mask,
                "{name}: merge"
            );
            assert_eq!(x86.xmm[1][1], merge[1], "{name}: merge");
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        }
    }

    let midpoint = 1.0f64 + 2.0f64.powi(-11);
    for (name, bytes, source, mxcsr, expected, expected_status) in [
        (
            "embedded nearest direct",
            &[0x62, 0xF5, 0xFF, 0x19, 0x5A, 0xCA][..],
            midpoint + 2.0f64.powi(-30),
            (0x1F80 & !(3 << 13)) | (1 << 13),
            0x3C01u16,
            0u32,
        ),
        (
            "embedded round up",
            &[0x62, 0xF5, 0xFF, 0x59, 0x5A, 0xCA][..],
            midpoint,
            (0x1F80 & !(3 << 13)) | (1 << 13),
            0x3C01,
            0,
        ),
        (
            "MXCSR round down",
            &[0x62, 0xF5, 0xFF, 0x09, 0x5A, 0xCA][..],
            midpoint,
            (0x1F80 & !(3 << 13)) | (1 << 13),
            0x3C00,
            1 << 5,
        ),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = merge;
            x86.xmm[1] = [u64::MAX; 16];
            x86.xmm[2][0] = source.to_bits();
            x86.k[1] = 1;
            x86.mxcsr = mxcsr;
        }
        execute_lifted_x86(bytes, &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] as u16, expected, "{name}");
            assert_eq!(x86.mxcsr & 0x3F, expected_status, "{name}: MXCSR");
        }
    }

    // SAE quiets an SNaN result without updating MXCSR.INVALID.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = merge;
        x86.xmm[1] = [u64::MAX; 16];
        x86.xmm[2][0] = 0x7C01;
        x86.k[1] = 1;
        x86.mxcsr = 0x1F80;
    }
    execute_lifted_x86(&[0x62, 0xF6, 0x7C, 0x19, 0x13, 0xCA], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1][0] as u32, 0x7FC0_2000);
        assert_eq!(x86.mxcsr & 0x3F, 0);
    }

    // FP16 denormals are never treated as zero, even when DAZ is set;
    // scalar SH conversions still report the denormal status.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = merge;
        x86.xmm[1] = [u64::MAX; 16];
        x86.xmm[2][0] = 1;
        x86.k[1] = 1;
        x86.mxcsr = 0x1F80 | (1 << 6);
    }
    execute_lifted_x86(&[0x62, 0xF6, 0x7C, 0x09, 0x13, 0xCA], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1][0] as u32, 0x3380_0000);
        assert_eq!(x86.mxcsr & (1 << 1), 1 << 1);
    }

    // Inactive memory masks suppress the load and all FP exceptions.
    ctx.write_vreg(rax, 0x400);
    for (zeroing, p2, expected_low) in [(false, 0x09u8, 0xBEEF), (true, 0x89, 0)] {
        let old = [0xCAFE_BABE_DEAD_BEEFu64; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = merge;
            x86.xmm[1] = old;
            x86.k[1] = 0;
            x86.mxcsr = 0x1F80 & !(1 << 7);
        }
        let exit = execute_lifted_x86(&[0x62, 0xF5, 0x7C, p2, 0x1D, 0x08], &mut ctx, &mut memory);
        assert!(
            matches!(exit, BlockResult::Exit(ExitReason::Halt)),
            "{zeroing}"
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[1][0] as u16, expected_low);
            assert_eq!(x86.xmm[1][0] & !0xFFFF, merge[0] & !0xFFFF);
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }
    }

    let original = [0xA5A5_5A5A_0123_4567u64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
        x86.k[1] = 1;
        x86.mxcsr = 0x1F80;
    }
    let fault = execute_lifted_x86(&[0x62, 0xF5, 0x7C, 0x09, 0x1D, 0x08], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], original);
        assert_eq!(x86.mxcsr & 0x3F, 0);
    }

    // An unmasked overflow records MXCSR status and traps atomically.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
        x86.xmm[2][0] = u64::from(f32::MAX.to_bits());
        x86.k[1] = 1;
        x86.mxcsr = 0x1F80 & !(1 << 10);
    }
    let overflow = execute_lifted_x86(&[0x62, 0xF5, 0x7C, 0x09, 0x1D, 0xCA], &mut ctx, &mut memory);
    assert!(matches!(
        overflow,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], original);
        assert_ne!(x86.mxcsr & (1 << 3), 0);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_evex_packed_fp_convert_honors_masks_high_regs_and_embedded_rounding() {
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let k3 = VReg::Arch(ArchReg::X86(X86Reg::K(3)));
    let k4 = VReg::Arch(ArchReg::X86(X86Reg::K(4)));
    let sentinel = 0xCAFE_BABE_DEAD_BEEFu64;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x1000);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        for lane in 0..8 {
            SmirInterpreter::set_lane(
                &mut x86.xmm[1],
                lane,
                32,
                (f32::from(lane + 1)).to_bits().into(),
            );
        }
    }
    ctx.write_vreg(k1, 0b0101_0101);
    execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0xC9, 0x5A, 0xC1], &mut ctx, &mut memory); // VCVTPS2PD zmm0{k1}{z},ymm1
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..8 {
            let actual = SmirInterpreter::get_lane(&x86.xmm[0], lane, 64);
            let expected = if lane % 2 == 0 {
                f64::from(lane + 1).to_bits()
            } else {
                0
            };
            assert_eq!(actual, expected, "zero-mask lane {lane}");
        }
        assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[18] = [sentinel; 16];
        for lane in 0..8 {
            SmirInterpreter::set_lane(
                &mut x86.xmm[17],
                lane,
                32,
                (f32::from(lane + 10)).to_bits().into(),
            );
        }
    }
    ctx.write_vreg(k3, 0b0000_1111);
    execute_lifted_x86(&[0x62, 0xA1, 0x7C, 0x4B, 0x5A, 0xD1], &mut ctx, &mut memory); // VCVTPS2PD zmm18{k3},ymm17
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..8 {
            let actual = SmirInterpreter::get_lane(&x86.xmm[18], lane, 64);
            let expected = if lane < 4 {
                f64::from(lane + 10).to_bits()
            } else {
                sentinel
            };
            assert_eq!(actual, expected, "merge-mask high lane {lane}");
        }
        assert!(x86.xmm[18][8..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[5] = [sentinel; 16];
        for lane in 0..8 {
            x86.xmm[6][lane as usize] = f64::from(lane + 1).to_bits();
        }
    }
    ctx.write_vreg(k4, 0b1010_0101);
    execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0xCC, 0x5A, 0xEE], &mut ctx, &mut memory); // VCVTPD2PS ymm5{k4}{z},zmm6
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..8 {
            let actual = SmirInterpreter::get_lane(&x86.xmm[5], lane, 32) as u32;
            let expected = if 0b1010_0101 & (1 << lane) != 0 {
                (f32::from(lane + 1)).to_bits()
            } else {
                0
            };
            assert_eq!(actual, expected, "narrow mask lane {lane}");
        }
        assert!(x86.xmm[5][4..].iter().all(|word| *word == 0));
    }

    let midpoint = 1.0f64 + 2.0f64.powi(-24);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[1][..8].fill(midpoint.to_bits());
        x86.mxcsr = (0x1F80 & !(3 << 13)) | (2 << 13); // MXCSR rounds up.
    }
    ctx.write_vreg(k1, 0xFF);
    execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0x39, 0x5A, 0xC1], &mut ctx, &mut memory); // VCVTPD2PS ymm0{k1},zmm1,{rd-sae}
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[0], lane, 32) as u32,
                1.0f32.to_bits(),
                "embedded round-down lane {lane}"
            );
        }
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_packed_fp16_precision_conversions_are_exact_and_honor_daz_ftz_er_sae() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let sentinel = 0xCAFE_BABE_DEAD_BEEFu64;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x1000);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    // VCVTPH2PS explicitly ignores DAZ and reports no denormal status.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[1][0] = 0x7C00_0001_C000_3C00;
        x86.mxcsr = 0x1F80 | (1 << 6);
    }
    let exit = execute_lifted_x86(&[0xC4, 0xE2, 0x79, 0x13, 0xC1], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            [
                SmirInterpreter::get_lane(&x86.xmm[0], 0, 32) as u32,
                SmirInterpreter::get_lane(&x86.xmm[0], 1, 32) as u32,
                SmirInterpreter::get_lane(&x86.xmm[0], 2, 32) as u32,
                SmirInterpreter::get_lane(&x86.xmm[0], 3, 32) as u32,
            ],
            [
                1.0f32.to_bits(),
                (-2.0f32).to_bits(),
                0x3380_0000,
                f32::INFINITY.to_bits(),
            ]
        );
        assert_eq!(x86.mxcsr & (1 << 1), 0);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
    }

    // VCVTPH2PD preserves FP16 denormals despite DAZ, reports DE, and
    // observes merging-mask semantics.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[1][0] = 1;
        x86.mxcsr = 0x1F80 | (1 << 6);
    }
    ctx.write_vreg(k1, 1);
    let exit = execute_lifted_x86(&[0x62, 0xF5, 0x7C, 0x09, 0x5A, 0xC1], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 2.0f64.powi(-24).to_bits());
        assert_eq!(x86.xmm[0][1], sentinel);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        assert_eq!(x86.mxcsr & (1 << 1), 1 << 1);
    }

    // VCVTPH2PSX broadcast preserves an FP16 denormal despite DAZ and,
    // unlike its non-broadcast form, reports the denormal exception.
    ctx.write_vreg(rax, 0x200);
    memory.write(0x200, &1u16.to_le_bytes()).unwrap();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.mxcsr = 0x1F80 | (1 << 6);
    }
    ctx.write_vreg(k1, 1);
    let exit = execute_lifted_x86(&[0x62, 0xF6, 0x7D, 0x19, 0x13, 0x00], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 0, 32), 0x3380_0000);
        assert_eq!(x86.mxcsr & (1 << 1), 1 << 1);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
    }

    // VCVTPS2PHX uses embedded round-up and never flushes FP16 outputs.
    let midpoint = 1.0f32 + 2.0f32.powi(-11);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        for lane in 0..16 {
            SmirInterpreter::set_lane(
                &mut x86.xmm[1],
                lane,
                32,
                u64::from(if lane == 0 {
                    midpoint.to_bits()
                } else if lane == 1 {
                    2.0f32.powi(-24).to_bits()
                } else {
                    1.0f32.to_bits()
                }),
            );
        }
        x86.mxcsr = ((0x1F80 & !(3 << 13)) | (1 << 13)) | (1 << 15);
    }
    ctx.write_vreg(k1, 0xFFFF);
    let mxcsr_before = match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.mxcsr,
        _ => unreachable!(),
    };
    let exit = execute_lifted_x86(&[0x62, 0xF5, 0x7D, 0x59, 0x1D, 0xC1], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 0, 16), 0x3C01);
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 1, 16), 0x0001);
        assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        assert_eq!(x86.mxcsr, mxcsr_before, "ER must suppress status updates");
    }

    // Direct FP64->FP16 conversion must avoid an FP32 double-rounding step.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [0; 16];
        x86.xmm[1][0] = (1.0f64 + 2.0f64.powi(-11) + 2.0f64.powi(-30)).to_bits();
        x86.mxcsr = 0x1F80;
    }
    ctx.write_vreg(k1, 1);
    let exit = execute_lifted_x86(&[0x62, 0xF5, 0xFD, 0x19, 0x5A, 0xC1], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], 0, 16), 0x3C01);
    }

    // SAE suppresses an FP16 signaling-NaN invalid exception and status.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[1][0] = 0x7D00;
        x86.mxcsr = 0x1F80 & !(1 << 7);
    }
    ctx.write_vreg(k1, 1);
    let mxcsr_before = match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.mxcsr,
        _ => unreachable!(),
    };
    let exit = execute_lifted_x86(&[0x62, 0xF5, 0x7C, 0x19, 0x5A, 0xC1], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let result = x86.xmm[0][0];
        assert_eq!(result & 0x7FF0_0000_0000_0000, 0x7FF0_0000_0000_0000);
        assert_ne!(result & 0x0008_0000_0000_0000, 0);
        assert_eq!(x86.mxcsr, mxcsr_before);
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_packed_fp16_precision_masks_suppress_faults_and_traps_atomically() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let original = [0x0123_4567_89AB_CDEFu64; 16];
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x202);
    ctx.write_vreg(rax, 0x200);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;
    memory.write(0x200, &0x3C00u16.to_le_bytes()).unwrap();

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = original;
    }
    ctx.write_vreg(k1, 1);
    let exit = execute_lifted_x86(&[0x62, 0xF5, 0x7C, 0x09, 0x5A, 0x00], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 1.0f64.to_bits());
        assert_eq!(x86.xmm[0][1], original[1]);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = original;
    }
    ctx.write_vreg(k1, 0b11);
    let exit = execute_lifted_x86(&[0x62, 0xF5, 0x7C, 0x09, 0x5A, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], original);
    }

    // A zero mask suppresses an out-of-range broadcast load.
    ctx.write_vreg(rax, 0x2000);
    ctx.write_vreg(k1, 0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = original;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF6, 0x7D, 0x99, 0x13, 0x00], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert!(x86.xmm[0].iter().all(|word| *word == 0));
    }

    // Unmasked overflow records sticky status, traps, and preserves DEST.
    ctx.write_vreg(k1, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = original;
        x86.xmm[1][0] = u64::from(f32::MAX.to_bits());
        x86.mxcsr = 0x1F80 & !(1 << 10);
    }
    let exit = execute_lifted_x86(&[0x62, 0xF5, 0x7D, 0x09, 0x1D, 0xC1], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mxcsr & (1 << 3), 1 << 3);
        assert_eq!(x86.xmm[0], original);
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_evex_packed_fp_convert_suppresses_masked_memory_faults_per_lane() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k3 = VReg::Arch(ArchReg::X86(X86Reg::K(3)));
    let original = [0x0123_4567_89AB_CDEFu64; 16];

    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x204);
    ctx.write_vreg(rax, 0x200);
    ctx.write_vreg(k3, 0);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = original;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x4B, 0x5A, 0x00], &mut ctx, &mut memory); // VCVTPS2PD zmm0{k3},[rax]
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[0][..8], &original[..8]);
        assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
    }

    memory
        .write(0x200, &1.5f32.to_bits().to_le_bytes())
        .unwrap();
    ctx.write_vreg(k3, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = original;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x4B, 0x5A, 0x00], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 1.5f64.to_bits());
        assert_eq!(&x86.xmm[0][1..8], &original[1..8]);
    }

    ctx.write_vreg(k3, 0b11);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = original;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x4B, 0x5A, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], original, "fault committed destination");
    }

    ctx.write_vreg(rax, 0x2000);
    ctx.write_vreg(k3, 0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = original;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0xDB, 0x5A, 0x00], &mut ctx, &mut memory); // VCVTPS2PD zmm0{k3}{z},dword [rax]{1to8}
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert!(x86.xmm[0].iter().all(|word| *word == 0));
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn executes_evex_two_table_permute_overwrite_masks_and_selected_memory_faults() {
    fn vec_u32(values: &[u32]) -> VecValue {
        vec_from_bytes(
            &values
                .iter()
                .copied()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
    }

    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = vec_u32(&[0, 5, 2, 7]);
        x86.xmm[1] = vec_u32(&[10, 11, 12, 13]);
        x86.xmm[2] = vec_u32(&[20, 21, 22, 23]);
        x86.k[1] = 0xB;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x89, 0x76, 0xC2], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], vec_u32(&[10, 21, 0, 23]));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = vec_u32(&[10, 11, 12, 13]);
        x86.xmm[1] = vec_u32(&[0, 5, 2, 7]);
        x86.xmm[2] = vec_u32(&[20, 21, 22, 23]);
        x86.k[1] = 0xB;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x09, 0x7E, 0xC2], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], vec_u32(&[10, 21, 12, 23]));
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    ctx.write_vreg(rax, 0x400);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = vec_u32(&[0, 0, 0, 0]);
        x86.xmm[1] = vec_u32(&[0xAA, 0xBB, 0xCC, 0xDD]);
        x86.k[1] = 1;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x89, 0x76, 0x00], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], vec_u32(&[0xAA, 0, 0, 0]));
    }

    memory.write(0x3FC, &0x1234_5678u32.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = vec_u32(&[4, 0, 0, 0]);
        x86.k[1] = 1;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x89, 0x76, 0x00], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], vec_u32(&[0x1234_5678, 0, 0, 0]));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = vec_u32(&[4, 5, 0, 7]);
        x86.k[1] = 0xB;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x99, 0x76, 0x00], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.xmm[0],
            vec_u32(&[0x1234_5678, 0x1234_5678, 0, 0x1234_5678])
        );
    }

    let sentinel = [0xA5A5_A5A5_A5A5_A5A5; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = vec_u32(&[7, 0, 0, 0]);
        x86.xmm[0][2..].copy_from_slice(&sentinel[2..]);
        x86.k[1] = 1;
    }
    let before = match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.xmm[0],
        _ => unreachable!(),
    };
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x89, 0x76, 0x00], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], before);
    }
}
#[test]
fn executes_evex_vpopcnt_elements_masks_broadcast_and_fault_atomicity() {
    fn bytes(value: &VecValue, count: usize) -> Vec<u8> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(count)
            .collect()
    }

    let input = [
        0u8, 1, 3, 7, 15, 31, 63, 127, 255, 0x55, 0xAA, 0x81, 0x18, 0xF0, 0xFE, 0x80,
    ];
    let mask = 0xA55Au64;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0x8D5);
    ctx.flags.lazy = None;
    let flags_before = ctx.flags.materialized.to_rflags();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[17] = [u64::MAX; 16];
        x86.xmm[18] = vec_from_bytes(&input);
        x86.k[2] = mask;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xA2, 0x7D, 0x8A, 0x54, 0xCA], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let actual = bytes(&x86.xmm[17], 16);
        for lane in 0..16 {
            assert_eq!(
                actual[lane],
                if mask & (1 << lane) != 0 {
                    input[lane].count_ones() as u8
                } else {
                    0
                }
            );
        }
        assert_eq!(&x86.xmm[17][2..], &[0; 14]);
    }

    let scalar = 0xF0F0_0F0Fu32;
    memory.write(0x3FC, &scalar.to_le_bytes()).unwrap();
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[17] = [u64::MAX; 16];
        x86.k[6] = 0x8001;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xE2, 0x7D, 0xDE, 0x55, 0x08], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[17], lane, 32),
                if matches!(lane, 0 | 15) {
                    u64::from(scalar.count_ones())
                } else {
                    0
                }
            );
        }
    }

    let sentinel = [0x5C5C_5C5C_5C5C_5C5C; 16];
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x400);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[17] = sentinel;
        x86.k[6] = 1;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xE2, 0x7D, 0xDE, 0x55, 0x08], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[17], sentinel);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn executes_evex_vplzcnt_zero_values_masks_broadcast_and_fault_atomicity() {
    fn vec_u32(values: &[u32]) -> VecValue {
        vec_from_bytes(
            &values
                .iter()
                .copied()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
    }

    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[17] = [u64::MAX; 16];
        x86.xmm[18] = vec_u32(&[0, 1, 0x8000_0000, 0x00F0_0000]);
        x86.k[2] = 0xB;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xA2, 0x7D, 0x8A, 0x44, 0xCA], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[17], vec_u32(&[32, 31, 0, 8]));
        assert_eq!(&x86.xmm[17][2..], &[0; 14]);
    }

    memory.write(0x3FC, &0x0000_0001u32.to_le_bytes()).unwrap();
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[21] = [u64::MAX; 16];
        x86.k[4] = 0x8001;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xE2, 0x7D, 0xDC, 0x44, 0x28], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[21], lane, 32),
                if matches!(lane, 0 | 15) { 31 } else { 0 }
            );
        }
    }

    let sentinel = [0x3D3D_3D3D_3D3D_3D3D; 16];
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x400);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[21] = sentinel;
        x86.k[4] = 1;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xE2, 0x7D, 0xDC, 0x44, 0x28], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[21], sentinel);
    }
}
