//! Exact source-byte replay classification for AVX VEX scalar `VMOVQ`.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Load,
    Store,
}

impl Direction {
    const ALL: [Self; 2] = [Self::Load, Self::Store];

    fn pp(self) -> u8 {
        match self {
            Self::Load => 2,
            Self::Store => 1,
        }
    }

    fn opcode(self) -> u8 {
        match self {
            Self::Load => 0x7E,
            Self::Store => 0xD6,
        }
    }

    fn reg_rm(self, destination: u8, source: u8) -> (u8, u8) {
        match self {
            Self::Load => (destination, source),
            Self::Store => (source, destination),
        }
    }
}

fn c5_encoding(direction: Direction, destination: u8, source: u8) -> [u8; 4] {
    assert!(destination < 16 && source < 16);
    let (reg, rm) = direction.reg_rm(destination, source);
    assert!(rm < 8);
    [
        0xC5,
        (if reg < 8 { 0x80 } else { 0 }) | 0x78 | direction.pp(),
        direction.opcode(),
        0xC0 | ((reg & 7) << 3) | rm,
    ]
}

fn c4_encoding(
    direction: Direction,
    w: bool,
    ignored_x: bool,
    destination: u8,
    source: u8,
) -> [u8; 5] {
    assert!(destination < 16 && source < 16);
    let (reg, rm) = direction.reg_rm(destination, source);
    let mut p0 = 0xE1;
    if reg >= 8 {
        p0 &= !0x80;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    if rm >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (u8::from(w) << 7) | 0x78 | direction.pp(),
        direction.opcode(),
        0xC0 | ((reg & 7) << 3) | (rm & 7),
    ]
}

#[test]
fn classifier_covers_all_2304_register_encodings_and_destinations() {
    let mut classified = 0usize;
    for direction in Direction::ALL {
        for destination in 0..16 {
            for source in 0..16 {
                let (_, rm) = direction.reg_rm(destination, source);
                if rm < 8 {
                    let bytes = c5_encoding(direction, destination, source);
                    let instruction = X86InstructionBytes::new(&bytes).unwrap();
                    assert!(instruction.is_vex_register_scalar_vmovq(), "{bytes:02X?}");
                    assert_eq!(
                        instruction.vex_register_scalar_vmovq_destination_index(),
                        Some(destination),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
                for w in [false, true] {
                    for ignored_x in [false, true] {
                        let bytes = c4_encoding(direction, w, ignored_x, destination, source);
                        let instruction = X86InstructionBytes::new(&bytes).unwrap();
                        assert!(instruction.is_vex_register_scalar_vmovq(), "{bytes:02X?}");
                        assert_eq!(
                            instruction.vex_register_scalar_vmovq_destination_index(),
                            Some(destination),
                            "{bytes:02X?}"
                        );
                        classified += 1;
                    }
                }
            }
        }
    }
    assert_eq!(classified, 2_304);
}

#[test]
fn classifier_exhausts_prefix_opcode_modrm_and_exact_shape_frontiers() {
    let base = c4_encoding(Direction::Load, false, false, 1, 2);
    for p0 in u8::MIN..=u8::MAX {
        let mut bytes = base;
        bytes[1] = p0;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_scalar_vmovq(),
            p0 & 0x1F == 1,
            "{bytes:02X?}"
        );
    }
    for p1 in u8::MIN..=u8::MAX {
        let mut bytes = base;
        bytes[2] = p1;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_scalar_vmovq(),
            p1 & 0x7F == 0x7A,
            "{bytes:02X?}"
        );
    }
    for opcode in u8::MIN..=u8::MAX {
        let mut bytes = base;
        bytes[3] = opcode;
        let expected = matches!((bytes[2] & 3, opcode), (2, 0x7E) | (1, 0xD6));
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_scalar_vmovq(),
            expected,
            "{bytes:02X?}"
        );
    }
    for modrm in u8::MIN..=u8::MAX {
        let mut bytes = base;
        bytes[4] = modrm;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_scalar_vmovq(),
            modrm >> 6 == 3,
            "{bytes:02X?}"
        );
    }

    let compact = c5_encoding(Direction::Store, 2, 9);
    for p1 in u8::MIN..=u8::MAX {
        let mut bytes = compact;
        bytes[1] = p1;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_scalar_vmovq(),
            p1 & 0x7F == 0x79,
            "{bytes:02X?}"
        );
    }

    for bytes in [
        &base[..4],
        &[base[0], base[1], base[2], base[3], base[4], 0][..],
        &[0xC5, 0xFA, 0x7E][..],
        &[0xC5, 0xFA, 0x7E, 0xCA, 0][..],
        &[0x62, 0xF1, 0xFE, 0x08, 0x7E, 0xCA][..],
        &[0xC4, 0xE2, 0x7A, 0x7E, 0xCA][..],
        &[0xC4, 0xE1, 0x72, 0x7E, 0xCA][..],
        &[0xC4, 0xE1, 0x7E, 0x7E, 0xCA][..],
        &[0xC4, 0xE1, 0x79, 0x7E, 0xCA][..],
        &[0xC4, 0xE1, 0x7A, 0xD6, 0xCA][..],
        &[0xC4, 0xE1, 0x7A, 0x7D, 0xCA][..],
        &[0xC4, 0xE1, 0x7A, 0x7E, 0x0A][..],
    ] {
        assert!(
            !X86InstructionBytes::new(bytes)
                .unwrap()
                .is_vex_register_scalar_vmovq(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn llvm_samples_and_replay_spans_preserve_exact_bytes() {
    let pc = 0x7ED6;
    let mut block = SmirBlock::new(BlockId(59), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    // LLVM 23.0.0 independently assembled the two 7Eh samples. The D6h
    // samples exercise the architecturally equivalent opposite opcode
    // direction from Intel SDM Vol. 2D, VMOVQ.
    for bytes in [
        &[0xC5, 0xFA, 0x7E, 0xCA][..],
        &[0xC4, 0x41, 0x7A, 0x7E, 0xCA][..],
        &[0xC5, 0xF9, 0xD6, 0xCA][..],
        &[0xC4, 0x41, 0x79, 0xD6, 0xCA][..],
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert!(instruction.is_vex_register_scalar_vmovq(), "{bytes:02X?}");
        let provenance = HashMap::from([((BlockId(59), pc), instruction)]);
        for spans in [
            x86_vex_scalar_vmovq_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            assert_eq!(
                spans.get(&0),
                Some(&X86NativeReplaySpan {
                    end: 1,
                    instruction,
                    needs_avx512vl: false,
                    needs_avx512dq: false,
                    needs_avx512fp16: false,
                    preserve_mxcsr_de: false,
                }),
                "{bytes:02X?}"
            );
        }
    }
    assert!(x86_vex_scalar_vmovq_replay_spans(&block, &HashMap::new()).is_empty());
}
