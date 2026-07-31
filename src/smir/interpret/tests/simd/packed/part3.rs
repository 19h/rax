//! packed part 3 tests

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn lifted_saturating_packs_execute_lane_groups_masks_and_fault_boundaries() {
    fn seeded(bytes: &[u8], fill: u64) -> VecValue {
        let mut value = [fill; 16];
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            value[index] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        value
    }

    fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(len)
            .collect()
    }

    fn pack_reference(
        first: &[u8],
        second: &[u8],
        src_bytes: usize,
        block_bytes: usize,
        to_unsigned: bool,
    ) -> Vec<u8> {
        let dst_bytes = src_bytes / 2;
        let block_lanes = block_bytes / src_bytes;
        let source_lanes = first.len() / src_bytes;
        let read_signed = |source: &[u8], lane: usize| -> i64 {
            let at = lane * src_bytes;
            match src_bytes {
                2 => i16::from_le_bytes(source[at..at + 2].try_into().unwrap()) as i64,
                4 => i32::from_le_bytes(source[at..at + 4].try_into().unwrap()) as i64,
                _ => unreachable!(),
            }
        };
        let saturate = |value: i64| -> u64 {
            if to_unsigned {
                value.clamp(0, (1i64 << (dst_bytes * 8)) - 1) as u64
            } else {
                let high = (1i64 << (dst_bytes * 8 - 1)) - 1;
                let low = -(1i64 << (dst_bytes * 8 - 1));
                value.clamp(low, high) as u64
            }
        };
        let mut result = Vec::with_capacity(first.len());
        for block_base in (0..source_lanes).step_by(block_lanes) {
            for source in [first, second] {
                for lane in block_base..block_base + block_lanes {
                    result.extend_from_slice(
                        &saturate(read_signed(source, lane)).to_le_bytes()[..dst_bytes],
                    );
                }
            }
        }
        result
    }

    let words1 = [
        -400i16,
        -129,
        -128,
        -1,
        0,
        1,
        127,
        128,
        255,
        256,
        i16::MAX,
        i16::MIN,
        42,
        -42,
        300,
        -300,
    ];
    let words2 = [
        500i16, 129, 128, 2, -2, 126, -127, -500, 254, 257, 1000, -1000, 7, -7, 200, -200,
    ];
    let dwords1 = [-100_000i32, -32_769, -32_768, -1, 0, 32_767, 65_535, 65_536];
    let dwords2 = [i32::MAX, i32::MIN, 1, 32_768, 65_534, 70_000, -2, 1234];
    let words1_bytes = words1
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<_>>();
    let words2_bytes = words2
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<_>>();
    let dwords1_bytes = dwords1
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<_>>();
    let dwords2_bytes = dwords2
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<_>>();
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let flags_before = 0xCD7;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    for (opcode, first, second, src_bytes, to_unsigned) in [
        (
            0x63,
            words1_bytes.as_slice(),
            words2_bytes.as_slice(),
            2,
            false,
        ),
        (
            0x67,
            words1_bytes.as_slice(),
            words2_bytes.as_slice(),
            2,
            true,
        ),
        (
            0x6B,
            dwords1_bytes.as_slice(),
            dwords2_bytes.as_slice(),
            4,
            false,
        ),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
            x86.mm[1] = u64::from_le_bytes(second[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 6 << 11;
        }
        execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0].to_le_bytes().as_slice(),
                pack_reference(&first[..8], &second[..8], src_bytes, 8, to_unsigned),
                "MMX opcode {opcode:02X}",
            );
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 6 << 11);
        }
    }

    // MMX memory packs consume exactly one 8-byte source before entering
    // MMX state and committing their destructive result.
    memory.write(0x3F8, &words2_bytes[..8]).unwrap();
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    ctx.write_vreg(rax, 0x3F8);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes(words1_bytes[..8].try_into().unwrap());
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0x63, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0].to_le_bytes().as_slice(),
            pack_reference(&words1_bytes[..8], &words2_bytes[..8], 2, 8, false)
        );
        assert_eq!(x86.x87.tag_word, 0);
    }

    // A source fault changes neither MMX data nor the x87 tag word.
    ctx.write_vreg(rax, 0x1000);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xA5A5_5A5A_C3C3_3C3C;
        x86.x87.tag_word = 0xFFFF;
    }
    let mmx_fault = execute_lifted_x86(&[0x0F, 0x63, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        mmx_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xA5A5_5A5A_C3C3_3C3C);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    for (legacy, vex, first, second, src_bytes, to_unsigned) in [
        (
            &[0x66, 0x0F, 0x63, 0xC1][..],
            &[0xC5, 0xF5, 0x63, 0xC2][..],
            words1_bytes.as_slice(),
            words2_bytes.as_slice(),
            2,
            false,
        ),
        (
            &[0x66, 0x0F, 0x67, 0xC1][..],
            &[0xC5, 0xF5, 0x67, 0xC2][..],
            words1_bytes.as_slice(),
            words2_bytes.as_slice(),
            2,
            true,
        ),
        (
            &[0x66, 0x0F, 0x6B, 0xC1][..],
            &[0xC5, 0xF5, 0x6B, 0xC2][..],
            dwords1_bytes.as_slice(),
            dwords2_bytes.as_slice(),
            4,
            false,
        ),
        (
            &[0x66, 0x0F, 0x38, 0x2B, 0xC1][..],
            &[0xC4, 0xE2, 0x75, 0x2B, 0xC2][..],
            dwords1_bytes.as_slice(),
            dwords2_bytes.as_slice(),
            4,
            true,
        ),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&first[..16], upper);
            x86.xmm[1] = seeded(&second[..16], 0);
        }
        execute_lifted_x86(legacy, &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                pack_reference(&first[..16], &second[..16], src_bytes, 16, to_unsigned),
                "legacy {legacy:02X?}",
            );
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1] = seeded(first, 0);
            x86.xmm[2] = seeded(second, 0);
        }
        execute_lifted_x86(vex, &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                pack_reference(first, second, src_bytes, 16, to_unsigned),
                "VEX {vex:02X?}",
            );
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }
    }

    // EVEX masking is applied to the packed word result, after independent
    // 128-bit groups. Merging and zeroing both clear backing state above VL.
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let raw = pack_reference(&dwords1_bytes, &dwords2_bytes, 4, 16, true);
    let mask = 0xA55Au64;
    for (p2, zeroing) in [(0x29, false), (0xA9, true)] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&[0xEE; 32], u64::MAX);
            x86.xmm[1] = seeded(&dwords1_bytes, 0);
            x86.xmm[2] = seeded(&dwords2_bytes, 0);
        }
        ctx.write_vreg(k1, mask);
        execute_lifted_x86(&[0x62, 0xF2, 0x75, p2, 0x2B, 0xC2], &mut ctx, &mut memory);
        let mut expected = Vec::with_capacity(32);
        for lane in 0..16 {
            if mask >> lane & 1 != 0 {
                expected.extend_from_slice(&raw[lane * 2..lane * 2 + 2]);
            } else if zeroing {
                expected.extend_from_slice(&[0, 0]);
            } else {
                expected.extend_from_slice(&[0xEE, 0xEE]);
            }
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 32), expected);
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

    // Non-EVEX memory forms retain their full-width all-or-fault boundary.
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    ctx.write_vreg(rax, 0x3F0);
    let exit = execute_lifted_x86(&[0xC5, 0xF5, 0x63, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }
}
#[test]
fn lifted_pshufb_executes_msb_zeroing_lane_locality_masks_and_faults() {
    fn seeded(bytes: &[u8], fill: u64) -> VecValue {
        let mut value = [fill; 16];
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            value[index] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        value
    }

    fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(len)
            .collect()
    }

    fn reference(source: &[u8], control: &[u8]) -> Vec<u8> {
        let mut result = vec![0; source.len()];
        for block_base in (0..source.len()).step_by(16) {
            for lane in 0..16 {
                let selector = control[block_base + lane];
                if selector & 0x80 == 0 {
                    result[block_base + lane] = source[block_base + usize::from(selector & 0x0F)];
                }
            }
        }
        result
    }

    let source = (0x10u8..=0x4F).collect::<Vec<_>>();
    let control_block = [
        0x00, 0x01, 0x0F, 0x10, 0x1F, 0x7F, 0x80, 0x8F, 0x02, 0x0E, 0x12, 0x2E, 0xFF, 0x04, 0x08,
        0x0C,
    ];
    let control = control_block.repeat(4);
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let flags_before = 0xCD7;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    let mmx_source = u64::from_le_bytes([0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17]);
    let mmx_control = u64::from_le_bytes([0x00, 0x07, 0x08, 0x87, 0x02, 0x06, 0x80, 0x03]);
    let mmx_expected = [0x10, 0x17, 0x10, 0x00, 0x12, 0x16, 0x00, 0x13];

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = mmx_source;
        x86.mm[1] = mmx_control;
        x86.x87.tag_word = 0xFFFF;
        x86.x87.status_word = 6 << 11;
    }
    execute_lifted_x86(&[0x0F, 0x38, 0x00, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0].to_le_bytes(), mmx_expected);
        assert_eq!(x86.mm[1], mmx_control);
        assert_eq!(x86.x87.tag_word, 0);
        assert_eq!(x86.x87.status_word & 0x3800, 6 << 11);
    }

    // The MMX control source is m64 and has no mandatory #GP alignment.
    // A faulting complete load leaves the destructive destination and the
    // x87/MMX state unchanged.
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    memory.write(0x81, &mmx_control.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x80);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = mmx_source;
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0x38, 0x00, 0x40, 0x01], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0].to_le_bytes(), mmx_expected);
        assert_eq!(x86.x87.tag_word, 0);
    }

    ctx.write_vreg(rax, 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
        x86.x87.tag_word = 0xFFFF;
    }
    let mmx_fault = execute_lifted_x86(&[0x0F, 0x38, 0x00, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        mmx_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = seeded(&source[..16], upper);
        x86.xmm[1] = seeded(&control[..16], 0);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x00, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 16),
            reference(&source[..16], &control[..16])
        );
        assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
    }

    // The legacy memory form checks its mandatory 16-byte alignment before
    // reading controls or modifying the destructive destination.
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    memory.write(0x101, &control[..16]).unwrap();
    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x00, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    // Destructive legacy aliasing must snapshot both data and controls
    // before the first architectural destination-byte write.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = seeded(&source[..16], upper);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x00, 0xC0], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 16),
            reference(&source[..16], &source[..16])
        );
    }

    // VEX.256 uses two independent 16-byte tables and clears all backing
    // state above bit 255.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [u64::MAX; 16];
        x86.xmm[1] = seeded(&source[..32], 0);
        x86.xmm[2] = seeded(&control[..32], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x00, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 32),
            reference(&source[..32], &control[..32])
        );
        assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
    }

    // EVEX masking operates on the shuffled byte result. Both merge and
    // zero forms clear backing state above the selected vector length.
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let raw = reference(&source, &control);
    let mask = 0xA55A_F00F_1234_89ABu64;
    for (p2, zeroing) in [(0x49, false), (0xC9, true)] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&[0xEE; 64], u64::MAX);
            x86.xmm[1] = seeded(&source, 0);
            x86.xmm[2] = seeded(&control, 0);
        }
        ctx.write_vreg(k1, mask);
        execute_lifted_x86(&[0x62, 0xF2, 0x75, p2, 0x00, 0xC2], &mut ctx, &mut memory);
        let mut expected = Vec::with_capacity(64);
        for lane in 0..64 {
            expected.push(if mask >> lane & 1 != 0 {
                raw[lane]
            } else if zeroing {
                0
            } else {
                0xEE
            });
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 64), expected);
            assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
        }
    }

    // A masked memory control byte is accessed iff its corresponding
    // output byte is active. Put byte 0 at the final valid address so lane
    // 0 succeeds while lane 1 demonstrates precise fault suppression.
    memory.write(0x3FF, &[0]).unwrap();
    ctx.write_vreg(rax, 0x3FF);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.xmm[1] = seeded(&source[..16], 0);
    }
    ctx.write_vreg(k1, 1);
    let suppressed =
        execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x09, 0x00, 0x00], &mut ctx, &mut memory);
    assert!(!matches!(
        suppressed,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    ctx.write_vreg(k1, 1 << 1);
    let exposed = execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x09, 0x00, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        exposed,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

    // VEX memory controls are full-width all-or-fault loads.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    ctx.write_vreg(rax, 0x3F0);
    let fault = execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x00, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }
}
#[test]
fn vdotproduct_executes_vnni_accumulation_wrapping_saturation_and_signed_words() {
    fn seeded(bytes: &[u8]) -> VecValue {
        let mut value = [0; 16];
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            value[index] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        value
    }

    fn run(
        acc: VecValue,
        src1: VecValue,
        src2: VecValue,
        src_elem: VecElementType,
        src1_unsigned: bool,
        saturate: bool,
        masking: Option<(u64, bool)>,
    ) -> VecValue {
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)));
        let first = VReg::Arch(ArchReg::X86(X86Reg::Zmm(1)));
        let second = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
        let k4 = VReg::Arch(ArchReg::X86(X86Reg::K(4)));
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = acc;
            x86.xmm[1] = src1;
            x86.xmm[2] = src2;
            x86.k[4] = masking.map_or(0, |(mask, _)| mask);
        }
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::VDotProduct {
                dst,
                acc: dst,
                src1: first,
                src2: second,
                mask: masking.map(|_| k4),
                src_elem,
                acc_elem: VecElementType::I32,
                width: VecWidth::V128,
                src1_unsigned,
                saturate,
                zeroing: masking.is_some_and(|(_, zeroing)| zeroing),
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let block = &builder.finish().blocks[0];
        let exit =
            SmirInterpreter::new().execute_block(&mut ctx, &mut FlatMemory::new(0x100), block);
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.xmm[0],
            _ => unreachable!(),
        }
    }

    let acc = [1_000i32, i32::MAX - 10, i32::MIN + 10, -100];
    let first = [
        1u8, 2, 3, 4, 255, 255, 255, 255, 255, 255, 255, 255, 0, 128, 255, 4,
    ];
    let second = [
        1i8, -2, 3, -4, 127, 127, 127, 127, -128, -128, -128, -128, -1, 1, -1, 127,
    ];
    let acc_vec = seeded(
        &acc.iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>(),
    );
    let first_vec = seeded(&first);
    let second_vec = seeded(&second.iter().map(|value| *value as u8).collect::<Vec<_>>());
    let sums = (0..4)
        .map(|lane| {
            i64::from(acc[lane])
                + (0..4)
                    .map(|term| {
                        i64::from(first[lane * 4 + term]) * i64::from(second[lane * 4 + term])
                    })
                    .sum::<i64>()
        })
        .collect::<Vec<_>>();
    let saturated = run(
        acc_vec,
        first_vec,
        second_vec,
        VecElementType::I8,
        true,
        true,
        None,
    );
    let wrapping = run(
        acc_vec,
        first_vec,
        second_vec,
        VecElementType::I8,
        true,
        false,
        None,
    );
    for lane in 0..4 {
        assert_eq!(
            SmirInterpreter::get_lane(&saturated, lane as u8, 32) as u32,
            sums[lane].clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32 as u32
        );
        assert_eq!(
            SmirInterpreter::get_lane(&wrapping, lane as u8, 32) as u32,
            sums[lane] as i32 as u32
        );
    }
    assert!(saturated[2..].iter().all(|word| *word == 0));

    let word_acc = [17i32, -33, 44, -55];
    let word_first = [-32768i16, 32767, -123, 456, 1000, -2000, 3000, -4000];
    let word_second = [-1i16, 2, 300, -400, -30, 40, -50, 60];
    let word_result = run(
        seeded(
            &word_acc
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        ),
        seeded(
            &word_first
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        ),
        seeded(
            &word_second
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        ),
        VecElementType::I16,
        false,
        false,
        None,
    );
    for lane in 0..4 {
        let expected = word_acc[lane].wrapping_add(
            i32::from(word_first[lane * 2]) * i32::from(word_second[lane * 2])
                + i32::from(word_first[lane * 2 + 1]) * i32::from(word_second[lane * 2 + 1]),
        );
        assert_eq!(
            SmirInterpreter::get_lane(&word_result, lane as u8, 32) as u32,
            expected as u32
        );
    }

    for (zeroing, masked_off) in [(false, acc[1] as u32), (true, 0)] {
        let masked = run(
            acc_vec,
            first_vec,
            second_vec,
            VecElementType::I8,
            true,
            false,
            Some((0b0101, zeroing)),
        );
        assert_eq!(
            SmirInterpreter::get_lane(&masked, 0, 32) as u32,
            sums[0] as i32 as u32
        );
        assert_eq!(SmirInterpreter::get_lane(&masked, 1, 32) as u32, masked_off);
        assert_eq!(
            SmirInterpreter::get_lane(&masked, 2, 32) as u32,
            sums[2] as i32 as u32
        );
        assert_eq!(
            SmirInterpreter::get_lane(&masked, 3, 32) as u32,
            if zeroing { 0 } else { acc[3] as u32 }
        );
    }
}
#[test]
fn lifted_pmaddubsw_executes_products_saturation_masks_aliases_and_faults() {
    fn seeded(bytes: &[u8], fill: u64) -> VecValue {
        let mut value = [fill; 16];
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            value[index] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        value
    }

    fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(len)
            .collect()
    }

    fn reference(unsigned: &[u8], signed: &[u8]) -> Vec<u8> {
        unsigned
            .chunks_exact(2)
            .zip(signed.chunks_exact(2))
            .flat_map(|(a, b)| {
                let sum = i32::from(a[0]) * i32::from(b[0] as i8)
                    + i32::from(a[1]) * i32::from(b[1] as i8);
                (sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16).to_le_bytes()
            })
            .collect()
    }

    let mut unsigned = (0..64)
        .map(|lane| (lane * 37 + 11) as u8)
        .collect::<Vec<_>>();
    let mut signed = (0..64)
        .map(|lane| ((lane as i8).wrapping_mul(29)).wrapping_sub(93) as u8)
        .collect::<Vec<_>>();
    unsigned[0..4].copy_from_slice(&[255, 255, 255, 255]);
    signed[0..4].copy_from_slice(&[127, 127, 0x80, 0x80]);
    let expected = reference(&unsigned, &signed);
    assert_eq!(
        i16::from_le_bytes(expected[0..2].try_into().unwrap()),
        i16::MAX
    );
    assert_eq!(
        i16::from_le_bytes(expected[2..4].try_into().unwrap()),
        i16::MIN
    );

    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes(unsigned[..8].try_into().unwrap());
        x86.mm[1] = u64::from_le_bytes(signed[..8].try_into().unwrap());
        x86.x87.tag_word = 0xFFFF;
        x86.x87.status_word = 3 << 11;
    }
    execute_lifted_x86(&[0x0F, 0x38, 0x04, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0],
            u64::from_le_bytes(expected[..8].try_into().unwrap())
        );
        assert_eq!(x86.x87.tag_word, 0);
        assert_eq!(x86.x87.status_word & 0x3800, 3 << 11);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = seeded(&unsigned[..16], upper);
        x86.xmm[1] = seeded(&signed[..16], 0);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x04, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 16), expected[..16]);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.xmm[1] = seeded(&unsigned[..32], 0);
        x86.xmm[2] = seeded(&signed[..32], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x04, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 32), expected[..32]);
        assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
    }

    // Destructive legacy aliasing must read every unsigned and signed byte
    // before merging any result word back into the shared destination.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = seeded(&unsigned[..16], upper);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x04, 0xC0], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 16),
            reference(&unsigned[..16], &unsigned[..16])
        );
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes(unsigned[..8].try_into().unwrap());
    }
    execute_lifted_x86(&[0x0F, 0x38, 0x04, 0xC0], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0],
            u64::from_le_bytes(
                reference(&unsigned[..8], &unsigned[..8])
                    .try_into()
                    .unwrap()
            )
        );
    }

    // EVEX applies each mask bit to one signed-word result. Validate both
    // merge and zero modes over all 32 ZMM result lanes.
    let raw = reference(&unsigned, &signed);
    let mask = 0xA55A_89ABu64;
    for (p2, zeroing) in [(0x49, false), (0xC9, true)] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
            x86.xmm[1] = seeded(&unsigned, 0);
            x86.xmm[2] = seeded(&signed, 0);
        }
        ctx.write_vreg(k1, mask);
        execute_lifted_x86(&[0x62, 0xF2, 0x75, p2, 0x04, 0xC2], &mut ctx, &mut memory);
        let mut masked = vec![0; 64];
        for lane in 0..32 {
            let at = lane * 2;
            if mask >> lane & 1 != 0 {
                masked[at..at + 2].copy_from_slice(&raw[at..at + 2]);
            } else if !zeroing {
                masked[at..at + 2].copy_from_slice(&[0x6B, 0x6B]);
            }
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 64), masked);
            assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
        }
    }

    // Independently encoded high-register EVEX form: zmm16 := zmm17,zmm18.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[16] = sentinel;
        x86.xmm[17] = seeded(&unsigned, 0);
        x86.xmm[18] = seeded(&signed, 0);
    }
    execute_lifted_x86(&[0x62, 0xA2, 0x75, 0x40, 0x04, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[16], 64), expected);
    }

    memory.write(0x101, &signed).unwrap();
    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x04, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes(unsigned[..8].try_into().unwrap());
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0x38, 0x04, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0],
            u64::from_le_bytes(expected[..8].try_into().unwrap())
        );
        assert_eq!(x86.x87.tag_word, 0);
    }

    ctx.write_vreg(rax, 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
        x86.x87.tag_word = 0xFFFF;
    }
    let mmx_fault = execute_lifted_x86(&[0x0F, 0x38, 0x04, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        mmx_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    // VEX accepts the identical unaligned address and performs a complete
    // all-or-fault 32-byte load.
    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.xmm[1] = seeded(&unsigned[..32], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x04, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 32), expected[..32]);
    }

    // E4NF: destination masking never suppresses the complete memory read.
    // Only two of the required 16 bytes are mapped, so both a single-bit
    // mask and an all-zero mask fault before modifying the destination.
    memory.write(0x3FE, &signed[..2]).unwrap();
    ctx.write_vreg(rax, 0x3FE);
    for mask in [1, 0] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&unsigned[..16], 0);
        }
        ctx.write_vreg(k1, mask);
        let fault =
            execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x09, 0x04, 0x00], &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], sentinel);
        }
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    ctx.write_vreg(rax, 0x3F0);
    let fault = execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x04, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }
}
#[test]
fn lifted_pmulhrsw_executes_rounding_masks_aliases_and_faults() {
    fn seeded(bytes: &[u8], fill: u64) -> VecValue {
        let mut value = [fill; 16];
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            value[index] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        value
    }

    fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(len)
            .collect()
    }

    fn reference(first: &[u8], second: &[u8]) -> Vec<u8> {
        first
            .chunks_exact(2)
            .zip(second.chunks_exact(2))
            .flat_map(|(a, b)| {
                let a = i32::from(i16::from_le_bytes(a.try_into().unwrap()));
                let b = i32::from(i16::from_le_bytes(b.try_into().unwrap()));
                (((a * b + 0x4000) >> 15) as i16).to_le_bytes()
            })
            .collect()
    }

    let first_words = [
        i16::MIN,
        i16::MAX,
        0x4000,
        -0x4000,
        1,
        -1,
        0x1234,
        -0x2345,
        i16::MIN,
        i16::MAX,
        0x2000,
        -0x2000,
        0x7FFE,
        -0x7FFF,
        17,
        -29,
        i16::MIN,
        i16::MAX,
        0x4000,
        -0x4000,
        1,
        -1,
        0x1234,
        -0x2345,
        i16::MIN,
        i16::MAX,
        0x2000,
        -0x2000,
        0x7FFE,
        -0x7FFF,
        17,
        -29,
    ];
    let second_words = [
        i16::MIN,
        i16::MIN,
        0x4000,
        0x4000,
        i16::MAX,
        i16::MAX,
        -0x3456,
        0x4567,
        i16::MIN,
        i16::MAX,
        -0x2000,
        -0x2000,
        0x7FFF,
        -0x7FFF,
        -31,
        43,
        i16::MIN,
        i16::MIN,
        0x4000,
        0x4000,
        i16::MAX,
        i16::MAX,
        -0x3456,
        0x4567,
        i16::MIN,
        i16::MAX,
        -0x2000,
        -0x2000,
        0x7FFF,
        -0x7FFF,
        -31,
        43,
    ];
    let first = first_words
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    let second = second_words
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    let expected = reference(&first, &second);
    assert_eq!(expected[..2], i16::MIN.to_le_bytes());

    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
        x86.mm[1] = u64::from_le_bytes(second[..8].try_into().unwrap());
        x86.x87.tag_word = 0xFFFF;
        x86.x87.status_word = 3 << 11;
    }
    execute_lifted_x86(&[0x0F, 0x38, 0x0B, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0],
            u64::from_le_bytes(expected[..8].try_into().unwrap())
        );
        assert_eq!(x86.x87.tag_word, 0);
        assert_eq!(x86.x87.status_word & 0x3800, 3 << 11);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = seeded(&first[..16], upper);
        x86.xmm[1] = seeded(&second[..16], 0);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x0B, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 16), expected[..16]);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.xmm[1] = seeded(&first[..32], 0);
        x86.xmm[2] = seeded(&second[..32], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x0B, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 32), expected[..32]);
        assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = seeded(&first[..16], upper);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x0B, 0xC0], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 16),
            reference(&first[..16], &first[..16])
        );
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
    }
    execute_lifted_x86(&[0x0F, 0x38, 0x0B, 0xC0], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0],
            u64::from_le_bytes(reference(&first[..8], &first[..8]).try_into().unwrap())
        );
    }

    let raw = reference(&first, &second);
    let mask = 0xA55A_89ABu64;
    for (p2, zeroing) in [(0x49, false), (0xC9, true)] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
            x86.xmm[1] = seeded(&first, 0);
            x86.xmm[2] = seeded(&second, 0);
        }
        ctx.write_vreg(k1, mask);
        execute_lifted_x86(&[0x62, 0xF2, 0x75, p2, 0x0B, 0xC2], &mut ctx, &mut memory);
        let mut masked = vec![0; 64];
        for lane in 0..32 {
            let at = lane * 2;
            if mask >> lane & 1 != 0 {
                masked[at..at + 2].copy_from_slice(&raw[at..at + 2]);
            } else if !zeroing {
                masked[at..at + 2].copy_from_slice(&[0x6B, 0x6B]);
            }
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 64), masked);
            assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
        }
    }

    memory.write(0x101, &second).unwrap();
    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x0B, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0x38, 0x0B, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0],
            u64::from_le_bytes(expected[..8].try_into().unwrap())
        );
        assert_eq!(x86.x87.tag_word, 0);
    }

    ctx.write_vreg(rax, 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
        x86.x87.tag_word = 0xFFFF;
    }
    let mmx_fault = execute_lifted_x86(&[0x0F, 0x38, 0x0B, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        mmx_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.xmm[1] = seeded(&first[..32], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x0B, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 32), expected[..32]);
    }

    memory.write(0x3FE, &second[..2]).unwrap();
    ctx.write_vreg(rax, 0x3FE);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.xmm[1] = seeded(&first[..16], 0);
    }
    ctx.write_vreg(k1, 1);
    let suppressed =
        execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x09, 0x0B, 0x00], &mut ctx, &mut memory);
    assert!(!matches!(
        suppressed,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 2), expected[..2]);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    ctx.write_vreg(k1, 1 << 1);
    let exposed = execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x09, 0x0B, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        exposed,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    ctx.write_vreg(rax, 0x3F0);
    let fault = execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x0B, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }
}
#[test]
fn lifted_pabs_family_executes_minima_masks_broadcasts_and_faults() {
    fn seeded(bytes: &[u8], fill: u64) -> VecValue {
        let mut value = [fill; 16];
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            value[index] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        value
    }

    fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(len)
            .collect()
    }

    fn reference(input: &[u8], elem_bytes: usize) -> Vec<u8> {
        input
            .chunks_exact(elem_bytes)
            .flat_map(|lane| match elem_bytes {
                1 => vec![(lane[0] as i8).wrapping_abs() as u8],
                2 => i16::from_le_bytes(lane.try_into().unwrap())
                    .wrapping_abs()
                    .to_le_bytes()
                    .to_vec(),
                4 => i32::from_le_bytes(lane.try_into().unwrap())
                    .wrapping_abs()
                    .to_le_bytes()
                    .to_vec(),
                8 => i64::from_le_bytes(lane.try_into().unwrap())
                    .wrapping_abs()
                    .to_le_bytes()
                    .to_vec(),
                _ => unreachable!(),
            })
            .collect()
    }

    let mut byte_input = (0..64)
        .map(|lane| (lane * 37 + 0x41) as u8)
        .collect::<Vec<_>>();
    byte_input[0] = i8::MIN as u8;
    byte_input[1] = (-1i8) as u8;
    byte_input[2] = 0;
    byte_input[3] = i8::MAX as u8;
    let word_input = [
        i16::MIN,
        -1,
        0,
        i16::MAX,
        -0x1234,
        0x2345,
        -17,
        29,
        i16::MIN,
        -1,
        0,
        i16::MAX,
        -0x3456,
        0x4567,
        -31,
        43,
        i16::MIN,
        -1,
        0,
        i16::MAX,
        -0x1234,
        0x2345,
        -17,
        29,
        i16::MIN,
        -1,
        0,
        i16::MAX,
        -0x3456,
        0x4567,
        -31,
        43,
    ]
    .iter()
    .flat_map(|value| value.to_le_bytes())
    .collect::<Vec<_>>();
    let dword_input = [
        i32::MIN,
        -1,
        0,
        i32::MAX,
        -0x1234_567,
        0x2345_678,
        -17,
        29,
        i32::MIN,
        -1,
        0,
        i32::MAX,
        -0x3456_789,
        0x4567_89A,
        -31,
        43,
    ]
    .iter()
    .flat_map(|value| value.to_le_bytes())
    .collect::<Vec<_>>();
    let qword_input = [
        i64::MIN,
        -1,
        0,
        i64::MAX,
        -0x1234_5678_9ABC,
        0x2345_6789_ABCD,
        -17,
        29,
    ]
    .iter()
    .flat_map(|value| value.to_le_bytes())
    .collect::<Vec<_>>();
    let cases = [
        (0x1C, 1usize, byte_input.as_slice()),
        (0x1D, 2, word_input.as_slice()),
        (0x1E, 4, dword_input.as_slice()),
        (0x1F, 8, qword_input.as_slice()),
    ];

    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    // Prefix-free SSSE3 PABS operates on an MMX destination and an mm/m64
    // source.  Exercise every element width, the wrapping minimum value,
    // the x87/MMX state transition, and a destructive register alias.
    for &(opcode, elem_bytes, input) in &cases {
        if opcode == 0x1F {
            continue;
        }
        let input = &input[..8];
        let source = u64::from_le_bytes(input.try_into().unwrap());
        let expected = u64::from_le_bytes(reference(input, elem_bytes).try_into().unwrap());
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = source;
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 5 << 11;
        }
        execute_lifted_x86(&[0x0F, 0x38, opcode, 0xC0], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], expected, "MMX opcode={opcode:02X}");
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 5 << 11);
        }
    }

    // The m64 form has no mandatory 16-byte #GP alignment.  Its complete
    // source load faults before either the destination or MMX state changes.
    let mmx_word_expected = u64::from_le_bytes(reference(&word_input[..8], 2).try_into().unwrap());
    memory.write(0x81, &word_input[..8]).unwrap();
    ctx.write_vreg(rax, 0x80);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0x38, 0x1D, 0x40, 0x01], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], mmx_word_expected);
        assert_eq!(x86.x87.tag_word, 0);
    }

    ctx.write_vreg(rax, 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
        x86.x87.tag_word = 0xFFFF;
    }
    let mmx_fault = execute_lifted_x86(&[0x0F, 0x38, 0x1D, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        mmx_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    for (opcode, elem_bytes, input) in cases {
        let expected = reference(input, elem_bytes);
        assert_eq!(&expected[..elem_bytes], &input[..elem_bytes]);

        if opcode != 0x1F {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = [upper; 16];
                x86.xmm[1] = seeded(&input[..16], 0);
            }
            execute_lifted_x86(&[0x66, 0x0F, 0x38, opcode, 0xC1], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 16), expected[..16]);
                assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
            }

            let vex_p2 = 0x7D;
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[0] = sentinel;
                x86.xmm[2] = seeded(&input[..32], 0);
            }
            execute_lifted_x86(&[0xC4, 0xE2, vex_p2, opcode, 0xC2], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(bytes(&x86.xmm[0], 32), expected[..32]);
                assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
            }
        }

        let evex_w = if opcode == 0x1F { 0xFD } else { 0x7D };
        let lanes = 64 / elem_bytes;
        let mask = 0xA55A_89AB_F00F_1357u64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
            x86.xmm[2] = seeded(input, 0);
        }
        ctx.write_vreg(k1, mask);
        execute_lifted_x86(
            &[0x62, 0xF2, evex_w, 0x49, opcode, 0xC2],
            &mut ctx,
            &mut memory,
        );
        let mut masked = vec![0; 64];
        for lane in 0..lanes {
            let at = lane * elem_bytes;
            if mask >> lane & 1 != 0 {
                masked[at..at + elem_bytes].copy_from_slice(&expected[at..at + elem_bytes]);
            } else {
                masked[at..at + elem_bytes].fill(0x6B);
            }
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 64), masked);
            assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
        }
    }

    // Dword broadcast repeats one wrapping absolute value across all lanes.
    memory.write(0x100, &i32::MIN.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x100);
    execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x58, 0x1E, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 64),
            i32::MIN
                .to_le_bytes()
                .into_iter()
                .cycle()
                .take(64)
                .collect::<Vec<_>>()
        );
    }

    // A zero mask suppresses a broadcast memory fault; any active lane
    // requires the single scalar read and exposes it.
    ctx.write_vreg(rax, 0x1000);
    ctx.write_vreg(k1, 0);
    let suppressed =
        execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x59, 0x1E, 0x00], &mut ctx, &mut memory);
    assert!(!matches!(
        suppressed,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    ctx.write_vreg(k1, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let exposed = execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x59, 0x1E, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        exposed,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    // Normal masked memory accesses are per element.
    memory.write(0x3FF, &[i8::MIN as u8]).unwrap();
    ctx.write_vreg(rax, 0x3FF);
    ctx.write_vreg(k1, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let lane0 = execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x09, 0x1C, 0x00], &mut ctx, &mut memory);
    assert!(!matches!(
        lane0,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 1), vec![i8::MIN as u8]);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
    }
    ctx.write_vreg(k1, 1 << 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let lane1 = execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x09, 0x1C, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        lane1,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    memory.write(0x101, &word_input).unwrap();
    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x1D, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x1D, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 32), reference(&word_input[..32], 2));
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_palignr_executes_immediates_grouping_masks_aliases_and_faults() {
    fn seeded(bytes: &[u8], fill: u64) -> VecValue {
        let mut value = [fill; 16];
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            value[index] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        value
    }

    fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(len)
            .collect()
    }

    fn reference(high: &[u8], low: &[u8], imm: u8) -> Vec<u8> {
        let mut result = vec![0; high.len()];
        let block_bytes = usize::min(16, high.len());
        for block in 0..high.len() / block_bytes {
            let base = block * block_bytes;
            for lane in 0..block_bytes {
                let index = usize::from(imm) + lane;
                result[base + lane] = if index < block_bytes {
                    low[base + index]
                } else if index < block_bytes * 2 {
                    high[base + index - block_bytes]
                } else {
                    0
                };
            }
        }
        result
    }

    let high = (0..64).map(|lane| (lane + 1) as u8).collect::<Vec<_>>();
    let low = (0..64).map(|lane| (0x80 + lane) as u8).collect::<Vec<_>>();
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    for imm in [0u8, 1, 7, 8, 9, 15, 16, 255] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(high[..8].try_into().unwrap());
            x86.mm[1] = u64::from_le_bytes(low[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
        }
        execute_lifted_x86(&[0x0F, 0x3A, 0x0F, 0xC1, imm], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                u64::from_le_bytes(reference(&high[..8], &low[..8], imm).try_into().unwrap()),
                "MMX imm={imm}"
            );
            assert_eq!(x86.x87.tag_word, 0);
        }
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes(high[..8].try_into().unwrap());
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0x3A, 0x0F, 0xC0, 5], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0],
            u64::from_le_bytes(reference(&high[..8], &high[..8], 5).try_into().unwrap())
        );
    }

    memory.write(0x181, &low[..8]).unwrap();
    ctx.write_vreg(rax, 0x180);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes(high[..8].try_into().unwrap());
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0x3A, 0x0F, 0x40, 0x01, 5], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0],
            u64::from_le_bytes(reference(&high[..8], &low[..8], 5).try_into().unwrap())
        );
        assert_eq!(x86.x87.tag_word, 0);
    }

    ctx.write_vreg(rax, 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
        x86.x87.tag_word = 0xFFFF;
    }
    let mmx_fault = execute_lifted_x86(&[0x0F, 0x3A, 0x0F, 0x00, 5], &mut ctx, &mut memory);
    assert!(matches!(
        mmx_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    for imm in [0u8, 1, 15, 16, 17, 31, 32, 255] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&high[..16], upper);
            x86.xmm[1] = seeded(&low[..16], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x3A, 0x0F, 0xC1, imm], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                reference(&high[..16], &low[..16], imm),
                "legacy imm={imm}"
            );
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&high[..32], 0);
            x86.xmm[2] = seeded(&low[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE3, 0x75, 0x0F, 0xC2, imm], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                reference(&high[..32], &low[..32], imm),
                "VEX imm={imm}"
            );
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }
    }

    // Legacy destructive alias and both VEX destination alias directions.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = seeded(&high[..16], upper);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x3A, 0x0F, 0xC0, 0x05], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 16),
            reference(&high[..16], &high[..16], 5)
        );
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = seeded(&high[..32], 0);
        x86.xmm[2] = seeded(&low[..32], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE3, 0x7D, 0x0F, 0xC2, 0x05], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 32),
            reference(&high[..32], &low[..32], 5)
        );
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = seeded(&low[..32], 0);
        x86.xmm[1] = seeded(&high[..32], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE3, 0x75, 0x0F, 0xC0, 0x05], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 32),
            reference(&high[..32], &low[..32], 5)
        );
    }

    let raw = reference(&high, &low, 5);
    let mask = 0xA55A_89AB_F00F_1357u64;
    for (p2, zeroing) in [(0x49, false), (0xC9, true)] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
            x86.xmm[1] = seeded(&high, 0);
            x86.xmm[2] = seeded(&low, 0);
        }
        ctx.write_vreg(k1, mask);
        execute_lifted_x86(
            &[0x62, 0xF3, 0x75, p2, 0x0F, 0xC2, 0x05],
            &mut ctx,
            &mut memory,
        );
        let expected = (0..64)
            .map(|lane| {
                if mask >> lane & 1 != 0 {
                    raw[lane]
                } else if zeroing {
                    0
                } else {
                    0x6B
                }
            })
            .collect::<Vec<_>>();
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 64), expected);
            assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
        }
    }

    memory.write(0x101, &low).unwrap();
    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let misaligned =
        execute_lifted_x86(&[0x66, 0x0F, 0x3A, 0x0F, 0x00, 0x01], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.xmm[1] = seeded(&high[..32], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE3, 0x75, 0x0F, 0x00, 0x01], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 32),
            reference(&high[..32], &low[..32], 1)
        );
    }

    // At imm=0, output byte n consumes memory byte n. Put byte 0 at the
    // final valid address to distinguish suppressed lane 0 from lane 1.
    memory.write(0x3FF, &[low[0]]).unwrap();
    ctx.write_vreg(rax, 0x3FF);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.xmm[1] = seeded(&high[..16], 0);
    }
    ctx.write_vreg(k1, 1);
    let lane0 = execute_lifted_x86(
        &[0x62, 0xF3, 0x75, 0x09, 0x0F, 0x00, 0x00],
        &mut ctx,
        &mut memory,
    );
    assert!(!matches!(
        lane0,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 1), vec![low[0]]);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == 0));
    }
    ctx.write_vreg(k1, 1 << 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let lane1 = execute_lifted_x86(
        &[0x62, 0xF3, 0x75, 0x09, 0x0F, 0x00, 0x00],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        lane1,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    // imm=16 selects only src1: active output bytes do not consume the
    // memory concatenand, so the invalid address remains suppressed.
    ctx.write_vreg(rax, 0x1000);
    ctx.write_vreg(k1, u64::MAX);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = seeded(&high[..16], 0);
    }
    let shifted_out = execute_lifted_x86(
        &[0x62, 0xF3, 0x75, 0x09, 0x0F, 0x00, 0x10],
        &mut ctx,
        &mut memory,
    );
    assert!(!matches!(
        shifted_out,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 16), high[..16]);
    }

    // Without a writemask, the complete memory operand is still read.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let full_fault = execute_lifted_x86(
        &[0x62, 0xF3, 0x75, 0x08, 0x0F, 0x00, 0x10],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        full_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_packed_extend_families_execute_sign_zero_masks_aliases_and_faults() {
    fn seeded(input: &[u8], fill: u64) -> VecValue {
        let mut value = [fill; 16];
        for (index, byte) in input.iter().copied().enumerate() {
            let word = index / 8;
            let shift = (index % 8) * 8;
            value[word] = (value[word] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
        }
        value
    }

    fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(len)
            .collect()
    }

    fn reference(
        input: &[u8],
        source_bytes: usize,
        destination_bytes: usize,
        signed: bool,
        destination_len: usize,
    ) -> Vec<u8> {
        let lanes = destination_len / destination_bytes;
        assert_eq!(input.len(), lanes * source_bytes);
        let source_bits = source_bytes * 8;
        let mut result = Vec::with_capacity(destination_len);
        for lane in input.chunks_exact(source_bytes) {
            let mut raw_bytes = [0u8; 8];
            raw_bytes[..source_bytes].copy_from_slice(lane);
            let raw = u64::from_le_bytes(raw_bytes);
            let extended = if signed {
                let shift = 64 - source_bits;
                ((raw << shift) as i64 >> shift) as u64
            } else {
                raw
            };
            result.extend_from_slice(&extended.to_le_bytes()[..destination_bytes]);
        }
        assert_eq!(result.len(), destination_len);
        result
    }

    let cases = [
        (0x20, 1usize, 2usize, true),
        (0x21, 1, 4, true),
        (0x22, 1, 8, true),
        (0x23, 2, 4, true),
        (0x24, 2, 8, true),
        (0x25, 4, 8, true),
        (0x30, 1, 2, false),
        (0x31, 1, 4, false),
        (0x32, 1, 8, false),
        (0x33, 2, 4, false),
        (0x34, 2, 8, false),
        (0x35, 4, 8, false),
    ];
    // Keeping the high bit set in every byte guarantees discriminating
    // negative lanes for each 8-, 16-, and 32-bit source element width.
    let source = (0..32)
        .map(|index| 0x80 | (((index * 29 + 3) as u8) & 0x7F))
        .collect::<Vec<_>>();
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    for (opcode, source_bytes, destination_bytes, signed) in cases {
        let legacy_source_len = 16 / destination_bytes * source_bytes;
        let legacy_expected = reference(
            &source[..legacy_source_len],
            source_bytes,
            destination_bytes,
            signed,
            16,
        );
        if signed {
            assert_ne!(
                legacy_expected,
                reference(
                    &source[..legacy_source_len],
                    source_bytes,
                    destination_bytes,
                    false,
                    16,
                ),
                "signed opcode {opcode:02X} lacks a discriminating lane"
            );
        }
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [upper; 16];
            x86.xmm[1] = seeded(&source[..legacy_source_len], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                legacy_expected,
                "legacy {opcode:02X}"
            );
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        let vex_source_len = 32 / destination_bytes * source_bytes;
        let vex_expected = reference(
            &source[..vex_source_len],
            source_bytes,
            destination_bytes,
            signed,
            32,
        );
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[2] = seeded(&source[..vex_source_len], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x7D, opcode, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 32), vex_expected, "VEX {opcode:02X}");
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }

        let evex_source_len = 64 / destination_bytes * source_bytes;
        let evex_raw = reference(
            &source[..evex_source_len],
            source_bytes,
            destination_bytes,
            signed,
            64,
        );
        let mask = 0xA5A5u64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
            x86.xmm[2] = seeded(&source[..evex_source_len], 0);
        }
        ctx.write_vreg(k1, mask);
        execute_lifted_x86(
            &[0x62, 0xF2, 0x7D, 0x49, opcode, 0xC2],
            &mut ctx,
            &mut memory,
        );
        let mut evex_expected = vec![0x6B; 64];
        for lane in 0..64 / destination_bytes {
            if mask >> lane & 1 != 0 {
                let at = lane * destination_bytes;
                evex_expected[at..at + destination_bytes]
                    .copy_from_slice(&evex_raw[at..at + destination_bytes]);
            }
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 64), evex_expected, "EVEX {opcode:02X}");
            assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
        }
    }

    // The source and destination may alias. All source bytes must be
    // captured before the VEX zeroing destination write begins.
    let alias_source = &source[..16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = seeded(alias_source, upper);
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x20, 0xC0], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 32),
            reference(alias_source, 1, 2, true, 32)
        );
        assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
    }

    // Legacy PMOVSXBQ reads exactly two bytes, accepts an unaligned
    // address, and commits no destination state when the second read faults.
    memory.write(0x3FE, &[0x80, 0x7F]).unwrap();
    ctx.write_vreg(rax, 0x3FE);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [upper; 16];
    }
    let exact = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x22, 0x00], &mut ctx, &mut memory);
    assert!(!matches!(
        exact,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 0xFFFF_FFFF_FFFF_FF80);
        assert_eq!(x86.xmm[0][1], 0x7F);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
    }

    ctx.write_vreg(rax, 0x3FF);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let partial_fault = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x22, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        partial_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    // EVEX VPMOVZXBQ maps destination mask bit n to source byte n.
    // A masked-off byte is not read, including at an invalid address.
    ctx.write_vreg(rax, 0x3FF);
    ctx.write_vreg(k1, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
    }
    let lane0 = execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x49, 0x32, 0x00], &mut ctx, &mut memory);
    assert!(!matches!(
        lane0,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], 0x7F);
        assert!(
            x86.xmm[0][1..8]
                .iter()
                .all(|word| *word == 0x6B6B_6B6B_6B6B_6B6B)
        );
        assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
    }

    ctx.write_vreg(k1, 1 << 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let lane1 = execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x49, 0x32, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        lane1,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    ctx.write_vreg(rax, 0x1000);
    ctx.write_vreg(k1, 0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let all_suppressed =
        execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x49, 0x32, 0x00], &mut ctx, &mut memory);
    assert!(!matches!(
        all_suppressed,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[0][..8], &sentinel[..8]);
        assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_packed_minmax_executes_signedness_masks_aliases_broadcasts_and_faults() {
    fn seeded(input: &[u8], fill: u64) -> VecValue {
        let mut value = [fill; 16];
        for (index, byte) in input.iter().copied().enumerate() {
            let word = index / 8;
            let shift = (index % 8) * 8;
            value[word] = (value[word] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
        }
        value
    }

    fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(len)
            .collect()
    }

    fn reference(lhs: &[u8], rhs: &[u8], elem_bytes: usize, signed: bool, min: bool) -> Vec<u8> {
        assert_eq!(lhs.len(), rhs.len());
        let bits = elem_bytes * 8;
        let signed_value = |raw: u64| {
            let shift = 64 - bits;
            ((raw << shift) as i64) >> shift
        };
        lhs.chunks_exact(elem_bytes)
            .zip(rhs.chunks_exact(elem_bytes))
            .flat_map(|(a, b)| {
                let mut a_bytes = [0u8; 8];
                let mut b_bytes = [0u8; 8];
                a_bytes[..elem_bytes].copy_from_slice(a);
                b_bytes[..elem_bytes].copy_from_slice(b);
                let av = u64::from_le_bytes(a_bytes);
                let bv = u64::from_le_bytes(b_bytes);
                let take_a = if signed {
                    if min {
                        signed_value(av) < signed_value(bv)
                    } else {
                        signed_value(av) > signed_value(bv)
                    }
                } else if min {
                    av < bv
                } else {
                    av > bv
                };
                if take_a { a.to_vec() } else { b.to_vec() }
            })
            .collect()
    }

    let lhs = (0..64)
        .map(|index| [0x80, 0x7F, 0xFF, 0x00, 0x01, 0xFE, 0x40, 0xC0][index % 8])
        .collect::<Vec<_>>();
    let rhs = (0..64)
        .map(|index| [0x7F, 0x80, 0x00, 0xFF, 0xFE, 0x01, 0xC0, 0x40][index % 8])
        .collect::<Vec<_>>();
    let cases = [
        (0x38, 1usize, true, true),
        (0x39, 4, true, true),
        (0x3A, 2, false, true),
        (0x3B, 4, false, true),
        (0x3C, 1, true, false),
        (0x3D, 4, true, false),
        (0x3E, 2, false, false),
        (0x3F, 4, false, false),
    ];
    let qword_cases = [
        (0x39, true, true),
        (0x3B, false, true),
        (0x3D, true, false),
        (0x3F, false, false),
    ];
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    for (opcode, elem_bytes, signed, min) in cases {
        let legacy_expected = reference(&lhs[..16], &rhs[..16], elem_bytes, signed, min);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&lhs[..16], upper);
            x86.xmm[1] = seeded(&rhs[..16], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                legacy_expected,
                "legacy {opcode:02X}"
            );
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        let vex_expected = reference(&lhs[..32], &rhs[..32], elem_bytes, signed, min);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&lhs[..32], 0);
            x86.xmm[2] = seeded(&rhs[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, opcode, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 32), vex_expected, "VEX {opcode:02X}");
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }

        let raw = reference(&lhs, &rhs, elem_bytes, signed, min);
        let mask = 0xA55Au64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
            x86.xmm[1] = seeded(&lhs, 0);
            x86.xmm[2] = seeded(&rhs, 0);
        }
        ctx.write_vreg(k1, mask);
        execute_lifted_x86(
            &[0x62, 0xF2, 0x75, 0x49, opcode, 0xC2],
            &mut ctx,
            &mut memory,
        );
        let mut expected = vec![0x6B; 64];
        for lane in 0..64 / elem_bytes {
            if mask >> lane & 1 != 0 {
                let at = lane * elem_bytes;
                expected[at..at + elem_bytes].copy_from_slice(&raw[at..at + elem_bytes]);
            }
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 64), expected, "EVEX {opcode:02X}");
            assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
        }
    }

    for (opcode, signed, min) in qword_cases {
        let raw = reference(&lhs, &rhs, 8, signed, min);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
            x86.xmm[1] = seeded(&lhs, 0);
            x86.xmm[2] = seeded(&rhs, 0);
        }
        ctx.write_vreg(k1, u64::MAX);
        execute_lifted_x86(
            &[0x62, 0xF2, 0xF5, 0x49, opcode, 0xC2],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 64), raw, "EVEX qword {opcode:02X}");
            assert!(x86.xmm[0][8..].iter().all(|word| *word == 0));
        }
    }

    // VEX permits destination aliasing with either input.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = seeded(&lhs[..32], 0);
        x86.xmm[2] = seeded(&rhs[..32], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x38, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 32),
            reference(&lhs[..32], &rhs[..32], 1, true, true)
        );
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = seeded(&rhs[..32], 0);
        x86.xmm[1] = seeded(&lhs[..32], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x38, 0xC0], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 32),
            reference(&lhs[..32], &rhs[..32], 1, true, true)
        );
    }

    memory.write(0x100, &rhs).unwrap();
    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x38, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.xmm[1] = seeded(&lhs[..32], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x38, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 32),
            reference(&lhs[..32], &rhs[1..33], 1, true, true)
        );
    }

    // Masked byte memory accesses are fault-suppressed per destination lane.
    memory.write(0x3FF, &[rhs[0]]).unwrap();
    ctx.write_vreg(rax, 0x3FF);
    ctx.write_vreg(k1, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
        x86.xmm[1] = seeded(&lhs, 0);
    }
    let lane0 = execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x49, 0x38, 0x00], &mut ctx, &mut memory);
    assert!(!matches!(
        lane0,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    ctx.write_vreg(k1, 1 << 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let lane1 = execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x49, 0x38, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        lane1,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    // A zero mask suppresses a qword-broadcast fault. Any active lane
    // requires the scalar eight-byte read.
    ctx.write_vreg(rax, 0x1000);
    ctx.write_vreg(k1, 0);
    let suppressed =
        execute_lifted_x86(&[0x62, 0xF2, 0xF5, 0x59, 0x3F, 0x00], &mut ctx, &mut memory);
    assert!(!matches!(
        suppressed,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    ctx.write_vreg(k1, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let exposed = execute_lifted_x86(&[0x62, 0xF2, 0xF5, 0x59, 0x3F, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        exposed,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_mmx_packed_multiply_executes_widths_aliases_state_and_faults() {
    fn words(value: u64) -> [u16; 4] {
        let bytes = value.to_le_bytes();
        std::array::from_fn(|lane| {
            u16::from_le_bytes(bytes[lane * 2..lane * 2 + 2].try_into().unwrap())
        })
    }

    fn pack_words(value: [u16; 4]) -> u64 {
        u64::from_le_bytes(
            value
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        )
    }

    fn reference(opcode: u8, lhs: u64, rhs: u64) -> u64 {
        if opcode == 0xF4 {
            return u64::from(lhs as u32) * u64::from(rhs as u32);
        }
        let lhs = words(lhs);
        let rhs = words(rhs);
        pack_words(std::array::from_fn(|lane| match opcode {
            0xD5 => lhs[lane].wrapping_mul(rhs[lane]),
            0xE4 => ((u32::from(lhs[lane]) * u32::from(rhs[lane])) >> 16) as u16,
            0xE5 => {
                let product = i32::from(lhs[lane] as i16) * i32::from(rhs[lane] as i16);
                (product >> 16) as i16 as u16
            }
            _ => unreachable!(),
        }))
    }

    let lhs = pack_words([0xFFFF, 0x8000, 0x1234, 0x7FFF]);
    let rhs = pack_words([0x0002, 0xFFFF, 0xFEDC, 0x8000]);
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    for opcode in [0xF4, 0xD5, 0xE4, 0xE5] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = lhs;
            x86.mm[1] = rhs;
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 3 << 11;
        }
        execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                reference(opcode, lhs, rhs),
                "opcode={opcode:02X}"
            );
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 3 << 11);
        }
    }

    // Destructive register aliases snapshot every input lane before the
    // first architectural write.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = lhs;
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0xD5, 0xC0], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], reference(0xD5, lhs, lhs));
        assert_eq!(x86.x87.tag_word, 0);
    }

    // PMULUDQ fetches the complete unaligned m64 source even though only
    // its low doubleword participates in the single qword product.
    memory.write(0x181, &rhs.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x180);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = lhs;
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0xF4, 0x40, 0x01], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], reference(0xF4, lhs, rhs));
        assert_eq!(x86.x87.tag_word, 0);
    }

    // A source fault precedes both the destructive result and EnterMmx.
    ctx.write_vreg(rax, 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
        x86.x87.tag_word = 0xFFFF;
    }
    let fault = execute_lifted_x86(&[0x0F, 0xF4, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_mmx_average_and_pmaddwd_execute_rounding_wrap_aliases_and_faults() {
    fn pavg(opcode: u8, lhs: u64, rhs: u64) -> u64 {
        if opcode == 0xE0 {
            let lhs = lhs.to_le_bytes();
            let rhs = rhs.to_le_bytes();
            return u64::from_le_bytes(std::array::from_fn(|lane| {
                ((u16::from(lhs[lane]) + u16::from(rhs[lane]) + 1) >> 1) as u8
            }));
        }
        let lhs = lhs.to_le_bytes();
        let rhs = rhs.to_le_bytes();
        u64::from_le_bytes(
            (0..4)
                .flat_map(|lane| {
                    let at = lane * 2;
                    let a = u16::from_le_bytes(lhs[at..at + 2].try_into().unwrap());
                    let b = u16::from_le_bytes(rhs[at..at + 2].try_into().unwrap());
                    ((u32::from(a) + u32::from(b) + 1) >> 1)
                        .to_le_bytes()
                        .into_iter()
                        .take(2)
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        )
    }

    fn pmaddwd(lhs: u64, rhs: u64) -> u64 {
        let lhs = lhs.to_le_bytes();
        let rhs = rhs.to_le_bytes();
        u64::from_le_bytes(
            (0..2)
                .flat_map(|lane| {
                    let at = lane * 4;
                    let word = |bytes: &[u8], offset: usize| {
                        i32::from(i16::from_le_bytes(
                            bytes[offset..offset + 2].try_into().unwrap(),
                        ))
                    };
                    let sum = word(&lhs, at)
                        .wrapping_mul(word(&rhs, at))
                        .wrapping_add(word(&lhs, at + 2).wrapping_mul(word(&rhs, at + 2)));
                    sum.to_le_bytes()
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        )
    }

    let lhs = 0xFFFF_8000_0100_00FFu64;
    let rhs = 0x8000_FFFF_00FF_0002u64;
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    for opcode in [0xE0, 0xE3] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = lhs;
            x86.mm[1] = rhs;
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 5 << 11;
        }
        execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], pavg(opcode, lhs, rhs), "opcode={opcode:02X}");
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 5 << 11);
        }
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = lhs;
        x86.mm[1] = rhs;
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0xF5, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], pmaddwd(lhs, rhs));
        assert_eq!(x86.x87.tag_word, 0);
    }

    // The only overflowing PMADDWD input wraps each pairwise sum to
    // 0x8000_0000 rather than saturating.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0x8000_8000_8000_8000;
        x86.mm[1] = 0x8000_8000_8000_8000;
    }
    execute_lifted_x86(&[0x0F, 0xF5, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0x8000_0000_8000_0000);
    }

    // A destructive self-alias remains unchanged under rounded averaging.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = lhs;
    }
    execute_lifted_x86(&[0x0F, 0xE0, 0xC0], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], lhs);
    }

    memory.write(0x181, &rhs.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x180);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = lhs;
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0xE3, 0x40, 0x01], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], pavg(0xE3, lhs, rhs));
        assert_eq!(x86.x87.tag_word, 0);
    }

    ctx.write_vreg(rax, 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
        x86.x87.tag_word = 0xFFFF;
    }
    let fault = execute_lifted_x86(&[0x0F, 0xF5, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_original_packed_minmax_executes_values_masks_e4_and_faults() {
    fn seeded(input: &[u8], fill: u64) -> VecValue {
        let mut value = [fill; 16];
        for (index, byte) in input.iter().copied().enumerate() {
            let shift = (index % 8) * 8;
            value[index / 8] =
                (value[index / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
        }
        value
    }

    fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(len)
            .collect()
    }

    fn reference(opcode: u8, lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
        match opcode {
            0xDA | 0xDE => lhs
                .iter()
                .zip(rhs)
                .map(|(a, b)| {
                    if opcode == 0xDA {
                        (*a).min(*b)
                    } else {
                        (*a).max(*b)
                    }
                })
                .collect(),
            0xEA | 0xEE => lhs
                .chunks_exact(2)
                .zip(rhs.chunks_exact(2))
                .flat_map(|(a, b)| {
                    let a = i16::from_le_bytes(a.try_into().unwrap());
                    let b = i16::from_le_bytes(b.try_into().unwrap());
                    if opcode == 0xEA { a.min(b) } else { a.max(b) }.to_le_bytes()
                })
                .collect(),
            _ => unreachable!(),
        }
    }

    let lhs = (0..64)
        .map(|lane| [0x80, 0x7F, 0xFF, 0x00, 0x01, 0xFE, 0x40, 0xC0][lane % 8])
        .collect::<Vec<_>>();
    let rhs = (0..64)
        .map(|lane| [0x7F, 0x80, 0x00, 0xFF, 0xFE, 0x01, 0xC0, 0x40][lane % 8])
        .collect::<Vec<_>>();
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let sentinel = [0x6B6B_6B6B_6B6B_6B6Bu64; 16];
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    for opcode in [0xDA, 0xDE, 0xEA, 0xEE] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(lhs[..8].try_into().unwrap());
            x86.mm[1] = u64::from_le_bytes(rhs[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 3 << 11;
        }
        execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0],
                u64::from_le_bytes(reference(opcode, &lhs[..8], &rhs[..8]).try_into().unwrap())
            );
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 3 << 11);
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&lhs[..16], upper);
            x86.xmm[1] = seeded(&rhs[..16], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                reference(opcode, &lhs[..16], &rhs[..16])
            );
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[8] = sentinel;
            x86.xmm[9] = seeded(&lhs[..32], 0);
            x86.xmm[10] = seeded(&rhs[..32], 0);
        }
        execute_lifted_x86(&[0xC4, 0x41, 0x35, opcode, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[8], 32),
                reference(opcode, &lhs[..32], &rhs[..32])
            );
            assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
        }

        let elem_bytes = if opcode < 0xE0 { 1 } else { 2 };
        let raw = reference(opcode, &lhs, &rhs);
        let mask = 0xA55Au64;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[16] = sentinel;
            x86.xmm[17] = seeded(&lhs, 0);
            x86.xmm[18] = seeded(&rhs, 0);
        }
        ctx.write_vreg(k1, mask);
        execute_lifted_x86(
            &[0x62, 0xA1, 0x75, 0x41, opcode, 0xC2],
            &mut ctx,
            &mut memory,
        );
        let mut expected = vec![0x6B; 64];
        for lane in 0..64 / elem_bytes {
            if mask >> lane & 1 != 0 {
                let at = lane * elem_bytes;
                expected[at..at + elem_bytes].copy_from_slice(&raw[at..at + elem_bytes]);
            }
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[16], 64), expected);
            assert!(x86.xmm[16][8..].iter().all(|word| *word == 0));
        }
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes(lhs[..8].try_into().unwrap());
    }
    execute_lifted_x86(&[0x0F, 0xDA, 0xC0], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0],
            u64::from_le_bytes(reference(0xDA, &lhs[..8], &lhs[..8]).try_into().unwrap())
        );
    }

    memory.write(0x3FF, &rhs[..1]).unwrap();
    ctx.write_vreg(rax, 0x3FF);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.xmm[1] = seeded(&lhs, 0);
    }
    ctx.write_vreg(k1, 1);
    let lane0 = execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xDA, 0x00], &mut ctx, &mut memory);
    assert!(!matches!(
        lane0,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));

    ctx.write_vreg(k1, 2);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let lane1 = execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xDA, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        lane1,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    ctx.write_vreg(rax, 0x1000);
    ctx.write_vreg(k1, 0);
    let suppressed =
        execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xEA, 0x00], &mut ctx, &mut memory);
    assert!(!matches!(
        suppressed,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));

    memory.write(0x181, &rhs[..8]).unwrap();
    ctx.write_vreg(rax, 0x181);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes(lhs[..8].try_into().unwrap());
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0xEE, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0],
            u64::from_le_bytes(reference(0xEE, &lhs[..8], &rhs[..8]).try_into().unwrap())
        );
        assert_eq!(x86.x87.tag_word, 0);
    }

    ctx.write_vreg(rax, 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
        x86.x87.tag_word = 0xFFFF;
    }
    let mmx_fault = execute_lifted_x86(&[0x0F, 0xDA, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        mmx_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    memory.write(0x100, &rhs[..16]).unwrap();
    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0xEE, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_ptest_vptest_executes_flag_truth_table_widths_alignment_and_faults() {
    fn vec_from(input: &[u8], fill: u64) -> VecValue {
        let mut value = [fill; 16];
        for (index, byte) in input.iter().copied().enumerate() {
            let word = index / 8;
            let shift = (index % 8) * 8;
            value[word] = (value[word] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
        }
        value
    }

    fn expected_flags(before: u64, first: &[u8], second: &[u8]) -> u64 {
        assert_eq!(first.len(), second.len());
        let zf = first.iter().zip(second).all(|(a, b)| (*a & *b) == 0);
        let cf = first.iter().zip(second).all(|(a, b)| ((!*a) & *b) == 0);
        (before & !0x8D5) | u64::from(cf) | (u64::from(zf) << 6)
    }

    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);

    let truth_table = [
        ([0x00; 16], [0x00; 16]), // ZF=1, CF=1
        ([0xFF; 16], [0xFF; 16]), // ZF=0, CF=1
        ([0x00; 16], [0xFF; 16]), // ZF=1, CF=0
        ([0x0F; 16], [0xFF; 16]), // ZF=0, CF=0
    ];
    for (first, second) in truth_table {
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        let first_state = vec_from(&first, 0xA5A5_A5A5_A5A5_A5A5);
        let second_state = vec_from(&second, 0x5A5A_5A5A_5A5A_5A5A);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = first_state;
            x86.xmm[1] = second_state;
        }
        execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x17, 0xC1], &mut ctx, &mut memory);
        ctx.flags.materialize_all();
        assert_eq!(
            ctx.flags.materialized.to_rflags(),
            expected_flags(flags_before, &first, &second)
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0], first_state);
            assert_eq!(x86.xmm[1], second_state);
        }
    }

    // Low 128 bits satisfy both zero reductions. Only the upper 128 bits
    // make both reductions nonzero, distinguishing VPTEST.128 from .256.
    let mut first = [0xFFu8; 32];
    let mut second = [0u8; 32];
    first[16..].fill(0x0F);
    second[16..].fill(0xFF);
    let first_state = vec_from(&first, 0xA5A5_A5A5_A5A5_A5A5);
    let second_state = vec_from(&second, 0x5A5A_5A5A_5A5A_5A5A);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = first_state;
        x86.xmm[1] = second_state;
    }
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0xC4, 0xE2, 0x79, 0x17, 0xC1], &mut ctx, &mut memory);
    ctx.flags.materialize_all();
    assert_eq!(
        ctx.flags.materialized.to_rflags(),
        expected_flags(flags_before, &first[..16], &second[..16])
    );
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x17, 0xC1], &mut ctx, &mut memory);
    ctx.flags.materialize_all();
    assert_eq!(
        ctx.flags.materialized.to_rflags(),
        expected_flags(flags_before, &first, &second)
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], first_state);
        assert_eq!(x86.xmm[1], second_state);
    }

    memory.write(0x101, &second).unwrap();
    ctx.write_vreg(rax, 0x101);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x17, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x17, 0x00], &mut ctx, &mut memory);
    ctx.flags.materialize_all();
    assert_eq!(
        ctx.flags.materialized.to_rflags(),
        expected_flags(flags_before, &first, &second)
    );

    // A faulting source read cannot expose any part of the flag update.
    ctx.write_vreg(rax, 0x3F0);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    let fault = execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x17, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], first_state);
    }
}
#[test]
fn lifted_vperm2x128_executes_controls_aliases_memory_and_faults() {
    fn reference(src1: &[u64; 4], src2: &[u64; 4], imm: u8) -> [u64; 4] {
        let mut out = [0; 4];
        for (output_half, control_shift, zero_bit) in [(0usize, 0u8, 3u8), (1, 4, 7)] {
            if (imm >> zero_bit) & 1 != 0 {
                continue;
            }
            let control = (imm >> control_shift) & 3;
            let source = if control < 2 { src1 } else { src2 };
            let source_half = usize::from(control & 1);
            out[output_half * 2..output_half * 2 + 2]
                .copy_from_slice(&source[source_half * 2..source_half * 2 + 2]);
        }
        out
    }

    let src1 = [10, 11, 12, 13];
    let src2 = [20, 21, 22, 23];
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let flags_before = 0xCD7;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x100);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    for imm in [0x00, 0x31, 0x88, 0x82, 0xFF] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [upper; 16];
            x86.xmm[1][..4].copy_from_slice(&src1);
            x86.xmm[2][..4].copy_from_slice(&src2);
        }
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE3, 0x75, 0x06, 0xC2, imm], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(&x86.xmm[0][..4], &reference(&src1, &src2, imm));
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }
    }

    // Destination aliases SRC2; all selected halves must be captured
    // before the architectural YMM write clears upper state.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0][..4].copy_from_slice(&src2);
        x86.xmm[1][..4].copy_from_slice(&src1);
    }
    execute_lifted_x86(&[0xC4, 0xE3, 0x75, 0x06, 0xC0, 0x23], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[0][..4], &reference(&src1, &src2, 0x23));
    }

    let memory_source = [30u64, 31, 32, 33];
    let raw = memory_source
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect::<Vec<_>>();
    memory.write(0x21, &raw).unwrap();
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    ctx.write_vreg(rax, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[8] = [upper; 16];
        x86.xmm[9][..4].copy_from_slice(&src1);
    }
    execute_lifted_x86(
        &[0xC4, 0x63, 0x35, 0x46, 0x40, 0x20, 0x82],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[8][..4], &reference(&src1, &memory_source, 0x82));
    }

    ctx.write_vreg(rax, 0xF0);
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[8] = sentinel;
    }
    let fault = execute_lifted_x86(
        &[0xC4, 0x63, 0x35, 0x46, 0x40, 0x20, 0x82],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[8], sentinel);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_variable_blends_execute_mask_bits_aliases_widths_and_faults() {
    fn vec_from(input: &[u8], fill: u64) -> VecValue {
        let mut value = [fill; 16];
        for (index, byte) in input.iter().copied().enumerate() {
            let word = index / 8;
            let shift = (index % 8) * 8;
            value[word] = (value[word] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
        }
        value
    }

    fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(len)
            .collect()
    }

    fn mask_for(elem_bytes: usize, len: usize) -> Vec<u8> {
        let mut mask = vec![0x5A; len];
        for (lane, chunk) in mask.chunks_exact_mut(elem_bytes).enumerate() {
            chunk[elem_bytes - 1] = if lane % 3 == 1 { 0x80 } else { 0x7F };
        }
        mask
    }

    fn reference(src1: &[u8], src2: &[u8], mask: &[u8], elem_bytes: usize) -> Vec<u8> {
        assert_eq!(src1.len(), src2.len());
        assert_eq!(src1.len(), mask.len());
        src1.chunks_exact(elem_bytes)
            .zip(src2.chunks_exact(elem_bytes))
            .zip(mask.chunks_exact(elem_bytes))
            .flat_map(|((a, b), m)| {
                if m[elem_bytes - 1] & 0x80 != 0 {
                    b.to_vec()
                } else {
                    a.to_vec()
                }
            })
            .collect()
    }

    let src1 = (0..32)
        .map(|index| (index * 29 + 0x13) as u8)
        .collect::<Vec<_>>();
    let src2 = (0..32)
        .map(|index| (0xF1u8).wrapping_sub((index * 17) as u8))
        .collect::<Vec<_>>();
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    for (legacy_opcode, vex_opcode, elem_bytes) in
        [(0x10, 0x4C, 1usize), (0x14, 0x4A, 4), (0x15, 0x4B, 8)]
    {
        let mask128 = mask_for(elem_bytes, 16);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = vec_from(&mask128, 0);
            x86.xmm[1] = vec_from(&src2[..16], 0);
            x86.xmm[2] = vec_from(&src1[..16], upper);
        }
        execute_lifted_x86(
            &[0x66, 0x0F, 0x38, legacy_opcode, 0xD1],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[2], 16),
                reference(&src1[..16], &src2[..16], &mask128, elem_bytes),
                "legacy opcode {legacy_opcode:02X}"
            );
            assert!(x86.xmm[2][2..].iter().all(|word| *word == upper));
        }

        for (p2, width) in [(0x61, 16usize), (0x65, 32)] {
            let mask = mask_for(elem_bytes, width);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.xmm[1] = sentinel;
                x86.xmm[2] = vec_from(&src2[..width], 0);
                x86.xmm[3] = vec_from(&src1[..width], 0);
                x86.xmm[4] = vec_from(&mask, 0);
            }
            execute_lifted_x86(
                &[0xC4, 0xE3, p2, vex_opcode, 0xCA, 0x40],
                &mut ctx,
                &mut memory,
            );
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                assert_eq!(
                    bytes(&x86.xmm[1], width),
                    reference(&src1[..width], &src2[..width], &mask, elem_bytes),
                    "VEX opcode {vex_opcode:02X} width {width}"
                );
                assert!(x86.xmm[1][width / 8..].iter().all(|word| *word == 0));
            }
        }
    }

    // Legacy destination=XMM0 aliases the implicit mask. Its original bits
    // are both source 1 data and the lane-selection mask.
    let alias_mask = mask_for(1, 16);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = vec_from(&alias_mask, upper);
        x86.xmm[1] = vec_from(&src2[..16], 0);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x10, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 16),
            reference(&alias_mask, &src2[..16], &alias_mask, 1)
        );
        assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
    }

    // VEX destination aliases the explicit mask register encoded by /is4.
    let explicit_mask = mask_for(1, 32);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = vec_from(&explicit_mask, 0);
        x86.xmm[2] = vec_from(&src2, 0);
        x86.xmm[3] = vec_from(&src1, 0);
    }
    execute_lifted_x86(&[0xC4, 0xE3, 0x65, 0x4C, 0xCA, 0x10], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[1], 32),
            reference(&src1, &src2, &explicit_mask, 1)
        );
        assert!(x86.xmm[1][4..].iter().all(|word| *word == 0));
    }

    memory.write(0x101, &src2).unwrap();
    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = vec_from(&mask_for(1, 16), 0);
        x86.xmm[2] = sentinel;
    }
    let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x10, 0x10], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], sentinel);
    }

    let mask256 = mask_for(4, 32);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
        x86.xmm[3] = vec_from(&src1, 0);
        x86.xmm[4] = vec_from(&mask256, 0);
    }
    execute_lifted_x86(&[0xC4, 0xE3, 0x65, 0x4A, 0x10, 0x40], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[2], 32), reference(&src1, &src2, &mask256, 4));
    }

    ctx.write_vreg(rax, 0x3F0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
    }
    let fault = execute_lifted_x86(&[0xC4, 0xE3, 0x65, 0x4A, 0x10, 0x40], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], sentinel);
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_pmuldq_executes_even_signed_lanes_masks_aliases_and_faults() {
    fn packed(values: &[i32], fill: u64) -> VecValue {
        let bytes = values
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        let mut out = [fill; 16];
        for (i, byte) in bytes.into_iter().enumerate() {
            let shift = (i % 8) * 8;
            out[i / 8] = (out[i / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
        }
        out
    }
    fn products(a: &[i32], b: &[i32]) -> Vec<u8> {
        a.iter()
            .step_by(2)
            .zip(b.iter().step_by(2))
            .flat_map(|(x, y)| (i64::from(*x) * i64::from(*y)).to_le_bytes())
            .collect()
    }
    fn bytes(v: &VecValue, len: usize) -> Vec<u8> {
        v.iter().flat_map(|w| w.to_le_bytes()).take(len).collect()
    }

    let a = [
        -1,
        0x1111,
        2,
        0x2222,
        i32::MIN,
        7,
        i32::MAX,
        -9,
        -3,
        4,
        5,
        6,
        -7,
        8,
        9,
        10,
    ];
    let b = [
        3,
        -1,
        i32::MAX,
        2,
        -1,
        5,
        2,
        6,
        -11,
        12,
        -13,
        14,
        15,
        -16,
        -17,
        18,
    ];
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = packed(&a[..4], upper);
        x86.xmm[1] = packed(&b[..4], 0);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x28, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 16), products(&a[..4], &b[..4]));
        assert!(x86.xmm[0][2..].iter().all(|w| *w == upper));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.xmm[1] = packed(&a[..8], 0);
        x86.xmm[2] = packed(&b[..8], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x75, 0x28, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 32), products(&a[..8], &b[..8]));
        assert!(x86.xmm[0][4..].iter().all(|w| *w == 0));
    }

    let raw = products(&a, &b);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
        x86.xmm[1] = packed(&a, 0);
        x86.xmm[2] = packed(&b, 0);
    }
    ctx.write_vreg(k1, 0x55);
    execute_lifted_x86(&[0x62, 0xF2, 0xF5, 0x49, 0x28, 0xC2], &mut ctx, &mut memory);
    let mut expected = vec![0x6B; 64];
    for lane in 0..8 {
        if (0x55 >> lane) & 1 != 0 {
            expected[lane * 8..lane * 8 + 8].copy_from_slice(&raw[lane * 8..lane * 8 + 8]);
        }
    }
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 64), expected);
    }

    // Same-register VEX source/destination must be captured before zeroing.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = packed(&a[..8], 0);
        x86.xmm[2] = packed(&b[..8], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x28, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[0], 32), products(&a[..8], &b[..8]));
    }

    memory
        .write(
            0x3F8,
            &b[..2]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    ctx.write_vreg(rax, 0x3F8);
    ctx.write_vreg(k1, 1);
    let ok = execute_lifted_x86(&[0x62, 0xF2, 0xF5, 0x49, 0x28, 0x00], &mut ctx, &mut memory);
    assert!(!matches!(
        ok,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    ctx.write_vreg(k1, 2);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let fault = execute_lifted_x86(&[0x62, 0xF2, 0xF5, 0x49, 0x28, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_pmuludq_executes_even_unsigned_lanes_widths_and_masks() {
    fn packed(values: &[u32], fill: u64) -> VecValue {
        let raw = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let mut out = [fill; 16];
        for (index, byte) in raw.into_iter().enumerate() {
            let shift = (index % 8) * 8;
            out[index / 8] = (out[index / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
        }
        out
    }

    fn products(a: &[u32], b: &[u32]) -> Vec<u8> {
        a.iter()
            .step_by(2)
            .zip(b.iter().step_by(2))
            .flat_map(|(x, y)| (u64::from(*x) * u64::from(*y)).to_le_bytes())
            .collect()
    }

    fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(len)
            .collect()
    }

    let a = [
        u32::MAX,
        1,
        0x8000_0000,
        2,
        3,
        4,
        0xFFFF_0001,
        5,
        7,
        6,
        11,
        8,
        13,
        9,
        17,
        10,
    ];
    let b = [
        u32::MAX,
        10,
        2,
        11,
        0xF000_0000,
        12,
        0x8000_0001,
        13,
        19,
        14,
        23,
        15,
        29,
        16,
        31,
        17,
    ];
    let flags_before = 0xCD7;
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(1);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = packed(&a[..4], upper);
        x86.xmm[1] = packed(&b[..4], 0);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0xF4, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[2], 16), products(&a[..4], &b[..4]));
        assert!(x86.xmm[2][2..].iter().all(|word| *word == upper));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[8] = [upper; 16];
        x86.xmm[9] = packed(&a[..8], 0);
        x86.xmm[10] = packed(&b[..8], 0);
    }
    execute_lifted_x86(&[0xC4, 0x41, 0x35, 0xF4, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[8], 32), products(&a[..8], &b[..8]));
        assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[16] = [0x6B6B_6B6B_6B6B_6B6B; 16];
        x86.xmm[17] = packed(&a, 0);
        x86.xmm[18] = packed(&b, 0);
        x86.k[1] = 0x55;
    }
    execute_lifted_x86(&[0x62, 0xA1, 0xF5, 0x41, 0xF4, 0xC2], &mut ctx, &mut memory);
    let raw = products(&a, &b);
    let mut expected = vec![0x6B; 64];
    for lane in 0..8 {
        if (0x55 >> lane) & 1 != 0 {
            expected[lane * 8..lane * 8 + 8].copy_from_slice(&raw[lane * 8..lane * 8 + 8]);
        }
    }
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(bytes(&x86.xmm[16], 64), expected);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_pmulld_pmulq_execute_widths_masks_broadcasts_and_fault_suppression() {
    fn packed32(values: &[u32], fill: u64) -> VecValue {
        let raw = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let mut out = [fill; 16];
        for (index, byte) in raw.into_iter().enumerate() {
            let shift = (index % 8) * 8;
            out[index / 8] = (out[index / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
        }
        out
    }

    fn lanes32(value: &VecValue, count: usize) -> Vec<u32> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(count * 4)
            .collect::<Vec<_>>()
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    let a32 = (0..16)
        .map(|lane| 0x8000_0001u32.wrapping_add(lane * 0x1111_1111))
        .collect::<Vec<_>>();
    let b32 = (0..16)
        .map(|lane| 0xFFFF_0001u32.wrapping_sub(lane * 0x0101_0101))
        .collect::<Vec<_>>();
    let a64 = [u64::MAX, 0x8000_0000_0000_0001, 3, 5, 7, 11, 13, 17];
    let b64 = [19, 23, u64::MAX, 29, 31, 37, 41, 43];
    let flags_before = 0xCD7;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x100);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[17] = packed32(&a32, 0);
        x86.xmm[18] = packed32(&b32, 0);
    }
    execute_lifted_x86(&[0x62, 0xA2, 0x75, 0x40, 0x40, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            lanes32(&x86.xmm[16], 16),
            a32.iter()
                .zip(&b32)
                .map(|(a, b)| a.wrapping_mul(*b))
                .collect::<Vec<_>>(),
        );
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[20][..8].copy_from_slice(&a64);
        x86.xmm[21][..8].copy_from_slice(&b64);
    }
    execute_lifted_x86(&[0x62, 0xA2, 0xDD, 0x40, 0x40, 0xDD], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            &x86.xmm[19][..8],
            &a64.iter()
                .zip(&b64)
                .map(|(a, b)| a.wrapping_mul(*b))
                .collect::<Vec<_>>(),
        );
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    memory.write(0xFC, &7u32.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0xF8);
    ctx.write_vreg(k1, 0xA55A);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [0x6B6B_6B6B_6B6B_6B6B; 16];
        x86.xmm[1] = packed32(&a32, 0);
    }
    execute_lifted_x86(
        &[0x62, 0xF2, 0x75, 0xD9, 0x40, 0x40, 0x01],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let actual = lanes32(&x86.xmm[0], 16);
        for lane in 0..16 {
            assert_eq!(
                actual[lane],
                if (0xA55A >> lane) & 1 != 0 {
                    a32[lane].wrapping_mul(7)
                } else {
                    0
                },
            );
        }
    }

    ctx.write_vreg(rax, 0x100);
    ctx.write_vreg(k1, 0);
    assert!(matches!(
        execute_lifted_x86(
            &[0x62, 0xF2, 0x75, 0xD9, 0x40, 0x40, 0x01],
            &mut ctx,
            &mut memory,
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    ctx.write_vreg(k1, 1);
    let fault = execute_lifted_x86(
        &[0x62, 0xF2, 0x75, 0xD9, 0x40, 0x40, 0x01],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_pmullw_executes_widths_masks_alignment_and_fault_suppression() {
    fn packed(values: &[u16], fill: u64) -> VecValue {
        let raw = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let mut out = [fill; 16];
        for (index, byte) in raw.into_iter().enumerate() {
            let shift = (index % 8) * 8;
            out[index / 8] = (out[index / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
        }
        out
    }

    fn lanes(value: &VecValue, count: usize) -> Vec<u16> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(count * 2)
            .collect::<Vec<_>>()
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    let a = (0..32)
        .map(|lane| 0x8001u16.wrapping_add((lane as u16).wrapping_mul(0x1111)))
        .collect::<Vec<_>>();
    let b = (0..32)
        .map(|lane| 0xFFF1u16.wrapping_sub((lane as u16).wrapping_mul(0x0101)))
        .collect::<Vec<_>>();
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let flags_before = 0xCD7;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x100);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = packed(&a[..8], upper);
        x86.xmm[1] = packed(&b[..8], 0);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0xD5, 0xD1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            lanes(&x86.xmm[2], 8),
            a[..8]
                .iter()
                .zip(&b[..8])
                .map(|(a, b)| a.wrapping_mul(*b))
                .collect::<Vec<_>>(),
        );
        assert!(x86.xmm[2][2..].iter().all(|word| *word == upper));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[8] = [upper; 16];
        x86.xmm[9] = packed(&a[..16], 0);
        x86.xmm[10] = packed(&b[..16], 0);
    }
    execute_lifted_x86(&[0xC4, 0x41, 0x35, 0xD5, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            lanes(&x86.xmm[8], 16),
            a[..16]
                .iter()
                .zip(&b[..16])
                .map(|(a, b)| a.wrapping_mul(*b))
                .collect::<Vec<_>>(),
        );
        assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[16] = [upper; 16];
        x86.xmm[17] = packed(&a, 0);
        x86.xmm[18] = packed(&b, 0);
        x86.k[1] = 0xA5A5_5A5A;
    }
    execute_lifted_x86(&[0x62, 0xA1, 0x75, 0x41, 0xD5, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let actual = lanes(&x86.xmm[16], 32);
        for lane in 0..32 {
            assert_eq!(
                actual[lane],
                if (0xA5A5_5A5Au64 >> lane) & 1 != 0 {
                    a[lane].wrapping_mul(b[lane])
                } else {
                    0xA5A5
                },
            );
        }
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let memory_words = (0..8).map(|lane| lane as u16 + 3).collect::<Vec<_>>();
    let raw = memory_words
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect::<Vec<_>>();
    memory.write(0xF0, &raw).unwrap();
    ctx.write_vreg(rax, 0xF0);
    ctx.write_vreg(k1, 0xFF);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [0; 16];
        x86.xmm[1] = packed(&a, 0);
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xD5, 0x00], &mut ctx, &mut memory,),
        BlockResult::Exit(ExitReason::Halt)
    ));
    ctx.write_vreg(k1, 1 << 8);
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let fault = execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xD5, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }

    // Legacy SSE requires 16-byte alignment before its source load.
    ctx.write_vreg(rax, 0x71);
    let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0xD5, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_pmulhw_pmulhuw_execute_signedness_masks_alignment_and_faults() {
    fn packed(values: &[u16], fill: u64) -> VecValue {
        let raw = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let mut out = [fill; 16];
        for (index, byte) in raw.into_iter().enumerate() {
            let shift = (index % 8) * 8;
            out[index / 8] = (out[index / 8] & !(0xFFu64 << shift)) | (u64::from(byte) << shift);
        }
        out
    }

    fn lanes(value: &VecValue, count: usize) -> Vec<u16> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(count * 2)
            .collect::<Vec<_>>()
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    fn signed_high(a: u16, b: u16) -> u16 {
        let product = i32::from(a as i16).wrapping_mul(i32::from(b as i16));
        ((product as u32) >> 16) as u16
    }

    fn unsigned_high(a: u16, b: u16) -> u16 {
        ((u32::from(a) * u32::from(b)) >> 16) as u16
    }

    let a = (0..32)
        .map(|lane| 0x8001u16.wrapping_add((lane as u16).wrapping_mul(0x1111)))
        .collect::<Vec<_>>();
    let b = (0..32)
        .map(|lane| 0xFFF1u16.wrapping_sub((lane as u16).wrapping_mul(0x0101)))
        .collect::<Vec<_>>();
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let flags_before = 0xCD7;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x100);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    for (opcode, reference) in [
        (0xE5u8, signed_high as fn(u16, u16) -> u16),
        (0xE4, unsigned_high as fn(u16, u16) -> u16),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = packed(&a[..8], upper);
            x86.xmm[1] = packed(&b[..8], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, opcode, 0xD1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                lanes(&x86.xmm[2], 8),
                a[..8]
                    .iter()
                    .zip(&b[..8])
                    .map(|(a, b)| reference(*a, *b))
                    .collect::<Vec<_>>(),
            );
            assert!(x86.xmm[2][2..].iter().all(|word| *word == upper));
        }
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[8] = [upper; 16];
        x86.xmm[9] = packed(&a[..16], 0);
        x86.xmm[10] = packed(&b[..16], 0);
    }
    execute_lifted_x86(&[0xC4, 0x41, 0x35, 0xE5, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            lanes(&x86.xmm[8], 16),
            a[..16]
                .iter()
                .zip(&b[..16])
                .map(|(a, b)| signed_high(*a, *b))
                .collect::<Vec<_>>(),
        );
        assert!(x86.xmm[8][4..].iter().all(|word| *word == 0));
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[16] = [upper; 16];
        x86.xmm[17] = packed(&a, 0);
        x86.xmm[18] = packed(&b, 0);
        x86.k[1] = 0xA5A5_5A5A;
    }
    execute_lifted_x86(&[0x62, 0xA1, 0x75, 0x41, 0xE4, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let actual = lanes(&x86.xmm[16], 32);
        for lane in 0..32 {
            assert_eq!(
                actual[lane],
                if (0xA5A5_5A5Au64 >> lane) & 1 != 0 {
                    unsigned_high(a[lane], b[lane])
                } else {
                    0xA5A5
                },
            );
        }
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let memory_words = (0..8).map(|lane| lane as u16 + 3).collect::<Vec<_>>();
    let raw = memory_words
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect::<Vec<_>>();
    memory.write(0xF0, &raw).unwrap();
    ctx.write_vreg(rax, 0xF0);
    ctx.write_vreg(k1, 0xFF);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [0; 16];
        x86.xmm[1] = packed(&a, 0);
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xE5, 0x00], &mut ctx, &mut memory,),
        BlockResult::Exit(ExitReason::Halt)
    ));
    ctx.write_vreg(k1, 1 << 8);
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    let fault = execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x49, 0xE5, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }
    ctx.write_vreg(rax, 0xF1);
    let misaligned = execute_lifted_x86(&[0x66, 0x0F, 0xE4, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn vfma_executes_fused_f32_f64_and_sign_controls() {
    let regs = [
        X86Reg::Xmm(0),
        X86Reg::Xmm(1),
        X86Reg::Xmm(2),
        X86Reg::Xmm(3),
    ]
    .map(|reg| VReg::Arch(ArchReg::X86(reg)));
    for (elem, values, expected) in [
        (
            VecElementType::F32,
            [
                f32::to_bits(1.5) as u64,
                f32::to_bits(4.0) as u64,
                f32::to_bits(2.0) as u64,
            ],
            f32::to_bits(-8.0) as u64,
        ),
        (
            VecElementType::F64,
            [f64::to_bits(1.5), f64::to_bits(4.0), f64::to_bits(2.0)],
            f64::to_bits(-8.0),
        ),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0][0] = values[0];
            x86.xmm[1][0] = values[1];
            x86.xmm[2][0] = values[2];
        }
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::VFma {
                dst: regs[3],
                src1: regs[0],
                src2: regs[1],
                acc: regs[2],
                elem,
                lanes: 1,
                negate_product: true,
                negate_acc: true,
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let func = builder.finish();
        let mut memory = FlatMemory::new(0x100);
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut ctx, &mut memory, &func.blocks[0]),
            BlockResult::Exit(ExitReason::Halt)
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[3][0], expected);
            assert!(x86.xmm[3][1..].iter().all(|word| *word == 0));
        }
    }
}
#[test]
fn reciprocal_estimates_execute_special_cases_and_accuracy_bound() {
    fn packed_f32(bits: &[u32]) -> VecValue {
        let mut out = [0; 16];
        for (lane, value) in bits.iter().copied().enumerate() {
            SmirInterpreter::set_lane(&mut out, lane as u8, 32, u64::from(value));
        }
        out
    }

    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = packed_f32(&[0, 0x8000_0000, 1, 0x8000_0001]);
        x86.xmm[1] = packed_f32(&[
            f32::INFINITY.to_bits(),
            f32::NEG_INFINITY.to_bits(),
            0x7FA1_2345,
            0xFFC5_4321,
        ]);
        x86.xmm[2] = packed_f32(&[
            7.0f32.to_bits(),
            (-11.0f32).to_bits(),
            f32::MAX.to_bits(),
            f32::MIN_POSITIVE.to_bits(),
        ]);
        x86.xmm[3] = packed_f32(&[
            4.0f32.to_bits(),
            (-4.0f32).to_bits(),
            f32::INFINITY.to_bits(),
            f32::NEG_INFINITY.to_bits(),
        ]);
    }
    let regs = (0..8)
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index))))
        .collect::<Vec<_>>();
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (dst, src, op) in [
        (4usize, 0usize, VecUnaryOp::FRecipEstimate),
        (5, 1, VecUnaryOp::FRecipEstimate),
        (6, 2, VecUnaryOp::FRecipEstimate),
        (7, 3, VecUnaryOp::FRsqrtEstimate),
    ] {
        builder.push_op(
            0x1000,
            OpKind::VUnary {
                dst: regs[dst],
                src: regs[src],
                elem: VecElementType::F32,
                lanes: 4,
                op,
            },
        );
    }
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let func = builder.finish();
    let mut memory = FlatMemory::new(0x100);
    assert!(matches!(
        SmirInterpreter::new().execute_block(&mut ctx, &mut memory, &func.blocks[0]),
        BlockResult::Exit(ExitReason::Halt)
    ));

    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            (0..4)
                .map(|lane| SmirInterpreter::get_lane(&x86.xmm[4], lane, 32) as u32)
                .collect::<Vec<_>>(),
            [
                f32::INFINITY.to_bits(),
                f32::NEG_INFINITY.to_bits(),
                f32::INFINITY.to_bits(),
                f32::NEG_INFINITY.to_bits(),
            ]
        );
        assert_eq!(
            (0..4)
                .map(|lane| SmirInterpreter::get_lane(&x86.xmm[5], lane, 32) as u32)
                .collect::<Vec<_>>(),
            [0, 0x8000_0000, 0x7FE1_2345, 0xFFC5_4321]
        );
        // Exact binary32 evaluation is a valid deterministic member of the
        // architectural estimate set. Verify the architectural error bound
        // independently in binary64 rather than requiring a hardware bit pattern.
        for (lane, input) in [(0u8, 7.0f64), (1, -11.0)] {
            let actual = f64::from(f32::from_bits(
                SmirInterpreter::get_lane(&x86.xmm[6], lane, 32) as u32,
            ));
            let exact = 1.0f64 / input;
            let relative_error = ((actual - exact) / exact).abs();
            assert!(relative_error <= 1.5 * 2.0f64.powi(-12));
        }
        assert_eq!(SmirInterpreter::get_lane(&x86.xmm[6], 2, 32), 0);
        assert_eq!(
            SmirInterpreter::get_lane(&x86.xmm[6], 3, 32),
            u64::from((1.0f32 / f32::MIN_POSITIVE).to_bits())
        );
        assert_eq!(
            (0..4)
                .map(|lane| SmirInterpreter::get_lane(&x86.xmm[7], lane, 32) as u32)
                .collect::<Vec<_>>(),
            [0.5f32.to_bits(), 0xFFC0_0000, 0, 0xFFC0_0000]
        );
    }
}
#[test]
fn lifted_vex_fma3_executes_orders_sign_families_alternation_scalar_and_faults() {
    fn packed_f32(values: &[f32], fill: u64) -> VecValue {
        let mut out = [fill; 16];
        for (lane, value) in values.iter().copied().enumerate() {
            SmirInterpreter::set_lane(&mut out, lane as u8, 32, u64::from(value.to_bits()));
        }
        out
    }

    let old = [1.5, -2.0, 3.25, -4.5, 5.0, -6.25, 7.5, -8.0];
    let vex = [0.5, 1.25, -1.5, 2.0, -2.5, 3.0, -3.5, 4.0];
    let rm = [2.0, -3.0, 4.0, -5.0, 6.0, -7.0, 8.0, -9.0];
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x100);
    for opcode in [
        0x96u8, 0x97, 0x98, 0x9A, 0x9C, 0x9E, 0xA6, 0xA7, 0xA8, 0xAA, 0xAC, 0xAE, 0xB6, 0xB7, 0xB8,
        0xBA, 0xBC, 0xBE,
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = packed_f32(&old, 0xA5A5_A5A5_A5A5_A5A5);
            x86.xmm[1] = packed_f32(&vex, 0);
            x86.xmm[3] = packed_f32(&rm, 0);
        }
        execute_lifted_x86(&[0xC4, 0xE2, 0x75, opcode, 0xD3], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..8u8 {
                let (a, b, c) = match opcode >> 4 {
                    9 => (old[lane as usize], rm[lane as usize], vex[lane as usize]),
                    10 => (vex[lane as usize], old[lane as usize], rm[lane as usize]),
                    11 => (vex[lane as usize], rm[lane as usize], old[lane as usize]),
                    _ => unreachable!(),
                };
                let low = opcode & 0xF;
                let alternating = matches!(low, 6 | 7);
                let negate_product = matches!(low, 0xC | 0xE);
                let negate_acc = if alternating {
                    (lane & 1 == 0) == (low == 6)
                } else {
                    matches!(low, 0xA | 0xE)
                };
                let expected = (if negate_product { -a } else { a })
                    .mul_add(b, if negate_acc { -c } else { c });
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                    u64::from(expected.to_bits()),
                    "opcode {opcode:02X}, lane {lane}",
                );
            }
            assert!(x86.xmm[2][4..].iter().all(|word| *word == 0));
        }
    }

    // Scalar FMA replaces only lane zero, preserves the old destination's
    // remaining XMM lanes, and clears state above bit 127.
    let upper = [9.0f32, -10.0, 11.0];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = packed_f32(&[2.0, upper[0], upper[1], upper[2]], u64::MAX);
        x86.xmm[1] = packed_f32(&[3.0], 0);
        x86.xmm[3] = packed_f32(&[5.0], 0);
    }
    execute_lifted_x86(&[0xC4, 0xE2, 0x71, 0xB9, 0xD3], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            SmirInterpreter::get_lane(&x86.xmm[2], 0, 32),
            17.0f32.to_bits() as u64
        );
        for lane in 1..4u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                u64::from(upper[(lane - 1) as usize].to_bits())
            );
        }
        assert!(x86.xmm[2][2..].iter().all(|word| *word == 0));
    }

    // A faulting source load occurs before any destination commit.
    let rdi = VReg::Arch(ArchReg::X86(X86Reg::Rdi));
    ctx.write_vreg(rdi, 0x1000);
    let sentinel = [0x6B6B_6B6B_6B6B_6B6B; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
    }
    assert!(matches!(
        execute_lifted_x86(&[0xC4, 0xE2, 0x71, 0x99, 0x17], &mut ctx, &mut memory,),
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], sentinel);
    }

    // EVEX packed masking merges or zeroes per lane after the fused result.
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let evex_mask = 0xA55Au64;
    ctx.write_vreg(k1, evex_mask);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = packed_f32(&old, 0);
        x86.xmm[1] = packed_f32(&vex, 0);
        x86.xmm[3] = packed_f32(&rm, 0);
    }
    execute_lifted_x86(&[0x62, 0xF2, 0x75, 0x49, 0x98, 0xD3], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            let expected = if lane < 8 && evex_mask >> lane & 1 != 0 {
                old[lane as usize].mul_add(rm[lane as usize], vex[lane as usize])
            } else if lane < 8 {
                old[lane as usize]
            } else {
                0.0
            };
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                u64::from(expected.to_bits())
            );
        }
    }

    // A zero EVEX mask suppresses a broadcast source fault and zeroing
    // clears every destination lane. Activating one lane exposes the fault
    // without committing the destination.
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    ctx.write_vreg(rax, 0x1000);
    ctx.write_vreg(k1, 0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xF2, 0x75, 0xD9, 0x98, 0x10], &mut ctx, &mut memory,),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert!(x86.xmm[2].iter().all(|word| *word == 0));
    }
    ctx.write_vreg(k1, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xF2, 0x75, 0xD9, 0x98, 0x10], &mut ctx, &mut memory,),
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], sentinel);
    }
}
