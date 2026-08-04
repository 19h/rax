//! packed part 1 tests

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn lifted_packed_sse_arithmetic_and_moves_execute() {
    fn f32x4(values: [f32; 4]) -> VecValue {
        let mut result = [0u64; 16];
        result[0] = values[0].to_bits() as u64 | ((values[1].to_bits() as u64) << 32);
        result[1] = values[2].to_bits() as u64 | ((values[3].to_bits() as u64) << 32);
        result
    }
    fn f64x2(values: [f64; 2]) -> VecValue {
        let mut result = [0u64; 16];
        result[0] = values[0].to_bits();
        result[1] = values[1].to_bits();
        result
    }

    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = f32x4([1.0, 2.0, 3.0, 4.0]);
        x86.xmm[1] = f32x4([5.0, 6.0, 7.0, 8.0]);
    }
    execute_lifted_x86(&[0x0F, 0x58, 0xC1], &mut ctx, &mut memory); // ADDPS XMM0,XMM1
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], f32x4([6.0, 8.0, 10.0, 12.0]));
        assert_eq!(x86.xmm[1], f32x4([5.0, 6.0, 7.0, 8.0]));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = f64x2([8.0, 9.0]);
        x86.xmm[1] = f64x2([2.0, 3.0]);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x5E, 0xC1], &mut ctx, &mut memory); // DIVPD
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], f64x2([4.0, 3.0]));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [
            0xFFFF_0000_FFFF_0000,
            0xAAAA_AAAA_5555_5555,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        x86.xmm[1] = [
            0x00FF_00FF_00FF_00FF,
            0xFFFF_0000_FFFF_0000,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
    }
    execute_lifted_x86(&[0x0F, 0x57, 0xC1], &mut ctx, &mut memory); // XORPS
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 0xFF00_00FF_FF00_00FF);
        assert_eq!(x86.xmm[0][1], 0x5555_AAAA_AAAA_5555);
    }

    let bytes: Vec<u8> = (0u8..16).collect();
    memory.write(0x200, &bytes).unwrap();
    ctx.write_vreg(rbx, 0x200);
    execute_lifted_x86(&[0x0F, 0x10, 0x03], &mut ctx, &mut memory); // MOVUPS XMM0,[RBX]
    execute_lifted_x86(&[0x0F, 0x11, 0x43, 0x10], &mut ctx, &mut memory); // MOVUPS [RBX+16],XMM0
    let mut copied = [0u8; 16];
    memory.read(0x210, &mut copied).unwrap();
    assert_eq!(copied.as_slice(), bytes.as_slice());
}
#[test]
fn lifted_scalar_sse_moves_and_arithmetic_preserve_or_clear_upper_lanes_exactly() {
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    let mut lhs = [0u64; 16];
    lhs[0] = 1.5f32.to_bits() as u64 | (0xA1B2_C3D4u64 << 32);
    lhs[1] = 0x1122_3344_5566_7788;
    lhs[2] = 0x99AA_BBCC_DDEE_FF00;
    let mut rhs = [0u64; 16];
    rhs[0] = 2.25f32.to_bits() as u64 | (0xDEAD_BEEFu64 << 32);
    rhs[1] = u64::MAX;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = lhs;
        x86.xmm[1] = rhs;
    }
    execute_lifted_x86(&[0xF3, 0x0F, 0x58, 0xC1], &mut ctx, &mut memory); // ADDSS
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut expected = lhs;
        expected[0] = 3.75f32.to_bits() as u64 | (lhs[0] & 0xFFFF_FFFF_0000_0000);
        assert_eq!(x86.xmm[0], expected);
        assert_eq!(x86.xmm[1], rhs);
    }

    let upper = [
        0x0123_4567_89AB_CDEF,
        0xAABB_CCDD_EEFF_0011,
        0x2233_4455_6677_8899,
    ];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0][0] = 9.0f64.to_bits();
        x86.xmm[0][1..4].copy_from_slice(&upper);
        x86.xmm[1][0] = 3.0f64.to_bits();
    }
    execute_lifted_x86(&[0xF2, 0x0F, 0x5E, 0xC1], &mut ctx, &mut memory); // DIVSD
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 3.0f64.to_bits());
        assert_eq!(&x86.xmm[0][1..4], &upper);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = lhs;
        x86.xmm[1] = rhs;
    }
    execute_lifted_x86(&[0xF3, 0x0F, 0x10, 0xC1], &mut ctx, &mut memory); // MOVSS reg
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut expected = lhs;
        expected[0] = (lhs[0] & 0xFFFF_FFFF_0000_0000) | (rhs[0] & 0xFFFF_FFFF);
        assert_eq!(x86.xmm[0], expected);
    }

    memory
        .write(0x200, &6.5f32.to_bits().to_le_bytes())
        .unwrap();
    ctx.write_vreg(rbx, 0x200);
    execute_lifted_x86(&[0xF3, 0x0F, 0x10, 0x03], &mut ctx, &mut memory); // MOVSS memory
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 6.5f32.to_bits() as u64);
        assert_eq!(x86.xmm[0][1], 0);
        assert_eq!(x86.xmm[0][2..], lhs[2..], "legacy AVX upper state");
    }

    let sentinel = [0xAA; 16];
    memory.write(0x300, &sentinel).unwrap();
    ctx.write_vreg(rbx, 0x304);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1][0] = 12.25f64.to_bits();
    }
    execute_lifted_x86(&[0xF2, 0x0F, 0x11, 0x0B], &mut ctx, &mut memory); // MOVSD [RBX],XMM1
    let mut stored = [0u8; 16];
    memory.read(0x300, &mut stored).unwrap();
    assert_eq!(&stored[..4], &[0xAA; 4]);
    assert_eq!(&stored[4..12], &12.25f64.to_bits().to_le_bytes());
    assert_eq!(&stored[12..], &[0xAA; 4]);

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_scalar_sse_memory_faults_preserve_destination_flags_and_memory() {
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let original = [0x0123_4567_89AB_CDEFu64; 16];
    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = original;
    }
    ctx.write_vreg(rbx, 0x200);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    let mut short_memory = FlatMemory::new(0x204);
    let exit = execute_lifted_x86(&[0xF2, 0x0F, 0x10, 0x03], &mut ctx, &mut short_memory); // MOVSD XMM0,[RBX]
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], original, "faulting scalar load destination");
    }

    let seed = [0xA5; 16];
    let mut inner = FlatMemory::new(0x400);
    inner.write(0x200, &seed).unwrap();
    let mut read_only = StoreFaultMemory {
        inner,
        stores_before_fault: 0,
    };
    let exit = execute_lifted_x86(&[0xF3, 0x0F, 0x11, 0x03], &mut ctx, &mut read_only); // MOVSS [RBX],XMM0
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
    ));
    let mut actual = [0u8; 16];
    read_only.inner.read(0x200, &mut actual).unwrap();
    assert_eq!(actual, seed);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], original, "faulting scalar store source");
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_vex_scalar_moves_and_arithmetic_merge_xmm_and_zero_avx_upper_state() {
    fn f32x4_with_upper(values: [f32; 4], upper: u64) -> VecValue {
        let mut result = [upper; 16];
        result[0] = values[0].to_bits() as u64 | ((values[1].to_bits() as u64) << 32);
        result[1] = values[2].to_bits() as u64 | ((values[3].to_bits() as u64) << 32);
        result
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    let src1 = f32x4_with_upper([1.5, 10.0, -3.0, 7.0], 0x1111_2222_3333_4444);
    let src2 = f32x4_with_upper([2.25, 99.0, 88.0, 77.0], 0xAAAA_BBBB_CCCC_DDDD);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [u64::MAX; 16];
        x86.xmm[1] = src1;
        x86.xmm[2] = src2;
    }
    execute_lifted_x86(&[0xC5, 0xF2, 0x58, 0xC2], &mut ctx, &mut memory); // VADDSS XMM0,XMM1,XMM2
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0] as u32, 3.75f32.to_bits());
        assert_eq!(x86.xmm[0][0] >> 32, src1[0] >> 32);
        assert_eq!(x86.xmm[0][1], src1[1]);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
        assert_eq!(x86.xmm[1], src1);
        assert_eq!(x86.xmm[2], src2);
    }

    // Destination/source1 alias: upper XMM lanes must be captured before
    // VEX zeroing rewrites the shared vector backing store.
    let alias_src = f32x4_with_upper([8.0, 2.0, 3.0, 4.0], 0xDEAD_BEEF_CAFE_BABE);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = alias_src;
        x86.xmm[1] = f32x4_with_upper([0.5, 0.0, 0.0, 0.0], 1);
    }
    execute_lifted_x86(&[0xC5, 0xFA, 0x58, 0xC1], &mut ctx, &mut memory); // VADDSS XMM0,XMM0,XMM1
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0] as u32, 8.5f32.to_bits());
        assert_eq!(x86.xmm[0][0] >> 32, alias_src[0] >> 32);
        assert_eq!(x86.xmm[0][1], alias_src[1]);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
    }

    let merge = f32x4_with_upper([100.0, 20.0, 30.0, 40.0], 0x1111);
    let low = f32x4_with_upper([-6.0, 200.0, 300.0, 400.0], 0x2222);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = merge;
        x86.xmm[1] = low;
        x86.xmm[2] = [u64::MAX; 16];
    }
    execute_lifted_x86(&[0xC5, 0xFA, 0x10, 0xD1], &mut ctx, &mut memory); // VMOVSS XMM2,XMM0,XMM1
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2][0] as u32, (-6.0f32).to_bits());
        assert_eq!(x86.xmm[2][0] >> 32, merge[0] >> 32);
        assert_eq!(x86.xmm[2][1], merge[1]);
        assert!(x86.xmm[2][2..].iter().all(|word| *word == 0));
    }

    memory
        .write(0x200, &12.5f64.to_bits().to_le_bytes())
        .unwrap();
    ctx.write_vreg(rax, 0x200);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = [u64::MAX; 16];
    }
    execute_lifted_x86(&[0xC5, 0xFB, 0x10, 0x18], &mut ctx, &mut memory); // VMOVSD XMM3,[RAX]
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[3][0], 12.5f64.to_bits());
        assert!(x86.xmm[3][1..].iter().all(|word| *word == 0));
    }

    let packed_source = [0x5555_AAAA_1234_5678u64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [u64::MAX; 16];
        x86.xmm[1] = packed_source;
    }
    execute_lifted_x86(&[0xC5, 0xF8, 0x10, 0xC1], &mut ctx, &mut memory); // VMOVUPS XMM0,XMM1
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[0][..2], &packed_source[..2]);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_vex_scalar_memory_faults_preserve_destination_flags_and_memory() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let original = [0xABCD_EF01_2345_6789u64; 16];
    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = original;
    }
    ctx.write_vreg(rax, 0x200);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    let mut short_memory = FlatMemory::new(0x204);
    let exit = execute_lifted_x86(&[0xC5, 0xFB, 0x10, 0x00], &mut ctx, &mut short_memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], original);
    }

    let seed = [0x5A; 16];
    let mut inner = FlatMemory::new(0x400);
    inner.write(0x200, &seed).unwrap();
    let mut read_only = StoreFaultMemory {
        inner,
        stores_before_fault: 0,
    };
    let exit = execute_lifted_x86(&[0xC5, 0xFA, 0x11, 0x00], &mut ctx, &mut read_only);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
    ));
    let mut actual = [0u8; 16];
    read_only.inner.read(0x200, &mut actual).unwrap();
    assert_eq!(actual, seed);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], original);
    }
    for bytes in [&[0x0F, 0x55, 0x00][..], &[0xC5, 0xF8, 0x55, 0x00][..]] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = original;
        }
        let exit = execute_lifted_x86(bytes, &mut ctx, &mut short_memory);
        assert!(
            matches!(
                exit,
                BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
            ),
            "{bytes:02X?}"
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], original, "{bytes:02X?}");
        }
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_vex_scalar_arithmetic_all_ops_and_memory_sources_execute() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    for (name, bytes, is_f64, a, b, expected) in [
        (
            "vaddss",
            &[0xC5, 0xF2, 0x58, 0xC2][..],
            false,
            7.5f64,
            2.5f64,
            10.0f64,
        ),
        (
            "vmulss",
            &[0xC5, 0xF2, 0x59, 0xC2][..],
            false,
            3.0,
            4.0,
            12.0,
        ),
        (
            "vsubss",
            &[0xC5, 0xF2, 0x5C, 0xC2][..],
            false,
            7.0,
            2.0,
            5.0,
        ),
        (
            "vdivss",
            &[0xC5, 0xF2, 0x5E, 0xC2][..],
            false,
            9.0,
            3.0,
            3.0,
        ),
        (
            "vaddsd",
            &[0xC5, 0xF3, 0x58, 0xC2][..],
            true,
            7.5,
            2.5,
            10.0,
        ),
        (
            "vmulsd",
            &[0xC5, 0xF3, 0x59, 0xC2][..],
            true,
            3.0,
            4.0,
            12.0,
        ),
        ("vsubsd", &[0xC5, 0xF3, 0x5C, 0xC2][..], true, 7.0, 2.0, 5.0),
        ("vdivsd", &[0xC5, 0xF3, 0x5E, 0xC2][..], true, 9.0, 3.0, 3.0),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1] = [0x1357_9BDF_2468_ACE0; 16];
            x86.xmm[2] = [0xAAAA_BBBB_CCCC_DDDD; 16];
            if is_f64 {
                x86.xmm[1][0] = a.to_bits();
                x86.xmm[2][0] = b.to_bits();
            } else {
                x86.xmm[1][0] = (a as f32).to_bits() as u64 | (0xDEAD_BEEFu64 << 32);
                x86.xmm[2][0] = (b as f32).to_bits() as u64;
            }
        }
        execute_lifted_x86(bytes, &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            if is_f64 {
                assert_eq!(x86.xmm[0][0], expected.to_bits(), "{name}");
                assert_eq!(x86.xmm[0][1], 0x1357_9BDF_2468_ACE0, "{name}: merge");
            } else {
                assert_eq!(x86.xmm[0][0] as u32, (expected as f32).to_bits(), "{name}");
                assert_eq!(x86.xmm[0][0] >> 32, 0xDEAD_BEEF, "{name}: merge");
                assert_eq!(x86.xmm[0][1], 0x1357_9BDF_2468_ACE0, "{name}: merge");
            }
            assert!(
                x86.xmm[0][2..].iter().all(|word| *word == 0),
                "{name}: upper"
            );
        }
    }

    ctx.write_vreg(rax, 0x200);
    memory
        .write(0x200, &1.25f32.to_bits().to_le_bytes())
        .unwrap();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1][0] = 2.75f32.to_bits() as u64 | (0x1234_5678u64 << 32);
    }
    execute_lifted_x86(&[0xC5, 0xF2, 0x58, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0] as u32, 4.0f32.to_bits());
        assert_eq!(x86.xmm[0][0] >> 32, 0x1234_5678);
    }

    memory
        .write(0x200, &2.0f64.to_bits().to_le_bytes())
        .unwrap();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1][0] = 10.0f64.to_bits();
        x86.xmm[1][1] = 0xCAFE_BABE_DEAD_BEEF;
    }
    execute_lifted_x86(&[0xC5, 0xF3, 0x5E, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 5.0f64.to_bits());
        assert_eq!(x86.xmm[0][1], 0xCAFE_BABE_DEAD_BEEF);
    }
}
#[test]
fn lifted_legacy_and_vex_movmsk_execute_exact_bits_zero_extend_and_preserve_flags() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
    let mut memory = FlatMemory::new(1);
    let mut ctx = SmirContext::new_x86_64();
    let flags_before = 0x8D7u64;
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    // MOVMSKPS: sign bits in lanes 0 and 2 become result bits 0 and 2.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = [0; 16];
        x86.xmm[1][0] = 0x0000_0000_8000_0000;
        x86.xmm[1][1] = 0x0000_0000_8000_0000;
    }
    ctx.write_vreg(rax, u64::MAX);
    execute_lifted_x86(&[0x0F, 0x50, 0xC1], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0b0101);
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

    // REX.W MOVMSKPD selects a 64-bit destination write; the two qword
    // sign bits still occupy only result bits 0 and 1.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1][0] = 0x7FFF_FFFF_FFFF_FFFF;
        x86.xmm[1][1] = 0x8000_0000_0000_0000;
    }
    ctx.write_vreg(rdx, u64::MAX);
    execute_lifted_x86(&[0x66, 0x48, 0x0F, 0x50, 0xD1], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rdx), 0b10);
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

    // VEX.256.WIG uses all eight dword lanes and honors both VEX.R and
    // VEX.B register extensions. W=1 is deliberately exercised here.
    let mut ymm9 = [0u64; 16];
    for lane in 0..8usize {
        if lane & 1 != 0 {
            ymm9[lane / 2] |= 0x8000_0000u64 << ((lane & 1) * 32);
        }
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[9] = ymm9;
    }
    ctx.write_vreg(r8, u64::MAX);
    execute_lifted_x86(&[0xC4, 0x41, 0xFC, 0x50, 0xC1], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(r8), 0xAA);
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_legacy_and_vex_pmovmskb_execute_exact_bits_and_zero_extend() {
    fn source(mask: u32, bytes: usize) -> VecValue {
        let mut raw = [0u8; 128];
        for (index, byte) in raw[..bytes].iter_mut().enumerate() {
            *byte = if mask & (1 << index) != 0 {
                0x80 | index as u8
            } else {
                index as u8
            };
        }
        let mut value = [0u64; 16];
        for (word, chunk) in value.iter_mut().zip(raw.chunks_exact(8)) {
            *word = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        value
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
    let flags_before = 0x8D7;
    let pattern = 0xA55A_C33C;
    let mut memory = FlatMemory::new(1);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = source(pattern, 16);
        x86.xmm[10] = source(pattern, 32);
    }

    ctx.write_vreg(rax, u64::MAX);
    assert!(matches!(
        execute_lifted_x86(&[0x66, 0x0F, 0xD7, 0xC1], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(ctx.read_vreg(rax), u64::from(pattern & 0xFFFF));

    ctx.write_vreg(r9, u64::MAX);
    assert!(matches!(
        execute_lifted_x86(&[0xC4, 0x41, 0xFD, 0xD7, 0xCA], &mut ctx, &mut memory,),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(ctx.read_vreg(r9), u64::from(pattern));
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_legacy_and_vex_lddqu_preserve_or_zero_upper_and_fault_atomically() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut memory = FlatMemory::new(0x80);
    let mut payload = [0u8; 32];
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(7).wrapping_add(3);
    }
    memory.write(3, &payload).unwrap();
    let mut ctx = SmirContext::new_x86_64();

    let legacy_old = [0xA5A5_A5A5_A5A5_A5A5u64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = legacy_old;
    }
    ctx.write_vreg(rax, 2);
    execute_lifted_x86(&[0xF2, 0x0F, 0xF0, 0x40, 0x01], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut low = [0u8; 16];
        for (word_index, word) in x86.xmm[0][..2].iter().enumerate() {
            low[word_index * 8..word_index * 8 + 8].copy_from_slice(&word.to_le_bytes());
        }
        assert_eq!(low, payload[..16]);
        assert_eq!(&x86.xmm[0][2..], &legacy_old[2..]);
    }

    let vex_old = [u64::MAX; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = vex_old;
    }
    ctx.write_vreg(rax, 0);
    execute_lifted_x86(&[0xC5, 0xFF, 0xF0, 0x40, 0x03], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut low = [0u8; 32];
        for (word_index, word) in x86.xmm[0][..4].iter().enumerate() {
            low[word_index * 8..word_index * 8 + 8].copy_from_slice(&word.to_le_bytes());
        }
        assert_eq!(low, payload);
        assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
    }

    // The memory read precedes the vector write; an out-of-range operand
    // leaves both the low destination and its shared upper state unchanged.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = vex_old;
    }
    ctx.write_vreg(rax, 0x1000);
    let exit = execute_lifted_x86(&[0xC5, 0xFF, 0xF0, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], vex_old);
    }
}
#[test]
fn lifted_legacy_and_vex_packed_integer_logic_executes_exact_bits_and_faults_atomically() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut memory = FlatMemory::new(0x100);
    let mut ctx = SmirContext::new_x86_64();
    let flags_before = 0xCD7;
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    let lhs = [
        0x00FF_00FF_AAAA_5555,
        0xF0F0_0F0F_1234_5678,
        0x1111_2222_3333_4444,
        0x5555_6666_7777_8888,
        0x9999_AAAA_BBBB_CCCC,
        0xDDDD_EEEE_FFFF_0000,
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0xA5A5_A5A5_A5A5_A5A5,
        0xA5A5_A5A5_A5A5_A5A5,
        0xA5A5_A5A5_A5A5_A5A5,
        0xA5A5_A5A5_A5A5_A5A5,
        0xA5A5_A5A5_A5A5_A5A5,
        0xA5A5_A5A5_A5A5_A5A5,
        0xA5A5_A5A5_A5A5_A5A5,
        0xA5A5_A5A5_A5A5_A5A5,
    ];
    let rhs = [
        0xFF00_FF00_0F0F_F0F0,
        0x0FF0_F00F_8765_4321,
        0xFFFF_0000_FFFF_0000,
        0x0000_FFFF_0000_FFFF,
        0x1357_9BDF_2468_ACE0,
        0xCAFE_BABE_DEAD_BEEF,
        0xFFFF_FFFF_0000_0000,
        0x0000_0000_FFFF_FFFF,
        0x5A5A_5A5A_5A5A_5A5A,
        0x5A5A_5A5A_5A5A_5A5A,
        0x5A5A_5A5A_5A5A_5A5A,
        0x5A5A_5A5A_5A5A_5A5A,
        0x5A5A_5A5A_5A5A_5A5A,
        0x5A5A_5A5A_5A5A_5A5A,
        0x5A5A_5A5A_5A5A_5A5A,
        0x5A5A_5A5A_5A5A_5A5A,
    ];

    for (opcode, apply) in [
        (0xDB, (|a: u64, b: u64| a & b) as fn(u64, u64) -> u64),
        (0xDF, (|a: u64, b: u64| !a & b) as fn(u64, u64) -> u64),
        (0xEB, (|a: u64, b: u64| a | b) as fn(u64, u64) -> u64),
        (0xEF, (|a: u64, b: u64| a ^ b) as fn(u64, u64) -> u64),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = lhs[0];
            x86.mm[1] = rhs[0];
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 4 << 11;
        }
        execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], apply(lhs[0], rhs[0]), "MMX {opcode:02X}");
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 4 << 11);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = lhs;
            x86.xmm[1] = rhs;
        }
        execute_lifted_x86(&[0x66, 0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], apply(lhs[0], rhs[0]), "legacy {opcode:02X}");
            assert_eq!(x86.xmm[0][1], apply(lhs[1], rhs[1]), "legacy {opcode:02X}");
            assert_eq!(&x86.xmm[0][2..], &lhs[2..], "legacy upper {opcode:02X}");
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1] = lhs;
            x86.xmm[2] = rhs;
        }
        execute_lifted_x86(&[0xC5, 0xF5, opcode, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for word in 0..4 {
                assert_eq!(
                    x86.xmm[0][word],
                    apply(lhs[word], rhs[word]),
                    "VEX {opcode:02X}, word {word}",
                );
            }
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }
    }

    // A successful MMX memory source reads exactly 8 bytes before entering
    // MMX state and computing the non-commutative PANDN result.
    memory.write(0xF8, &rhs[0].to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0xF8);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = lhs[0];
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0xDF, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], !lhs[0] & rhs[0]);
        assert_eq!(x86.x87.tag_word, 0);
    }

    // A faulting MMX memory read precedes both the destination write and
    // the x87 tag transition.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = lhs[0];
        x86.x87.tag_word = 0xFFFF;
    }
    ctx.write_vreg(rax, 0x1000);
    let mmx_fault = execute_lifted_x86(&[0x0F, 0xDF, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        mmx_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], lhs[0]);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    // A memory fault occurs before the architectural destination write.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = lhs;
    }
    ctx.write_vreg(rax, 0x1000);
    let exit = execute_lifted_x86(&[0x66, 0x0F, 0xDF, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], lhs);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_saturating_packed_add_subtract_clamps_all_signed_unsigned_boundaries() {
    fn alternating_bytes(lo: u8, hi: u8) -> u64 {
        u64::from_le_bytes([lo, hi, lo, hi, lo, hi, lo, hi])
    }
    fn alternating_words(lo: u16, hi: u16) -> u64 {
        u64::from(lo) | (u64::from(hi) << 16) | (u64::from(lo) << 32) | (u64::from(hi) << 48)
    }

    let cases = [
        (
            0xEC,
            alternating_bytes(120, (-120i8) as u8),
            alternating_bytes(20, (-20i8) as u8),
            alternating_bytes(i8::MAX as u8, i8::MIN as u8),
        ),
        (
            0xED,
            alternating_words(30_000, (-30_000i16) as u16),
            alternating_words(10_000, (-10_000i16) as u16),
            alternating_words(i16::MAX as u16, i16::MIN as u16),
        ),
        (
            0xDC,
            u64::from_le_bytes([250; 8]),
            u64::from_le_bytes([10; 8]),
            u64::MAX,
        ),
        (
            0xDD,
            alternating_words(65_000, 65_000),
            alternating_words(1_000, 1_000),
            u64::MAX,
        ),
        (
            0xE8,
            alternating_bytes(120, (-120i8) as u8),
            alternating_bytes((-20i8) as u8, 20),
            alternating_bytes(i8::MAX as u8, i8::MIN as u8),
        ),
        (
            0xE9,
            alternating_words(30_000, (-30_000i16) as u16),
            alternating_words((-10_000i16) as u16, 10_000),
            alternating_words(i16::MAX as u16, i16::MIN as u16),
        ),
        (
            0xD8,
            u64::from_le_bytes([5; 8]),
            u64::from_le_bytes([10; 8]),
            0,
        ),
        (
            0xD9,
            alternating_words(500, 500),
            alternating_words(1_000, 1_000),
            0,
        ),
    ];
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();
    for (opcode, lhs_word, rhs_word, expected_word) in cases {
        let mut lhs = [0xA5A5_A5A5_A5A5_A5A5u64; 16];
        lhs[0] = lhs_word;
        lhs[1] = lhs_word;
        let rhs = [rhs_word; 16];
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = lhs_word;
            x86.mm[1] = rhs_word;
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], expected_word, "MMX {opcode:02X}");
            assert_eq!(x86.x87.tag_word, 0);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = lhs;
            x86.xmm[1] = rhs;
        }
        execute_lifted_x86(&[0x66, 0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], expected_word, "legacy {opcode:02X}");
            assert_eq!(x86.xmm[0][1], expected_word, "legacy {opcode:02X}");
            assert_eq!(&x86.xmm[0][2..], &lhs[2..], "legacy upper {opcode:02X}");
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1] = [lhs_word; 16];
            x86.xmm[2] = rhs;
        }
        execute_lifted_x86(&[0xC5, 0xF5, opcode, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[0][..4], &[expected_word; 4], "VEX {opcode:02X}");
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }
    }

    // EVEX byte merge/zero masks preserve saturation at element granularity.
    let src1 = [alternating_bytes(120, (-120i8) as u8); 16];
    let src2 = [alternating_bytes(20, (-20i8) as u8); 16];
    let old = [0xA5A5_A5A5_A5A5_A5A5u64; 16];
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = src1;
        x86.xmm[1] = src2;
        x86.xmm[2] = old;
    }
    ctx.write_vreg(k1, 0xAAAA_AAAA_AAAA_AAAA);
    execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x49, 0xEC, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[2][..8], &[0x80A5_80A5_80A5_80A5u64; 8]);
        assert!(x86.xmm[2][8..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = old;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0xC9, 0xEC, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[2][..8], &[0x8000_8000_8000_8000u64; 8]);
    }

    // Masked word memory faults occur before architectural destination update.
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    memory.write(0x3FE, &1u16.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x3FE);
    ctx.write_vreg(k1, 2);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [alternating_words(30_000, 30_000); 16];
        x86.xmm[2] = old;
    }
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7D, 0x49, 0xED, 0x10], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], old);
    }
}
#[test]
fn lifted_legacy_packed_sse_preserves_shared_state_above_bit_127() {
    fn f32x4(values: [f32; 4], upper: u64) -> VecValue {
        let mut result = [upper; 16];
        result[0] = values[0].to_bits() as u64 | ((values[1].to_bits() as u64) << 32);
        result[1] = values[2].to_bits() as u64 | ((values[3].to_bits() as u64) << 32);
        result
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let sentinel = 0xCAFE_BABE_DEAD_BEEFu64;
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    let mut source = [0x1111_2222_3333_4444u64; 16];
    source[0] = 0x0123_4567_89AB_CDEF;
    source[1] = 0xFEDC_BA98_7654_3210;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[1] = source;
    }
    execute_lifted_x86(&[0x0F, 0x10, 0xC1], &mut ctx, &mut memory); // MOVUPS
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[0][..2], &source[..2]);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
    }

    let lhs = f32x4([1.0, 2.0, 3.0, 4.0], sentinel);
    let rhs = f32x4([10.0, 20.0, 30.0, 40.0], 0x9999);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = lhs;
        x86.xmm[1] = rhs;
    }
    execute_lifted_x86(&[0x0F, 0x58, 0xC1], &mut ctx, &mut memory); // ADDPS
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let expected = f32x4([11.0, 22.0, 33.0, 44.0], sentinel);
        assert_eq!(x86.xmm[0], expected);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[0][0] = 0xFFFF_0000_F0F0_0F0F;
        x86.xmm[0][1] = 0xAAAA_5555_1234_5678;
        x86.xmm[1][0] = 0x0FF0_0FF0_FFFF_0000;
        x86.xmm[1][1] = 0xFFFF_0000_FFFF_0000;
    }
    execute_lifted_x86(&[0x0F, 0x54, 0xC1], &mut ctx, &mut memory); // ANDPS
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 0x0FF0_0000_F0F0_0000);
        assert_eq!(x86.xmm[0][1], 0xAAAA_0000_1234_0000);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[0][0] = 0xFFFF_0000_F0F0_0F0F;
        x86.xmm[0][1] = 0xAAAA_5555_1234_5678;
        x86.xmm[1][0] = 0x0FF0_0FF0_FFFF_0000;
        x86.xmm[1][1] = 0xFFFF_0000_FFFF_0000;
    }
    execute_lifted_x86(&[0x0F, 0x55, 0xC1], &mut ctx, &mut memory); // ANDNPS
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 0x0000_0FF0_0F0F_0000);
        assert_eq!(x86.xmm[0][1], 0x5555_0000_EDCB_0000);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[0][0] = 0xFFFF_0000_F0F0_0F0F;
        x86.xmm[0][1] = 0xAAAA_5555_1234_5678;
        x86.xmm[1][0] = 0x0FF0_0FF0_FFFF_0000;
        x86.xmm[1][1] = 0xFFFF_0000_FFFF_0000;
    }
    execute_lifted_x86(&[0xC5, 0xF8, 0x55, 0xC1], &mut ctx, &mut memory); // VANDNPS
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 0x0000_0FF0_0F0F_0000);
        assert_eq!(x86.xmm[0][1], 0x5555_0000_EDCB_0000);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[0][0] = 1 | (2u64 << 32);
        x86.xmm[0][1] = 3 | (4u64 << 32);
        x86.xmm[1][0] = 10 | (20u64 << 32);
        x86.xmm[1][1] = 30 | (40u64 << 32);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0xFE, 0xC1], &mut ctx, &mut memory); // PADDD
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 11 | (22u64 << 32));
        assert_eq!(x86.xmm[0][1], 33 | (44u64 << 32));
        assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[0][0] = 2 | (3u64 << 32);
        x86.xmm[0][1] = 4 | (5u64 << 32);
        x86.xmm[1][0] = 6 | (7u64 << 32);
        x86.xmm[1][1] = 8 | (9u64 << 32);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x40, 0xC1], &mut ctx, &mut memory); // PMULLD
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 12 | (21u64 << 32));
        assert_eq!(x86.xmm[0][1], 32 | (45u64 << 32));
        assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
    }

    ctx.write_vreg(rax, 0x200);
    let bytes = 0x8877_6655_4433_2211u64
        .to_le_bytes()
        .into_iter()
        .chain(0x00FF_EEDD_CCBB_AA99u64.to_le_bytes())
        .collect::<Vec<_>>();
    memory.write(0x200, &bytes).unwrap();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
    }
    execute_lifted_x86(&[0x0F, 0x10, 0x00], &mut ctx, &mut memory); // MOVUPS [RAX]
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 0x8877_6655_4433_2211);
        assert_eq!(x86.xmm[0][1], 0x00FF_EEDD_CCBB_AA99);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_legacy_packed_sse_load_fault_preserves_full_destination() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let original = [0x0123_4567_89AB_CDEFu64; 16];
    let mut ctx = SmirContext::new_x86_64();
    ctx.write_vreg(rax, 0x200);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = original;
    }
    let mut short_memory = FlatMemory::new(0x208);
    let exit = execute_lifted_x86(&[0x0F, 0x10, 0x00], &mut ctx, &mut short_memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], original);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_x86_packed_int_to_fp32_fp64_is_exact_masked_atomic_and_sae_aware() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let original = [0xCAFE_BABE_DEAD_BEEFu64; 16];
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    // Dynamic round-up distinguishes both signs at the first inexact I32
    // boundary. The masked lane merges, and only active lanes contribute PE.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
        x86.xmm[3] = [0; 16];
        SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 32, 16_777_217);
        SmirInterpreter::set_lane(&mut x86.xmm[3], 1, 32, 16_777_217);
        SmirInterpreter::set_lane(&mut x86.xmm[3], 2, 32, u64::from((-16_777_217i32) as u32));
        x86.mxcsr = (0x1F80 & !(3 << 13)) | (2 << 13);
    }
    ctx.write_vreg(k2, 0b0101);
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0x7C, 0x4A, 0x5B, 0xCB], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            SmirInterpreter::get_lane(&x86.xmm[1], 0, 32),
            u64::from(16_777_218.0f32.to_bits())
        );
        assert_eq!(
            SmirInterpreter::get_lane(&x86.xmm[1], 1, 32),
            SmirInterpreter::get_lane(&original, 1, 32)
        );
        assert_eq!(
            SmirInterpreter::get_lane(&x86.xmm[1], 2, 32),
            u64::from((-16_777_216.0f32).to_bits())
        );
        assert_ne!(x86.mxcsr & (1 << 5), 0);
    }

    // Unsigned I64 -> F32 with RZ-SAE uses the full source domain, commits
    // max-finite below 2^64, and does not alter MXCSR despite inexactness.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
        x86.xmm[3] = [u64::MAX; 16];
        x86.mxcsr = 0x1F80 | 0x21;
    }
    ctx.write_vreg(k2, 1);
    let mxcsr_before = match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.mxcsr,
        _ => unreachable!(),
    };
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0xFF, 0x7A, 0x7A, 0xCB], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 32), 0x5F7F_FFFF);
        for lane in 1..8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                SmirInterpreter::get_lane(&original, lane, 32)
            );
        }
        assert!(x86.xmm[1][4..].iter().all(|word| *word == 0));
        assert_eq!(x86.mxcsr, mxcsr_before);
    }

    // An all-zero mask suppresses an out-of-range broadcast access. One
    // active lane exposes the fault before any destination modification.
    ctx.write_vreg(rax, 0x1000);
    ctx.write_vreg(k2, 0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
        x86.mxcsr = 0x1F80;
    }
    let bytes = [0x62, 0xF1, 0x7F, 0xDA, 0x7A, 0x48, 0x7F];
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
fn lifted_x86_packed_fp32_fp64_to_int_is_exact_atomic_and_daz_sae_aware() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let original = [0x0123_4567_89AB_CDEFu64; 16];
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    // Nearest-even signed F32 -> I64 covers half-way values, a NaN
    // indefinite result, and exact -2^63. IE and PE accumulate atomically.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
        x86.xmm[3] = [0; 16];
        for (lane, value) in [1.5f32, 2.5, -1.5, f32::NAN].into_iter().enumerate() {
            SmirInterpreter::set_lane(&mut x86.xmm[3], lane as u8, 32, u64::from(value.to_bits()));
        }
        SmirInterpreter::set_lane(
            &mut x86.xmm[3],
            4,
            32,
            u64::from((-9_223_372_036_854_775_808.0f32).to_bits()),
        );
        x86.mxcsr = 0x1F80;
    }
    ctx.write_vreg(k2, 0b1_1111);
    let bytes = [0x62, 0xF1, 0x7D, 0x4A, 0x7B, 0xCB];
    let exit = execute_lifted_x86(&bytes, &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 64), 2);
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 1, 64), 2);
        assert_eq!(
            SmirInterpreter::get_lane(&x86.xmm[1], 2, 64),
            (-2i64) as u64
        );
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 3, 64), 1u64 << 63);
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 4, 64), 1u64 << 63);
        assert_eq!(x86.mxcsr & ((1 << 0) | (1 << 5)), (1 << 0) | (1 << 5));
    }

    // Unmasking Invalid updates MXCSR but leaves the complete destination
    // unchanged when any active lane cannot be represented.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
        x86.xmm[3] = [0; 16];
        SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 32, u64::from(f32::NAN.to_bits()));
        x86.mxcsr = 0x1F80 & !(1 << 7);
    }
    ctx.write_vreg(k2, 1);
    let exit = execute_lifted_x86(&bytes, &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], original);
        assert_ne!(x86.mxcsr & 1, 0);
    }

    // Unsigned truncation with SAE commits the all-ones indefinite value
    // for a negative input, truncates positive input, and preserves MXCSR.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = original;
        x86.xmm[3] = [0; 16];
        SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 64, (-1.0f64).to_bits());
        SmirInterpreter::set_lane(&mut x86.xmm[3], 1, 64, 1.9f64.to_bits());
        SmirInterpreter::set_lane(&mut x86.xmm[3], 2, 64, f64::MAX.to_bits());
        x86.mxcsr = 0x1F80 | 0x21;
    }
    ctx.write_vreg(k2, 0b111);
    let mxcsr_before = match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.mxcsr,
        _ => unreachable!(),
    };
    let exit = execute_lifted_x86(&[0x62, 0xF1, 0xFD, 0x1A, 0x78, 0xCB], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 64), u64::MAX);
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 1, 64), 1);
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 2, 64), u64::MAX);
        assert_eq!(x86.mxcsr, mxcsr_before);
    }

    // DAZ converts a binary32 subnormal to signed zero before conversion.
    // Without DAZ it is inexact (PE), but this instruction does not raise DE.
    for (daz, expected_status) in [(false, 1 << 5), (true, 0)] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = original;
            x86.xmm[3] = [0; 16];
            SmirInterpreter::set_lane(&mut x86.xmm[3], 0, 32, 1);
            x86.mxcsr = (0x1F80 & !0x3F) | if daz { 1 << 6 } else { 0 };
        }
        let exit = execute_lifted_x86(&[0xC5, 0xFE, 0x5B, 0xCB], &mut ctx, &mut memory);
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 32), 0);
            assert_eq!(x86.mxcsr & 0x3F, expected_status, "DAZ={daz}");
        }
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_x86_packed_fp_convert_honors_lanes_rounding_and_upper_state() {
    fn pack_f32(values: [f32; 4], upper: u64) -> VecValue {
        let mut result = [upper; 16];
        result[0] = u64::from(values[0].to_bits()) | (u64::from(values[1].to_bits()) << 32);
        result[1] = u64::from(values[2].to_bits()) | (u64::from(values[3].to_bits()) << 32);
        result
    }

    let sentinel = 0xCAFE_BABE_DEAD_BEEFu64;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x1000);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[1] = pack_f32([1.5, -2.25, 99.0, 100.0], 0x1111);
        x86.mxcsr = 0x1F80;
    }
    execute_lifted_x86(&[0x0F, 0x5A, 0xC1], &mut ctx, &mut memory); // CVTPS2PD
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 1.5f64.to_bits());
        assert_eq!(x86.xmm[0][1], (-2.25f64).to_bits());
        assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
    }

    let midpoint = 1.0f64 + 2.0f64.powi(-24);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[1][0] = midpoint.to_bits();
        x86.xmm[1][1] = 2.25f64.to_bits();
        x86.mxcsr = (0x1F80 & !(3 << 13)) | (2 << 13);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x5A, 0xC1], &mut ctx, &mut memory); // CVTPD2PS
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0] as u32, 1.0f32.to_bits() + 1);
        assert_eq!((x86.xmm[0][0] >> 32) as u32, 2.25f32.to_bits());
        assert_eq!(x86.xmm[0][1], 0, "legacy narrowing clears bits 127:64");
        assert!(x86.xmm[0][2..].iter().all(|word| *word == sentinel));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[1] = pack_f32([1.0, 2.0, 3.0, 4.0], 0x2222);
        x86.mxcsr = 0x1F80;
    }
    execute_lifted_x86(&[0xC5, 0xFC, 0x5A, 0xC1], &mut ctx, &mut memory); // VCVTPS2PD ymm
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            &x86.xmm[0][..4],
            &[
                1.0f64.to_bits(),
                2.0f64.to_bits(),
                3.0f64.to_bits(),
                4.0f64.to_bits(),
            ]
        );
        assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [sentinel; 16];
        x86.xmm[1][..4].copy_from_slice(&[
            1.0f64.to_bits(),
            2.0f64.to_bits(),
            3.0f64.to_bits(),
            4.0f64.to_bits(),
        ]);
    }
    execute_lifted_x86(&[0xC5, 0xFD, 0x5A, 0xC1], &mut ctx, &mut memory); // VCVTPD2PS xmm,ymm
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[0][..2], &pack_f32([1.0, 2.0, 3.0, 4.0], 0)[..2]);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_x86_packed_fp_convert_memory_fault_preserves_destination_and_flags() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let original = [0xA5A5_5A5A_0123_4567u64; 16];
    for (name, bytes) in [
        ("legacy CVTPS2PD", &[0x0F, 0x5A, 0x00][..]),
        ("VEX VCVTPD2PS ymm source", &[0xC5, 0xFD, 0x5A, 0x00][..]),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        ctx.write_vreg(rax, 0x200);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = original;
        }
        let mut short_memory = FlatMemory::new(0x204);
        let exit = execute_lifted_x86(bytes, &mut ctx, &mut short_memory);
        assert!(
            matches!(
                exit,
                BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
            ),
            "{name}: {exit:?}"
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], original, "{name}: destination");
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7, "{name}: flags");
    }
}
#[test]
fn interprets_vpermute_single_and_two_table_domains() {
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Ymm(0)));
    let table = VReg::Arch(ArchReg::X86(X86Reg::Ymm(1)));
    let indices = VReg::Arch(ArchReg::X86(X86Reg::Ymm(2)));
    let second = VReg::Arch(ArchReg::X86(X86Reg::Xmm(3)));
    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = vec_from_bytes(&(10u32..18).flat_map(u32::to_le_bytes).collect::<Vec<_>>());
        x86.xmm[2] = vec_from_bytes(
            &[7u32, 0, 6, 1, 13, 2, 12, 3]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        x86.xmm[3] = vec_from_bytes(&(0x80u8..0x90).collect::<Vec<_>>());
    }
    let interp = SmirInterpreter::new();
    let mut memory = FlatMemory::new(0x1000);
    interp
        .execute_op(
            &mut ctx,
            &mut memory,
            &SmirOp::new(
                OpId(0),
                0x1000,
                OpKind::VPermute {
                    dst,
                    src1: table,
                    src2: None,
                    indices,
                    elem: VecElementType::I32,
                    width: VecWidth::V256,
                    overwrite_table: false,
                },
            ),
        )
        .unwrap();
    let expected = [17u32, 10, 16, 11, 15, 12, 14, 13]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    assert_eq!(
        SmirInterpreter::read_vec(&ctx, dst),
        vec_from_bytes(&expected)
    );

    let byte_indices = VReg::Virtual(VirtualId(200));
    let byte_dst = VReg::Virtual(VirtualId(201));
    SmirInterpreter::write_vec(
        &mut ctx,
        byte_indices,
        vec_from_bytes(&[0, 15, 16, 31, 32, 47, 48, 63, 8, 24, 40, 56, 1, 17, 33, 49]),
    );
    interp
        .execute_op(
            &mut ctx,
            &mut memory,
            &SmirOp::new(
                OpId(1),
                0x1000,
                OpKind::VPermute {
                    dst: byte_dst,
                    src1: table,
                    src2: Some(second),
                    indices: byte_indices,
                    elem: VecElementType::I8,
                    width: VecWidth::V128,
                    overwrite_table: false,
                },
            ),
        )
        .unwrap();
    let out = SmirInterpreter::read_vec(&ctx, byte_dst);
    let table1 = SmirInterpreter::read_vec(&ctx, table);
    let table2 = SmirInterpreter::read_vec(&ctx, second);
    for (lane, selected) in [0u8, 15, 16, 31, 0, 15, 16, 31, 8, 24, 8, 24, 1, 17, 1, 17]
        .into_iter()
        .enumerate()
    {
        let expected = if selected < 16 {
            SmirInterpreter::get_lane(&table1, selected, 8)
        } else {
            SmirInterpreter::get_lane(&table2, selected - 16, 8)
        };
        assert_eq!(SmirInterpreter::get_lane(&out, lane as u8, 8), expected);
    }
}
#[test]
fn executes_avx_permute_domains_masks_aliases_and_e4nf_fault_precision() {
    fn vec_u32(values: &[u32]) -> VecValue {
        vec_from_bytes(
            &values
                .iter()
                .copied()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
    }

    fn vec_u64(values: &[u64]) -> VecValue {
        vec_from_bytes(
            &values
                .iter()
                .copied()
                .flat_map(u64::to_le_bytes)
                .collect::<Vec<_>>(),
        )
    }

    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = vec_u32(&[7, 0, 6, 1, 5, 2, 4, 3]);
        x86.xmm[3] = vec_u32(&[10, 11, 12, 13, 14, 15, 16, 17]);
    }
    assert!(matches!(
        execute_lifted_x86(&[0xC4, 0xE2, 0x6D, 0x36, 0xCB], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], vec_u32(&[17, 10, 16, 11, 15, 12, 14, 13]));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = vec_u32(&[100, 101, 102, 103, 104, 105, 106, 107]);
        x86.xmm[3] = vec_u32(&[0, 1, 2, 3, 4, 5, 6, 7]);
    }
    assert!(matches!(
        execute_lifted_x86(&[0xC4, 0xE2, 0x69, 0x0C, 0xD3], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], vec_u32(&[100, 101, 102, 103]));
        assert_eq!(&x86.xmm[2][2..], &[0; 14]);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[5] = vec_u64(&[10, 11, 20, 21]);
        x86.xmm[6] = vec_u64(&[0, 2, 2, 0]);
    }
    assert!(matches!(
        execute_lifted_x86(&[0xC4, 0xE2, 0x55, 0x0D, 0xE6], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[4], vec_u64(&[10, 11, 21, 20]));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = vec_u64(&[1, 2, 3, 4]);
    }
    assert!(matches!(
        execute_lifted_x86(&[0xC4, 0xE3, 0xFD, 0x00, 0xCA, 0x1B], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], vec_u64(&[4, 3, 2, 1]));
        assert_eq!(&x86.xmm[1][4..], &[0; 12]);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[18] = vec_u64(&[100, 101, 102, 103, 104, 105, 106, 107]);
        x86.xmm[19] = vec_u64(&[10, 11, 12, 13, 14, 15, 16, 17]);
        x86.k[5] = 0x55;
    }
    assert!(matches!(
        execute_lifted_x86(
            &[0x62, 0xA3, 0xFD, 0x4D, 0x05, 0xD3, 0xA5],
            &mut ctx,
            &mut memory
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[18], vec_u64(&[11, 101, 13, 103, 14, 105, 16, 107]));
    }

    memory
        .write(0x200, &0x1122_3344_5566_7788u64.to_le_bytes())
        .unwrap();
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x200);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[17] = [u64::MAX; 16];
        x86.k[4] = 0xA5;
    }
    assert!(matches!(
        execute_lifted_x86(
            &[0x62, 0xE3, 0xFD, 0xDC, 0x00, 0x08, 0x1B],
            &mut ctx,
            &mut memory
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[17], lane, 64),
                if 0xA5 & (1 << lane) != 0 {
                    0x1122_3344_5566_7788
                } else {
                    0
                }
            );
        }
    }

    let mut full_tuple = [0u8; 64];
    full_tuple[..4].copy_from_slice(&0xA1B2_C3D4u32.to_le_bytes());
    memory.write(0x300, &full_tuple).unwrap();
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x300);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[20] = [0xCCCC_CCCC_CCCC_CCCC; 16];
        x86.xmm[21] = vec_u32(&[
            0, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
        ]);
        x86.k[5] = 1;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xE2, 0x55, 0xC5, 0x36, 0x20], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[20], 0, 32), 0xA1B2_C3D4);
        for lane in 1..16 {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[20], lane, 32), 0);
        }
    }

    let sentinel = [0x5A5A_5A5A_5A5A_5A5A; 16];
    memory.write(0x3FC, &0xA1B2_C3D4u32.to_le_bytes()).unwrap();
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[20] = sentinel;
        // Even though the only selected lane names the mapped first dword,
        // Type E4NF requires the complete 64-byte tuple to be read.
        x86.k[5] = 1;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xE2, 0x55, 0xC5, 0x36, 0x20], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[20], sentinel);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.k[5] = 0;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xE2, 0x55, 0xC5, 0x36, 0x20], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[20], sentinel);
    }
}
#[test]
fn executes_vnni_dot_signedness_saturation_masks_broadcast_and_fault_atomicity() {
    fn vec_bytes(bytes: &[u8]) -> VecValue {
        vec_from_bytes(bytes)
    }
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = vec_bytes(&100i32.to_le_bytes().repeat(4));
        x86.xmm[2] = vec_bytes(&[1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4]);
        x86.xmm[3] = vec_bytes(&[-1i8 as u8, 2, -3i8 as u8, 4].repeat(4));
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x69, 0x50, 0xCB], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..4 {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 32), 110);
        }
    }

    let broadcast = [1i16, -2]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect::<Vec<_>>();
    memory.write(0x3FC, &broadcast).unwrap();
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[20] = vec_bytes(&i32::MAX.to_le_bytes().repeat(8));
        x86.xmm[21] = vec_bytes(
            &[2i16, -3]
                .into_iter()
                .cycle()
                .take(16)
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        x86.k[3] = 0x55;
    }
    execute_lifted_x86(&[0x62, 0xE2, 0x55, 0x33, 0x53, 0x20], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[20], lane, 32),
                if lane % 2 == 0 {
                    i32::MAX as u64
                } else {
                    i32::MAX as u64
                }
            );
        }
    }

    let sentinel = [0x4242_4242_4242_4242; 16];
    memory
        .write(0x3FC, &1.0f32.to_bits().to_le_bytes())
        .unwrap();
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.k[1] = 1;
    }
    let sparse = execute_lifted_x86(&[0x62, 0xF2, 0x7E, 0x09, 0x72, 0x08], &mut ctx, &mut memory);
    assert!(matches!(sparse, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], 0, 16), 0x3F80);
        for lane in 1..4 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], lane, 16),
                SmirInterpreter::get_lane(&sentinel, lane, 16)
            );
        }
        assert_eq!(&x86.xmm[1][1..], &[0; 15]);
    }

    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x400);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[20] = sentinel;
        x86.k[3] = 1;
    }
    let exit = execute_lifted_x86(&[0x62, 0xE2, 0x55, 0x33, 0x53, 0x20], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[20], sentinel);
    }
}
#[test]
fn executes_avx_vnni_int8_int16_signedness_saturation_aliases_memory_and_faults() {
    fn reference(
        acc: u32,
        lhs: &[u64],
        rhs: &[u64],
        bits: u32,
        lhs_signed: bool,
        rhs_signed: bool,
        saturate: bool,
    ) -> u32 {
        let signed = |value: u64| -> i128 {
            let shift = 128 - bits;
            (i128::from(value) << shift) >> shift
        };
        let unsigned_result = !lhs_signed && !rhs_signed;
        let mut sum = if unsigned_result {
            i128::from(acc)
        } else {
            i128::from(acc as i32)
        };
        for (&a, &b) in lhs.iter().zip(rhs) {
            let a = if lhs_signed { signed(a) } else { i128::from(a) };
            let b = if rhs_signed { signed(b) } else { i128::from(b) };
            sum += a * b;
        }
        if !saturate {
            sum as u32
        } else if unsigned_result {
            sum.clamp(0, i128::from(u32::MAX)) as u32
        } else {
            sum.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32 as u32
        }
    }

    let cases = [
        (
            &[0xC4, 0xE2, 0x6B, 0x50, 0xCB][..],
            8,
            true,
            true,
            false,
            4usize,
        ),
        (&[0xC4, 0xE2, 0x57, 0x51, 0xE6][..], 8, true, true, true, 8),
        (
            &[0xC4, 0xE2, 0x6A, 0x50, 0xCB][..],
            8,
            true,
            false,
            false,
            4,
        ),
        (&[0xC4, 0xE2, 0x56, 0x51, 0xE6][..], 8, true, false, true, 8),
        (
            &[0xC4, 0xE2, 0x68, 0x50, 0xCB][..],
            8,
            false,
            false,
            false,
            4,
        ),
        (
            &[0xC4, 0xE2, 0x54, 0x51, 0xE6][..],
            8,
            false,
            false,
            true,
            8,
        ),
        (
            &[0xC4, 0xE2, 0x6A, 0xD2, 0xCB][..],
            16,
            true,
            false,
            false,
            4,
        ),
        (
            &[0xC4, 0xE2, 0x56, 0xD3, 0xE6][..],
            16,
            true,
            false,
            true,
            8,
        ),
        (
            &[0xC4, 0xE2, 0x69, 0xD2, 0xCB][..],
            16,
            false,
            true,
            false,
            4,
        ),
        (
            &[0xC4, 0xE2, 0x55, 0xD3, 0xE6][..],
            16,
            false,
            true,
            true,
            8,
        ),
        (
            &[0xC4, 0xE2, 0x68, 0xD2, 0xCB][..],
            16,
            false,
            false,
            false,
            4,
        ),
        (
            &[0xC4, 0xE2, 0x54, 0xD3, 0xE6][..],
            16,
            false,
            false,
            true,
            8,
        ),
    ];
    for (bytes, bits, lhs_signed, rhs_signed, saturate, lanes) in cases {
        let dst_reg = if lanes == 4 { 1 } else { 4 };
        let lhs_reg = if lanes == 4 { 2 } else { 5 };
        let rhs_reg = if lanes == 4 { 3 } else { 6 };
        let terms = 32 / bits;
        let mask = (1u64 << bits) - 1;
        let positive = (1u64 << (bits - 1)) - 1;
        let negative = 1u64 << (bits - 1);
        let unsigned_max = mask;
        let mut acc = Vec::with_capacity(lanes);
        let mut lhs = Vec::with_capacity(lanes * terms as usize);
        let mut rhs = Vec::with_capacity(lanes * terms as usize);
        for lane in 0..lanes {
            acc.push(match lane % 4 {
                0 => {
                    if !lhs_signed && !rhs_signed {
                        u32::MAX
                    } else {
                        i32::MAX as u32
                    }
                }
                1 => {
                    if !lhs_signed && !rhs_signed {
                        0
                    } else {
                        i32::MIN as u32
                    }
                }
                2 => 100,
                _ => 0xA5A5_5A5A,
            });
            for term in 0..terms {
                let (a, b) = match lane % 4 {
                    0 => (
                        if lhs_signed { positive } else { unsigned_max },
                        if rhs_signed { positive } else { unsigned_max },
                    ),
                    1 if lhs_signed => (negative, if rhs_signed { positive } else { unsigned_max }),
                    1 if rhs_signed => (if lhs_signed { positive } else { unsigned_max }, negative),
                    1 => (unsigned_max, unsigned_max),
                    2 => ((term as u64 + 1) & mask, (term as u64 + 2) & mask),
                    _ => (mask.wrapping_sub(term as u64) & mask, term as u64 & mask),
                };
                lhs.push(a);
                rhs.push(b);
            }
        }
        let expected = (0..lanes)
            .map(|lane| {
                let start = lane * terms as usize;
                reference(
                    acc[lane],
                    &lhs[start..start + terms as usize],
                    &rhs[start..start + terms as usize],
                    bits,
                    lhs_signed,
                    rhs_signed,
                    saturate,
                )
            })
            .collect::<Vec<_>>();
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x400);
        ctx.flags.materialized = MaterializedFlags::from_rflags(0x8D5);
        ctx.flags.lazy = None;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            let mut dst = [0xDEAD_BEEF_DEAD_BEEF; 16];
            let mut first = [0x1111_1111_1111_1111; 16];
            let mut second = [0x2222_2222_2222_2222; 16];
            for lane in 0..lanes {
                SmirInterpreter::set_lane(&mut dst, lane as u8, 32, u64::from(acc[lane]));
            }
            for (lane, value) in lhs.iter().enumerate() {
                SmirInterpreter::set_lane(&mut first, lane as u8, bits, *value);
            }
            for (lane, value) in rhs.iter().enumerate() {
                SmirInterpreter::set_lane(&mut second, lane as u8, bits, *value);
            }
            x86.xmm[dst_reg] = dst;
            x86.xmm[lhs_reg] = first;
            x86.xmm[rhs_reg] = second;
        }
        execute_lifted_x86(bytes, &mut ctx, &mut memory);
        ctx.flags.materialize_all();
        assert_eq!(
            ctx.flags.materialized.to_rflags(),
            MaterializedFlags::from_rflags(0x8D5).to_rflags()
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for (lane, expected) in expected.into_iter().enumerate() {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[dst_reg], lane as u8, 32),
                    u64::from(expected),
                    "case {bytes:02X?}, lane {lane}"
                );
            }
            assert_eq!(&x86.xmm[dst_reg][lanes / 2..], &[0; 16][..16 - lanes / 2]);
        }
    }

    // dst == src1 == src2 must use snapshots of the original vector.
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    let alias = vec_from_bytes(&[1u8, 2, 3, 4].repeat(4));
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = alias;
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x73, 0x50, 0xC9], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..4 {
            let original_acc = SmirInterpreter::get_lane(&alias, lane, 32) as u32;
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                u64::from(original_acc.wrapping_add(30))
            );
        }
    }

    // Type-4 memory operands are unaligned, read the complete vector, and
    // fault before modifying the architectural accumulator.
    let source = [0xFFu8; 32];
    memory.write(0x101, &source).unwrap();
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[4] = vec_from_bytes(&u32::MAX.to_le_bytes().repeat(8));
        x86.xmm[5] = vec_from_bytes(&[0xFF; 32]);
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x54, 0x51, 0x20], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[4], lane, 32),
                u32::MAX as u64
            );
        }
    }

    let sentinel = [0x4242_4242_4242_4242; 16];
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3F0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[4] = sentinel;
    }
    let exit = execute_lifted_x86(&[0xC4, 0xE2, 0x54, 0x51, 0x20], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[4], sentinel);
    }
}
