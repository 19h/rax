//! Exact source-byte replay classification for AVX VEX packed bit tests.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestKind {
    Vptest,
    Vtestps,
    Vtestpd,
}

impl TestKind {
    const ALL: [Self; 3] = [Self::Vptest, Self::Vtestps, Self::Vtestpd];

    fn opcode(self) -> u8 {
        match self {
            Self::Vptest => 0x17,
            Self::Vtestps => 0x0E,
            Self::Vtestpd => 0x0F,
        }
    }

    fn valid_w_values(self) -> &'static [bool] {
        match self {
            Self::Vptest => &[false, true],
            Self::Vtestps | Self::Vtestpd => &[false],
        }
    }
}

fn encoding(
    kind: TestKind,
    w: bool,
    wide: bool,
    ignored_x: bool,
    first: u8,
    second: u8,
) -> [u8; 5] {
    assert!(first < 16 && second < 16);
    assert!(kind == TestKind::Vptest || !w);
    let mut p0 = 0xE2;
    if first >= 8 {
        p0 &= !0x80;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    if second >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (u8::from(w) << 7) | 0x79 | (u8::from(wide) << 2),
        kind.opcode(),
        0xC0 | ((first & 7) << 3) | (second & 7),
    ]
}

#[test]
fn classifier_covers_all_4096_legal_register_encodings() {
    let mut classified = 0usize;
    for kind in TestKind::ALL {
        for &w in kind.valid_w_values() {
            for wide in [false, true] {
                for ignored_x in [false, true] {
                    for first in 0..16 {
                        for second in 0..16 {
                            let bytes = encoding(kind, w, wide, ignored_x, first, second);
                            assert!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .is_vex_register_ptest(),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 4_096);
}

#[test]
fn classifier_exhausts_prefix_opcode_modrm_and_exact_shape_frontiers() {
    let base = encoding(TestKind::Vptest, false, false, false, 1, 2);
    for p0 in u8::MIN..=u8::MAX {
        let mut bytes = base;
        bytes[1] = p0;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_ptest(),
            p0 & 0x1F == 2,
            "{bytes:02X?}"
        );
    }
    for p1 in u8::MIN..=u8::MAX {
        let mut bytes = base;
        bytes[2] = p1;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_ptest(),
            p1 & 0x7B == 0x79,
            "{bytes:02X?}"
        );
    }
    for opcode in u8::MIN..=u8::MAX {
        let mut bytes = base;
        bytes[3] = opcode;
        let expected = match opcode {
            0x0E | 0x0F => bytes[2] & 0x80 == 0,
            0x17 => true,
            _ => false,
        };
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_ptest(),
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
                .is_vex_register_ptest(),
            modrm >> 6 == 3,
            "{bytes:02X?}"
        );
    }

    let vtestps = encoding(TestKind::Vtestps, false, true, true, 15, 14);
    for p1 in u8::MIN..=u8::MAX {
        let mut bytes = vtestps;
        bytes[2] = p1;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_ptest(),
            p1 & 0xFB == 0x79,
            "{bytes:02X?}"
        );
    }

    for bytes in [
        &base[..4],
        &[base[0], base[1], base[2], base[3], base[4], 0][..],
        &[0xC5, 0xF9, 0x17, 0xCA][..],
        &[0x62, 0xF2, 0x7D, 0x08, 0x17, 0xCA][..],
        &[0xC4, 0xE1, 0x79, 0x17, 0xCA][..],
        &[0xC4, 0xE2, 0x71, 0x17, 0xCA][..],
        &[0xC4, 0xE2, 0x78, 0x17, 0xCA][..],
        &[0xC4, 0xE2, 0xF9, 0x0E, 0xCA][..],
        &[0xC4, 0xE2, 0xF9, 0x0F, 0xCA][..],
        &[0xC4, 0xE2, 0x79, 0x16, 0xCA][..],
        &[0xC4, 0xE2, 0x79, 0x17, 0x0A][..],
    ] {
        assert!(
            !X86InstructionBytes::new(bytes)
                .unwrap()
                .is_vex_register_ptest(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn llvm_samples_and_replay_spans_preserve_exact_bytes() {
    let pc = 0x170E;
    let mut block = SmirBlock::new(BlockId(58), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    // LLVM 23.0.0 independently assembled these low- and high-register
    // 128-/256-bit samples.
    for bytes in [
        &[0xC4, 0xE2, 0x79, 0x17, 0xCA][..],
        &[0xC4, 0x42, 0x7D, 0x17, 0xCA][..],
        &[0xC4, 0xE2, 0x79, 0x0E, 0xDC][..],
        &[0xC4, 0x42, 0x7D, 0x0E, 0xDC][..],
        &[0xC4, 0xE2, 0x79, 0x0F, 0xEE][..],
        &[0xC4, 0x42, 0x7D, 0x0F, 0xEE][..],
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert!(instruction.is_vex_register_ptest(), "{bytes:02X?}");
        let provenance = HashMap::from([((BlockId(58), pc), instruction)]);
        for spans in [
            x86_vex_ptest_replay_spans(&block, &provenance),
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
    assert!(x86_vex_ptest_replay_spans(&block, &HashMap::new()).is_empty());
}
