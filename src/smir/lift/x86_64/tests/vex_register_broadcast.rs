//! Exhaustive strict-lift coverage for register-source AVX2 VEX scalar
//! broadcasts.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BroadcastKind {
    Float32,
    Float64,
    Integer8,
    Integer16,
    Integer32,
    Integer64,
}

impl BroadcastKind {
    fn opcode(self) -> u8 {
        match self {
            Self::Float32 => 0x18,
            Self::Float64 => 0x19,
            Self::Integer8 => 0x78,
            Self::Integer16 => 0x79,
            Self::Integer32 => 0x58,
            Self::Integer64 => 0x59,
        }
    }

    fn element(self) -> VecElementType {
        match self {
            Self::Float32 => VecElementType::F32,
            Self::Float64 => VecElementType::F64,
            Self::Integer8 => VecElementType::I8,
            Self::Integer16 => VecElementType::I16,
            Self::Integer32 => VecElementType::I32,
            Self::Integer64 => VecElementType::I64,
        }
    }

    fn element_bits(self) -> usize {
        usize::try_from(self.element().bytes() * 8).unwrap()
    }
}

const SHAPES: [(BroadcastKind, bool); 11] = [
    (BroadcastKind::Float32, false),
    (BroadcastKind::Float32, true),
    (BroadcastKind::Float64, true),
    (BroadcastKind::Integer8, false),
    (BroadcastKind::Integer8, true),
    (BroadcastKind::Integer16, false),
    (BroadcastKind::Integer16, true),
    (BroadcastKind::Integer32, false),
    (BroadcastKind::Integer32, true),
    (BroadcastKind::Integer64, false),
    (BroadcastKind::Integer64, true),
];

fn xmm(register: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(register)))
}

fn destination(register: u8, ymm: bool) -> VReg {
    VReg::Arch(ArchReg::X86(if ymm {
        X86Reg::Ymm(register)
    } else {
        X86Reg::Xmm(register)
    }))
}

fn encoding(kind: BroadcastKind, ymm: bool, ignored_x: bool, dst: u8, src: u8) -> [u8; 5] {
    assert!(dst < 16 && src < 16);
    let mut p0 = 0xE2;
    if dst >= 8 {
        p0 &= !0x80;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    if src >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        0x79 | (u8::from(ymm) << 2),
        kind.opcode(),
        0xC0 | ((dst & 7) << 3) | (src & 7),
    ]
}

fn assert_exact_register_lift(bytes: &[u8], kind: BroadcastKind, ymm: bool, dst: u8, src: u8) {
    let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert_eq!(lifted.ops.len(), 3, "{bytes:02X?}: {:#?}", lifted.ops);

    let OpKind::VExtractLane {
        dst: scalar,
        vec,
        lane,
        elem,
        sign,
    } = &lifted.ops[0].kind
    else {
        panic!("{bytes:02X?}: {:#?}", lifted.ops)
    };
    assert_eq!(*vec, xmm(src), "{bytes:02X?}");
    assert_eq!(*lane, 0, "{bytes:02X?}");
    assert_eq!(*elem, kind.element(), "{bytes:02X?}");
    assert_eq!(*sign, SignExtend::Zero, "{bytes:02X?}");

    let OpKind::VBroadcast {
        dst: broadcast,
        scalar: broadcast_scalar,
        elem,
        lanes,
    } = &lifted.ops[1].kind
    else {
        panic!("{bytes:02X?}: {:#?}", lifted.ops)
    };
    assert_eq!(*broadcast_scalar, *scalar, "{bytes:02X?}");
    assert_eq!(*elem, kind.element(), "{bytes:02X?}");
    assert_eq!(
        usize::from(*lanes),
        (if ymm { 256 } else { 128 }) / kind.element_bits(),
        "{bytes:02X?}"
    );

    let OpKind::VMov {
        dst: actual_dst,
        src,
        width,
    } = &lifted.ops[2].kind
    else {
        panic!("{bytes:02X?}: {:#?}", lifted.ops)
    };
    assert_eq!(*actual_dst, destination(dst, ymm), "{bytes:02X?}");
    assert_eq!(*src, *broadcast, "{bytes:02X?}");
    assert_eq!(
        *width,
        if ymm { VecWidth::V256 } else { VecWidth::V128 },
        "{bytes:02X?}"
    );
}

#[test]
fn all_5632_structural_register_samples_strictly_lift_with_exact_operands() {
    let mut lifted = 0usize;
    for (kind, ymm) in SHAPES {
        for ignored_x in [false, true] {
            for dst in 0u8..16 {
                for src in 0u8..16 {
                    let bytes = encoding(kind, ymm, ignored_x, dst, src);
                    assert_exact_register_lift(&bytes, kind, ymm, dst, src);
                    lifted += 1;
                }
            }
        }
    }
    assert_eq!(lifted, 5_632);
}

#[test]
fn reserved_vvvv_w1_and_float64_xmm_are_precise_invalid_encodings() {
    let mut rejected = 0usize;
    for (kind, ymm) in SHAPES {
        for ignored_x in [false, true] {
            let base = encoding(kind, ymm, ignored_x, 9, 11);
            for raw_vvvv in 0u8..15 {
                let mut bytes = base;
                bytes[2] = (bytes[2] & !0x78) | (raw_vvvv << 3);
                assert!(
                    matches!(lift_single(&bytes), Err(LiftError::InvalidEncoding { .. })),
                    "{bytes:02X?}"
                );
                assert!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vex_register_broadcast_element_bits()
                        .is_none(),
                    "{bytes:02X?}"
                );
                rejected += 1;
            }

            let mut w1 = base;
            w1[2] |= 0x80;
            assert!(
                matches!(lift_single(&w1), Err(LiftError::InvalidEncoding { .. })),
                "{w1:02X?}"
            );
            assert!(
                X86InstructionBytes::new(&w1)
                    .unwrap()
                    .vex_register_broadcast_element_bits()
                    .is_none(),
                "{w1:02X?}"
            );
            rejected += 1;
        }
    }
    assert_eq!(rejected, 352);

    for ignored_x in [false, true] {
        for dst in [1, 9] {
            for src in [3, 11] {
                let bytes = encoding(BroadcastKind::Float64, false, ignored_x, dst, src);
                assert!(
                    matches!(lift_single(&bytes), Err(LiftError::InvalidEncoding { .. })),
                    "{bytes:02X?}"
                );
                assert!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vex_register_broadcast_element_bits()
                        .is_none(),
                    "{bytes:02X?}"
                );
            }
        }
    }
}

#[test]
fn representative_memory_forms_lift_but_never_enter_native_replay() {
    let cases: &[&[u8]] = &[
        &[0xC4, 0xE2, 0x79, 0x18, 0x00],
        &[0xC4, 0xE2, 0x7D, 0x19, 0x48, 0x20],
        &[0xC4, 0xE2, 0x79, 0x78, 0x10],
        &[0xC4, 0xE2, 0x7D, 0x79, 0x18],
        &[0xC4, 0xE2, 0x79, 0x58, 0x20],
        &[0xC4, 0xE2, 0x7D, 0x59, 0x28],
    ];
    for &bytes in cases {
        let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(
            lifted
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::Load { .. })),
            "{bytes:02X?}: {:#?}",
            lifted.ops
        );
        assert!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_register_broadcast_element_bits()
                .is_none(),
            "{bytes:02X?}"
        );
    }
}
