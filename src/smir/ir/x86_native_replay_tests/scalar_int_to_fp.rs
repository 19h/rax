//! Exact classifier tests for VEX/EVEX scalar integer-to-floating-point replay.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DestinationFormat {
    F32,
    F64,
    F16,
}

impl DestinationFormat {
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
    format: DestinationFormat,
    signed: bool,
    w: bool,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    merge: u8,
    source: u8,
) -> [u8; 6] {
    assert!(ll < 4 && destination < 32 && merge < 32 && source < 16);
    let (map, pp, _) = format.fields();
    let mut p0 = 0xF0 | map;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    [
        0x62,
        p0,
        if w { 0x80 } else { 0 } | ((!merge & 0x0F) << 3) | 0x04 | pp,
        (ll << 5)
            | if embedded_control { 0x10 } else { 0 }
            | if merge & 0x10 == 0 { 0x08 } else { 0 },
        if signed { 0x2A } else { 0x7B },
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

fn valid_control(ll: u8, embedded_control: bool) -> bool {
    ll != 3 || embedded_control
}

#[test]
fn classifier_accepts_exactly_12_600_sampled_legal_register_encodings() {
    let destinations = [0u8, 3, 8, 17, 31];
    let merges = [0u8, 2, 8, 18, 31];
    let sources = [0u8, 3, 8, 12, 13, 15];
    let mut classified = 0usize;

    for format in DestinationFormat::ALL {
        for signed in [false, true] {
            for w in [false, true] {
                for ll in 0..=3 {
                    for embedded_control in [false, true] {
                        for destination in destinations {
                            for merge in merges {
                                for source in sources {
                                    let bytes = encoding(
                                        format,
                                        signed,
                                        w,
                                        ll,
                                        embedded_control,
                                        destination,
                                        merge,
                                        source,
                                    );
                                    let expected = valid_control(ll, embedded_control)
                                        .then_some(format.fields().2);
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_scalar_int_to_fp_requires_fp16(),
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
    assert_eq!(classified, 12_600);

    // Independently assembled by LLVM 21.1.8. Collectively these exercise all
    // six mnemonics, W0/W1, destination XMM16-31, merge XMM16-31, and GPR8-15.
    for (bytes, needs_fp16) in [
        ([0x62, 0x41, 0x6E, 0x00, 0x2A, 0xCA], false),
        ([0x62, 0x41, 0xEE, 0x00, 0x2A, 0xCA], false),
        ([0x62, 0x41, 0x67, 0x00, 0x2A, 0xD3], false),
        ([0x62, 0x41, 0xE7, 0x00, 0x2A, 0xD3], false),
        ([0x62, 0x45, 0x5E, 0x00, 0x2A, 0xDC], true),
        ([0x62, 0x45, 0xDE, 0x00, 0x2A, 0xDC], true),
        ([0x62, 0x41, 0x56, 0x00, 0x7B, 0xE5], false),
        ([0x62, 0x41, 0xD6, 0x00, 0x7B, 0xE5], false),
        ([0x62, 0x41, 0x4F, 0x00, 0x7B, 0xEE], false),
        ([0x62, 0x41, 0xCF, 0x00, 0x7B, 0xEE], false),
        ([0x62, 0x45, 0x46, 0x00, 0x7B, 0xF7], true),
        ([0x62, 0x45, 0xC6, 0x00, 0x7B, 0xF7], true),
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_int_to_fp_requires_fp16(),
            Some(needs_fp16),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_rejects_every_reserved_or_unsafe_frontier() {
    let canonical = encoding(DestinationFormat::F32, true, false, 0, false, 17, 18, 10);
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
        (1, canonical[1] & !0x40), // Fabricated source GPR bit 4 through EVEX.X.
        (2, canonical[2] & !0x04), // Missing EVEX fixed-one bit.
        (3, canonical[3] | 0x80),  // Zeroing is reserved.
        (3, canonical[3] | 0x01),  // Opmask is reserved.
        (4, 0x2D),                 // Neighboring conversion opcode.
        (5, canonical[5] & 0x3F),  // Memory source.
    ] {
        let mut bytes = canonical;
        bytes[index] = value;
        invalid.push(bytes.to_vec());
    }
    for source in [4, 5] {
        invalid
            .push(encoding(DestinationFormat::F64, false, true, 3, true, 31, 30, source).to_vec());
    }

    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_int_to_fp_requires_fp16(),
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
        (9, 2),
    ] {
        let bytes = [0x62, 0xF0 | map, 0x6C | pp, 0x08, 0x2A, 0xC8];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_int_to_fp_requires_fp16(),
            None,
            "{bytes:02X?}"
        );
    }

    for source in [12, 13] {
        let bytes = encoding(DestinationFormat::F16, false, true, 2, true, 31, 30, source);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_int_to_fp_requires_fp16(),
            Some(true),
            "R{source} must remain safe: {bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_expose_exact_fp16_requirements() {
    let pc = 0x2A7B;
    let mut block = SmirBlock::new(BlockId(123), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for format in DestinationFormat::ALL {
        for signed in [false, true] {
            for w in [false, true] {
                for ll in 0..=3 {
                    for embedded_control in [false, true] {
                        let bytes = encoding(format, signed, w, ll, embedded_control, 31, 30, 15);
                        let instruction = X86InstructionBytes::new(&bytes).unwrap();
                        let provenance =
                            std::collections::HashMap::from([((BlockId(123), pc), instruction)]);
                        let valid = valid_control(ll, embedded_control);
                        for spans in [
                            x86_evex_scalar_int_to_fp_replay_spans(&block, &provenance),
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
                            assert_eq!(span.needs_avx512fp16, format.fields().2, "{bytes:02X?}");
                        }
                    }
                }
            }
        }
    }
}

fn expected_vex_int_to_fp_operands(bytes: &[u8]) -> (u8, u8) {
    match bytes {
        [0xC5, p1, 0x2A, modrm] => (
            (u8::from(p1 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
            modrm & 7,
        ),
        [0xC4, p0, _p1, 0x2A, modrm] => (
            (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
            (u8::from(p0 & 0x20 == 0) << 3) | (modrm & 7),
        ),
        _ => unreachable!("test constructs only C4/C5 VEX scalar conversions"),
    }
}

fn assert_vex_int_to_fp_image(bytes: &[u8]) {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    let (destination, source) = expected_vex_int_to_fp_operands(bytes);
    assert_eq!(
        instruction.vex_scalar_int_to_fp_destination_index(),
        Some(destination),
        "{bytes:02X?}"
    );
    assert_eq!(
        instruction.vex_scalar_int_to_fp_source_index(),
        Some(source),
        "{bytes:02X?}"
    );

    let rewritten = instruction
        .vex_scalar_int_to_fp_with_source(0)
        .unwrap_or_else(|| panic!("{bytes:02X?} -> RAX"));
    assert_eq!(
        rewritten.vex_scalar_int_to_fp_source_index(),
        Some(0),
        "{bytes:02X?}"
    );
    let mut expected = bytes.to_vec();
    match expected.as_mut_slice() {
        [0xC5, _p1, 0x2A, modrm] => *modrm &= !0x07,
        [0xC4, p0, _p1, 0x2A, modrm] => {
            *p0 |= 0x20;
            *modrm &= !0x07;
        }
        _ => unreachable!(),
    }
    assert_eq!(rewritten.as_slice(), expected, "{bytes:02X?}");
}

#[test]
fn vex_classifier_covers_all_36864_defined_l0_register_images_and_rewrites_sources() {
    let mut classified = 0usize;
    for encoded_r in [false, true] {
        for encoded_vvvv in 0u8..16 {
            for pp in [2u8, 3] {
                let p1 = (u8::from(encoded_r) << 7) | (encoded_vvvv << 3) | pp;
                for modrm in 0xC0u8..=0xFF {
                    assert_vex_int_to_fp_image(&[0xC5, p1, 0x2A, modrm]);
                    classified += 1;
                }
            }
        }
    }
    for extension_bits in 0u8..8 {
        let p0 = (extension_bits << 5) | 1;
        for w in [false, true] {
            for encoded_vvvv in 0u8..16 {
                for pp in [2u8, 3] {
                    let p1 = (u8::from(w) << 7) | (encoded_vvvv << 3) | pp;
                    for modrm in 0xC0u8..=0xFF {
                        assert_vex_int_to_fp_image(&[0xC4, p0, p1, 0x2A, modrm]);
                        classified += 1;
                    }
                }
            }
        }
    }
    assert_eq!(classified, 36_864);

    // Independently assembled by LLVM 23.0.0git.
    for bytes in [
        &[0xC5, 0xEA, 0x2A, 0xC8][..],       // vcvtsi2ss xmm1,xmm2,eax
        &[0xC4, 0x61, 0xAA, 0x2A, 0xCC][..], // vcvtsi2ss xmm9,xmm10,rsp
        &[0xC4, 0xE1, 0xDB, 0x2A, 0xDD][..], // vcvtsi2sd xmm3,xmm4,rbp
        &[0xC4, 0x41, 0x93, 0x2A, 0xE6][..], // vcvtsi2sd xmm12,xmm13,r14
    ] {
        assert_vex_int_to_fp_image(bytes);
    }

    for bytes in [
        &[0xC5, 0xEA, 0x2A, 0xCC][..],
        &[0xC5, 0xEA, 0x2A, 0xCD][..],
        &[0xC4, 0x61, 0xAA, 0x2A, 0xCC][..],
        &[0xC4, 0x61, 0xAA, 0x2A, 0xCD][..],
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        for source in [0u8, 4, 5] {
            let rewritten = instruction
                .vex_scalar_int_to_fp_with_source(source)
                .unwrap();
            assert_eq!(
                rewritten.vex_scalar_int_to_fp_source_index(),
                Some(source),
                "{bytes:02X?}"
            );
        }
    }
    let c4 = X86InstructionBytes::new(&[0xC4, 0x61, 0xAA, 0x2A, 0xCC]).unwrap();
    assert_eq!(
        c4.vex_scalar_int_to_fp_with_source(15)
            .unwrap()
            .vex_scalar_int_to_fp_source_index(),
        Some(15)
    );
    let c5 = X86InstructionBytes::new(&[0xC5, 0xEA, 0x2A, 0xCC]).unwrap();
    assert_eq!(c5.vex_scalar_int_to_fp_with_source(8), None);
}

#[test]
fn vex_classifier_rejects_unpredictable_memory_and_nonfamily_frontiers() {
    let canonical = [0xC4, 0x41, 0x93, 0x2A, 0xE6];
    let mut invalid = vec![
        canonical[..4].to_vec(),
        canonical.iter().copied().chain([0xA5]).collect(),
        [0xC3, canonical[1], canonical[2], canonical[3], canonical[4]].to_vec(),
    ];
    for (index, value) in [
        (1, (canonical[1] & !0x1F) | 2), // Wrong map.
        (2, canonical[2] | 0x04),        // VEX.L=1 is unpredictable.
        (2, (canonical[2] & !3) | 1),    // Wrong mandatory prefix.
        (3, 0x2D),                       // Neighboring conversion opcode.
        (4, canonical[4] & 0x3F),        // Memory source.
    ] {
        let mut bytes = canonical;
        bytes[index] = value;
        invalid.push(bytes.to_vec());
    }
    invalid.extend([
        vec![0xC5, 0xEE, 0x2A, 0xC8], // VEX.L=1.
        vec![0xC5, 0xE9, 0x2A, 0xC8], // Wrong mandatory prefix.
        vec![0xC5, 0xEA, 0x2D, 0xC8], // Neighboring conversion opcode.
        vec![0xC5, 0xEA, 0x2A, 0x08], // Memory source.
    ]);

    for bytes in invalid {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.vex_scalar_int_to_fp_destination_index(),
            None,
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_scalar_int_to_fp_source_index(),
            None,
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_scalar_int_to_fp_with_source(0),
            None,
            "{bytes:02X?}"
        );
    }
    let valid = X86InstructionBytes::new(&canonical).unwrap();
    assert_eq!(valid.vex_scalar_int_to_fp_with_source(16), None);
}

#[test]
fn vex_replay_spans_preserve_exact_defined_source_provenance() {
    let pc = 0x2A;
    let block_id = BlockId(0x2A);
    let mut block = SmirBlock::new(block_id, pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));

    for bytes in [
        &[0xC5, 0xEA, 0x2A, 0xC8][..],
        &[0xC4, 0x61, 0xAA, 0x2A, 0xCC][..],
        &[0xC4, 0xE1, 0xDB, 0x2A, 0xDD][..],
        &[0xC4, 0x41, 0x93, 0x2A, 0xE6][..],
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = std::collections::HashMap::from([((block_id, pc), instruction)]);
        for spans in [
            x86_vex_scalar_int_to_fp_replay_spans(&block, &provenance),
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
        x86_vex_scalar_int_to_fp_replay_spans(&block, &std::collections::HashMap::new()).is_empty()
    );
    let mut noncontiguous = block.clone();
    noncontiguous
        .ops
        .insert(1, SmirOp::new(OpId(2), pc + 1, OpKind::Nop));
    let instruction = X86InstructionBytes::new(&[0xC5, 0xEA, 0x2A, 0xC8]).unwrap();
    let provenance = std::collections::HashMap::from([((block_id, pc), instruction)]);
    assert!(x86_vex_scalar_int_to_fp_replay_spans(&noncontiguous, &provenance).is_empty());
}
