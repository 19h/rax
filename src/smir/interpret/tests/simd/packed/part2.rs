//! packed part 2 tests

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn executes_vpshufbitqmb_lane_domains_opmask_zeroing_memory_and_faults() {
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    let mut data = [0u64; 16];
    let mut controls = [0u64; 16];
    for qword in 0..8u8 {
        SmirInterpreter::set_lane(
            &mut data,
            qword,
            64,
            0x8040_2010_0804_0201u64.rotate_left(u32::from(qword)),
        );
        for byte in 0..8u8 {
            SmirInterpreter::set_lane(
                &mut controls,
                qword * 8 + byte,
                8,
                u64::from((byte * 9 + qword) & 0x3F) | 0xC0,
            );
        }
    }
    let reference = |width: VecWidth, mask: u64| -> u64 {
        let mut result = 0u64;
        for qword in 0..(width.bytes() / 8) as u8 {
            let source = SmirInterpreter::get_lane(&data, qword, 64);
            for byte in 0..8u8 {
                let output = qword * 8 + byte;
                if mask & (1u64 << output) != 0 {
                    let control = SmirInterpreter::get_lane(&controls, output, 8) & 0x3F;
                    result |= ((source >> control) & 1) << output;
                }
            }
        }
        result
    };

    for (bytes, width, dst, src, indices, mask) in [
        (
            &[0x62, 0xF2, 0x6D, 0x08, 0x8F, 0xCB][..],
            VecWidth::V128,
            1usize,
            2usize,
            3usize,
            u64::MAX,
        ),
        (
            &[0x62, 0xF2, 0x55, 0x2B, 0x8F, 0xE6][..],
            VecWidth::V256,
            4,
            5,
            6,
            0xA5A5_5A5A,
        ),
        (
            &[0x62, 0xB2, 0x6D, 0x42, 0x8F, 0xFB][..],
            VecWidth::V512,
            7,
            18,
            19,
            0xF0F0_0F0F_AA55_55AA,
        ),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[src] = data;
            x86.xmm[indices] = controls;
            x86.k[dst] = u64::MAX;
            x86.k[2] = if dst == 7 { mask } else { x86.k[2] };
            x86.k[3] = if dst == 4 { mask } else { x86.k[3] };
        }
        execute_lifted_x86(bytes, &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.k[dst], reference(width, mask));
        }
    }

    // A sparse E4 mask may read the last mapped byte without touching any
    // masked-off byte beyond the memory boundary.
    memory.write(0x3FF, &[5]).unwrap();
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3FF);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = [0; 16];
        SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 64, 1 << 5);
        x86.k[1] = 0xDEAD_BEEF_DEAD_BEEF;
        x86.k[3] = 1;
    }
    let sparse = execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x0B, 0x8F, 0x08], &mut ctx, &mut memory);
    assert!(matches!(sparse, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.k[1], 1);
    }

    // Activating the next byte faults before the K destination is changed.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.k[1] = 0xDEAD_BEEF_DEAD_BEEF;
        x86.k[3] = 2;
    }
    let fault = execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x0B, 0x8F, 0x08], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.k[1], 0xDEAD_BEEF_DEAD_BEEF);
    }
}
#[test]
fn executes_packed_variable_shifts_boundaries_signedness_masks_and_aliases() {
    let cases = [
        (
            &[0x62, 0xF2, 0xED, 0x08, 0x10, 0xCB][..],
            16u32,
            ShiftOp::Lsr,
        ),
        (&[0x62, 0xF2, 0x6D, 0x08, 0x46, 0xCB][..], 32, ShiftOp::Asr),
        (&[0x62, 0xF2, 0xED, 0x08, 0x47, 0xCB][..], 64, ShiftOp::Lsl),
    ];
    let mut memory = FlatMemory::new(0x100);
    for (bytes, bits, shift) in cases {
        let mut ctx = SmirContext::new_x86_64();
        let lanes = 128 / bits;
        let counts = [
            0u64,
            u64::from(bits - 1),
            u64::from(bits),
            u64::from(bits + 1),
        ];
        let mask = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..lanes as u8 {
                SmirInterpreter::set_lane(&mut x86.xmm[2], lane, bits, mask ^ u64::from(lane));
                SmirInterpreter::set_lane(
                    &mut x86.xmm[3],
                    lane,
                    bits,
                    counts[usize::from(lane) & 3],
                );
            }
        }
        execute_lifted_x86(bytes, &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            for lane in 0..lanes as u8 {
                let value = mask ^ u64::from(lane);
                let amount = counts[usize::from(lane) & 3];
                let expected = if amount >= u64::from(bits) {
                    if shift == ShiftOp::Asr && value & (1u64 << (bits - 1)) != 0 {
                        mask
                    } else {
                        0
                    }
                } else {
                    match shift {
                        ShiftOp::Lsr => value >> amount,
                        ShiftOp::Lsl => (value << amount) & mask,
                        ShiftOp::Asr => {
                            let signed = if bits == 64 {
                                value as i64
                            } else {
                                ((value << (64 - bits)) as i64) >> (64 - bits)
                            };
                            ((signed >> amount) as u64) & mask
                        }
                        _ => unreachable!(),
                    }
                };
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, bits), expected);
            }
            assert_eq!(&x86.xmm[1][2..], &[0; 14]);
        }
    }
}
#[test]
fn executes_load_broadcasts_tuple_order_masks_gpr_aliases_and_e6_fault_suppression() {
    let mut memory = FlatMemory::new(0x400);
    let flags_before = 0xCD7;

    // Register tuple broadcast repeats lanes 0,1 and preserves inactive
    // destination lanes under merging masking.
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    let control = 0xA55Au64;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 32, 0x1122_3344);
        SmirInterpreter::set_lane(&mut x86.xmm[2], 1, 32, 0xAABB_CCDD);
        for lane in 0..16u8 {
            SmirInterpreter::set_lane(&mut x86.xmm[1], lane, 32, 0xDEAD_0000 + u64::from(lane));
        }
        x86.k[1] = control;
    }
    execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x49, 0x19, 0xCA], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            let expected = if control & (1u64 << lane) != 0 {
                if lane & 1 == 0 {
                    0x1122_3344
                } else {
                    0xAABB_CCDD
                }
            } else {
                0xDEAD_0000 + u64::from(lane)
            };
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[1], lane, 32), expected);
        }
    }

    // The GPR source may alias an extended destination encoding; zeroing
    // masking applies at qword granularity.
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::R9)), 0x0123_4567_89AB_CDEF);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[17] = [u64::MAX; 16];
        x86.k[3] = 0b0101_1010;
    }
    execute_lifted_x86(&[0x62, 0xC2, 0xFD, 0xCB, 0x7C, 0xC9], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..8u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[17], lane, 64),
                if 0b0101_1010 & (1u64 << lane) != 0 {
                    0x0123_4567_89AB_CDEF
                } else {
                    0
                }
            );
        }
    }

    // Mask-to-vector broadcasts zero-extend only the low byte/word of the
    // K source before repeating it at qword/dword granularity.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.k[7] = 0x1234;
        x86.k[3] = 0x12_3456;
    }
    execute_lifted_x86(&[0x62, 0xE2, 0xFE, 0x48, 0x2A, 0xCF], &mut ctx, &mut memory);
    execute_lifted_x86(&[0x62, 0xF2, 0x7E, 0x28, 0x3A, 0xD3], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..8u8 {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[17], lane, 64), 0x34);
        }
        for lane in 0..8u8 {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[2], lane, 32), 0x3456);
        }
    }

    // Compressed displacement is scaled by the complete 16-byte tuple.
    let tuple = [0x1111_1111u32, 0x2222_2222, 0x3333_3333, 0x4444_4444];
    for (lane, value) in tuple.into_iter().enumerate() {
        memory
            .write(0x100 + lane as u64 * 4, &value.to_le_bytes())
            .unwrap();
    }
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x80);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.k[1] = u64::MAX;
    }
    execute_lifted_x86(
        &[0x62, 0xF2, 0x7D, 0xC9, 0x1A, 0x48, 0x08],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for lane in 0..16u8 {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                u64::from(tuple[usize::from(lane % 4)])
            );
        }
    }

    // Type E6 suppresses the complete tuple read for an all-zero effective
    // mask. Any active destination lane requires the complete tuple and
    // faults before the architectural destination changes.
    ctx.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::Rax)), 0x3F8);
    let sentinel = [0xA5A5_A5A5_A5A5_A5A5; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.k[1] = 0;
    }
    let suppressed =
        execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0xC9, 0x1A, 0x08], &mut ctx, &mut memory);
    assert!(matches!(suppressed, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], [0; 16]);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.k[1] = 1;
    }
    let fault = execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0xC9, 0x1A, 0x08], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], sentinel);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn smir_bextr_bzhi_result_ops_preserve_x86_flags_and_edge_counts() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let flags = 0x2 | 0x1 | 0x40 | 0x80 | 0x800;

    let (value, got_flags) = exec_x86_rax_op(
        OpKind::Bextr {
            dst: rax,
            src: rax,
            control: rcx,
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        0xf0f0,
        (8 << 8) | 4,
        flags,
    );
    assert_eq!(value, 0x0f);
    assert_eq!(got_flags, flags);

    let (value, got_flags) = exec_x86_rax_op(
        OpKind::Bextr {
            dst: rax,
            src: rax,
            control: rcx,
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        0x1234_5678,
        64,
        flags,
    );
    assert_eq!(value, 0);
    assert_eq!(got_flags, flags);

    let (value, got_flags) = exec_x86_rax_op(
        OpKind::Bzhi {
            dst: rax,
            src: rax,
            index: rcx,
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        0xffff_1234_5678_9abc,
        16,
        flags,
    );
    assert_eq!(value, 0x9abc);
    assert_eq!(got_flags, flags);

    let (value, got_flags) = exec_x86_rax_op(
        OpKind::Bzhi {
            dst: rax,
            src: rax,
            index: rcx,
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        0xffff_1234_5678_9abc,
        64,
        flags,
    );
    assert_eq!(value, 0xffff_1234_5678_9abc);
    assert_eq!(got_flags, flags);
}
#[test]
fn smir_andn_updates_only_defined_x86_flags() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    const CF: u64 = 1 << 0;
    const PF: u64 = 1 << 2;
    const AF: u64 = 1 << 4;
    const ZF: u64 = 1 << 6;
    const SF: u64 = 1 << 7;
    const OF: u64 = 1 << 11;
    let defined = FlagSet::CF
        .union(FlagSet::ZF)
        .union(FlagSet::SF)
        .union(FlagSet::OF);
    let initial = 0x2 | CF | PF | AF | ZF | OF;

    let (value, got_flags) = exec_x86_rax_op(
        OpKind::AndNot {
            dst: rax,
            src1: rax,
            src2: SrcOperand::Reg(rcx),
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(defined),
        },
        0x8000_0000_0000_0000,
        0,
        initial,
    );
    assert_eq!(value, 0x8000_0000_0000_0000);
    assert_eq!(got_flags & CF, 0, "ANDN clears CF");
    assert_eq!(got_flags & ZF, 0, "nonzero ANDN clears ZF");
    assert_ne!(got_flags & SF, 0, "ANDN sets SF from its result");
    assert_eq!(got_flags & OF, 0, "ANDN clears OF");
    assert_ne!(got_flags & PF, 0, "ANDN preserves undefined PF");
    assert_ne!(got_flags & AF, 0, "ANDN preserves undefined AF");

    let (value, got_flags) = exec_x86_rax_op(
        OpKind::AndNot {
            dst: rax,
            src1: rax,
            src2: SrcOperand::Reg(rcx),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
        0xffff_ffff,
        0xffff_fff0,
        initial,
    );
    assert_eq!(value, 0x0f);
    assert_eq!(got_flags, initial, "APX NF ANDN preserves every flag");
}
#[test]
fn test_vwidenmul_byte_layout() {
    // V0 bytes = [3,7,3,7,...], V1 = [5,2,5,2,...].
    // lo.h[i] = even_byte products = 3*5 = 15; hi.h[i] = odd = 7*2 = 14.
    let v0 = [0x0703_0703_0703_0703u64; 16];
    let v1 = [0x0205_0205_0205_0205u64; 16];
    let (lo, hi) = run_widenmul(v0, v1, VecElementType::I8, true, true);
    assert_eq!(lo, [0x000F_000F_000F_000Fu64; 16]); // 15 per halfword
    assert_eq!(hi, [0x000E_000E_000E_000Eu64; 16]); // 14 per halfword
}
#[test]
fn test_vwidenmul_half_to_word() {
    // half*half -> word pair. V0 half = 0x0003, V1 half = 0x0005 -> 15.
    let v0 = [0x0003_0003_0003_0003u64; 16];
    let v1 = [0x0005_0005_0005_0005u64; 16];
    let (lo, hi) = run_widenmul(v0, v1, VecElementType::I16, true, true);
    assert_eq!(lo, [0x0000_000F_0000_000Fu64; 16]); // word = 15
    assert_eq!(hi, [0x0000_000F_0000_000Fu64; 16]);
}
#[test]
fn test_vnarrowshiftsat_wh_interleave() {
    // word->half, signed src, no round, no shift (rt=0), saturate signed.
    // V0 (src_lo/Vv) word = 0x0000_1234, V1 (src_hi/Vu) word = 0x0000_5678.
    // out half[2i] = sat(0x1234) = 0x1234 (even <- Vv);
    // out half[2i+1] = sat(0x5678) = 0x5678 (odd <- Vu).
    let v0 = [0x0000_1234_0000_1234u64; 16];
    let v1 = [0x0000_5678_0000_5678u64; 16];
    let out = run_narrow_shift_sat(v0, v1, 0, VecElementType::I32, true, false, 1);
    // each 32-bit out word = [Vv-half | Vu-half<<16] = 0x5678_1234
    assert_eq!(out, [0x5678_1234_5678_1234u64; 16]);
}
#[test]
fn test_vwidenaddsub_byte_layout() {
    // V0 bytes = [3,7,...], V1 = [5,2,...]. Even-byte add -> lo.h = 3+5=8,
    // odd-byte add -> hi.h = 7+2=9. Sub: lo.h = 3-5=-2=0xFFFE, hi.h=7-2=5.
    let v0 = [0x0703_0703_0703_0703u64; 16];
    let v1 = [0x0205_0205_0205_0205u64; 16];
    let run = |sub: bool, s1: bool, s2: bool, acc: bool| -> ([u64; 16], [u64; 16]) {
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
            hex.set_v(0, v0);
            hex.set_v(1, v1);
            hex.set_v(2, [0u64; 16]);
            hex.set_v(3, [0u64; 16]);
        }
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let block = SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: OpKind::VWidenAddSub {
                    dst_lo: mkv(2),
                    dst_hi: mkv(3),
                    src1: mkv(0),
                    src2: mkv(1),
                    src_elem: VecElementType::I8,
                    signed1: s1,
                    signed2: s2,
                    sub,
                    acc,
                },
                x86_hint: None,
            }],
            terminator: Terminator::Trap {
                kind: TrapKind::Halt,
            },
            exec_count: 0,
        };
        interp.execute_block(&mut ctx, &mut memory, &block);
        match &ctx.arch_regs {
            ArchRegState::Hexagon(hex) => (hex.get_v(2), hex.get_v(3)),
            _ => panic!("not hexagon"),
        }
    };
    let (lo, hi) = run(false, false, false, false);
    assert_eq!(lo, [0x0008_0008_0008_0008u64; 16]); // 3+5=8
    assert_eq!(hi, [0x0009_0009_0009_0009u64; 16]); // 7+2=9
    let (lo, hi) = run(true, false, false, false);
    assert_eq!(lo, [0xFFFE_FFFE_FFFE_FFFEu64; 16]); // 3-5=-2
    assert_eq!(hi, [0x0005_0005_0005_0005u64; 16]); // 7-2=5
}
#[test]
fn test_vreducemul_byte4_to_word() {
    // 4-tap byte dot product -> word. Every byte of V0 = 2, V1 = 3.
    // Each word lane = sum of 4 products = 4 * (2*3) = 24 = 0x18.
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        hex.set_v(0, [0x0202_0202_0202_0202u64; 16]);
        hex.set_v(1, [0x0303_0303_0303_0303u64; 16]);
    }
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let mk = |op| SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: op,
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(
        &mut ctx,
        &mut memory,
        &mk(OpKind::VReduceMul {
            dst: mkv(2),
            src1: mkv(0),
            src2: mkv(1),
            src1_elem: VecElementType::I8,
            src2_elem: VecElementType::I8,
            out_elem: VecElementType::I32,
            taps: 4,
            sat: false,
            set_ovf: false,
            signed1: false,
            signed2: false,
            acc: false,
        }),
    );
    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        assert_eq!(hex.get_v(2), [0x0000_0018_0000_0018u64; 16]); // word = 24
    }
    // Accumulate: dst already holds 24 per word; +24 -> 48 = 0x30.
    interp.execute_block(
        &mut ctx,
        &mut memory,
        &mk(OpKind::VReduceMul {
            dst: mkv(2),
            src1: mkv(0),
            src2: mkv(1),
            src1_elem: VecElementType::I8,
            src2_elem: VecElementType::I8,
            out_elem: VecElementType::I32,
            taps: 4,
            sat: false,
            set_ovf: false,
            signed1: false,
            signed2: false,
            acc: true,
        }),
    );
    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        assert_eq!(hex.get_v(2), [0x0000_0030_0000_0030u64; 16]); // word = 48
    }
}
#[test]
fn test_vwidenext_interleave_zero() {
    // vzb: every byte = 0xAB. Interleaved zero-extend byte->half.
    // lo.h[i] = ZE(byte 2i) = 0x00AB; hi.h[i] = ZE(byte 2i+1) = 0x00AB.
    let (lo, hi) = run_widenext(
        [0xABAB_ABAB_ABAB_ABABu64; 16],
        VecElementType::I8,
        false,
        true,
    );
    assert_eq!(lo, [0x00AB_00AB_00AB_00ABu64; 16]);
    assert_eq!(hi, [0x00AB_00AB_00AB_00ABu64; 16]);
}
#[test]
fn test_vwidenext_interleave_sign() {
    // vsb: every byte = 0x80 (-128). Sign-extend byte->half = 0xFF80.
    let (lo, hi) = run_widenext(
        [0x8080_8080_8080_8080u64; 16],
        VecElementType::I8,
        true,
        true,
    );
    assert_eq!(lo, [0xFF80_FF80_FF80_FF80u64; 16]);
    assert_eq!(hi, [0xFF80_FF80_FF80_FF80u64; 16]);
}
#[test]
fn vshuffle_uses_explicit_lanes_two_source_indices_and_zeroes_inactive_state() {
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        let mut first = [u64::MAX; 16];
        first[0] = 0x0013_0012_0011_0010;
        let mut second = [u64::MAX; 16];
        second[0] = 0x0023_0022_0021_0020;
        let mut indices = [0u64; 16];
        indices[0] = 0x0007_0003_0004_0000;
        hex.set_v(0, first);
        hex.set_v(1, second);
        hex.set_v(3, indices);
        hex.set_v(2, [0xA5A5_A5A5_A5A5_A5A5; 16]);
    }
    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp::new(
            OpId(0),
            0x1000,
            OpKind::VShuffle {
                dst: mkv(2),
                src1: mkv(0),
                src2: Some(mkv(1)),
                indices: mkv(3),
                elem: VecElementType::I16,
                lanes: 4,
            },
        )],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    SmirInterpreter::new().execute_block(&mut ctx, &mut memory, &block);
    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        assert_eq!(hex.get_v(2)[0], 0x0023_0013_0020_0010);
        assert!(hex.get_v(2)[1..].iter().all(|word| *word == 0));
    }
}
#[test]
fn vinterleave_selects_halves_independently_in_each_lane_block() {
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let mut first = [0u64; 16];
    first[..4].copy_from_slice(&[
        0x1111_1111_0000_0000,
        0x3333_3333_2222_2222,
        0x5555_5555_4444_4444,
        0x7777_7777_6666_6666,
    ]);
    let mut second = [0u64; 16];
    second[..4].copy_from_slice(&[
        0xBBBB_BBBB_AAAA_AAAA,
        0xDDDD_DDDD_CCCC_CCCC,
        0xFFFF_FFFF_EEEE_EEEE,
        0x9999_9999_8888_8888,
    ]);

    for (high, expected) in [
        (
            false,
            [
                0xAAAA_AAAA_0000_0000,
                0xBBBB_BBBB_1111_1111,
                0xEEEE_EEEE_4444_4444,
                0xFFFF_FFFF_5555_5555,
            ],
        ),
        (
            true,
            [
                0xCCCC_CCCC_2222_2222,
                0xDDDD_DDDD_3333_3333,
                0x8888_8888_6666_6666,
                0x9999_9999_7777_7777,
            ],
        ),
    ] {
        let out = run_vec2(
            first,
            second,
            OpKind::VInterleave {
                dst: mkv(2),
                src1: mkv(0),
                src2: mkv(1),
                elem: VecElementType::I32,
                lanes: 8,
                block_lanes: 4,
                high,
            },
        );
        assert_eq!(out[..4], expected);
        assert!(out[4..].iter().all(|word| *word == 0));
    }
}
#[test]
fn test_vshuffle2_byte_roundtrip() {
    // shuffle then deal must be identity. Use a distinguishable per-byte pattern.
    let mut v0 = [0u64; 16];
    for (i, w) in v0.iter_mut().enumerate() {
        // each byte = its global index (mod 256)
        let mut x = 0u64;
        for b in 0..8 {
            x |= (((i * 8 + b) as u64) & 0xff) << (b * 8);
        }
        *w = x;
    }
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    // shuffle V0 -> V2
    let shuffled = run_vec2(
        v0,
        [0u64; 16],
        OpKind::VShuffle2 {
            dst: mkv(2),
            src: mkv(0),
            elem: VecElementType::I8,
            deal: false,
        },
    );
    // deal the shuffled value -> should recover v0
    let dealt = run_vec2(
        shuffled,
        [0u64; 16],
        OpKind::VShuffle2 {
            dst: mkv(2),
            src: mkv(0),
            elem: VecElementType::I8,
            deal: true,
        },
    );
    assert_eq!(dealt, v0, "deal(shuffle(x)) must equal x");
    // explicit check: shuffle out[0]=src.b[0], out[1]=src.b[64].
    assert_eq!((shuffled[0] & 0xff) as u8, 0); // src byte 0
    assert_eq!(((shuffled[0] >> 8) & 0xff) as u8, 64); // src byte 64
}
#[test]
fn test_vshuffleeo_even_byte() {
    // vshuffeb: out.b[2i] = Vv.b[2i], out.b[2i+1] = Vu.b[2i].
    // V0(=Vu) halfwords = 0x__11 (byte0=0x11), V1(=Vv) = 0x__22 (byte0=0x22).
    let v0 = [0xAA11_AA11_AA11_AA11u64; 16];
    let v1 = [0xBB22_BB22_BB22_BB22u64; 16];
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let out = run_vec2(
        v0,
        v1,
        OpKind::VShuffleEO {
            dst: mkv(2),
            src1: mkv(0),
            src2: mkv(1),
            elem: VecElementType::I8,
            odd: false,
        },
    );
    // every output halfword = Vv.b0(0x22) | Vu.b0(0x11)<<8 = 0x1122.
    assert_eq!(out, [0x1122_1122_1122_1122u64; 16]);
}
#[test]
fn test_vbroadcast_gpr_to_words() {
    // Splat GPR R5 = 0xDEADBEEF into every word lane of V2.
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();
    ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::R(5)), 0xDEAD_BEEF);
    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::VBroadcast {
                dst: VReg::Arch(ArchReg::Hexagon(HexagonReg::V(2))),
                scalar: VReg::Arch(ArchReg::Hexagon(HexagonReg::R(5))),
                elem: VecElementType::I32,
                lanes: 32,
            },
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(&mut ctx, &mut memory, &block);
    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        assert_eq!(hex.get_v(2), [0xDEAD_BEEF_DEAD_BEEFu64; 16]);
    }
}
#[test]
fn test_vlanecond_byte() {
    // if (Q0) V0.b += V1.b: byte0 Q-set -> add, byte1 Q-clear -> unchanged.
    let mut vx = [0u64; 16];
    vx[0] = 0x0000_0000_0000_2010; // byte0=0x10, byte1=0x20
    let mut vu = [0u64; 16];
    vu[0] = 0x0000_0000_0000_0505; // byte0=0x05, byte1=0x05
    let mut q = [0u64; 16];
    q[0] = 0b01; // only Q bit0 set (covers byte0)
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let out = run_lanecond(
        vx,
        vu,
        q,
        OpKind::VLaneCond {
            dst: mkv(0),
            src: mkv(1),
            mask_q: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(0))),
            elem: VecElementType::I8,
            lanes: 128,
            sub: false,
            negate: false,
        },
    );
    // byte0: 0x10+0x05=0x15 (Q set); byte1: 0x20 unchanged (Q clear).
    assert_eq!(out[0] & 0xffff, 0x2015);
    // negate: byte0 unchanged, byte1 adds.
    let out_n = run_lanecond(
        vx,
        vu,
        q,
        OpKind::VLaneCond {
            dst: mkv(0),
            src: mkv(1),
            mask_q: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(0))),
            elem: VecElementType::I8,
            lanes: 128,
            sub: false,
            negate: true,
        },
    );
    assert_eq!(out_n[0] & 0xffff, 0x2510); // byte0=0x10, byte1=0x20+0x05=0x25
}
#[test]
fn lifted_mmx_pmovmskb_extracts_byte_signs_and_enters_mmx_state() {
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
    let flags_before = 0xCD7;
    let mut memory = FlatMemory::new(0x100);
    let mut ctx = SmirContext::new_x86_64();
    ctx.write_vreg(r8, u64::MAX);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        // Little-endian byte sign bits are 10101010b from byte 7 to 0.
        x86.mm[1] = 0x80_7F_FF_00_81_01_FE_7E;
        x86.x87.tag_word = 0xFFFF;
        x86.x87.status_word = 3 << 11;
    }

    let exit = execute_lifted_x86(&[0x4C, 0x0F, 0xD7, 0xC1], &mut ctx, &mut memory);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(ctx.read_vreg(r8), 0xAA);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[1], 0x80_7F_FF_00_81_01_FE_7E);
        assert_eq!(x86.x87.tag_word, 0);
        assert_eq!(x86.x87.status_word & 0x3800, 3 << 11);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_mmx_pinsrw_pextrw_execute_rex_lanes_state_memory_and_faults() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
    let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
    let flags_before = 0xCD7;
    let mut memory = FlatMemory::new(0x100);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[1] = 0x4444_3333_2222_1111;
        x86.x87.tag_word = 0xFFFF;
        x86.x87.status_word = 6 << 11;
    }
    ctx.write_vreg(r8, 0xDEAD_BEEF_CAFE_A1B2);

    // REX.B selects R8 as the scalar source, REX.R is ignored for MM1,
    // and only imm8[1:0] selects one of four words.
    execute_lifted_x86(&[0x45, 0x0F, 0xC4, 0xC8, 0xFF], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[1], 0xA1B2_3333_2222_1111);
        assert_eq!(x86.x87.tag_word, 0);
        assert_eq!(x86.x87.status_word & 0x3800, 6 << 11);
    }

    // REX.R selects R9 as the destination, REX.B is ignored for MM1, and
    // PEXTRW clears every destination bit above the selected word.
    ctx.write_vreg(r9, u64::MAX);
    execute_lifted_x86(&[0x45, 0x0F, 0xC5, 0xC9, 0xFE], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(r9), 0x3333);

    // An unaligned m16 source reads exactly two bytes before entering MMX
    // state or changing its destination.
    memory.write(0x41, &0x7788u16.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x40);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[1] = 0x4444_3333_2222_1111;
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0xC4, 0x48, 0x01, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[1], 0x4444_3333_2222_7788);
        assert_eq!(x86.x87.tag_word, 0);
    }

    ctx.write_vreg(rax, 0x100);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[1] = 0xA5A5_5A5A_C3C3_3C3C;
        x86.x87.tag_word = 0xFFFF;
    }
    let fault = execute_lifted_x86(&[0x0F, 0xC4, 0x08, 0x03], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[1], 0xA5A5_5A5A_C3C3_3C3C);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
    assert_eq!(ctx.read_vreg(r8), 0xDEAD_BEEF_CAFE_A1B2);
}
#[test]
fn lifted_mmx_movq_executes_directions_rex_memory_state_and_faults() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let flags_before = 0xCD7;
    let mut memory = FlatMemory::new(0x100);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xAAAA_AAAA_AAAA_AAAA;
        x86.mm[1] = 0x0123_4567_89AB_CDEF;
        x86.x87.tag_word = 0xFFFF;
        x86.x87.status_word = 4 << 11;
    }

    // REX.R/REX.B do not extend either three-bit MM field.
    execute_lifted_x86(&[0x45, 0x0F, 0x6F, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0x0123_4567_89AB_CDEF);
        assert_eq!(x86.mm[1], 0x0123_4567_89AB_CDEF);
        assert_eq!(x86.x87.tag_word, 0);
        assert_eq!(x86.x87.status_word & 0x3800, 4 << 11);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xFEDC_BA98_7654_3210;
    }
    execute_lifted_x86(&[0x45, 0x0F, 0x7F, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[1], 0xFEDC_BA98_7654_3210);
    }

    let memory_value = 0x8877_6655_4433_2211u64;
    memory.write(0x41, &memory_value.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x40);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0x6F, 0x48, 0x01], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[1], memory_value);
        assert_eq!(x86.x87.tag_word, 0);
    }
    ctx.write_vreg(rax, 0x50);
    execute_lifted_x86(&[0x0F, 0x7F, 0x48, 0x01], &mut ctx, &mut memory);
    let mut stored = [0u8; 8];
    memory.read(0x51, &mut stored).unwrap();
    assert_eq!(u64::from_le_bytes(stored), memory_value);

    for (bytes, write) in [
        (&[0x0F, 0x6F, 0x08][..], false),
        (&[0x0F, 0x7F, 0x08][..], true),
    ] {
        ctx.write_vreg(rax, 0x100);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[1] = 0xA5A5_5A5A_C3C3_3C3C;
            x86.x87.tag_word = 0xFFFF;
        }
        let fault = execute_lifted_x86(bytes, &mut ctx, &mut memory);
        assert!(matches!(
            fault,
            BlockResult::Exit(ExitReason::MemoryFault {
                write: actual,
                ..
            }) if actual == write
        ));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[1], 0xA5A5_5A5A_C3C3_3C3C);
            assert_eq!(x86.x87.tag_word, 0xFFFF);
        }
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_legacy_and_vex_packed_integer_compares_execute_signedness_widths_and_faults() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let flags_before = 0xCD7;
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let mut memory = FlatMemory::new(0x400);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    let mmx_cases = [
        (
            0x64,
            0x3412_00FF_0505_7F80,
            0x3512_01FF_0605_807F,
            0x0000_0000_0000_FF00,
        ),
        (
            0x65,
            0xFFFE_FFFF_7FFF_8000,
            0xFFFF_FFFE_8000_7FFF,
            0x0000_FFFF_FFFF_0000,
        ),
        (
            0x66,
            0x7FFF_FFFF_8000_0000,
            0x8000_0000_7FFF_FFFF,
            0xFFFF_FFFF_0000_0000,
        ),
        (
            0x74,
            0xAA22_CC44_5566_7788,
            0xAA00_CCFF_5500_77FF,
            0xFF00_FF00_FF00_FF00,
        ),
        (
            0x75,
            0xAAAA_BBBB_CCCC_DDDD,
            0xAAAA_0000_CCCC_1111,
            0xFFFF_0000_FFFF_0000,
        ),
        (
            0x76,
            0xAAAA_BBBB_CCCC_DDDD,
            0xAAAA_BBBB_1111_2222,
            0xFFFF_FFFF_0000_0000,
        ),
    ];
    for (opcode, lhs, rhs, expected) in mmx_cases {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = lhs;
            x86.mm[1] = rhs;
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 2 << 11;
        }
        execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mm[0], expected, "MMX {opcode:02X}");
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 2 << 11);
        }
    }

    // The MMX memory form consumes exactly 8 bytes before entering MMX
    // state and committing the compare result.
    memory
        .write(0x3F8, &0xAA00_CCFF_5500_77FFu64.to_le_bytes())
        .unwrap();
    ctx.write_vreg(rax, 0x3F8);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xAA22_CC44_5566_7788;
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0x74, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xFF00_FF00_FF00_FF00);
        assert_eq!(x86.x87.tag_word, 0);
    }

    // A faulting source changes neither the destination nor the x87 tags.
    ctx.write_vreg(rax, 0x1000);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xA5A5_5A5A_C3C3_3C3C;
        x86.x87.tag_word = 0xFFFF;
    }
    let mmx_fault = execute_lifted_x86(&[0x0F, 0x74, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        mmx_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xA5A5_5A5A_C3C3_3C3C);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    let signed_cases = [
        (
            &[0x66, 0x0F, 0x64, 0xC1][..],
            0x3412_00FF_0505_7F80,
            0x3512_01FF_0605_807F,
            0x0000_0000_0000_FF00,
            0,
        ),
        (
            &[0x66, 0x0F, 0x65, 0xC1][..],
            0xFFFE_FFFF_7FFF_8000,
            0xFFFF_FFFE_8000_7FFF,
            0x0000_FFFF_FFFF_0000,
            0,
        ),
        (
            &[0x66, 0x0F, 0x66, 0xC1][..],
            0x7FFF_FFFF_8000_0000,
            0x8000_0000_7FFF_FFFF,
            0xFFFF_FFFF_0000_0000,
            0,
        ),
        (
            &[0x66, 0x0F, 0x38, 0x37, 0xC1][..],
            u64::MAX,
            u64::MAX - 1,
            u64::MAX,
            0,
        ),
    ];
    for (bytes, lhs, rhs, expected0, expected1) in signed_cases {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [upper; 16];
            x86.xmm[0][0] = lhs;
            x86.xmm[0][1] = 0;
            x86.xmm[1][0] = rhs;
            x86.xmm[1][1] = 1;
        }
        execute_lifted_x86(bytes, &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[0][0], expected0, "{bytes:02X?}");
            assert_eq!(x86.xmm[0][1], expected1, "{bytes:02X?}");
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }
    }

    // Equality produces an all-ones lane, including qwords, and treats
    // signed zero identically because it is an integer comparison.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [upper; 16];
        x86.xmm[0][0] = 0x0123_4567_89AB_CDEF;
        x86.xmm[0][1] = 0x8000_0000_0000_0000;
        x86.xmm[1][0] = 0x0123_4567_89AB_CDEF;
        x86.xmm[1][1] = 0;
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x29, 0xC1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], u64::MAX);
        assert_eq!(x86.xmm[0][1], 0);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
    }

    // VEX.256 compares all 256 bits and clears the shared backing state
    // above bit 255. Destination/source aliasing is safe because operands
    // are read before the single VCmp write.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = [u64::MAX; 16];
        x86.xmm[1][..4].copy_from_slice(&[
            0xA000_0000_0000_0001,
            0xB000_0000_0000_0002,
            0xC000_0000_0000_0003,
            0xD000_0000_0000_0004,
        ]);
        x86.xmm[2][..4].copy_from_slice(&[
            0xA100_0000_0000_0001,
            0xB000_0000_0000_0009,
            0xC000_0000_0000_0003,
            0xD100_0000_0000_0008,
        ]);
    }
    execute_lifted_x86(&[0xC5, 0xF5, 0x76, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            &x86.xmm[0][..4],
            &[0x0000_0000_FFFF_FFFF, 0xFFFF_FFFF_0000_0000, u64::MAX, 0,]
        );
        assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
    }

    // EVEX fixed comparisons write a k-mask, apply the input writemask,
    // zero inactive/high result bits, and remain correct when dst==mask.
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1][0] = 2u64 << 32 | 1;
        x86.xmm[1][1] = 4u64 << 32 | 3;
        x86.xmm[2][0] = 9u64 << 32 | 1;
        x86.xmm[2][1] = 4u64 << 32 | 3;
    }
    ctx.write_vreg(k1, 0b1011);
    ctx.write_vreg(k2, u64::MAX);
    execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x09, 0x76, 0xD2], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(k2), 0b1001);

    ctx.write_vreg(k1, 0b1111);
    execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x09, 0x76, 0xCA], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(k1), 0b1101);

    // A zero writemask suppresses every memory access. Enabling one lane
    // exposes the fault, without committing the k destination.
    ctx.write_vreg(rax, 0x1000);
    ctx.write_vreg(k1, 0);
    ctx.write_vreg(k2, 0xDEAD_BEEF);
    let suppressed =
        execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x09, 0x76, 0x10], &mut ctx, &mut memory);
    assert!(!matches!(
        suppressed,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    assert_eq!(ctx.read_vreg(k2), 0);

    ctx.write_vreg(k1, 1);
    ctx.write_vreg(k2, 0xDEAD_BEEF);
    let exposed = execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x09, 0x76, 0x10], &mut ctx, &mut memory);
    assert!(matches!(
        exposed,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    assert_eq!(ctx.read_vreg(k2), 0xDEAD_BEEF);

    // A masked EVEX.512 broadcast reads one dword for each active lane and
    // compares it against all selected source lanes.
    memory.write(0x100, &7u32.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x100);
    ctx.write_vreg(k1, (1 << 0) | (1 << 5) | (1 << 15));
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = [0; 16];
        x86.xmm[1][0] = 7;
        x86.xmm[1][2] = 7u64 << 32;
        x86.xmm[1][7] = 8u64 << 32;
    }
    execute_lifted_x86(&[0x62, 0xF1, 0x75, 0x59, 0x76, 0x10], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(k2), (1 << 0) | (1 << 5));

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);

    // A memory fault precedes the architectural compare destination write.
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    ctx.write_vreg(rax, 0x1000);
    let exit = execute_lifted_x86(&[0xC5, 0xF5, 0x74, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }
}
#[test]
fn lifted_legacy_and_vex_packed_unpacks_interleave_per_128_bit_lane() {
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

    fn expected(
        first: &[u8],
        second: &[u8],
        elem: usize,
        block_bytes: usize,
        high: bool,
    ) -> Vec<u8> {
        let mut result = Vec::with_capacity(first.len());
        for (a, b) in first
            .chunks_exact(block_bytes)
            .zip(second.chunks_exact(block_bytes))
        {
            let half = block_bytes / 2;
            let start = if high { half } else { 0 };
            for offset in (start..start + half).step_by(elem) {
                result.extend_from_slice(&a[offset..offset + elem]);
                result.extend_from_slice(&b[offset..offset + elem]);
            }
        }
        result
    }

    let first = (1u8..=32).collect::<Vec<_>>();
    let second = (0x81u8..=0xA0).collect::<Vec<_>>();
    let cases = [
        (0x60, 1, false),
        (0x61, 2, false),
        (0x62, 4, false),
        (0x6C, 8, false),
        (0x68, 1, true),
        (0x69, 2, true),
        (0x6A, 4, true),
        (0x6D, 8, true),
    ];
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let flags_before = 0xCD7;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    for (opcode, elem, high) in [
        (0x60, 1, false),
        (0x61, 2, false),
        (0x62, 4, false),
        (0x68, 1, true),
        (0x69, 2, true),
        (0x6A, 4, true),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
            x86.mm[1] = u64::from_le_bytes(second[..8].try_into().unwrap());
            x86.x87.tag_word = 0xFFFF;
            x86.x87.status_word = 4 << 11;
        }
        execute_lifted_x86(&[0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                x86.mm[0].to_le_bytes(),
                expected(&first[..8], &second[..8], elem, 8, high).as_slice(),
                "MMX opcode {opcode:02X}"
            );
            assert_eq!(x86.x87.tag_word, 0);
            assert_eq!(x86.x87.status_word & 0x3800, 4 << 11);
        }
    }

    // Low MMX memory forms access only m32. Placing the operand at the
    // exact end of memory distinguishes that architectural width from an
    // incorrect 8-byte read.
    let low_memory = [0xA1, 0xA2, 0xA3, 0xA4];
    memory.write(0x3FC, &low_memory).unwrap();
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    ctx.write_vreg(rax, 0x3FC);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0x60, 0x00], &mut ctx, &mut memory);
    let mut low_source = [0u8; 8];
    low_source[..4].copy_from_slice(&low_memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0].to_le_bytes(),
            expected(&first[..8], &low_source, 1, 8, false).as_slice()
        );
        assert_eq!(x86.x87.tag_word, 0);
    }

    // High MMX memory forms consume the complete m64 source.
    let high_memory = [0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8];
    memory.write(0x3F8, &high_memory).unwrap();
    ctx.write_vreg(rax, 0x3F8);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = u64::from_le_bytes(first[..8].try_into().unwrap());
        x86.x87.tag_word = 0xFFFF;
    }
    execute_lifted_x86(&[0x0F, 0x68, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            x86.mm[0].to_le_bytes(),
            expected(&first[..8], &high_memory, 1, 8, true).as_slice()
        );
        assert_eq!(x86.x87.tag_word, 0);
    }

    // A source fault precedes both the MMX-state transition and result.
    ctx.write_vreg(rax, 0x1000);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm[0] = 0xA5A5_5A5A_C3C3_3C3C;
        x86.x87.tag_word = 0xFFFF;
    }
    let mmx_fault = execute_lifted_x86(&[0x0F, 0x60, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        mmx_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm[0], 0xA5A5_5A5A_C3C3_3C3C);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
    }

    for (opcode, elem, high) in cases {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&first[..16], upper);
            x86.xmm[1] = seeded(&second[..16], 0);
        }
        execute_lifted_x86(&[0x66, 0x0F, opcode, 0xC1], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 16),
                expected(&first[..16], &second[..16], elem, 16, high),
                "legacy opcode {opcode:02X}"
            );
            assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
        }

        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = [u64::MAX; 16];
            x86.xmm[1] = seeded(&first, 0);
            x86.xmm[2] = seeded(&second, 0);
        }
        execute_lifted_x86(&[0xC5, 0xF5, opcode, 0xC2], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                bytes(&x86.xmm[0], 32),
                expected(&first, &second, elem, 16, high),
                "VEX opcode {opcode:02X}"
            );
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }
    }

    // Same-register legacy sources are captured before the destination
    // merge, so each selected element is duplicated exactly.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = seeded(&first[..16], upper);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x60, 0xC0], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            bytes(&x86.xmm[0], 16),
            expected(&first[..16], &first[..16], 1, 16, false)
        );
    }

    // EVEX merge/zero masks apply to output elements after each 128-bit
    // lane-local interleave and clear backing state above VL.
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let raw = expected(&first, &second, 2, 16, true);
    let mask = 0xA55Au64;
    for (p2, zeroing) in [(0x29, false), (0xA9, true)] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = seeded(&[0xEE; 32], u64::MAX);
            x86.xmm[1] = seeded(&first, 0);
            x86.xmm[2] = seeded(&second, 0);
        }
        ctx.write_vreg(k1, mask);
        execute_lifted_x86(&[0x62, 0xF1, 0x75, p2, 0x69, 0xC2], &mut ctx, &mut memory);
        let mut masked = Vec::with_capacity(32);
        for lane in 0..16 {
            if mask >> lane & 1 != 0 {
                masked.extend_from_slice(&raw[lane * 2..lane * 2 + 2]);
            } else if zeroing {
                masked.extend_from_slice(&[0, 0]);
            } else {
                masked.extend_from_slice(&[0xEE, 0xEE]);
            }
        }
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(bytes(&x86.xmm[0], 32), masked);
            assert!(x86.xmm[0][4..].iter().all(|word| *word == 0));
        }
    }

    // EVEX integer unpack is E4NF/E4NF.nb: every memory form performs the
    // complete vector access before masking, including even-only or
    // all-zero masks.
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.xmm[1] = seeded(&first, 0);
    }
    ctx.write_vreg(rax, 0x1000);
    for mask in [0x55, 0x00] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[0] = sentinel;
        }
        ctx.write_vreg(k1, mask);
        let fault =
            execute_lifted_x86(&[0x62, 0xF1, 0xF5, 0x49, 0x6D, 0x00], &mut ctx, &mut memory);
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

    // VEX.256 memory forms require the complete 32-byte operand before any
    // architectural destination write.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
    }
    ctx.write_vreg(rax, 0x3F0);
    let exit = execute_lifted_x86(&[0xC5, 0xF5, 0x60, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], sentinel);
    }
}
