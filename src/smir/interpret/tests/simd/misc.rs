//! simd::misc tests

use super::*;
use crate::smir::interpret::*;
use crate::smir::interpret::tests::*;

    #[test]
    fn smir_bextr_bzhi_flagful_ops_update_defined_x86_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        const CF: u64 = 1 << 0;
        const PF: u64 = 1 << 2;
        const AF: u64 = 1 << 4;
        const ZF: u64 = 1 << 6;
        const SF: u64 = 1 << 7;
        const OF: u64 = 1 << 11;

        let bextr_flags = FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF));
        let bzhi_flags = FlagUpdate::Specific(
            FlagSet::CF
                .union(FlagSet::ZF)
                .union(FlagSet::SF)
                .union(FlagSet::OF),
        );
        let stale_flags = 0x2 | CF | PF | AF | ZF | SF | OF;

        let (value, got_flags) = exec_x86_rax_op(
            OpKind::Bextr {
                dst: rax,
                src: rax,
                control: rcx,
                width: OpWidth::W64,
                flags: bextr_flags,
            },
            0xf0f0,
            (8 << 8) | 4,
            stale_flags,
        );
        assert_eq!(value, 0x0f);
        assert_eq!(got_flags & CF, 0, "BEXTR clears CF");
        assert_eq!(got_flags & ZF, 0, "BEXTR clears ZF for non-zero");
        assert_ne!(got_flags & SF, 0, "BEXTR preserves undefined SF");
        assert_ne!(got_flags & PF, 0, "BEXTR preserves undefined PF");
        assert_ne!(got_flags & AF, 0, "BEXTR preserves undefined AF");
        assert_eq!(got_flags & OF, 0, "BEXTR clears OF");

        let (value, got_flags) = exec_x86_rax_op(
            OpKind::Bextr {
                dst: rax,
                src: rax,
                control: rcx,
                width: OpWidth::W64,
                flags: bextr_flags,
            },
            0x1234,
            64,
            stale_flags,
        );
        assert_eq!(value, 0);
        assert_ne!(got_flags & ZF, 0, "BEXTR sets ZF for zero");
        assert_ne!(got_flags & SF, 0, "BEXTR preserves undefined SF");
        assert_ne!(got_flags & PF, 0, "BEXTR preserves undefined PF");
        assert_ne!(got_flags & AF, 0, "BEXTR preserves undefined AF");
        assert_eq!(got_flags & CF, 0, "BEXTR keeps CF clear");
        assert_eq!(got_flags & OF, 0, "BEXTR keeps OF clear");

        let (value, got_flags) = exec_x86_rax_op(
            OpKind::Bzhi {
                dst: rax,
                src: rax,
                index: rcx,
                width: OpWidth::W64,
                flags: bzhi_flags,
            },
            0x8000_0000_0000_0001,
            64,
            stale_flags,
        );
        assert_eq!(value, 0x8000_0000_0000_0001);
        assert_ne!(got_flags & CF, 0, "BZHI sets CF when index >= width");
        assert_eq!(got_flags & ZF, 0, "BZHI clears ZF for non-zero");
        assert_ne!(got_flags & SF, 0, "BZHI sets SF from result sign");
        assert_ne!(got_flags & PF, 0, "BZHI preserves undefined PF");
        assert_ne!(got_flags & AF, 0, "BZHI preserves undefined AF");
        assert_eq!(got_flags & OF, 0, "BZHI clears OF");

        let (value, got_flags) = exec_x86_rax_op(
            OpKind::Bzhi {
                dst: rax,
                src: rax,
                index: rcx,
                width: OpWidth::W64,
                flags: bzhi_flags,
            },
            0x1234_5678,
            0,
            stale_flags,
        );
        assert_eq!(value, 0);
        assert_eq!(got_flags & CF, 0, "BZHI clears CF when index < width");
        assert_ne!(got_flags & ZF, 0, "BZHI sets ZF for zero");
        assert_eq!(got_flags & SF, 0, "BZHI clears SF for zero");
        assert_ne!(got_flags & PF, 0, "BZHI preserves undefined PF");
        assert_ne!(got_flags & AF, 0, "BZHI preserves undefined AF");
        assert_eq!(got_flags & OF, 0, "BZHI clears OF");
    }
    #[test]
    fn smir_pdep_pext_result_ops_preserve_x86_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let flags = 0x2 | 0x1 | 0x40 | 0x80 | 0x800;

        let (value, got_flags) = exec_x86_rax_op(
            OpKind::Pdep {
                dst: rax,
                src: rax,
                mask: rcx,
                width: OpWidth::W64,
            },
            0b101,
            0b0101_0100,
            flags,
        );
        assert_eq!(value, 0b0100_0100);
        assert_eq!(got_flags, flags);

        let (value, got_flags) = exec_x86_rax_op(
            OpKind::Pext {
                dst: rax,
                src: rax,
                mask: rcx,
                width: OpWidth::W64,
            },
            0b0100_0100,
            0b0101_0100,
            flags,
        );
        assert_eq!(value, 0b101);
        assert_eq!(got_flags, flags);
    }
    #[test]
    fn test_vlane_hexagon_vreg_end_to_end() {
        // VLane over a full 128-byte HVX vector: V2.b = vadd(V0.b, V1.b).
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
            hex.set_v(0, [0x0101_0101_0101_0101u64; 16]); // every byte = 1
            hex.set_v(1, [0x0202_0202_0202_0202u64; 16]); // every byte = 2
        }
        let v2 = VReg::Arch(ArchReg::Hexagon(HexagonReg::V(2)));
        let v0 = VReg::Arch(ArchReg::Hexagon(HexagonReg::V(0)));
        let v1 = VReg::Arch(ArchReg::Hexagon(HexagonReg::V(1)));
        let block = SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: OpKind::VLane {
                    dst: v2,
                    src1: v0,
                    src2: v1,
                    elem: VecElementType::I8,
                    lanes: 128,
                    op: VLaneOp::Add,
                    signed: false,
                    set_ovf: false,
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
            assert_eq!(hex.get_v(2), [0x0303_0303_0303_0303u64; 16]); // every byte = 3
        } else {
            panic!("not hexagon");
        }
    }
    #[test]
    fn test_vlane_oversized_lane_count_does_not_abort() {
        // A VLane whose lane count exceeds what fits in the 1024-bit VecValue
        // (here I8 x 200 lanes; lane 128 maps to word index 16) must not index
        // out of bounds and abort the emulator. Lanes within the register are
        // computed; out-of-range lanes have no storage and are dropped.
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
            hex.set_v(0, [0x0101_0101_0101_0101u64; 16]); // every byte = 1
            hex.set_v(1, [0x0202_0202_0202_0202u64; 16]); // every byte = 2
        }
        let v2 = VReg::Arch(ArchReg::Hexagon(HexagonReg::V(2)));
        let v0 = VReg::Arch(ArchReg::Hexagon(HexagonReg::V(0)));
        let v1 = VReg::Arch(ArchReg::Hexagon(HexagonReg::V(1)));
        let block = SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: OpKind::VLane {
                    dst: v2,
                    src1: v0,
                    src2: v1,
                    elem: VecElementType::I8,
                    lanes: 200, // exceeds the 128 byte-lanes the register holds
                    op: VLaneOp::Add,
                    signed: false,
                    set_ovf: false,
                },
                x86_hint: None,
            }],
            terminator: Terminator::Trap {
                kind: TrapKind::Halt,
            },
            exec_count: 0,
        };
        // Must complete without a panic/abort.
        interp.execute_block(&mut ctx, &mut memory, &block);
        if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
            // All 128 in-register byte lanes were computed (1 + 2 = 3).
            assert_eq!(hex.get_v(2), [0x0303_0303_0303_0303u64; 16]);
        } else {
            panic!("not hexagon");
        }
    }
    #[test]
    fn test_vwidenmul_signedness() {
        // Every byte of V0 = 0xFF, V1 = 0x02.
        let v0 = [0xFFFF_FFFF_FFFF_FFFFu64; 16];
        let v1 = [0x0202_0202_0202_0202u64; 16];
        // signed*signed: (-1)*2 = -2 = 0xFFFE per halfword.
        let (lo, _hi) = run_widenmul(v0, v1, VecElementType::I8, true, true);
        assert_eq!(lo, [0xFFFE_FFFE_FFFE_FFFEu64; 16]);
        // unsigned*unsigned: 255*2 = 510 = 0x01FE per halfword.
        let (lo_u, _hi) = run_widenmul(v0, v1, VecElementType::I8, false, false);
        assert_eq!(lo_u, [0x01FE_01FE_01FE_01FEu64; 16]);
    }
    #[test]
    fn test_vnarrowshiftsat_unsigned_clamp() {
        // word->unsigned half, signed src, no shift; negative source clamps to 0,
        // a large positive clamps to 0xFFFF.
        // V0 word = 0xFFFF_FFFF = -1 (signed) -> unsigned sat -> 0.
        // V1 word = 0x0007_FFFF = 524287 -> unsigned half sat -> 0xFFFF.
        let v0 = [0xFFFF_FFFF_FFFF_FFFFu64; 16];
        let v1 = [0x0007_FFFF_0007_FFFFu64; 16];
        let out = run_narrow_shift_sat(v0, v1, 0, VecElementType::I32, true, false, 2);
        // each word = [0x0000 | 0xFFFF<<16] = 0xFFFF_0000
        assert_eq!(out, [0xFFFF_0000_FFFF_0000u64; 16]);
    }
    #[test]
    fn test_vnarrowshiftsat_truncate() {
        // vasrwh (sat=0): no clamp, just truncate low 16 bits after arithmetic >>.
        // src word = 0x0001_8000 = 98304, shift 0 -> low 16 bits = 0x8000.
        let v0 = [0x0001_8000_0001_8000u64; 16];
        let v1 = [0x0001_8000_0001_8000u64; 16];
        let out = run_narrow_shift_sat(v0, v1, 0, VecElementType::I32, true, false, 0);
        assert_eq!(out, [0x8000_8000_8000_8000u64; 16]);
    }
    #[test]
    fn test_vnarrowshiftsat_unsigned_source() {
        // vasruwuh (arith=false): zero-extend the wide source. word = 0xFFFF_FFFF,
        // shift 16, no round -> 0xFFFF_FFFF >> 16 = 0xFFFF, unsigned sat -> 0xFFFF.
        let v0 = [0xFFFF_FFFF_FFFF_FFFFu64; 16];
        let v1 = [0xFFFF_FFFF_FFFF_FFFFu64; 16];
        let out = run_narrow_shift_sat(v0, v1, 16, VecElementType::I32, false, false, 2);
        assert_eq!(out, [0xFFFF_FFFF_FFFF_FFFFu64; 16]);
    }
    #[test]
    fn test_vnarrowshiftv_per_lane() {
        // vasrvwuhsat: pair source (V0=lo even, V1=hi odd), per-sub-lane shift
        // from V2 (Vv.uh), unsigned-half saturate. src word = 0x0000_0100 = 256.
        // amount sub-lane = 4 -> 256>>4 = 16 per half.
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
            hex.set_v(0, [0x0000_0100_0000_0100u64; 16]); // src_lo words = 256
            hex.set_v(1, [0x0000_0100_0000_0100u64; 16]); // src_hi words = 256
            hex.set_v(2, [0x0004_0004_0004_0004u64; 16]); // every uh shamt = 4
        }
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let block = SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: OpKind::VNarrowShiftV {
                    dst: mkv(3),
                    src_lo: mkv(0),
                    src_hi: mkv(1),
                    amount: mkv(2),
                    src_elem: VecElementType::I32,
                    arith: true,
                    round: false,
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
            assert_eq!(hex.get_v(3), [0x0010_0010_0010_0010u64; 16]); // 16 per half
        } else {
            panic!("not hexagon");
        }
    }
    #[test]
    fn test_vlaneunary_ops() {
        let run = |v: [u64; 16], elem: VecElementType, lanes: u8, op: u8| -> [u64; 16] {
            let mut ctx = SmirContext::new_hexagon();
            let mut memory = FlatMemory::new(0x1000);
            let interp = SmirInterpreter::new();
            if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
                hex.set_v(0, v);
            }
            let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
            let block = SmirBlock {
                id: BlockId(0),
                guest_pc: 0x1000,
                phis: vec![],
                ops: vec![SmirOp {
                    id: OpId(0),
                    guest_pc: 0x1000,
                    kind: OpKind::VLaneUnary {
                        dst: mkv(1),
                        src: mkv(0),
                        elem,
                        lanes,
                        op,
                        signed: true,
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
                ArchRegState::Hexagon(hex) => hex.get_v(1),
                _ => panic!("not hexagon"),
            }
        };
        // Not: ~0 = 0xFFFF... (op 0)
        assert_eq!(
            run([0u64; 16], VecElementType::I32, 32, 0),
            [0xFFFF_FFFF_FFFF_FFFFu64; 16]
        );
        // Abs of 0xFFFE (=-2 as i16) per halfword -> 2 (op 1)
        assert_eq!(
            run([0xFFFE_FFFE_FFFE_FFFEu64; 16], VecElementType::I16, 64, 1),
            [0x0002_0002_0002_0002u64; 16]
        );
        // Clz of 0x0001 halfword -> 15 (op 3)
        assert_eq!(
            run([0x0001_0001_0001_0001u64; 16], VecElementType::I16, 64, 3),
            [0x000F_000F_000F_000Fu64; 16]
        );
        // Popcount of 0x00FF halfword -> 8 (op 4)
        assert_eq!(
            run([0x00FF_00FF_00FF_00FFu64; 16], VecElementType::I16, 64, 4),
            [0x0008_0008_0008_0008u64; 16]
        );
        // NormAmt of 0x0001 halfword: max(clz=15, clz(~)=0)-1 = 14 (op 5)
        assert_eq!(
            run([0x0001_0001_0001_0001u64; 16], VecElementType::I16, 64, 5),
            [0x000E_000E_000E_000Eu64; 16]
        );
        // NormAmt of -1 halfword: all redundant sign bits -> 15 (op 5)
        assert_eq!(
            run([0xFFFF_FFFF_FFFF_FFFFu64; 16], VecElementType::I16, 64, 5),
            [0x000F_000F_000F_000Fu64; 16]
        );
    }
    #[test]
    fn test_vreducemul_signed() {
        // signed byte dot product: V0 byte = 0xFF (-1), V1 byte = 2.
        // Each word = 4 * (-1 * 2) = -8 = 0xFFFFFFF8.
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
            hex.set_v(0, [0xFFFF_FFFF_FFFF_FFFFu64; 16]);
            hex.set_v(1, [0x0202_0202_0202_0202u64; 16]);
        }
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let block = SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: OpKind::VReduceMul {
                    dst: mkv(2),
                    src1: mkv(0),
                    src2: mkv(1),
                    src1_elem: VecElementType::I8,
                    src2_elem: VecElementType::I8,
                    out_elem: VecElementType::I32,
                    taps: 4,
                    sat: false,
                    set_ovf: false,
                    signed1: true,
                    signed2: true,
                    acc: false,
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
            assert_eq!(hex.get_v(2), [0xFFFF_FFF8_FFFF_FFF8u64; 16]); // word = -8
        }
    }
    #[test]
    fn test_vwidenext_sequential() {
        // vunpackub: sequential. lo.h[i] = ZE(byte i), hi.h[i] = ZE(byte i+64).
        // All bytes = 0x07 -> every output halfword = 0x0007.
        let (lo, hi) = run_widenext(
            [0x0707_0707_0707_0707u64; 16],
            VecElementType::I8,
            false,
            false,
        );
        assert_eq!(lo, [0x0007_0007_0007_0007u64; 16]);
        assert_eq!(hi, [0x0007_0007_0007_0007u64; 16]);
    }
    #[test]
    fn test_vmaskzero() {
        // vandvqv: Q0 byte0 bit set; src(V0)=0xAA. out.byte0=0xAA, rest 0.
        let mut q = [0u64; 16];
        q[0] = 0x1;
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
            hex.set_v(0, [0xAAAA_AAAA_AAAA_AAAAu64; 16]);
            hex.set_q(0, q);
        }
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let mkblock = |negate| SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: OpKind::VMaskZero {
                    dst: mkv(2),
                    mask_q: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(0))),
                    src: mkv(0),
                    negate,
                    oracc: false,
                },
                x86_hint: None,
            }],
            terminator: Terminator::Trap {
                kind: TrapKind::Halt,
            },
            exec_count: 0,
        };
        interp.execute_block(&mut ctx, &mut memory, &mkblock(false));
        if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
            assert_eq!(hex.get_v(2)[0], 0x0000_0000_0000_00AA); // byte0 = 0xAA, rest 0
            assert_eq!(hex.get_v(2)[1], 0);
        }
        // negate: byte0 -> 0, all other bytes -> 0xAA.
        interp.execute_block(&mut ctx, &mut memory, &mkblock(true));
        if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
            assert_eq!(hex.get_v(2)[0], 0xAAAA_AAAA_AAAA_AA00);
            assert_eq!(hex.get_v(2)[1], 0xAAAA_AAAA_AAAA_AAAA);
        }
    }
    #[test]
    fn test_vmulshiftsat_vmpyhvsrs() {
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let op = |dst, s1, s2| OpKind::VMulShiftSat {
            dst,
            src1: s1,
            src2: s2,
            src_elem: VecElementType::I16,
            lanes: 64,
            signed1: true,
            signed2: true,
            shift_left: 1,
            round: true,
            sat_bits: 32,
            out_shift: 16,
        };
        // non-saturating: 0x4000*0x4000<<1 +0x8000 = 0x20008000; >>16 = 0x2000.
        let out = run_vec2(
            [0x4000_4000_4000_4000u64; 16],
            [0x4000_4000_4000_4000u64; 16],
            op(mkv(2), mkv(0), mkv(1)),
        );
        assert_eq!(out, [0x2000_2000_2000_2000u64; 16]);
        // saturating: (-32768)^2<<1 +0x8000 overflows i32 -> clamp 0x7FFFFFFF; >>16 = 0x7FFF.
        let out2 = run_vec2(
            [0x8000_8000_8000_8000u64; 16],
            [0x8000_8000_8000_8000u64; 16],
            op(mkv(2), mkv(0), mkv(1)),
        );
        assert_eq!(out2, [0x7FFF_7FFF_7FFF_7FFFu64; 16]);
    }
    #[test]
    fn test_vmulshiftsat_vmpyuhvs() {
        // unsigned 16x16, no shift/round/sat, take high 16: 0xFFFF*0xFFFF>>16 = 0xFFFE.
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let out = run_vec2(
            [0xFFFF_FFFF_FFFF_FFFFu64; 16],
            [0xFFFF_FFFF_FFFF_FFFFu64; 16],
            OpKind::VMulShiftSat {
                dst: mkv(2),
                src1: mkv(0),
                src2: mkv(1),
                src_elem: VecElementType::I16,
                lanes: 64,
                signed1: false,
                signed2: false,
                shift_left: 0,
                round: false,
                sat_bits: 0,
                out_shift: 16,
            },
        );
        assert_eq!(out, [0xFFFE_FFFE_FFFE_FFFEu64; 16]);
    }
    #[test]
    fn test_valign_right_shift4() {
        // valignb shift=4: out[i] = i+4<128 ? Vv[i+4] : Vu[i+4-128].
        // Vu(V0) all 0xAA, Vv(V1) all 0xBB -> bytes 0..123 = 0xBB, 124..127 = 0xAA.
        let v0 = [0xAAAA_AAAA_AAAA_AAAAu64; 16]; // Vu
        let v1 = [0xBBBB_BBBB_BBBB_BBBBu64; 16]; // Vv
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let out = run_vec2(
            v0,
            v1,
            OpKind::VAlign {
                dst: mkv(2),
                src1: mkv(0),
                src2: mkv(1),
                amount: SrcOperand::Imm(4),
                left: false,
            },
        );
        assert_eq!(out[0], 0xBBBB_BBBB_BBBB_BBBBu64); // bytes 0-7 from Vv
        // bytes 120-123 = 0xBB (from Vv), 124-127 = 0xAA (wrapped from Vu)
        assert_eq!(out[15], 0xAAAA_AAAA_BBBB_BBBBu64);
    }
    #[test]
    fn test_valign_vror() {
        // vror = VAlign(src,src,Rt,left=false): out[i] = src[(i+amt)&127].
        // Distinguishable: V0 lane0 low byte = 0x11, all else 0. amt=127 -> rotate so
        // the byte at index 0 moves to index 1 (out[127]=src[(127+127)&127]=src[126]=0,
        // out[0]=src[127]=0, ... out[1]=src[(1+127)&127]=src[0]=0x11).
        let mut v0 = [0u64; 16];
        v0[0] = 0x11; // byte 0 = 0x11
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let out = run_vec2(
            v0,
            v0,
            OpKind::VAlign {
                dst: mkv(2),
                src1: mkv(0),
                src2: mkv(0),
                amount: SrcOperand::Imm(127),
                left: false,
            },
        );
        // out byte 1 = src byte 0 = 0x11; everything else 0.
        assert_eq!(out[0], 0x0000_0000_0000_1100u64); // byte1 = 0x11
        for w in &out[1..] {
            assert_eq!(*w, 0);
        }
    }
    #[test]
    fn test_vmaskzero_oracc() {
        // vandqrt_acc: V2 |= (Q0 ? src : 0). V2 prior = 0x0F per byte;
        // src = 0xF0 per byte; Q0 byte0 set -> byte0 = 0x0F|0xF0=0xFF, others 0x0F.
        let mut q = [0u64; 16];
        q[0] = 0b01;
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
            hex.set_v(2, [0x0F0F_0F0F_0F0F_0F0Fu64; 16]); // dst prior
            hex.set_v(0, [0xF0F0_F0F0_F0F0_F0F0u64; 16]); // src
            hex.set_q(0, q);
        }
        let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
        let block = SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: OpKind::VMaskZero {
                    dst: mkv(2),
                    mask_q: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(0))),
                    src: mkv(0),
                    negate: false,
                    oracc: true,
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
            let v = hex.get_v(2);
            assert_eq!(v[0] & 0xff, 0xFF); // byte0 OR'd
            assert_eq!((v[0] >> 8) & 0xff, 0x0F); // byte1 unchanged
        }
    }
