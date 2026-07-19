//! simd::sse tests

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn interprets_vshufflebitqm_writes_k_mask() {
    let src = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
    let indices = VReg::Arch(ArchReg::X86(X86Reg::Zmm(3)));
    let dst = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = vec_from_bytes(&[
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00,
            0xff, 0x00,
        ]);
        x86.xmm[3] = vec_from_bytes(&[0, 63, 1, 62, 2, 61, 3, 60, 8, 15, 16, 47, 48, 55, 56, 63]);
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
                OpKind::VShuffleBitQM {
                    dst,
                    src,
                    indices,
                    mask: None,
                    width: VecWidth::V128,
                },
            ),
        )
        .unwrap();

    assert_eq!(ctx.read_vreg(dst), 0x3303);
    assert_eq!(ctx.read_vreg(src), 0x8000_0000_0000_0001);
    assert_eq!(ctx.read_vreg(indices), 0x3c033d023e013f00);
}
#[test]
fn test_vblend_mux() {
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();
    // Q0 = byte0 bit set only; src_true(V0)=0xAA, src_false(V1)=0xBB.
    let mut q = [0u64; 16];
    q[0] = 0x1; // only byte 0
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        hex.set_v(0, [0xAAAA_AAAA_AAAA_AAAAu64; 16]);
        hex.set_v(1, [0xBBBB_BBBB_BBBB_BBBBu64; 16]);
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
            kind: OpKind::VBlend {
                dst: mkv(2),
                mask_q: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(0))),
                src_true: mkv(0),
                src_false: mkv(1),
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
        // byte0 = 0xAA (Q bit set), bytes 1-7 = 0xBB.
        assert_eq!(hex.get_v(2)[0], 0xBBBB_BBBB_BBBB_BBAA);
        assert_eq!(hex.get_v(2)[1], 0xBBBB_BBBB_BBBB_BBBB);
    }
}
