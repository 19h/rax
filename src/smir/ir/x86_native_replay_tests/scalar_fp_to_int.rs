//! Exact classifier tests for VEX/EVEX scalar floating-point-to-integer replay.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceFormat {
    F32,
    F64,
    F16,
}

impl SourceFormat {
    const ALL: [Self; 3] = [Self::F32, Self::F64, Self::F16];

    fn fields(self) -> (u8, u8, bool) {
        match self {
            Self::F32 => (1, 2, false),
            Self::F64 => (1, 3, false),
            Self::F16 => (5, 2, true),
        }
    }
}

fn encoding(
    format: SourceFormat,
    signed: bool,
    truncate: bool,
    w: bool,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    source: u8,
) -> [u8; 6] {
    assert!(ll < 4 && destination < 16 && source < 32);
    let (map, pp, _) = format.fields();
    let opcode = match (signed, truncate) {
        (true, false) => 0x2D,
        (true, true) => 0x2C,
        (false, false) => 0x79,
        (false, true) => 0x78,
    };
    let mut p0 = 0xF0 | map;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7C | pp | if w { 0x80 } else { 0 },
        (ll << 5) | if embedded_control { 0x10 } else { 0 } | 0x08,
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

fn valid_control(truncate: bool, ll: u8, embedded_control: bool) -> bool {
    ll != 3 || (embedded_control && !truncate)
}

#[test]
fn classifier_accepts_exactly_3_120_sampled_legal_register_encodings() {
    let destinations = [0u8, 3, 8, 12, 15];
    let sources = [0u8, 8, 16, 31];
    let mut classified = 0usize;

    for format in SourceFormat::ALL {
        for signed in [false, true] {
            for truncate in [false, true] {
                for w in [false, true] {
                    for ll in 0..=3 {
                        for embedded_control in [false, true] {
                            for destination in destinations {
                                for source in sources {
                                    let bytes = encoding(
                                        format,
                                        signed,
                                        truncate,
                                        w,
                                        ll,
                                        embedded_control,
                                        destination,
                                        source,
                                    );
                                    let expected = valid_control(truncate, ll, embedded_control)
                                        .then_some(format.fields().2);
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_scalar_fp_to_int_requires_fp16(),
                                        expected,
                                        "{format:?} {bytes:02X?}"
                                    );
                                    classified += usize::from(expected.is_some());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 3_120);

    // Independently assembled by LLVM 21.1.8. Collectively these exercise all
    // 12 mnemonics, W0/W1, dynamic/embedded controls, high GPRs, and XMM16-31.
    for (bytes, needs_fp16) in [
        ([0x62, 0x31, 0x7E, 0x08, 0x2D, 0xCA], false),
        ([0x62, 0x31, 0xFE, 0x38, 0x2D, 0xCA], false),
        ([0x62, 0x11, 0x7E, 0x08, 0x2C, 0xD0], false),
        ([0x62, 0x11, 0xFE, 0x18, 0x2C, 0xD0], false),
        ([0x62, 0x11, 0x7F, 0x08, 0x2D, 0xDF], false),
        ([0x62, 0x11, 0xFF, 0x58, 0x2D, 0xDF], false),
        ([0x62, 0x31, 0x7F, 0x08, 0x2C, 0xE0], false),
        ([0x62, 0x31, 0xFF, 0x18, 0x2C, 0xE0], false),
        ([0x62, 0x31, 0x7E, 0x08, 0x79, 0xE9], false),
        ([0x62, 0x31, 0xFE, 0x78, 0x79, 0xE9], false),
        ([0x62, 0x11, 0x7E, 0x08, 0x78, 0xF1], false),
        ([0x62, 0x11, 0xFE, 0x18, 0x78, 0xF1], false),
        ([0x62, 0x11, 0x7F, 0x08, 0x79, 0xFA], false),
        ([0x62, 0x11, 0xFF, 0x18, 0x79, 0xFA], false),
        ([0x62, 0x91, 0x7F, 0x08, 0x78, 0xC3], false),
        ([0x62, 0x91, 0xFF, 0x18, 0x78, 0xC3], false),
        ([0x62, 0x15, 0x7E, 0x08, 0x2D, 0xC4], true),
        ([0x62, 0x15, 0xFE, 0x38, 0x2D, 0xC4], true),
        ([0x62, 0x15, 0x7E, 0x08, 0x2C, 0xCD], true),
        ([0x62, 0x15, 0xFE, 0x18, 0x2C, 0xCD], true),
        ([0x62, 0x15, 0x7E, 0x08, 0x79, 0xD6], true),
        ([0x62, 0x15, 0xFE, 0x58, 0x79, 0xD6], true),
        ([0x62, 0x15, 0x7E, 0x08, 0x78, 0xDF], true),
        ([0x62, 0x15, 0xFE, 0x18, 0x78, 0xDF], true),
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_fp_to_int_requires_fp16(),
            Some(needs_fp16),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_rejects_every_reserved_or_unsafe_frontier() {
    let canonical = encoding(SourceFormat::F32, true, false, false, 0, false, 1, 18);
    let mut invalid = vec![
        [
            0x61,
            canonical[1],
            canonical[2],
            canonical[3],
            canonical[4],
            canonical[5],
        ]
        .to_vec(),
        canonical[..5].to_vec(),
        canonical.iter().copied().chain([0xA5]).collect(),
    ];
    for (index, value) in [
        (1, canonical[1] & !0x10), // Fabricated GPR bit 4 through EVEX.R'.
        (2, canonical[2] & !0x04), // Missing EVEX fixed-one bit.
        (2, canonical[2] & !0x08), // Reserved vvvv.
        (3, canonical[3] & !0x08), // Reserved V'.
        (3, canonical[3] | 0x80),  // Zeroing is reserved.
        (3, canonical[3] | 0x01),  // Opmask is reserved.
        (4, 0x2A),                 // Neighboring conversion opcode.
        (5, canonical[5] & 0x3F),  // Memory source.
    ] {
        let mut bytes = canonical;
        bytes[index] = value;
        invalid.push(bytes.to_vec());
    }
    for destination in [4, 5] {
        invalid.push(
            encoding(
                SourceFormat::F64,
                false,
                true,
                true,
                3,
                true,
                destination,
                31,
            )
            .to_vec(),
        );
    }

    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_fp_to_int_requires_fp16(),
            None,
            "{bytes:02X?}"
        );
    }

    for (map, pp) in [
        (0, 2),
        (1, 0),
        (1, 1),
        (2, 2),
        (3, 2),
        (5, 0),
        (5, 1),
        (5, 3),
        (6, 2),
    ] {
        let bytes = [0x62, 0xF0 | map, 0x7C | pp, 0x08, 0x2D, 0xC8];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_fp_to_int_requires_fp16(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_expose_exact_fp16_requirements() {
    let pc = 0x2D79;
    let mut block = SmirBlock::new(BlockId(79), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for format in SourceFormat::ALL {
        for signed in [false, true] {
            for truncate in [false, true] {
                for w in [false, true] {
                    for ll in 0..=3 {
                        for embedded_control in [false, true] {
                            let bytes =
                                encoding(format, signed, truncate, w, ll, embedded_control, 15, 31);
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            let provenance =
                                std::collections::HashMap::from([((BlockId(79), pc), instruction)]);
                            let valid = valid_control(truncate, ll, embedded_control);
                            for spans in [
                                x86_evex_scalar_fp_to_int_replay_spans(&block, &provenance),
                                x86_evex_native_replay_spans(&block, &provenance),
                            ] {
                                let Some(span) = spans.get(&0) else {
                                    assert!(!valid, "missing legal replay span: {bytes:02X?}");
                                    continue;
                                };
                                assert!(valid, "admitted reserved replay encoding: {bytes:02X?}");
                                assert_eq!(span.end, 1, "{bytes:02X?}");
                                assert_eq!(span.instruction, instruction, "{bytes:02X?}");
                                assert!(!span.needs_avx512vl, "{bytes:02X?}");
                                assert!(!span.needs_avx512dq, "{bytes:02X?}");
                                assert_eq!(
                                    span.needs_avx512fp16,
                                    format.fields().2,
                                    "{bytes:02X?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

fn expected_vex_fp_to_int_destination(bytes: &[u8]) -> u8 {
    match bytes {
        [0xC5, p1, _opcode, modrm] => (u8::from(p1 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
        [0xC4, p0, _p1, _opcode, modrm] => (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
        _ => unreachable!("test constructs only C4/C5 VEX scalar conversions"),
    }
}

fn assert_vex_fp_to_int_image(bytes: &[u8]) {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    let destination = expected_vex_fp_to_int_destination(bytes);
    assert_eq!(
        instruction.vex_scalar_fp_to_int_destination_index(),
        Some(destination),
        "{bytes:02X?}"
    );

    for rewritten_destination in [0u8, 4, 5, 15] {
        let rewritten = instruction
            .vex_scalar_fp_to_int_with_destination(rewritten_destination)
            .unwrap_or_else(|| panic!("{bytes:02X?} -> R{rewritten_destination}"));
        assert_eq!(
            rewritten.vex_scalar_fp_to_int_destination_index(),
            Some(rewritten_destination),
            "{bytes:02X?}"
        );
        let mut expected = bytes.to_vec();
        match expected.as_mut_slice() {
            [0xC5, p1, _opcode, modrm] => {
                if rewritten_destination < 8 {
                    *p1 |= 0x80;
                } else {
                    *p1 &= !0x80;
                }
                *modrm = (*modrm & !0x38) | ((rewritten_destination & 7) << 3);
            }
            [0xC4, p0, _p1, _opcode, modrm] => {
                if rewritten_destination < 8 {
                    *p0 |= 0x80;
                } else {
                    *p0 &= !0x80;
                }
                *modrm = (*modrm & !0x38) | ((rewritten_destination & 7) << 3);
            }
            _ => unreachable!(),
        }
        assert_eq!(rewritten.as_slice(), expected, "{bytes:02X?}");
    }
}

#[test]
fn vex_classifier_covers_all_4608_defined_l0_register_images_and_rewrites_destinations() {
    let mut classified = 0usize;
    for encoded_r in [false, true] {
        for pp in [2u8, 3] {
            let p1 = (u8::from(encoded_r) << 7) | 0x78 | pp;
            for opcode in [0x2Cu8, 0x2D] {
                for modrm in 0xC0u8..=0xFF {
                    assert_vex_fp_to_int_image(&[0xC5, p1, opcode, modrm]);
                    classified += 1;
                }
            }
        }
    }
    for extension_bits in 0u8..8 {
        let p0 = (extension_bits << 5) | 1;
        for w in [false, true] {
            for pp in [2u8, 3] {
                let p1 = (u8::from(w) << 7) | 0x78 | pp;
                for opcode in [0x2Cu8, 0x2D] {
                    for modrm in 0xC0u8..=0xFF {
                        assert_vex_fp_to_int_image(&[0xC4, p0, p1, opcode, modrm]);
                        classified += 1;
                    }
                }
            }
        }
    }
    assert_eq!(classified, 4_608);

    // Independently assembled by LLVM 23.0.0git.
    for bytes in [
        &[0xC5, 0xFA, 0x2D, 0xC1][..],       // vcvtss2si eax,xmm1
        &[0xC4, 0x41, 0xFA, 0x2D, 0xCA][..], // vcvtss2si r9,xmm10
        &[0xC4, 0xC1, 0xFA, 0x2C, 0xE7][..], // vcvttss2si rsp,xmm15
        &[0xC4, 0xE1, 0xFB, 0x2D, 0xE9][..], // vcvtsd2si rbp,xmm1
        &[0xC4, 0x41, 0xFB, 0x2C, 0xD1][..], // vcvttsd2si r10,xmm9
    ] {
        assert_vex_fp_to_int_image(bytes);
    }
}

#[test]
fn vex_classifier_rejects_unpredictable_reserved_memory_and_nonfamily_frontiers() {
    let canonical = [0xC4, 0x41, 0xFA, 0x2D, 0xCA];
    let mut invalid = vec![
        canonical[..4].to_vec(),
        canonical.iter().copied().chain([0xA5]).collect(),
        [0xC3, canonical[1], canonical[2], canonical[3], canonical[4]].to_vec(),
    ];
    for (index, value) in [
        (1, (canonical[1] & !0x1F) | 2), // Wrong map.
        (2, canonical[2] | 0x04),        // VEX.L=1 is unpredictable.
        (2, canonical[2] & !0x08),       // Reserved VEX.vvvv.
        (2, (canonical[2] & !3) | 1),    // Wrong mandatory prefix.
        (3, 0x2A),                       // Neighboring conversion opcode.
        (4, canonical[4] & 0x3F),        // Memory source.
    ] {
        let mut bytes = canonical;
        bytes[index] = value;
        invalid.push(bytes.to_vec());
    }
    invalid.extend([
        vec![0xC5, 0x82, 0x2D, 0xC1], // Reserved VEX.vvvv.
        vec![0xC5, 0xFE, 0x2D, 0xC1], // VEX.L=1.
        vec![0xC5, 0xFA, 0x2A, 0xC1], // Neighboring conversion opcode.
        vec![0xC5, 0xFA, 0x2D, 0x01], // Memory source.
    ]);

    for bytes in invalid {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.vex_scalar_fp_to_int_destination_index(),
            None,
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_scalar_fp_to_int_with_destination(0),
            None,
            "{bytes:02X?}"
        );
    }
    let valid = X86InstructionBytes::new(&canonical).unwrap();
    assert_eq!(valid.vex_scalar_fp_to_int_with_destination(16), None);
}

#[test]
fn vex_replay_spans_preserve_exact_defined_source_provenance() {
    let pc = 0x2C2D;
    let block_id = BlockId(0x2D);
    let mut block = SmirBlock::new(block_id, pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));

    for bytes in [
        &[0xC5, 0xFA, 0x2D, 0xC1][..],
        &[0xC4, 0xC1, 0xFA, 0x2C, 0xE7][..],
        &[0xC4, 0xE1, 0xFB, 0x2D, 0xE9][..],
        &[0xC4, 0x41, 0xFB, 0x2C, 0xD1][..],
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = std::collections::HashMap::from([((block_id, pc), instruction)]);
        for spans in [
            x86_vex_scalar_fp_to_int_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            assert_eq!(
                spans.get(&0),
                Some(&X86NativeReplaySpan {
                    end: 2,
                    instruction,
                    needs_avx512vl: false,
                    needs_avx512dq: false,
                    needs_avx512fp16: false,
                    preserve_mxcsr_de: false,
                }),
                "{bytes:02X?}"
            );
        }
        assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());
    }

    assert!(
        x86_vex_scalar_fp_to_int_replay_spans(&block, &std::collections::HashMap::new()).is_empty()
    );
    let mut noncontiguous = block.clone();
    noncontiguous
        .ops
        .insert(1, SmirOp::new(OpId(2), pc + 1, OpKind::Nop));
    let instruction = X86InstructionBytes::new(&[0xC5, 0xFA, 0x2D, 0xC1]).unwrap();
    let provenance = std::collections::HashMap::from([((block_id, pc), instruction)]);
    assert!(x86_vex_scalar_fp_to_int_replay_spans(&noncontiguous, &provenance).is_empty());
}
