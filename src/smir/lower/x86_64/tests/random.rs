//! Exact native encoding and state-bridge coverage for `RDRAND`/`RDSEED`.

use super::*;
use crate::smir::VirtualId;

fn gpr(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

fn random(dst: VReg, width: OpWidth, seed: bool) -> OpKind {
    OpKind::X86Random { dst, width, seed }
}

fn native_encoding(width: OpWidth, seed: bool, destination: u8) -> Vec<u8> {
    let mut expected = Vec::with_capacity(5);
    if width == OpWidth::W16 {
        expected.push(0x66);
    }
    if width == OpWidth::W64 || destination >= 8 {
        expected.push(0x40 | u8::from(width == OpWidth::W64) * 8 | u8::from(destination >= 8));
    }
    expected.extend([
        0x0F,
        0xC7,
        0xC0 | (if seed { 7 } else { 6 }) << 3 | destination & 7,
    ]);
    expected
}

#[test]
fn random_lowering_emits_exact_identity_and_state_backed_encodings() {
    for seed in [false, true] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let identity = lower_single_op(random(gpr(9), width, seed));
            let expected = native_encoding(width, seed, 9);
            assert!(
                identity
                    .windows(expected.len())
                    .any(|bytes| bytes == expected),
                "identity seed={seed} {width:?}: {identity:02X?}"
            );

            for destination in [4u8, 5, 16, 31] {
                let state = lower_single_op(random(gpr(destination), width, seed));
                let expected = native_encoding(width, seed, 2);
                assert!(
                    state.windows(expected.len()).any(|bytes| bytes == expected),
                    "state GPR{destination} seed={seed} {width:?}: {state:02X?}"
                );

                let slot_offset = i32::from(destination) * 8;
                let slot_offset_bytes = slot_offset.to_le_bytes();
                assert!(
                    destination < 16 || state.windows(4).any(|bytes| bytes == slot_offset_bytes),
                    "state GPR{destination} slot displacement absent: {state:02X?}"
                );
                if destination == 5 {
                    let saved_rbp = if width == OpWidth::W16 {
                        &[0x66, 0x89, 0x55, 0x00][..]
                    } else {
                        &[0x48, 0x89, 0x55, 0x00][..]
                    };
                    assert!(
                        state
                            .windows(saved_rbp.len())
                            .any(|bytes| bytes == saved_rbp),
                        "guest RBP synchronization absent: {state:02X?}"
                    );
                }
            }
        }
    }
}

#[test]
fn random_lowering_rejects_every_unmodeled_shape() {
    let malformed = [
        random(VReg::Virtual(VirtualId(9)), OpWidth::W64, false),
        random(
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            OpWidth::W64,
            false,
        ),
        random(gpr(9), OpWidth::W8, false),
        random(gpr(9), OpWidth::W128, true),
    ];
    for kind in malformed {
        assert!(matches!(
            lower_single_op_err(kind),
            LowerError::InvalidOperand { .. }
        ));
    }

    for destination in [4u8, 9, 16] {
        assert!(matches!(
            lower_single_hinted_op_err(
                random(gpr(destination), OpWidth::W64, false),
                X86OpHint::RexByteReg,
            ),
            LowerError::InvalidOperand { .. } | LowerError::InvalidRegister(_)
        ));
    }
}
