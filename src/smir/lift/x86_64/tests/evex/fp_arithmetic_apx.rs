//! APX-extended memory-address coverage for EVEX floating-point arithmetic.

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

fn assert_sib_address(
    bytes: &[u8],
    expected_base: X86Reg,
    expected_index: X86Reg,
    expected_disp: i32,
) -> LiftResult {
    let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(
        lifted.ops.iter().any(|op| {
            matches!(
                &op.kind,
                OpKind::Lea {
                    addr:
                        Address::BaseIndexScale {
                            base: Some(VReg::Arch(ArchReg::X86(base))),
                            index: VReg::Arch(ArchReg::X86(index)),
                            scale: 2,
                            disp,
                            disp_size: DispSize::Disp8,
                        },
                    ..
                }
                    | OpKind::PredLoad {
                        addr:
                            Address::BaseIndexScale {
                                base: Some(VReg::Arch(ArchReg::X86(base))),
                                index: VReg::Arch(ArchReg::X86(index)),
                                scale: 2,
                                disp,
                                disp_size: DispSize::Disp8,
                            },
                        ..
                    }
                    if *base == expected_base
                        && *index == expected_index
                        && *disp == expected_disp
            )
        }),
        "{bytes:02X?}: {:#?}",
        lifted.ops
    );
    lifted
}

#[test]
fn packed_evex_fp_arithmetic_preserves_apx_b4_x4_memory_address_bits() {
    // VADDPS xmm16{k1},xmm17,[base+index*2+16]. EVEX disp8 is
    // compressed by the 16-byte full-vector tuple.
    for (p0, p1, expected_base, expected_index) in [
        (0xE9, 0x70, X86Reg::R16, X86Reg::R17),
        (0xE9, 0x74, X86Reg::R16, X86Reg::Rcx),
        (0xE1, 0x70, X86Reg::Rax, X86Reg::R17),
    ] {
        let bytes = [0x62, p0, p1, 0x01, 0x58, 0x44, 0x48, 0x01];
        let lifted = assert_sib_address(&bytes, expected_base, expected_index, 16);
        assert!(
            lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86FpBinary {
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                    elem: VecElementType::F32,
                    lanes: 4,
                    op: X86FpBinaryOp::Add,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    ..
                }
            )),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn scalar_evex_fp_arithmetic_preserves_apx_b4_x4_memory_address_bits() {
    // VADDSD xmm16{k1},xmm17,[r16+r17*2+8]. EVEX disp8 is compressed
    // by the 8-byte scalar tuple.
    let bytes = [0x62, 0xE9, 0xF3, 0x01, 0x58, 0x44, 0x48, 0x01];
    let lifted = assert_sib_address(&bytes, X86Reg::R16, X86Reg::R17, 8);
    assert!(lifted.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86FpBinary {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            mask: Some(_),
            elem: VecElementType::F64,
            lanes: 1,
            op: X86FpBinaryOp::Add,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            ..
        }
    )));
}
