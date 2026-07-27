//! Native replay coverage for register-only AVX-VNNI-INT8/INT16 dot products.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0xD160;
const OPERANDS: [(u8, u8, u8); 10] = [
    (1, 2, 3),
    (9, 10, 11),
    (1, 1, 3),
    (1, 2, 1),
    (1, 2, 2),
    (9, 9, 11),
    (9, 10, 9),
    (15, 15, 15),
    (15, 8, 13),
    (13, 14, 15),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DotKind {
    ByteSs,
    ByteSu,
    ByteUu,
    WordSu,
    WordUs,
    WordUu,
}

impl DotKind {
    const ALL: [Self; 6] = [
        Self::ByteSs,
        Self::ByteSu,
        Self::ByteUu,
        Self::WordSu,
        Self::WordUs,
        Self::WordUu,
    ];

    fn pp_opcode(self, saturate: bool) -> (u8, u8) {
        let saturating_bit = u8::from(saturate);
        match self {
            Self::ByteSs => (3, 0x50 | saturating_bit),
            Self::ByteSu => (2, 0x50 | saturating_bit),
            Self::ByteUu => (0, 0x50 | saturating_bit),
            Self::WordSu => (2, 0xD2 | saturating_bit),
            Self::WordUs => (1, 0xD2 | saturating_bit),
            Self::WordUu => (0, 0xD2 | saturating_bit),
        }
    }

    fn int16(self) -> bool {
        matches!(self, Self::WordSu | Self::WordUs | Self::WordUu)
    }

    fn source_bits(self) -> usize {
        if self.int16() { 16 } else { 8 }
    }

    fn source_signedness(self) -> (bool, bool) {
        match self {
            Self::ByteSs => (true, true),
            Self::ByteSu | Self::WordSu => (true, false),
            Self::WordUs => (false, true),
            Self::ByteUu | Self::WordUu => (false, false),
        }
    }

    fn unsigned_result(self) -> bool {
        let (first, second) = self.source_signedness();
        !first && !second
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DotCase {
    kind: DotKind,
    saturate: bool,
    ymm: bool,
    destination: u8,
    source1: u8,
    source2: u8,
    clear_ignored_x: bool,
}

fn encoding(case: DotCase) -> [u8; 5] {
    assert!(case.destination < 16 && case.source1 < 16 && case.source2 < 16);
    let (pp, opcode) = case.kind.pp_opcode(case.saturate);
    let mut p0 = 0xE2;
    if case.destination >= 8 {
        p0 &= !0x80;
    }
    if case.clear_ignored_x {
        p0 &= !0x40;
    }
    if case.source2 >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (((!case.source1) & 0x0F) << 3) | (u8::from(case.ymm) << 2) | pp,
        opcode,
        0xC0 | ((case.destination & 7) << 3) | (case.source2 & 7),
    ]
}

fn cases() -> Vec<DotCase> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for kind in DotKind::ALL {
        for saturate in [false, true] {
            for ymm in [false, true] {
                for (destination, source1, source2) in OPERANDS {
                    cases.push(DotCase {
                        kind,
                        saturate,
                        ymm,
                        destination,
                        source1,
                        source2,
                        clear_ignored_x: ordinal & 1 != 0,
                    });
                    ordinal += 1;
                }
            }
        }
    }
    cases
}

fn function(bytes: &[u8]) -> crate::smir::ir::SmirFunction {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());
    function
}

#[test]
fn strict_lift_and_replay_cover_all_196_608_defined_register_images() {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, SourceArch, VReg, VecElementType, VecWidth, X86Reg};
    use crate::smir::ir::{SmirBlock, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifted = 0usize;
    let mut lifter = X86_64Lifter::strict();
    for kind in DotKind::ALL {
        for saturate in [false, true] {
            let (pp, opcode) = kind.pp_opcode(saturate);
            let (src1_signed, src2_signed) = kind.source_signedness();
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for encoded_vvvv in 0u8..16 {
                    for ymm in [false, true] {
                        for modrm in 0xC0u8..=0xFF {
                            let bytes = [
                                0xC4,
                                extension_bits | 2,
                                (encoded_vvvv << 3) | (u8::from(ymm) << 2) | pp,
                                opcode,
                                modrm,
                            ];
                            let mut context = LiftContext::new(SourceArch::X86_64);
                            let result = lifter
                                .lift_insn(PC, &bytes, &mut context)
                                .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                            assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
                            assert_eq!(result.ops.len(), 1, "{bytes:02X?}");

                            let destination =
                                (u8::from(extension_bits & 0x80 == 0) << 3) | ((modrm >> 3) & 7);
                            let source1 = (!encoded_vvvv) & 0x0F;
                            let source2 = (u8::from(extension_bits & 0x20 == 0) << 3) | (modrm & 7);
                            let vector = |register| {
                                let register = if ymm {
                                    X86Reg::Ymm(register)
                                } else {
                                    X86Reg::Xmm(register)
                                };
                                VReg::Arch(ArchReg::X86(register))
                            };
                            assert!(
                                matches!(
                                    result.ops[0].kind,
                                    OpKind::VDotProductExt {
                                        dst,
                                        acc,
                                        src1,
                                        src2,
                                        src_elem,
                                        acc_elem: VecElementType::I32,
                                        width,
                                        src1_signed: actual_src1_signed,
                                        src2_signed: actual_src2_signed,
                                        saturate: actual_saturate,
                                    } if dst == vector(destination)
                                        && acc == dst
                                        && src1 == vector(source1)
                                        && src2 == vector(source2)
                                        && src_elem
                                            == if kind.int16() {
                                                VecElementType::I16
                                            } else {
                                                VecElementType::I8
                                            }
                                        && width
                                            == if ymm {
                                                VecWidth::V256
                                            } else {
                                                VecWidth::V128
                                            }
                                        && actual_src1_signed == src1_signed
                                        && actual_src2_signed == src2_signed
                                        && actual_saturate == saturate
                                ),
                                "{bytes:02X?} {:?}",
                                result.ops[0].kind
                            );

                            let mut block = SmirBlock::new(BlockId(7), PC);
                            block.ops = result.ops;
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            let provenance =
                                std::collections::HashMap::from([((BlockId(7), PC), instruction)]);
                            for spans in [
                                crate::smir::ir::x86_vex_integer_dot_ext_replay_spans(
                                    &block,
                                    &provenance,
                                ),
                                crate::smir::ir::x86_native_replay_spans(&block, &provenance),
                            ] {
                                let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
                                assert_eq!(span.end, 1, "{bytes:02X?}");
                                assert_eq!(span.instruction, instruction, "{bytes:02X?}");
                            }
                            lifted += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(lifted, 196_608);
}

#[test]
fn replay_features_select_independent_int8_int16_cpuid_guards_and_ymm16_bridge() {
    for kind in DotKind::ALL {
        let case = DotCase {
            kind,
            saturate: true,
            ymm: true,
            destination: 15,
            source1: 14,
            source2: 13,
            clear_ignored_x: true,
        };
        let bytes = encoding(case);
        let function = function(&bytes);
        let excluded = std::collections::HashMap::new();
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(requirements.any, "{case:?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert!(requirements.needs_avx, "{case:?}");
        assert!(!requirements.needs_avx2, "{case:?}");
        assert_eq!(requirements.needs_avx_vnni_int8, !kind.int16(), "{case:?}");
        assert_eq!(requirements.needs_avx_vnni_int16, kind.int16(), "{case:?}");
        assert!(!requirements.needs_sse3, "{case:?}");
        assert!(!requirements.needs_f16c, "{case:?}");
        assert!(!requirements.needs_fma, "{case:?}");
        assert!(!requirements.needs_fma4, "{case:?}");
        assert!(!requirements.needs_xop, "{case:?}");
        assert!(!requirements.needs_avx512bw, "{case:?}");
        assert!(!requirements.needs_avx512vl, "{case:?}");
        assert!(!requirements.needs_avx512dq, "{case:?}");
        assert!(!requirements.needs_avx512fp16, "{case:?}");
        assert!(!requirements.needs_avx512cd, "{case:?}");
        assert!(!requirements.needs_gfni, "{case:?}");
        assert!(!requirements.needs_avx512vp2intersect, "{case:?}");
        assert!(!requirements.needs_pclmulqdq, "{case:?}");
        assert!(!requirements.needs_vpclmulqdq, "{case:?}");
        assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
            &function, &excluded
        ));

        #[cfg(target_arch = "x86_64")]
        {
            let feature = if kind.int16() {
                x86_host_has_avx_vnni_int16()
            } else {
                x86_host_has_avx_vnni_int8()
            };
            let expected = std::is_x86_feature_detected!("avx") && feature;
            assert_eq!(requirements.x86_host_supported(), expected, "{case:?}");
            assert_eq!(
                x86_native_vector_features_supported_excluding(&function, &excluded),
                expected,
                "{case:?}"
            );
        }
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[test]
fn replay_admits_emits_and_upper_clears_all_480_o0_o2_variant_width_alias_shapes() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    assert_eq!(
        encoding(DotCase {
            kind: DotKind::ByteSs,
            saturate: false,
            ymm: false,
            destination: 9,
            source1: 10,
            source2: 11,
            clear_ignored_x: false,
        }),
        [0xC4, 0x42, 0x2B, 0x50, 0xCB]
    );
    assert_eq!(
        encoding(DotCase {
            kind: DotKind::WordUs,
            saturate: true,
            ymm: true,
            destination: 15,
            source1: 14,
            source2: 13,
            clear_ignored_x: true,
        }),
        [0xC4, 0x02, 0x0D, 0xD3, 0xFD]
    );

    let cases = cases();
    assert_eq!(cases.len(), 240);
    let mut lowered = 0usize;
    for case in cases {
        let bytes = encoding(case);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let mut function = function(&bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            assert!(
                is_native_clobber_safe(&function),
                "{level:?} {case:?} {bytes:02X?}"
            );
            assert!(
                uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
                "{level:?} {case:?} {bytes:02X?}"
            );

            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_avx_ymm16_vector_state(true);
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let instruction_offset = find_subslice(&code, &bytes)
                .unwrap_or_else(|| panic!("{level:?} {case:?} {bytes:02X?}"));
            let tail = &code[instruction_offset + bytes.len()..];
            assert!(
                tail.starts_with(&[
                    0x9C,
                    0x50,
                    0x48,
                    0x8B,
                    0x45,
                    crate::smir::lower::X86_STATE_PTR_AT_RBP as u8,
                ]),
                "{level:?} {case:?} {bytes:02X?}"
            );
            let upper =
                crate::smir::lower::X86_GUEST_ZMM_OFFSET + i32::from(case.destination) * 64 + 32;
            for offset in (upper..upper + 32).step_by(8) {
                let mut clear = vec![0x48, 0xC7, 0x80];
                clear.extend_from_slice(&(offset as u32).to_le_bytes());
                clear.extend_from_slice(&0u32.to_le_bytes());
                assert!(
                    find_subslice(tail, &clear).is_some(),
                    "missing upper clear {offset}: {level:?} {case:?} {bytes:02X?}"
                );
            }
            lowered += 1;
        }
    }
    assert_eq!(lowered, 480);

    let case = DotCase {
        kind: DotKind::ByteSs,
        saturate: false,
        ymm: true,
        destination: 1,
        source1: 2,
        source2: 3,
        clear_ignored_x: false,
    };
    let bytes = encoding(case);
    let mut missing = function(&bytes);
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let mutations: [fn(&mut [u8; 5]); 3] = [
        |bytes: &mut [u8; 5]| bytes[4] &= 0x3F,
        |bytes: &mut [u8; 5]| bytes[2] |= 0x80,
        |bytes: &mut [u8; 5]| bytes[1] ^= 1,
    ];
    for mutate in mutations {
        let mut invalid = bytes;
        mutate(&mut invalid);
        let mut invalid_metadata = function(&bytes);
        invalid_metadata.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&invalid).unwrap(),
        );
        assert!(!is_native_clobber_safe(&invalid_metadata), "{invalid:02X?}");
    }

    let mut memory = bytes;
    memory[4] &= 0x3F;
    assert!(!is_native_clobber_safe(&function(&memory)));
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DotState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

const VECTOR_PATTERNS: [u64; 12] = [
    0,
    u64::MAX,
    0x0001_007F_0080_00FF,
    0x7FFF_8000_FFFF_0001,
    0x7FFF_FFFF_8000_0000,
    0x8000_0000_7FFF_FFFF,
    0x0102_0408_1020_4080,
    0xFEFD_FBF7_EFDF_BF7F,
    0x7F80_FF00_0180_FE01,
    0xFFFF_0000_FFFE_0001,
    0x4000_C000_2000_E000,
    0xA5A5_5A5A_3CC3_C33C,
];

fn source_element(vector: &[u64; 8], lane: usize, bits: usize) -> u32 {
    let bit = lane * bits;
    let mask = if bits == 16 { 0xFFFF } else { 0xFF };
    ((vector[bit / 64] >> (bit % 64)) as u32) & mask
}

fn set_source_element(vector: &mut [u64; 8], lane: usize, bits: usize, value: u32) {
    let bit = lane * bits;
    let mask = if bits == 16 { 0xFFFFu64 } else { 0xFFu64 };
    let shift = bit % 64;
    vector[bit / 64] = (vector[bit / 64] & !(mask << shift)) | ((u64::from(value) & mask) << shift);
}

fn dword(vector: &[u64; 8], lane: usize) -> u32 {
    (vector[lane / 2] >> ((lane % 2) * 32)) as u32
}

fn set_dword(vector: &mut [u64; 8], lane: usize, value: u32) {
    let shift = (lane % 2) * 32;
    vector[lane / 2] =
        (vector[lane / 2] & !(u64::from(u32::MAX) << shift)) | (u64::from(value) << shift);
}

fn initial_state(case: DotCase, ordinal: usize) -> DotState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            VECTOR_PATTERNS[(ordinal + register * 3 + word * 5) % VECTOR_PATTERNS.len()]
        })
    });

    // For non-aliased shapes, force positive, negative, and unsigned
    // saturation boundaries in every variant. Aliased shapes retain the
    // adversarial bit-pattern corpus and exercise snapshot-before-write.
    if case.destination != case.source1
        && case.destination != case.source2
        && case.source1 != case.source2
    {
        let bits = case.kind.source_bits();
        let lanes = if case.ymm { 8 } else { 4 };
        for lane in 0..lanes {
            set_dword(
                &mut vectors[usize::from(case.destination)],
                lane,
                match lane % 4 {
                    0 => i32::MAX as u32,
                    1 => i32::MIN as u32,
                    2 => u32::MAX,
                    _ => 0,
                },
            );
            for term in 0..(32 / bits) {
                let source_lane = lane * (32 / bits) + term;
                let (first, second) = match (case.kind, lane % 4) {
                    (DotKind::ByteSs, 1) => (0x80, 0x7F),
                    (DotKind::ByteSs, _) => (0x7F, 0x7F),
                    (DotKind::ByteSu, 1) => (0x80, 0xFF),
                    (DotKind::ByteSu, _) => (0x7F, 0xFF),
                    (DotKind::ByteUu, _) => (0xFF, 0xFF),
                    (DotKind::WordSu, 1) => (0x8000, 0xFFFF),
                    (DotKind::WordSu, _) => (0x7FFF, 0xFFFF),
                    (DotKind::WordUs, 1) => (0xFFFF, 0x8000),
                    (DotKind::WordUs, _) => (0xFFFF, 0x7FFF),
                    (DotKind::WordUu, _) => (0xFFFF, 0xFFFF),
                };
                set_source_element(
                    &mut vectors[usize::from(case.source1)],
                    source_lane,
                    bits,
                    first,
                );
                set_source_element(
                    &mut vectors[usize::from(case.source2)],
                    source_lane,
                    bits,
                    second,
                );
            }
        }
    }

    DotState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors,
        masks: [
            0x6996_F00F_3CC3_A55A,
            0,
            1,
            0x0123_4567_89AB_CDEF,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0000,
            0xF0F0_0F0F_A5A5_5A5A,
            u64::MAX,
        ],
        rflags: 0x2 | 0x0CD5,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][ordinal % 4],
    }
}

fn sign_extend(value: u32, bits: usize) -> i128 {
    let shift = 128 - bits;
    (i128::from(value) << shift) >> shift
}

fn lane_sum(case: DotCase, initial: &DotState, lane: usize) -> i128 {
    let first = initial.vectors[usize::from(case.source1)];
    let second = initial.vectors[usize::from(case.source2)];
    let accumulator = dword(&initial.vectors[usize::from(case.destination)], lane);
    let (src1_signed, src2_signed) = case.kind.source_signedness();
    let bits = case.kind.source_bits();
    let terms = 32 / bits;
    let mut sum = if case.kind.unsigned_result() {
        i128::from(accumulator)
    } else {
        i128::from(accumulator as i32)
    };
    for term in 0..terms {
        let source_lane = lane * terms + term;
        let first_raw = source_element(&first, source_lane, bits);
        let second_raw = source_element(&second, source_lane, bits);
        let first = if src1_signed {
            sign_extend(first_raw, bits)
        } else {
            i128::from(first_raw)
        };
        let second = if src2_signed {
            sign_extend(second_raw, bits)
        } else {
            i128::from(second_raw)
        };
        sum += first * second;
    }
    sum
}

fn architectural_expected(case: DotCase, initial: &DotState) -> DotState {
    let mut expected = initial.clone();
    expected.vectors[usize::from(case.destination)] = [0; 8];
    let lanes = if case.ymm { 8 } else { 4 };
    for lane in 0..lanes {
        let sum = lane_sum(case, initial, lane);
        let value = if case.saturate {
            if case.kind.unsigned_result() {
                sum.clamp(0, i128::from(u32::MAX)) as u32
            } else {
                sum.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32 as u32
            }
        } else {
            sum as u32
        };
        set_dword(
            &mut expected.vectors[usize::from(case.destination)],
            lane,
            value,
        );
    }
    expected
}

fn optimized_function(
    bytes: &[u8],
    level: crate::smir::optimize::OptLevel,
    halt: bool,
) -> crate::smir::ir::SmirFunction {
    let mut function = function(bytes);
    if halt {
        function.blocks[0].set_terminator(Terminator::Trap {
            kind: crate::smir::ir::TrapKind::Halt,
        });
    }
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn interpret(bytes: &[u8], initial: &DotState, level: crate::smir::optimize::OptLevel) -> DotState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let function = optimized_function(bytes, level, true);
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gprs;
        for (index, value) in initial.vectors.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.masks;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &function.blocks[0],
    );
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    DotState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_o0_o2_equations_saturation_wrap_aliases_and_upper_zeroing() {
    let cases = cases();
    assert_eq!(cases.len(), 240);
    let mut saw_signed_positive_saturation = false;
    let mut saw_signed_negative_saturation = false;
    let mut saw_unsigned_saturation = false;
    let mut saw_wrapping_overflow = false;

    for (ordinal, case) in cases.into_iter().enumerate() {
        let bytes = encoding(case);
        let initial = initial_state(case, ordinal);
        let expected = architectural_expected(case, &initial);
        let lanes = if case.ymm { 8 } else { 4 };
        for lane in 0..lanes {
            let sum = lane_sum(case, &initial, lane);
            if case.kind.unsigned_result() {
                saw_unsigned_saturation |= case.saturate && sum > i128::from(u32::MAX);
                saw_wrapping_overflow |= !case.saturate && sum > i128::from(u32::MAX);
            } else {
                saw_signed_positive_saturation |= case.saturate && sum > i128::from(i32::MAX);
                saw_signed_negative_saturation |= case.saturate && sum < i128::from(i32::MIN);
                saw_wrapping_overflow |=
                    !case.saturate && !(i128::from(i32::MIN)..=i128::from(i32::MAX)).contains(&sum);
            }
        }
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                interpret(&bytes, &initial, level),
                expected,
                "{level:?} {case:?} {bytes:02X?}"
            );
        }
    }

    assert!(saw_signed_positive_saturation);
    assert!(saw_signed_negative_saturation);
    assert!(saw_unsigned_saturation);
    assert!(saw_wrapping_overflow);
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &DotState,
    level: crate::smir::optimize::OptLevel,
) -> DotState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = optimized_function(bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map VEX extended integer dot-product replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        vector_active: X86_VECTOR_STATE_YMM16,
        k: initial.masks,
        mxcsr: initial.mxcsr,
        ..GuestRegs::default()
    };
    for (index, value) in initial.vectors.iter().enumerate() {
        registers.set_zmm(index, *value);
    }
    exec.run(lowered.entry_offset, &mut registers);

    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        *value = registers.get_zmm(index);
    }
    DotState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn native_feature_supported(kind: DotKind) -> bool {
    std::is_x86_feature_detected!("avx")
        && if kind.int16() {
            x86_host_has_avx_vnni_int16()
        } else {
            x86_host_has_avx_vnni_int8()
        }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_INTEGER_DOT_EXT_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
fn child_range() -> Option<std::ops::Range<usize>> {
    let value = std::env::var(CHILD_RANGE_ENV).ok()?;
    let (start, end) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid {CHILD_RANGE_ENV}: {value}"));
    Some(
        start
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} start: {value}"))
            ..end
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} end: {value}")),
    )
}

#[cfg(target_arch = "x86_64")]
fn supported_native_cases() -> Vec<DotCase> {
    cases()
        .into_iter()
        .filter(|case| native_feature_supported(case.kind))
        .collect()
}

#[cfg(target_arch = "x86_64")]
fn execute_native_case_range(cases: &[DotCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = initial_state(case, ordinal);
        let expected = architectural_expected(case, &initial);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                interpret(&bytes, &initial, level),
                expected,
                "interpreter {level:?} {case:?} {bytes:02X?}"
            );
            assert_eq!(
                execute_native(&bytes, &initial, level),
                expected,
                "native {level:?} {case:?} {bytes:02X?}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn run_child_range(test_name: &str, range: std::ops::Range<usize>) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_RANGE_ENV, format!("{}:{}", range.start, range.end))
        .output()
        .expect("run isolated native VEX extended integer dot-product differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = supported_native_cases();
    if cases.is_empty() {
        eprintln!(
            "skipping native VEX extended integer dot-product differential: \
             host lacks AVX-VNNI-INT8 and AVX-VNNI-INT16"
        );
        return;
    }
    eprintln!(
        "executing {} native VEX extended integer dot-product cases",
        cases.len()
    );
    if let Some(range) = child_range() {
        execute_native_case_range(&cases, range);
        return;
    }

    let whole = run_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        return;
    }
    let mut start = 0usize;
    let mut end = cases.len();
    while end - start > 1 {
        let middle = start + (end - start) / 2;
        if run_child_range(test_name, start..middle).status.success() {
            start = middle;
        } else {
            end = middle;
        }
    }
    let singleton = run_child_range(test_name, start..end);
    let case = cases[start];
    let bytes = encoding(case);
    panic!(
        "isolated native VEX extended integer dot-product failure at case {start}/{}: \
         {case:?} {bytes:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_intel_o0_o2_equations_on_every_available_host_feature() {
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_integer_dot_ext_replay::\
         replay_matches_intel_o0_o2_equations_on_every_available_host_feature",
    );
}
